//! # Phase F — Heartbeat scheduler
//!
//! `HeartbeatRegistry` owns one tokio task per enabled persona that has
//! `heartbeat_interval_secs > 0`. Each task runs a wall-clock-aligned
//! `tokio::time::interval` and calls the built-in summariser on every tick.
//!
//! ## Design decision — F.7 polling vs MPSC
//!
//! We use **polling (option b)**: each heartbeat task re-reads the persona row
//! at the top of every tick and stops itself when `enabled=0` or
//! `heartbeat_interval_secs IS NULL`. This avoids IPC plumbing between
//! `chat-mcp`'s tool dispatcher and `poly-host`'s scheduler.
//!
//! Trade-off: a change to `heartbeat_interval_secs` takes up to one full
//! interval to be observed by the running task. For 15-minute or 1-hour
//! heartbeats this is acceptable. The registry's `restart(slug)` method
//! provides an escape hatch when the caller wants immediate reconfiguration
//! (e.g. from a UI button).
//!
//! ## Startup seeding
//!
//! Call `HeartbeatRegistry::start_all_enabled(mem, provider)` once after
//! `MemoryDb` opens. It reads all enabled personas with a configured heartbeat
//! and spawns a task for each. Re-starting poly-host will not re-fire all
//! heartbeats at once because each task computes its first tick from
//! `last_run_at` and applies `MissedTickBehavior::Skip`.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Local, Timelike as _};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, MissedTickBehavior, interval_at};
use tracing::{info, warn};

use crate::memory::MemoryDb;
use crate::persona::context::{
    PersonaBackendProvider, PersonaContextBundle, PersonaContextRequest,
    build as build_context,
};

// ─── Heartbeat output types ───────────────────────────────────────────────────

/// The two things the built-in summariser can emit per chat.
#[derive(Debug)]
pub enum HeartbeatOutput {
    /// A textual summary notification for a chat that has new activity.
    Notification {
        chat_id: String,
        chat_name: Option<String>,
        summary: String,
    },
    /// A draft placeholder for a chat that has an unanswered question.
    DraftPlaceholder {
        chat_id: String,
        chat_name: Option<String>,
        prompt: String,
    },
}

// ─── Pure summariser — F.4 ────────────────────────────────────────────────────

/// Built-in summariser — pure function, no I/O.
///
/// For each chat in the bundle:
/// - If it has ≥1 recent messages: emit a `Notification`.
/// - If the last message looks like an unanswered question (contains `?` in
///   its last 200 characters and the author is not the persona slug itself):
///   also emit a `DraftPlaceholder`.
///
/// "Pure" here means no database calls and no randomness.  Easy to unit-test.
pub fn summarise(bundle: &PersonaContextBundle) -> Vec<HeartbeatOutput> {
    let mut out = Vec::new();

    let persona_name = bundle
        .persona
        .get("slug")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    for chat in &bundle.chats {
        let msg_count = chat.recent_messages.len();
        if msg_count == 0 && chat.summary.is_none() {
            continue;
        }

        // Notification for any chat with activity.
        if msg_count > 0 || chat.summary.is_some() {
            let summary_text = if let Some(s) = &chat.summary {
                format!(
                    "{name} noticed activity in {chat}: {s}",
                    name = bundle.persona.get("name").and_then(|v| v.as_str()).unwrap_or(persona_name),
                    chat = chat.chat_name.as_deref().unwrap_or(&chat.chat_id),
                    s = s,
                )
            } else {
                let senders: std::collections::HashSet<&str> =
                    chat.recent_messages.iter().map(|m| m.from.as_str()).collect();
                format!(
                    "{name} noticed {count} new message{s} from {people} in {chat}.",
                    name = bundle.persona.get("name").and_then(|v| v.as_str()).unwrap_or(persona_name),
                    count = msg_count,
                    s = if msg_count == 1 { "" } else { "s" },
                    people = senders.len(),
                    chat = chat.chat_name.as_deref().unwrap_or(&chat.chat_id),
                )
            };
            out.push(HeartbeatOutput::Notification {
                chat_id: chat.chat_id.clone(),
                chat_name: chat.chat_name.clone(),
                summary: summary_text,
            });
        }

        // Draft placeholder if the most-recent message looks like an unanswered
        // question.  `recent_messages` is ordered most-recent-first (Discord et al.
        // return newest-first), so `.first()` is the newest message.
        if let Some(newest_msg) = chat.recent_messages.first() {
            let tail = if newest_msg.text.len() > 200 {
                &newest_msg.text[newest_msg.text.len() - 200..]
            } else {
                &newest_msg.text
            };
            // Heuristic: contains `?` and the author is not the persona itself.
            if tail.contains('?') && newest_msg.from != persona_name {
                out.push(HeartbeatOutput::DraftPlaceholder {
                    chat_id: chat.chat_id.clone(),
                    chat_name: chat.chat_name.clone(),
                    prompt: format!(
                        "{name}: there is an unanswered question in {chat}: \"{msg}\"",
                        name = bundle.persona.get("name").and_then(|v| v.as_str()).unwrap_or(persona_name),
                        chat = chat.chat_name.as_deref().unwrap_or(&chat.chat_id),
                        msg = newest_msg.text.chars().take(300).collect::<String>(),
                    ),
                });
            }
        }
    }

    out
}

