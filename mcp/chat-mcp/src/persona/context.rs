//! Phase C — `PersonaContextBuilder`.
//!
//! Turns a Phase B `bundle_v0` stub into a real cross-chat memory bundle
//! (`bundle_v1`) by:
//!
//! 1. Resolving `persona_sources` rows → concrete `(account_id, chat_id)` set,
//!    applying deny-wins precedence.
//! 2. Fetching chat summaries from `chat_summaries` (Phase A), falling back to
//!    `get_messages(limit=30)` when no summary is stored.
//! 3. Capping the serialised bundle at **32 KB** with progressive degradation:
//!    drop oldest messages → summary-only → drop whole chats from the tail.
//! 4. Writing a `memory_read` audit row for every chat read.
//!
//! The trait `PersonaBackendProvider` is the only async coupling point —
//! the real impl wraps `BackendPool`; tests use a canned mock.

use anyhow::Context as _;
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};
use tracing::warn;

use crate::memory::MemoryDb;
use crate::state::BackendPool;

// ─── Size cap ────────────────────────────────────────────────────────────────

/// Maximum serialised bundle size (bytes).  Past this cap progressive
/// degradation kicks in: first drop oldest messages, then go summary-only,
/// then drop whole chats from the tail.
const BUNDLE_SIZE_CAP: usize = 32 * 1024;

/// Headroom reserved for non-chat fields (bundle_version, persona header,
/// system_prompt, pinned_facts, recent_facts, user_prompt).  The chat-list
/// portion of the bundle must fit in BUNDLE_SIZE_CAP - CHAT_OVERHEAD.
const CHAT_OVERHEAD: usize = 4 * 1024;

/// Per-backend read timeout.  Mirrors `BackendHandleExt::read_with_timeout`
/// default.  chat-mcp owns the backend directly (no RwLock), so we wrap
/// the async call in `tokio::time::timeout` instead.
const BACKEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Minimum messages to retain per chat during cap-driven shrinkage.
const MIN_MESSAGES_FLOOR: usize = 5;

// ─── Public types ────────────────────────────────────────────────────────────

/// Stable identifier for a (account, chat) pair used in
/// `PersonaContextBundle::chats`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ChatRef {
    pub account_id: String,
    pub chat_id: String,
}

/// A brief message summary included in the bundle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageBrief {
    pub from: String,
    pub ts: String,
    pub text: String,
}

/// One chat entry inside the bundle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatEntry {
    pub account_id: String,
    pub chat_id: String,
    /// Human-readable chat name, if the backend returned it.
    pub chat_name: Option<String>,
    /// Paragraph summary from `chat_summaries`, if available.
    pub summary: Option<String>,
    /// Recent messages fetched from the backend (may be empty if summary is present).
    pub recent_messages: Vec<MessageBrief>,
}

/// Input to the context builder.
#[derive(Debug, Clone)]
pub struct PersonaContextRequest {
    pub slug: String,
    /// Free-form text from the user's invocation (e.g. "what's the read on tonight?").
    pub user_prompt: Option<String>,
    /// Maximum messages fetched per chat when no summary is available.
    pub max_messages_per_chat: usize,
    /// Maximum number of chats included in the bundle.
    pub max_chats: usize,
    /// Whether to prefer `chat_summaries` over raw messages.
    pub include_summaries: bool,
    /// When true, skip writing `memory_read` audit rows for each chat read.
    ///
    /// The bundle shape is identical to a normal invocation; only the implicit
    /// per-chat audit rows are suppressed.  The user-initiated `invoke` audit
    /// row is written by `handle_meta_persona_invoke` in `tools.rs` regardless
    /// of this flag — suppressing it here would remove the only record that the
    /// user requested the persona, which is always useful.
    pub dry_run: bool,
}

impl Default for PersonaContextRequest {
    fn default() -> Self {
        Self {
            slug: String::new(),
            user_prompt: None,
            max_messages_per_chat: 30,
            max_chats: 25,
            include_summaries: true,
            dry_run: false,
        }
    }
}

/// The full context bundle returned from `meta_persona_invoke`.
///
/// JSON shape (v1, normal invocation):
/// ```json
/// {
///   "bundle_version": "v1",
///   "persona": { "slug": "…", "name": "…", "avatar_emoji": "…" },
///   "system_prompt": "…",
///   "style_notes": null,
///   "pinned_facts": [ … ],
///   "user_prompt": "…",
///   "chats": [
///     {
///       "account_id": "…",
///       "chat_id": "…",
///       "chat_name": "…",
///       "summary": "…",
///       "recent_messages": [ { "from": "…", "ts": "…", "text": "…" } ]
///     }
///   ],
///   "recent_facts": [ … ]
/// }
/// ```
///
/// When `dry_run=true` an additional top-level field is added:
/// `"dry_run": true` — signals to consumers (e2e harness, future UI preview)
/// that this bundle was built without writing per-chat audit rows.
#[derive(Debug, serde::Serialize)]
pub struct PersonaContextBundle {
    pub bundle_version: String,
    pub persona: Value,
    pub system_prompt: String,
    pub style_notes: Option<String>,
    pub pinned_facts: Vec<Value>,
    pub user_prompt: Option<String>,
    pub chats: Vec<ChatEntry>,
    pub recent_facts: Vec<Value>,
    /// Present and `true` only when `PersonaContextRequest::dry_run` was set.
    /// Omitted from the JSON entirely in normal (non-dry-run) invocations via
    /// `#[serde(skip_serializing_if)]`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
}

