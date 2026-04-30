//! H.3 — Daily auto-prune cron for `persona_audit` rows.
//!
//! Deletes audit rows older than 30 days once per 24-hour wall-clock period.
//! Mirrors the `HeartbeatRegistry` tokio::interval pattern from Phase F.
//!
//! ## Usage
//!
//! Spawn once at process startup after opening `MemoryDb`:
//!
//! ```no_run
//! tokio::spawn(poly_chat_mcp::persona_audit_prune::run_forever(mem.clone()));
//! ```
//!
//! ## Schedule
//! First run: immediately on spawn (prunes any backlog accumulated while the
//! process was offline).
//! Subsequent runs: every 24 hours.
//!
//! ## Retention
//! `RETENTION_DAYS = 30` — audit rows older than 30 calendar days are deleted.

use crate::memory::MemoryDb;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{MissedTickBehavior, interval};
use tracing::{info, warn};

/// Prune persona_audit rows older than this many days.
pub const RETENTION_DAYS: u64 = 30;

/// Run the prune loop forever (cancel by aborting the task handle).
///
/// The loop fires immediately on start (prune backlog), then every 24 hours.
pub async fn run_forever(mem: MemoryDb) {
    let period = Duration::from_secs(24 * 3600);
    let mut ticker = interval(period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let cutoff = cutoff_iso8601(RETENTION_DAYS);
        match mem.prune_persona_audit_before(&cutoff) {
            Ok(deleted) => {
                if deleted > 0 {
                    info!(deleted, cutoff, "persona_audit prune: removed old rows");
                } else {
                    info!("persona_audit prune: nothing to delete");
                }
            }
            Err(e) => warn!("persona_audit prune failed: {e}"),
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn cutoff_iso8601(retention_days: u64) -> String {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff_secs = now_secs.saturating_sub(retention_days * 86_400);
    unix_secs_to_iso8601(cutoff_secs)
}

fn unix_secs_to_iso8601(secs: u64) -> String {
    let sec  = secs % 60;
    let min  = (secs / 60) % 60;
    let h    = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{min:02}:{sec:02}Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn cutoff_is_30_days_ago() {
        let c = cutoff_iso8601(30);
        assert_eq!(c.len(), 20, "wrong length: {c}");
        assert!(c.ends_with('Z'));
        // Cutoff must be in the past relative to today.
        let today_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let today_str = unix_secs_to_iso8601(today_secs);
        assert!(c < today_str, "cutoff {c} should be before today {today_str}");
    }

    #[test]
    fn unix_iso8601_round_trip() {
        // 2026-01-01T00:00:00Z = 1767225600
        let ts = "2026-01-01T00:00:00Z";
        // Parse forward.
        let parts: Vec<u64> = ts
            .trim_end_matches('Z')
            .split(['T', '-', ':'])
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(parts.len(), 6);
        // Just verify round-trip from a known secs value.
        let back = unix_secs_to_iso8601(1_767_225_600);
        assert_eq!(back, ts);
    }
}