// ─── Quiet-hours guard — F.6 ─────────────────────────────────────────────────

/// Return `true` if the current local hour is within the outbound quiet window
/// (22:00–08:00 inclusive, i.e. h >= 22 OR h < 8).
///
/// "Local TZ" is the server process's system timezone as reported by
/// `chrono::Local`.  Per the plan, quiet hours apply to OUTBOUND only;
/// notify and draft still fire so the user sees them in the morning.
pub fn in_quiet_hours() -> bool {
    let h = Local::now().hour();
    h >= 22 || h < 8
}

// ─── Per-task heartbeat runner ────────────────────────────────────────────────

/// Run one heartbeat loop for `slug`.
///
/// The task self-terminates when:
/// - The oneshot `cancel_rx` fires.
/// - The persona row is missing, `enabled=0`, or has no `heartbeat_interval_secs`.
///
/// Every tick:
/// 1. Re-reads persona row (F.7 polling).
/// 2. Builds context bundle.
/// 3. Runs summariser.
/// 4. Checks rate limit (F.5) and quiet hours (F.6).
/// 5. Persists outputs based on `proactivity`.
/// 6. Updates `last_run_at` and writes `heartbeat_run` audit row.
async fn run_heartbeat_task<P>(
    slug: String,
    first_tick: Instant,
    interval_secs: u64,
    mem: MemoryDb,
    provider: Arc<P>,
    mut cancel_rx: oneshot::Receiver<()>,
) where
    P: PersonaBackendProvider + 'static,
{
    let period = Duration::from_secs(interval_secs);
    let mut ticker = interval_at(first_tick, period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = &mut cancel_rx => {
                info!(slug, "heartbeat cancelled");
                return;
            }
        }

        // F.7 — polling: re-read persona row each tick.
        let persona = match mem.get_persona(&slug) {
            Ok(Some(p)) => p,
            Ok(None) => {
                warn!(slug, "heartbeat: persona deleted — stopping task");
                return;
            }
            Err(e) => {
                warn!(slug, "heartbeat: db error reading persona: {e}");
                continue;
            }
        };

        // Stop if disabled or heartbeat removed.
        let enabled = persona.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        let hb_secs = persona
            .get("heartbeat_interval_secs")
            .and_then(|v| v.as_i64())
            .filter(|&s| s > 0);

        if !enabled || hb_secs.is_none() {
            info!(slug, "heartbeat: disabled or cleared — stopping task");
            return;
        }

        // F.3 — Build context bundle.
        let req = PersonaContextRequest {
            slug: slug.clone(),
            user_prompt: Some("Catch me up on what's happened since your last run.".to_string()),
            dry_run: false,
            ..PersonaContextRequest::default()
        };

        let bundle_result = build_context(req, &mem, provider.as_ref()).await;
        let bundle = match bundle_result {
            Ok(b) => b,
            Err(e) => {
                warn!(slug, "heartbeat: context build failed: {e}");
                let _ = mem.record_persona_audit(
                    &slug, "heartbeat", "heartbeat_run",
                    None, None,
                    Some(&format!("{{\"error\":\"{e}\"}}")),
                    "error",
                    Some(&e.to_string()),
                );
                let _ = mem.update_persona_last_run_at(&slug);
                continue;
            }
        };

        // F.4 — Summarise.
        let outputs = summarise(&bundle);

        // F.5 — Rate-limit check.
        let rate_limit = persona
            .get("rate_limit_per_hour")
            .and_then(|v| v.as_i64())
            .unwrap_or(4);

        let proactivity = persona
            .get("proactivity")
            .and_then(|v| v.as_str())
            .unwrap_or("drafts-only")
            .to_string();

        // One-hour cutoff in ISO-8601 UTC.
        let cutoff_1h = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(3600);
            unix_secs_to_iso8601(secs)
        };

        let recent_action_count = match mem.count_persona_audit_since(&slug, &cutoff_1h) {
            Ok(n) => n,
            Err(e) => {
                warn!(slug, "heartbeat: rate-limit query failed: {e}");
                0
            }
        };

        if recent_action_count >= rate_limit {
            warn!(slug, recent_action_count, rate_limit, "heartbeat: rate limited");
            let _ = mem.record_persona_audit(
                &slug, "heartbeat", "rate_limited",
                None, None,
                Some(&format!("{{\"count\":{recent_action_count},\"limit\":{rate_limit}}}")),
                "ok",
                None,
            );
            let _ = mem.update_persona_last_run_at(&slug);
            continue;
        }

        // F.6 — Quiet-hours guard (outbound only).
        let quiet = in_quiet_hours();

        // Determine what to emit based on proactivity.
        // Proactivity map (from the plan section 6):
        //   drafts-only          → drafts yes, notifications no, outbound no
        //   notify               → drafts yes, notifications yes, outbound no
        //   outbound-allowlisted → drafts yes, notifications yes, outbound yes
        let emit_notify = proactivity == "notify" || proactivity == "outbound-allowlisted";
        let emit_outbound = proactivity == "outbound-allowlisted" && !quiet;

        let mut touched_accounts: std::collections::HashSet<String> = std::collections::HashSet::new();
        for chat in &bundle.chats {
            touched_accounts.insert(chat.account_id.clone());
        }

        let mut draft_count = 0_u32;
        let mut notify_count = 0_u32;

        for output in &outputs {
            match output {
                HeartbeatOutput::Notification { chat_id, summary, .. } => {
                    if emit_notify {
                        // Persist a draft for the notification so the user sees it.
                        // We reuse the `drafts` table — heartbeat notifications are
                        // stored as drafts with `suggested_by = "heartbeat:<slug>"`.
                        // Find the account_id for this chat from the bundle.
                        if let Some(chat_entry) = bundle.chats.iter().find(|c| &c.chat_id == chat_id) {
                            match mem.draft_insert(
                                &chat_entry.account_id,
                                chat_id,
                                summary,
                                &format!("heartbeat:{slug}"),
                                None,
                            ) {
                                Ok(_) => {
                                    let _ = mem.record_persona_audit(
                                        &slug, "heartbeat", "notify",
                                        Some(&chat_entry.account_id),
                                        Some(chat_id),
                                        None,
                                        "ok",
                                        None,
                                    );
                                    notify_count += 1;
                                }
                                Err(e) => warn!(slug, %chat_id, "heartbeat: notify draft insert failed: {e}"),
                            }
                        }
                    }
                }
                HeartbeatOutput::DraftPlaceholder { chat_id, prompt, .. } => {
                    // Draft placeholders always emitted (proactivity covers drafts in all modes).
                    if let Some(chat_entry) = bundle.chats.iter().find(|c| &c.chat_id == chat_id) {
                        match mem.draft_insert(
                            &chat_entry.account_id,
                            chat_id,
                            prompt,
                            &format!("heartbeat:{slug}"),
                            None,
                        ) {
                            Ok(_) => {
                                let _ = mem.record_persona_audit(
                                    &slug, "heartbeat", "draft_create",
                                    Some(&chat_entry.account_id),
                                    Some(chat_id),
                                    None,
                                    "ok",
                                    None,
                                );
                                draft_count += 1;
                            }
                            Err(e) => warn!(slug, %chat_id, "heartbeat: draft insert failed: {e}"),
                        }
                    }

                    // F.6 — If outbound is enabled and not quiet hours, we would send
                    // here.  Phase G (outbound) is not yet shipped; log the intent.
                    if emit_outbound {
                        info!(slug, %chat_id, "heartbeat: outbound would fire (Phase G not yet shipped)");
                    }
                }
            }
        }

        // Quiet-hours outbound skip audit.
        if proactivity == "outbound-allowlisted" && quiet {
            let _ = mem.record_persona_audit(
                &slug, "heartbeat", "quiet_hours_skipped",
                None, None,
                None,
                "ok",
                None,
            );
        }

        // F.3 — Write heartbeat_run audit row at completion.
        let accts: Vec<&str> = touched_accounts.iter().map(|s| s.as_str()).collect();
        let payload = format!(
            "{{\"chats_touched\":{ct},\"drafts\":{dc},\"notifications\":{nc},\"accounts\":{an}}}",
            ct = bundle.chats.len(),
            dc = draft_count,
            nc = notify_count,
            an = serde_json::to_string(&accts).unwrap_or_else(|_| "[]".to_string()),
        );
        let _ = mem.record_persona_audit(
            &slug, "heartbeat", "heartbeat_run",
            None, None,
            Some(&payload),
            "ok",
            None,
        );

        // F.3 — Update last_run_at unconditionally.
        let _ = mem.update_persona_last_run_at(&slug);

        info!(slug, "heartbeat tick complete");
    }
}