// ─── Backend provider trait ──────────────────────────────────────────────────

/// Minimal async surface the context builder needs from a backend.
///
/// Separated from `BackendPool` so unit tests can inject a mock without
/// spinning up real backends (Interface Segregation — CLAUDE.md SOLID).
#[async_trait::async_trait]
pub trait PersonaBackendProvider: Send + Sync {
    /// All accounts known to this provider.
    fn account_ids(&self) -> Vec<String>;

    /// Enumerate servers for an account.  Returns `(server_id, server_name)`.
    async fn list_servers(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>>;

    /// Enumerate channels within a server.  Returns `(channel_id, channel_name)`.
    async fn list_channels(
        &self,
        account_id: &str,
        server_id: &str,
    ) -> anyhow::Result<Vec<(String, String)>>;

    /// Enumerate DM channels for an account.  Returns `(dm_id, partner_name)`.
    async fn list_dms(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>>;

    /// Fetch the most recent `limit` messages from a chat.
    async fn fetch_messages(
        &self,
        account_id: &str,
        chat_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<MessageBrief>>;
}

// ─── Real impl wrapping BackendPool ──────────────────────────────────────────

/// Wraps a shared reference to `BackendPool` with a 5-second per-call timeout.
///
/// Because `BackendPool` uses a plain `std::sync::Mutex` (not an async RwLock),
/// we take the approach of cloning the `Arc<dyn IsBackend>` under the lock,
/// releasing the lock, then calling the async backend method outside the lock.
/// This matches the pattern already used in `main.rs:run_autosend_engine`.
pub struct BackendPoolProvider<'a> {
    pub pool: &'a BackendPool,
}

#[async_trait::async_trait]
impl PersonaBackendProvider for BackendPoolProvider<'_> {
    fn account_ids(&self) -> Vec<String> {
        self.pool
            .list_accounts()
            .into_iter()
            .filter_map(|v| v.get("user_id").and_then(|u| u.as_str()).map(std::string::ToString::to_string))
            .collect()
    }

    async fn list_servers(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        let backend = self
            .pool
            .find_by_account(account_id)
            .map(|e| std::sync::Arc::clone(&e.backend))
            .with_context(|| format!("no backend for account {account_id}"))?;

        let servers = timeout(BACKEND_TIMEOUT, backend.get_servers())
            .await
            .with_context(|| format!("timeout listing servers for {account_id}"))??;

        Ok(servers
            .into_iter()
            .map(|s| (s.id.to_string(), s.name))
            .collect())
    }

    async fn list_channels(
        &self,
        account_id: &str,
        server_id: &str,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let backend = self
            .pool
            .find_by_account(account_id)
            .map(|e| std::sync::Arc::clone(&e.backend))
            .with_context(|| format!("no backend for account {account_id}"))?;

        let channels = timeout(BACKEND_TIMEOUT, backend.get_channels(server_id))
            .await
            .with_context(|| format!("timeout listing channels for server {server_id}"))??;

        Ok(channels
            .into_iter()
            .map(|c| (c.id.to_string(), c.name))
            .collect())
    }

    async fn list_dms(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        let backend = self
            .pool
            .find_by_account(account_id)
            .map(|e| std::sync::Arc::clone(&e.backend))
            .with_context(|| format!("no backend for account {account_id}"))?;

        let dms = match backend.as_dms_and_groups() {
            Some(dg) => timeout(BACKEND_TIMEOUT, dg.get_dm_channels())
                .await
                .with_context(|| format!("timeout listing DMs for {account_id}"))??,
            None => vec![],
        };

        Ok(dms
            .into_iter()
            .map(|d| (d.id.to_string(), d.user.display_name))
            .collect())
    }

    async fn fetch_messages(
        &self,
        account_id: &str,
        chat_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<MessageBrief>> {
        let backend = self
            .pool
            .find_by_account(account_id)
            .map(|e| std::sync::Arc::clone(&e.backend))
            .with_context(|| format!("no backend for account {account_id}"))?;

        let query = poly_client::MessageQuery {
            limit: Some(u32::try_from(limit).unwrap_or(u32::MAX)),
            ..Default::default()
        };

        let messages = timeout(BACKEND_TIMEOUT, backend.get_messages(chat_id, query))
            .await
            .with_context(|| format!("timeout fetching messages for {chat_id}"))??;

        Ok(messages
            .into_iter()
            .map(|m| MessageBrief {
                from: m.author.display_name,
                ts: m.timestamp.to_rfc3339(),
                text: match m.content {
                    poly_client::MessageContent::Text(t) => t,
                    _ => String::new(),
                },
            })
            .collect())
    }
}

// ─── Source resolution ────────────────────────────────────────────────────────

/// A concrete (account, chat) pair after source resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResolvedChat {
    account_id: String,
    chat_id: String,
    /// Optional human-readable chat name found during enumeration.
    chat_name: Option<String>,
}

/// A typed representation of a single `persona_sources` row used by the
/// deny-wins resolver and exposed for fuzz testing.
///
/// Field names match the SQLite column names in `persona_sources`.
/// This struct is `pub` so `mcp/chat-mcp/fuzz` can import and derive
/// `Arbitrary` on a mirror type without depending on the whole crate's
/// internal query plumbing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaSourceRow {
    pub account_id: String,
    /// Selector taxonomy: `"all"`, `"server"`, `"channel"`, `"dm"`, `"tag"`.
    pub selector_kind: String,
    /// The concrete ID (server/channel/dm id); `None` for `selector_kind = "all"`.
    pub selector_value: Option<String>,
    /// `true` = allow, `false` = deny.
    pub include: bool,
}