// ─── Registry — F.1 ──────────────────────────────────────────────────────────

struct HeartbeatEntry {
    handle: JoinHandle<()>,
    /// Dropped to signal the worker to stop.
    _cancel_tx: oneshot::Sender<()>,
}

/// Registry of per-persona heartbeat tasks.
///
/// Each entry is a tokio task plus a cancellation oneshot.  Dropping the
/// `_cancel_tx` closes the channel so the worker's `select!` branch fires.
///
/// Thread-safety: wrap in `Arc<tokio::sync::Mutex<HeartbeatRegistry>>` when
/// shared across async contexts.
pub struct HeartbeatRegistry {
    entries: HashMap<String, HeartbeatEntry>,
}

impl HeartbeatRegistry {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Start a heartbeat task for `slug`.
    ///
    /// If a task already exists for this slug, it is stopped first (equivalent
    /// to calling [`stop`] then [`start`]).
    ///
    /// `interval_secs` — heartbeat period.
    /// `last_run_at`   — ISO-8601 UTC timestamp from the persona row, or `None`
    ///                   to fire immediately on the first tick.
    pub fn start<P>(
        &mut self,
        slug: &str,
        interval_secs: u64,
        last_run_at: Option<&str>,
        mem: MemoryDb,
        provider: Arc<P>,
    ) where
        P: PersonaBackendProvider + 'static,
    {
        self.stop(slug);

        let first_tick = compute_first_tick(interval_secs, last_run_at);
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        let slug_owned = slug.to_string();

        let handle = tokio::spawn(run_heartbeat_task(
            slug_owned,
            first_tick,
            interval_secs,
            mem,
            provider,
            cancel_rx,
        ));

        self.entries.insert(
            slug.to_string(),
            HeartbeatEntry { handle, _cancel_tx: cancel_tx },
        );
        info!(slug, interval_secs, "heartbeat started");
    }