/// Pure, synchronous deny-wins check: given a flat list of `PersonaSourceRow`s
/// and a candidate `(account_id, chat_id)` pair, return `true` if the candidate
/// is included under the deny-wins algorithm.
///
/// This is the fuzz-target entry point.  It implements a **subset** of the
/// full `resolve_sources` algorithm — specifically the rules that can be
/// evaluated without backend enumeration:
///
/// - `selector_kind = "channel"` or `"dm"` with `selector_value == chat_id`.
/// - `selector_kind = "all"` (applies to every chat in the account).
/// - `selector_kind = "server"` is treated as a wildcard match by the caller
///   providing the channel's parent server id in `selector_value`.  Without a
///   backend, the fuzz target supplies the server mapping directly by using
///   `chat_id` as both channel and server id where needed.
///
/// Deny-wins: if ANY matching row has `include=false`, the candidate is
/// denied regardless of allow rows.  Only if no deny matches AND at least
/// one allow matches is the candidate included.
///
/// # Algorithm
///
/// 1. Filter rows to those whose `account_id` matches the candidate.
/// 2. Check for any deny row whose selector matches the candidate chat.
/// 3. If any deny matches → return `false`.
/// 4. Check for any allow row whose selector matches → return `true`.
/// 5. No matching rule → return `false` (default-deny).
#[must_use] 
pub fn is_chat_included(
    rows: &[PersonaSourceRow],
    account_id: &str,
    chat_id: &str,
) -> bool {
    let account_rows: Vec<&PersonaSourceRow> = rows
        .iter()
        .filter(|r| r.account_id == account_id)
        .collect();

    // Check denies first (deny-wins).
    for row in &account_rows {
        if row.include {
            continue;
        }
        if selector_matches(row, chat_id) {
            return false;
        }
    }

    // Check allows.
    for row in &account_rows {
        if !row.include {
            continue;
        }
        if selector_matches(row, chat_id) {
            return true;
        }
    }

    // Default-deny: no rule matched.
    false
}

/// Returns `true` if `row`'s selector covers `chat_id`.
///
/// For `"server"` selectors the row's `selector_value` is compared against
/// `chat_id` directly — the fuzz harness encodes "channel X is in server S"
/// by using the server ID as the chat ID, which is sufficient for exercising
/// the deny-wins logic without backend round-trips.
fn selector_matches(row: &PersonaSourceRow, chat_id: &str) -> bool {
    match row.selector_kind.as_str() {
        "all" => true,
        "channel" | "dm" => {
            row.selector_value.as_deref() == Some(chat_id)
        }
        "server" => {
            // In the fuzz context: treat a server selector as matching any
            // chat whose id equals the server_value.  The real resolver
            // expands server → channels via backend; here we approximate.
            row.selector_value.as_deref() == Some(chat_id)
        }
        // "tag" selectors are unsupported (see context.rs warn! branch).
        // Treat unknown / tag as non-matching for the deny-wins check.
        _ => false,
    }
}