    /// Stop the heartbeat task for `slug` if one is running.  No-op otherwise.
    pub fn stop(&mut self, slug: &str) {
        if let Some(entry) = self.entries.remove(slug) {
            // Drop _cancel_tx → closes channel → worker's select! fires.
            drop(entry._cancel_tx);
            entry.handle.abort();
            info!(slug, "heartbeat stopped");
        }
    }

    /// Stop then start a heartbeat task, re-reading the interval from the DB.
    ///
    /// Called when `meta_persona_set_heartbeat` or `meta_persona_update` fires
    /// and the caller wants immediate reconfiguration without waiting for the
    /// next tick.
    pub fn restart<P>(
        &mut self,
        slug: &str,
        interval_secs: u64,
        last_run_at: Option<&str>,
        mem: MemoryDb,
        provider: Arc<P>,
    ) where
        P: PersonaBackendProvider + 'static,
    {
        self.stop(slug);
        self.start(slug, interval_secs, last_run_at, mem, provider);
    }

    /// Seed the registry from the database at startup.
    ///
    /// Queries all `enabled=1` personas with a `heartbeat_interval_secs`,
    /// spawns a task for each, and computes the first tick from `last_run_at`
    /// so a restart doesn't burst-fire all heartbeats at once.
    pub fn start_all_enabled<P>(&mut self, mem: &MemoryDb, provider: Arc<P>)
    where
        P: PersonaBackendProvider + 'static,
    {
        let rows = match mem.list_personas_for_heartbeat() {
            Ok(r) => r,
            Err(e) => {
                warn!("heartbeat startup: failed to list personas: {e}");
                return;
            }
        };
        for row in rows {
            let slug = match row.get("slug").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let interval_secs = match row
                .get("heartbeat_interval_secs")
                .and_then(|v| v.as_i64())
                .filter(|&s| s > 0)
            {
                Some(s) => s as u64,
                None => continue,
            };
            let last_run_at = row
                .get("last_run_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            self.start(
                &slug,
                interval_secs,
                last_run_at.as_deref(),
                mem.clone(),
                Arc::clone(&provider),
            );
        }
        info!("heartbeat startup complete ({} persona(s))", self.entries.len());
    }

    /// Number of active heartbeat tasks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if no tasks are running.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for HeartbeatRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Timing helpers ───────────────────────────────────────────────────────────

/// Compute the `Instant` for the first tick.
///
/// - If `last_run_at` is `None` → fire immediately.
/// - Otherwise compute `last_run_at + interval_secs`.  If that is in the past,
///   fire immediately (the persona is overdue).
fn compute_first_tick(interval_secs: u64, last_run_at: Option<&str>) -> Instant {
    let Some(last_str) = last_run_at else {
        return Instant::now();
    };

    let last_unix = parse_iso8601_to_unix(last_str).unwrap_or(0);
    if last_unix == 0 {
        return Instant::now();
    }

    use std::time::{SystemTime, UNIX_EPOCH};
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let next_unix = last_unix.saturating_add(interval_secs);
    if next_unix <= now_unix {
        // Overdue — fire immediately.
        Instant::now()
    } else {
        let delay_secs = next_unix - now_unix;
        Instant::now() + Duration::from_secs(delay_secs)
    }
}

/// Minimal ISO-8601 UTC parser for "YYYY-MM-DDTHH:MM:SSZ" format
/// (the format `now_iso8601()` in memory.rs produces).
///
/// Returns seconds since Unix epoch, or `None` on parse failure.
fn parse_iso8601_to_unix(s: &str) -> Option<u64> {
    // Expect: "YYYY-MM-DDTHH:MM:SSZ" (exactly 20 chars)
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let year  = parse_u64(&s[0..4])?;
    let month = parse_u64(&s[5..7])?;
    let day   = parse_u64(&s[8..10])?;
    let hour  = parse_u64(&s[11..13])?;
    let min   = parse_u64(&s[14..16])?;
    let sec   = parse_u64(&s[17..19])?;

    // Days since epoch via Gregorian calendar.
    let days = ymd_to_days(year, month, day)?;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

fn parse_u64(s: &str) -> Option<u64> {
    s.parse().ok()
}

/// Convert (year, month, day) to days since 1970-01-01 (Gregorian).
fn ymd_to_days(y: u64, m: u64, d: u64) -> Option<u64> {
    // Port of the inverse algorithm used in memory.rs `days_to_ymd`.
    // Reference: https://howardhinnant.github.io/date_algorithms.html
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y / 400;
    let yoe = y % 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Convert Unix seconds to ISO-8601 UTC "YYYY-MM-DDTHH:MM:SSZ".
fn unix_secs_to_iso8601(secs: u64) -> String {
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let h   = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd_pub(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{min:02}:{sec:02}Z")
}

/// Mirror of `days_to_ymd` from memory.rs (private there).
fn days_to_ymd_pub(days: u64) -> (u64, u64, u64) {
    let z   = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let mo  = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::persona::context::{ChatEntry, MessageBrief, PersonaContextBundle};
    use serde_json::json;

    fn make_bundle(chats: Vec<ChatEntry>) -> PersonaContextBundle {
        PersonaContextBundle {
            bundle_version: "v1".to_string(),
            persona: json!({"slug": "test-persona", "name": "Test", "avatar_emoji": "🧪"}),
            system_prompt: "You are a test.".to_string(),
            style_notes: None,
            pinned_facts: vec![],
            user_prompt: None,
            chats,
            recent_facts: vec![],
            dry_run: false,
        }
    }

    fn msg(from: &str, text: &str) -> MessageBrief {
        MessageBrief { from: from.to_string(), ts: "2026-01-01T00:00:00Z".to_string(), text: text.to_string() }
    }

    fn chat(id: &str, name: &str, msgs: Vec<MessageBrief>) -> ChatEntry {
        ChatEntry {
            account_id: "acc1".to_string(),
            chat_id: id.to_string(),
            chat_name: Some(name.to_string()),
            summary: None,
            recent_messages: msgs,
        }
    }

    // ── F.4 — summarise ───────────────────────────────────────────────────────

    #[test]
    fn summarise_empty_bundle() {
        let bundle = make_bundle(vec![]);
        let out = summarise(&bundle);
        assert!(out.is_empty());
    }

    #[test]
    fn summarise_notification_for_chat_with_messages() {
        let bundle = make_bundle(vec![chat("ch1", "general", vec![msg("alice", "hello")])]);
        let out = summarise(&bundle);
        assert!(!out.is_empty());
        let notif = out.iter().find(|o| matches!(o, HeartbeatOutput::Notification { .. }));
        assert!(notif.is_some(), "expected a Notification");
    }

    #[test]
    fn summarise_draft_placeholder_for_question() {
        let bundle = make_bundle(vec![
            chat("ch1", "general", vec![msg("alice", "What time is the meeting?")]),
        ]);
        let out = summarise(&bundle);
        let draft = out.iter().find(|o| matches!(o, HeartbeatOutput::DraftPlaceholder { .. }));
        assert!(draft.is_some(), "expected a DraftPlaceholder");
    }

    #[test]
    fn summarise_no_draft_placeholder_when_question_from_self() {
        // Author = persona slug = "test-persona" — should NOT produce a draft.
        let bundle = make_bundle(vec![
            chat("ch1", "general", vec![msg("test-persona", "Did I already send this?")]),
        ]);
        let out = summarise(&bundle);
        let draft = out.iter().find(|o| matches!(o, HeartbeatOutput::DraftPlaceholder { .. }));
        assert!(draft.is_none(), "self-authored question should not produce draft");
    }

    #[test]
    fn summarise_no_draft_placeholder_when_no_question_mark() {
        let bundle = make_bundle(vec![
            chat("ch1", "general", vec![msg("alice", "Just saying hi")]),
        ]);
        let out = summarise(&bundle);
        let draft = out.iter().find(|o| matches!(o, HeartbeatOutput::DraftPlaceholder { .. }));
        assert!(draft.is_none(), "no ? → no draft placeholder");
    }

    // ── Timing helpers ────────────────────────────────────────────────────────

    #[test]
    fn parse_iso8601_round_trip() {
        // A well-known timestamp: 2026-01-01T00:00:00Z = 1767225600
        let ts = "2026-01-01T00:00:00Z";
        let unix = parse_iso8601_to_unix(ts).expect("parse");
        let back = unix_secs_to_iso8601(unix);
        assert_eq!(back, ts);
    }

    #[test]
    fn compute_first_tick_none_is_immediate() {
        let tick = compute_first_tick(3600, None);
        // Should be very close to Instant::now() (within 1s).
        assert!(tick <= Instant::now() + Duration::from_secs(1));
    }

    #[test]
    fn compute_first_tick_overdue_is_immediate() {
        // last_run_at 10 hours ago, interval 1 hour → overdue → fire immediately.
        use std::time::{SystemTime, UNIX_EPOCH};
        let now_unix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let ten_hours_ago = now_unix.saturating_sub(36_000);
        let ts = unix_secs_to_iso8601(ten_hours_ago);
        let tick = compute_first_tick(3600, Some(&ts));
        assert!(tick <= Instant::now() + Duration::from_secs(2));
    }

    // ── F.6 — quiet hours (smoke test; doesn't mock Local) ───────────────────

    #[test]
    fn quiet_hours_returns_bool() {
        // Just ensure it doesn't panic.
        let _ = in_quiet_hours();
    }
}