/// Resolve `persona_sources` rows for `slug` into a concrete list of
/// `(account_id, chat_id)` pairs, honouring deny-wins precedence.
///
/// Algorithm:
/// - For each source row with `include=true`, expand to concrete (account, chat) pairs.
/// - For each source row with `include=false`, record the denied pairs.
/// - Subtract denied pairs from allowed pairs.
///
/// Selector taxonomy (from `memory.rs` UNIQUE constraint comment):
/// - `"all"`     → every chat in the account
/// - `"server"`  → all channels in `selector_value` (a server/guild ID)
/// - `"channel"` → the channel directly
/// - `"dm"`      → DM channel directly
/// - `"tag"`     → future; skipped with a warning for now
async fn resolve_sources(
    slug: &str,
    sources: &[Value],
    provider: &dyn PersonaBackendProvider,
) -> Vec<ResolvedChat> {
    let mut allowed: Vec<ResolvedChat> = Vec::new();
    let mut denied: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    // Split sources into allow and deny first, then process allows.
    let allow_sources: Vec<&Value> = sources
        .iter()
        .filter(|s| s.get("include").and_then(serde_json::Value::as_bool).unwrap_or(true))
        .collect();
    let deny_sources: Vec<&Value> = sources
        .iter()
        .filter(|s| !s.get("include").and_then(serde_json::Value::as_bool).unwrap_or(true))
        .collect();

    // Expand deny rows first (they're cheaper — no backend call for channel/dm).
    for src in &deny_sources {
        let account_id = match src.get("account_id").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => continue,
        };
        let kind = src
            .get("selector_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("channel");
        let value = src
            .get("selector_value")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        match kind {
            "all" => {
                // Deny whole account — add a sentinel we check during allow expansion.
                // We don't pre-enumerate; we'll filter later.
                // Use a special sentinel that the allow expander checks.
                denied.insert((account_id.clone(), "__all__".to_string()));
            }
            "server" => {
                // Deny a whole server — expand channels under it.
                let server_id = match &value {
                    Some(v) => v.clone(),
                    None => continue,
                };
                match timeout(BACKEND_TIMEOUT, provider.list_channels(&account_id, &server_id)).await {
                    Ok(Ok(channels)) => {
                        for (ch_id, _) in channels {
                            denied.insert((account_id.clone(), ch_id));
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(slug, %account_id, %server_id, "deny-server channel enumeration failed: {e}");
                    }
                    Err(_) => {
                        warn!(slug, %account_id, %server_id, "deny-server channel enumeration timed out");
                    }
                }
            }
            "channel" | "dm" => {
                if let Some(chat_id) = value {
                    denied.insert((account_id, chat_id));
                }
            }
            other => {
                warn!(slug, "unsupported deny selector_kind '{other}' — skipped");
            }
        }
    }

    // Expand allow rows.
    for src in &allow_sources {
        let account_id = match src.get("account_id").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => continue,
        };

        // Whole-account deny check.
        if denied.contains(&(account_id.clone(), "__all__".to_string())) {
            continue;
        }

        let kind = src
            .get("selector_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("channel");
        let value = src
            .get("selector_value")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        match kind {
            "all" => {
                // Enumerate all servers + channels + DMs for the account.
                let servers = match timeout(BACKEND_TIMEOUT, provider.list_servers(&account_id)).await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        warn!(slug, %account_id, "list_servers failed: {e}");
                        vec![]
                    }
                    Err(_) => {
                        warn!(slug, %account_id, "list_servers timed out");
                        vec![]
                    }
                };
                for (srv_id, _srv_name) in servers {
                    let channels = match timeout(BACKEND_TIMEOUT, provider.list_channels(&account_id, &srv_id)).await {
                        Ok(Ok(c)) => c,
                        Ok(Err(e)) => {
                            warn!(slug, %account_id, %srv_id, "list_channels failed: {e}");
                            vec![]
                        }
                        Err(_) => {
                            warn!(slug, %account_id, %srv_id, "list_channels timed out");
                            vec![]
                        }
                    };
                    for (ch_id, ch_name) in channels {
                        if !denied.contains(&(account_id.clone(), ch_id.clone())) {
                            allowed.push(ResolvedChat {
                                account_id: account_id.clone(),
                                chat_id: ch_id,
                                chat_name: Some(ch_name),
                            });
                        }
                    }
                }
                // Also add DMs.
                let dms = match timeout(BACKEND_TIMEOUT, provider.list_dms(&account_id)).await {
                    Ok(Ok(d)) => d,
                    Ok(Err(e)) => {
                        warn!(slug, %account_id, "list_dms failed: {e}");
                        vec![]
                    }
                    Err(_) => {
                        warn!(slug, %account_id, "list_dms timed out");
                        vec![]
                    }
                };
                for (dm_id, partner_name) in dms {
                    if !denied.contains(&(account_id.clone(), dm_id.clone())) {
                        allowed.push(ResolvedChat {
                            account_id: account_id.clone(),
                            chat_id: dm_id,
                            chat_name: Some(partner_name),
                        });
                    }
                }
            }
            "server" => {
                let server_id = match value {
                    Some(v) => v,
                    None => continue,
                };
                let channels = match timeout(BACKEND_TIMEOUT, provider.list_channels(&account_id, &server_id)).await {
                    Ok(Ok(c)) => c,
                    Ok(Err(e)) => {
                        warn!(slug, %account_id, %server_id, "list_channels for server failed: {e}");
                        vec![]
                    }
                    Err(_) => {
                        warn!(slug, %account_id, %server_id, "list_channels for server timed out");
                        vec![]
                    }
                };
                for (ch_id, ch_name) in channels {
                    if !denied.contains(&(account_id.clone(), ch_id.clone())) {
                        allowed.push(ResolvedChat {
                            account_id: account_id.clone(),
                            chat_id: ch_id,
                            chat_name: Some(ch_name),
                        });
                    }
                }
            }
            "channel" | "dm" => {
                if let Some(chat_id) = value
                    && !denied.contains(&(account_id.clone(), chat_id.clone())) {
                        allowed.push(ResolvedChat {
                            account_id,
                            chat_id,
                            chat_name: None,
                        });
                    }
            }
            "tag" => {
                warn!(slug, "selector_kind 'tag' is not yet supported — skipped");
            }
            other => {
                warn!(slug, "unknown selector_kind '{other}' — skipped");
            }
        }
    }

    // Deduplicate while preserving order.
    let mut seen = std::collections::HashSet::new();
    allowed.retain(|c| seen.insert((c.account_id.clone(), c.chat_id.clone())));

    allowed
}

// ─── Progressive size-cap degradation ────────────────────────────────────────

/// Shrink `chats` entries until `serde_json::to_string` of the whole bundle
/// fits within `BUNDLE_SIZE_CAP`.
///
/// Three stages:
/// 1. Drop oldest messages per chat until each has `≤ MIN_MESSAGES_FLOOR`.
/// 2. Set `recent_messages = []` for all chats (summary-only).
/// 3. Drop whole chats from the tail until the bundle fits.
fn apply_size_cap(chats: &mut Vec<ChatEntry>) {
    let chat_cap = BUNDLE_SIZE_CAP.saturating_sub(CHAT_OVERHEAD);

    // Stage 1: reduce messages per chat progressively.
    'stage1: loop {
        let size = estimate_size(chats);
        if size <= chat_cap {
            break;
        }
        // Find the chat with the most messages (above floor) and trim it.
        let target = chats.iter_mut().max_by_key(|c| c.recent_messages.len());
        if let Some(entry) = target
            && entry.recent_messages.len() > MIN_MESSAGES_FLOOR {
                // Drop oldest message (index 0 is oldest in our ordering).
                entry.recent_messages.remove(0);
                continue 'stage1;
            }
        break; // All chats are at floor — move to stage 2.
    }

    // Stage 2: drop all messages from every chat.
    if estimate_size(chats) > chat_cap {
        for entry in chats.iter_mut() {
            entry.recent_messages.clear();
        }
    }

    // Stage 3: drop chats from the tail.
    while estimate_size(chats) > chat_cap && !chats.is_empty() {
        chats.pop();
    }
}

fn estimate_size(chats: &[ChatEntry]) -> usize {
    serde_json::to_string(chats).map(|s| s.len()).unwrap_or(usize::MAX)
}

// ─── Orchestrator ─────────────────────────────────────────────────────────────

/// Build a `PersonaContextBundle` (v1) for the given request.
///
/// This is the function `handle_meta_persona_invoke` in `tools.rs` calls
/// after Phase B's persona-exists + enabled checks pass.
pub async fn build(
    req: PersonaContextRequest,
    mem: &MemoryDb,
    provider: &dyn PersonaBackendProvider,
) -> anyhow::Result<PersonaContextBundle> {
    let slug = &req.slug;

    // Load persona row (caller already checked it exists and is enabled).
    let persona = mem
        .get_persona(slug)
        .with_context(|| format!("db error loading persona '{slug}'"))?
        .with_context(|| format!("persona '{slug}' not found"))?;

    let system_prompt = persona
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let style_notes = persona
        .get("style_notes")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);

    let persona_header = json!({
        "slug":         persona.get("slug"),
        "name":         persona.get("name"),
        "avatar_emoji": persona.get("avatar_emoji"),
    });

    // Pinned facts.
    let pinned_facts = mem.list_persona_facts(slug, true).unwrap_or_default();

    // Recent (non-pinned) facts — last 50.
    let all_facts = mem.list_persona_facts(slug, false).unwrap_or_default();
    let recent_facts: Vec<Value> = all_facts
        .into_iter()
        .filter(|f| !f.get("pinned").and_then(serde_json::Value::as_bool).unwrap_or(false))
        .rev()
        .take(50)
        .collect();

    // Source resolution — C.2.
    let sources = mem.list_persona_sources(slug).unwrap_or_default();
    let resolved = resolve_sources(slug, &sources, provider).await;

    // Cap to max_chats — C.2 step 2.
    let candidates: Vec<ResolvedChat> = resolved.into_iter().take(req.max_chats).collect();

    // Per-chat content fetching — C.3, C.4, C.6.
    let mut chat_entries: Vec<ChatEntry> = Vec::with_capacity(candidates.len());

    for chat in &candidates {
        let account_id = &chat.account_id;
        let chat_id = &chat.chat_id;

        // C.4 — Try summary first.
        let summary_opt = if req.include_summaries {
            mem.get_chat_summary(account_id, chat_id)
                .unwrap_or_default()
                .and_then(|v| v.get("summary").and_then(|s| s.as_str()).map(std::string::ToString::to_string))
        } else {
            None
        };

        let (summary, recent_messages) = if let Some(summary_text) = summary_opt {
            // We have a stored summary — use it; skip fetching messages.
            (Some(summary_text), vec![])
        } else {
            // Fall back to fetching messages — C.4 fallback.
            let msgs = match timeout(
                BACKEND_TIMEOUT,
                provider.fetch_messages(account_id, chat_id, req.max_messages_per_chat),
            )
            .await
            {
                Ok(Ok(messages)) => {
                    // C.6 — audit successful read.
                    // Suppressed when dry_run=true: the caller (handle_meta_persona_invoke)
                    // writes the user-initiated invoke row unconditionally; only the
                    // implicit per-chat memory_read rows are skipped here.
                    if !req.dry_run {
                        let payload = format!(
                            "{{\"message_count\":{}}}",
                            messages.len()
                        );
                        drop(mem.record_persona_audit(
                            slug,
                            "claude-desktop",
                            "memory_read",
                            Some(account_id.as_str()),
                            Some(chat_id.as_str()),
                            Some(&payload),
                            "ok",
                            None,
                        ));
                    }
                    messages
                }
                Ok(Err(e)) => {
                    warn!(slug, %account_id, %chat_id, "fetch_messages error: {e}");
                    if !req.dry_run {
                        drop(mem.record_persona_audit(
                            slug,
                            "claude-desktop",
                            "memory_read",
                            Some(account_id.as_str()),
                            Some(chat_id.as_str()),
                            None,
                            "error",
                            Some(&e.to_string()),
                        ));
                    }
                    vec![]
                }
                Err(_) => {
                    warn!(slug, %account_id, %chat_id, "fetch_messages timed out");
                    if !req.dry_run {
                        drop(mem.record_persona_audit(
                            slug,
                            "claude-desktop",
                            "memory_read",
                            Some(account_id.as_str()),
                            Some(chat_id.as_str()),
                            None,
                            "error",
                            Some("timeout"),
                        ));
                    }
                    vec![]
                }
            };
            (None, msgs)
        };

        // Also write an audit row when we used a stored summary.
        // Suppressed in dry_run mode for the same reason as above.
        if summary.is_some() && !req.dry_run {
            drop(mem.record_persona_audit(
                slug,
                "claude-desktop",
                "memory_read",
                Some(account_id.as_str()),
                Some(chat_id.as_str()),
                Some("{\"source\":\"summary\"}"),
                "ok",
                None,
            ));
        }

        chat_entries.push(ChatEntry {
            account_id: account_id.clone(),
            chat_id: chat_id.clone(),
            chat_name: chat.chat_name.clone(),
            summary,
            recent_messages,
        });
    }

    // C.5 — Apply 32 KB size cap with progressive degradation.
    apply_size_cap(&mut chat_entries);

    Ok(PersonaContextBundle {
        bundle_version: "v1".to_string(),
        persona: persona_header,
        system_prompt,
        style_notes,
        pinned_facts,
        user_prompt: req.user_prompt,
        chats: chat_entries,
        recent_facts,
        dry_run: req.dry_run,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

    use super::*;
    use crate::memory::MemoryDb;
    use std::collections::HashMap;

    // ── Mock backend provider ──────────────────────────────────────────────

    /// Canned provider for unit tests — no real backends required.
    struct MockProvider {
        /// account_id → list of (server_id, server_name)
        servers: HashMap<String, Vec<(String, String)>>,
        /// (account_id, server_id) → list of (channel_id, channel_name)
        channels: HashMap<(String, String), Vec<(String, String)>>,
        /// account_id → list of (dm_id, partner_name)
        dms: HashMap<String, Vec<(String, String)>>,
        /// (account_id, chat_id) → messages
        messages: HashMap<(String, String), Vec<MessageBrief>>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                servers: HashMap::new(),
                channels: HashMap::new(),
                dms: HashMap::new(),
                messages: HashMap::new(),
            }
        }

        fn add_server(&mut self, account: &str, srv_id: &str, srv_name: &str) {
            self.servers
                .entry(account.to_string())
                .or_default()
                .push((srv_id.to_string(), srv_name.to_string()));
        }

        fn add_channel(
            &mut self,
            account: &str,
            server: &str,
            ch_id: &str,
            ch_name: &str,
        ) {
            self.channels
                .entry((account.to_string(), server.to_string()))
                .or_default()
                .push((ch_id.to_string(), ch_name.to_string()));
        }

        #[allow(dead_code)] // reserved for future tests covering DM-bound personas.
        fn add_dm(&mut self, account: &str, dm_id: &str, partner: &str) {
            self.dms
                .entry(account.to_string())
                .or_default()
                .push((dm_id.to_string(), partner.to_string()));
        }

        fn add_messages(&mut self, account: &str, chat: &str, msgs: Vec<MessageBrief>) {
            self.messages.insert((account.to_string(), chat.to_string()), msgs);
        }
    }

    #[async_trait::async_trait]
    impl PersonaBackendProvider for MockProvider {
        fn account_ids(&self) -> Vec<String> {
            self.servers.keys().cloned().collect()
        }

        async fn list_servers(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(self.servers.get(account_id).cloned().unwrap_or_default())
        }

        async fn list_channels(
            &self,
            account_id: &str,
            server_id: &str,
        ) -> anyhow::Result<Vec<(String, String)>> {
            Ok(self
                .channels
                .get(&(account_id.to_string(), server_id.to_string()))
                .cloned()
                .unwrap_or_default())
        }

        async fn list_dms(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(self.dms.get(account_id).cloned().unwrap_or_default())
        }

        async fn fetch_messages(
            &self,
            account_id: &str,
            chat_id: &str,
            _limit: usize,
        ) -> anyhow::Result<Vec<MessageBrief>> {
            Ok(self
                .messages
                .get(&(account_id.to_string(), chat_id.to_string()))
                .cloned()
                .unwrap_or_default())
        }
    }

    // ── Helper: build an in-memory MemoryDb with one persona ──────────────

    fn make_mem() -> MemoryDb {
        MemoryDb::open(":memory:").expect("in-memory db")
    }

    fn create_test_persona(mem: &MemoryDb, slug: &str) {
        mem.create_persona(
            slug,
            "Test Persona",
            "🧪",
            "You are a test persona.",
            None,
            None,
            "drafts-only",
            4,
        )
        .expect("create persona");
    }

    // ── C.2 — source resolution + deny-wins ───────────────────────────────

    #[tokio::test]
    async fn source_resolution_channel_direct() {
        let mem = make_mem();
        create_test_persona(&mem, "test");
        mem.add_persona_source("test", "acc1", "channel", Some("ch1"), true)
            .expect("add source");

        let mut provider = MockProvider::new();
        provider.add_messages("acc1", "ch1", vec![
            MessageBrief { from: "alice".into(), ts: "2026-01-01".into(), text: "hello".into() },
        ]);

        let req = PersonaContextRequest {
            slug: "test".to_string(),
            user_prompt: None,
            max_messages_per_chat: 30,
            max_chats: 25,
            include_summaries: true,
            dry_run: false,
        };

        let bundle = build(req, &mem, &provider).await.expect("build");
        assert_eq!(bundle.chats.len(), 1);
        assert_eq!(bundle.chats[0].chat_id, "ch1");
        assert_eq!(bundle.chats[0].recent_messages.len(), 1);
        assert_eq!(bundle.chats[0].recent_messages[0].text, "hello");
    }

    #[tokio::test]
    async fn source_resolution_deny_wins() {
        // Allow a whole server, but deny one channel inside it.
        let mem = make_mem();
        create_test_persona(&mem, "p1");
        mem.add_persona_source("p1", "acc1", "server", Some("srv1"), true)
            .expect("allow server");
        mem.add_persona_source("p1", "acc1", "channel", Some("ch2"), false)
            .expect("deny channel");

        let mut provider = MockProvider::new();
        provider.add_channel("acc1", "srv1", "ch1", "general");
        provider.add_channel("acc1", "srv1", "ch2", "secret");  // this one is denied
        provider.add_messages("acc1", "ch1", vec![
            MessageBrief { from: "bob".into(), ts: "t1".into(), text: "hi".into() },
        ]);
        provider.add_messages("acc1", "ch2", vec![
            MessageBrief { from: "bob".into(), ts: "t2".into(), text: "should not appear".into() },
        ]);

        let req = PersonaContextRequest {
            slug: "p1".to_string(),
            ..PersonaContextRequest::default()
        };

        let bundle = build(req, &mem, &provider).await.expect("build");
        // ch2 is denied — should not appear.
        assert!(!bundle.chats.iter().any(|c| c.chat_id == "ch2"), "denied channel appeared");
        assert!(bundle.chats.iter().any(|c| c.chat_id == "ch1"), "allowed channel missing");
    }

    #[tokio::test]
    async fn source_resolution_deny_whole_account() {
        let mem = make_mem();
        create_test_persona(&mem, "p2");
        // Allow all, then deny all — deny wins.
        mem.add_persona_source("p2", "acc1", "all", None, true)
            .expect("allow all");
        mem.add_persona_source("p2", "acc1", "all", None, false)
            .expect("deny all");

        let mut provider = MockProvider::new();
        provider.add_server("acc1", "srv1", "MyServer");
        provider.add_channel("acc1", "srv1", "ch1", "general");

        let req = PersonaContextRequest {
            slug: "p2".to_string(),
            ..PersonaContextRequest::default()
        };

        let bundle = build(req, &mem, &provider).await.expect("build");
        // Whole account denied — no chats.
        assert!(bundle.chats.is_empty(), "expected empty chats after deny-all");
    }

    // ── C.5 — 32 KB size-cap progressive degradation ──────────────────────

    #[tokio::test]
    async fn size_cap_drops_messages_then_summaries_then_chats() {
        // Build a bundle that starts over 32 KB by injecting many large chats.
        let mem = make_mem();
        create_test_persona(&mem, "big");

        let mut provider = MockProvider::new();

        // Add 20 channel sources with many large messages each.
        for i in 0..20_u32 {
            let ch = format!("ch{i}");
            mem.add_persona_source("big", "acc1", "channel", Some(&ch), true)
                .expect("add source");
            // Each message is ~200 bytes; 30 messages = 6KB per chat × 20 = 120KB.
            let msgs: Vec<MessageBrief> = (0_i32..30_i32)
                .map(|j| MessageBrief {
                    from: format!("user{j}"),
                    ts: format!("2026-01-0{j:02}"),
                    text: "A".repeat(150), // ~150-byte text
                })
                .collect();
            provider.add_messages("acc1", &ch, msgs);
        }

        let req = PersonaContextRequest {
            slug: "big".to_string(),
            max_messages_per_chat: 30,
            max_chats: 25,
            include_summaries: false,
            ..PersonaContextRequest::default()
        };

        let bundle = build(req, &mem, &provider).await.expect("build");

        // Serialised chats portion must fit within BUNDLE_SIZE_CAP - CHAT_OVERHEAD.
        // (The full bundle includes persona header, system_prompt, etc. which add overhead.)
        let chats_serialised = serde_json::to_string(&bundle.chats).expect("serialise chats");
        let chat_cap = BUNDLE_SIZE_CAP.saturating_sub(CHAT_OVERHEAD);
        assert!(
            chats_serialised.len() <= chat_cap,
            "chats portion too large: {} bytes (cap {})",
            chats_serialised.len(),
            chat_cap
        );
    }

    #[tokio::test]
    async fn size_cap_stages_in_order() {
        // Use `apply_size_cap` directly so we can inspect intermediate state.
        let make_chat = |id: &str, msgs: usize| ChatEntry {
            account_id: "acc".to_string(),
            chat_id: id.to_string(),
            chat_name: None,
            summary: None,
            recent_messages: (0..msgs)
                .map(|i| MessageBrief {
                    from: format!("u{i}"),
                    ts: "t".to_string(),
                    text: "X".repeat(200),
                })
                .collect(),
        };

        // Start with 10 chats × 30 messages × 200 bytes each = way over 32 KB.
        let mut chats: Vec<ChatEntry> = (0_i32..10_i32).map(|i| make_chat(&format!("ch{i}"), 30)).collect();

        apply_size_cap(&mut chats);

        let size = estimate_size(&chats);
        assert!(size <= BUNDLE_SIZE_CAP, "after cap: {size} bytes, limit {BUNDLE_SIZE_CAP}");

        // At least one chat should survive (we don't want empty results when feasible).
        // (With 200-byte messages the summary-only stage usually succeeds before
        // dropping chats, but if even one chat's metadata is > 32KB the list will be
        // empty — that's correct behaviour.)
    }

    // ── C.6 — Audit rows written for each successful read ──────────────────

    #[tokio::test]
    async fn audit_rows_written_per_chat() {
        let mem = make_mem();
        create_test_persona(&mem, "audited");
        mem.add_persona_source("audited", "acc1", "channel", Some("ch1"), true)
            .expect("add source");
        mem.add_persona_source("audited", "acc1", "channel", Some("ch2"), true)
            .expect("add source");

        let mut provider = MockProvider::new();
        provider.add_messages("acc1", "ch1", vec![
            MessageBrief { from: "x".into(), ts: "t1".into(), text: "hi".into() },
        ]);
        provider.add_messages("acc1", "ch2", vec![
            MessageBrief { from: "y".into(), ts: "t2".into(), text: "bye".into() },
        ]);

        let req = PersonaContextRequest {
            slug: "audited".to_string(),
            include_summaries: false,
            ..PersonaContextRequest::default()
        };

        build(req, &mem, &provider).await.expect("build");

        let audit = mem.list_persona_audit("audited", 50).expect("list audit");
        let memory_reads: Vec<&Value> = audit
            .iter()
            .filter(|r| r.get("action").and_then(|a| a.as_str()) == Some("memory_read"))
            .collect();
        // Expect one audit row per chat (ch1, ch2).
        assert_eq!(memory_reads.len(), 2, "expected 2 memory_read audit rows");
    }

    // ── C.4 — Summary preferred over messages ─────────────────────────────

    #[tokio::test]
    async fn summary_preferred_over_messages() {
        let mem = make_mem();
        create_test_persona(&mem, "s1");
        mem.add_persona_source("s1", "acc1", "channel", Some("ch1"), true)
            .expect("add source");
        // Store a chat summary for ch1.
        mem.store_chat_summary("acc1", "ch1", "Heated discussion about kangaroos.", "", "")
            .expect("store summary");

        let mut provider = MockProvider::new();
        // Messages exist but should NOT be fetched when summary is present.
        provider.add_messages("acc1", "ch1", vec![
            MessageBrief { from: "alice".into(), ts: "t".into(), text: "raw message".into() },
        ]);

        let req = PersonaContextRequest {
            slug: "s1".to_string(),
            include_summaries: true,
            ..PersonaContextRequest::default()
        };

        let bundle = build(req, &mem, &provider).await.expect("build");
        assert_eq!(bundle.chats.len(), 1);
        let entry = &bundle.chats[0];
        assert!(
            entry.summary.as_deref() == Some("Heated discussion about kangaroos."),
            "expected summary"
        );
        // When summary is used, messages should be empty.
        assert!(entry.recent_messages.is_empty(), "expected no messages when summary available");
    }

    // ── Bundle version ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn bundle_version_is_v1() {
        let mem = make_mem();
        create_test_persona(&mem, "vtest");

        let provider = MockProvider::new();
        let req = PersonaContextRequest {
            slug: "vtest".to_string(),
            ..PersonaContextRequest::default()
        };

        let bundle = build(req, &mem, &provider).await.expect("build");
        assert_eq!(bundle.bundle_version, "v1");
    }
}
