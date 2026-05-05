//! Private helpers shared across all memory sub-modules.
//!
//! All functions here are `pub(super)` — accessible within the `memory`
//! module tree but not exported to external callers.

use sqlite::State;

use super::MemoryError;

// ─── Timestamp ────────────────────────────────────────────────────────────────

// poly-lint: bounded calendar arithmetic on u64 timestamps.
#[allow(clippy::arithmetic_side_effects, clippy::integer_division)]
pub(super) fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
// poly-lint: textbook Hinnant Gregorian-calendar algorithm; operands bounded by Unix epoch range.
#[allow(clippy::arithmetic_side_effects, clippy::integer_division)]
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

// ─── Statement helpers ────────────────────────────────────────────────────────

/// Step a statement to completion (for INSERT/UPDATE/DELETE).
pub(super) fn drain(stmt: &mut sqlite::Statement<'_>) -> Result<(), MemoryError> {
    while stmt.next()? != State::Done {}
    Ok(())
}

/// Bind an `Option<&str>` to a positional parameter (NULL when `None`).
pub(super) fn bind_opt_str(
    stmt: &mut sqlite::Statement<'_>,
    pos: usize,
    val: Option<&str>,
) -> Result<(), MemoryError> {
    match val {
        Some(v) => stmt.bind((pos, v))?,
        None    => stmt.bind((pos, sqlite::Value::Null))?,
    }
    Ok(())
}

// ─── Row collectors ───────────────────────────────────────────────────────────

pub(super) fn collect_facts(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        out.push(serde_json::json!({
            "id":          stmt.read::<i64, _>(0)?,
            "account_id":  stmt.read::<String, _>(1)?,
            "contact_id":  stmt.read::<String, _>(2)?,
            "category":    stmt.read::<String, _>(3)?,
            "fact_text":   stmt.read::<String, _>(4)?,
            "created_at":  stmt.read::<String, _>(5)?,
            "updated_at":  stmt.read::<String, _>(6)?,
        }));
    }
    Ok(out)
}

pub(super) fn collect_notes(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        out.push(serde_json::json!({
            "id":          stmt.read::<i64, _>(0)?,
            "account_id":  stmt.read::<String, _>(1)?,
            "chat_id":     stmt.read::<String, _>(2)?,
            "note_text":   stmt.read::<String, _>(3)?,
            "created_at":  stmt.read::<String, _>(4)?,
            "updated_at":  stmt.read::<String, _>(5)?,
        }));
    }
    Ok(out)
}

pub(super) fn collect_drafts(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        let auto_send_at: Option<String> = match stmt.read::<sqlite::Value, _>(6)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        out.push(serde_json::json!({
            "id":           stmt.read::<i64, _>(0)?,
            "account_id":   stmt.read::<String, _>(1)?,
            "chat_id":      stmt.read::<String, _>(2)?,
            "body":         stmt.read::<String, _>(3)?,
            "suggested_by": stmt.read::<String, _>(4)?,
            "created_at":   stmt.read::<String, _>(5)?,
            "auto_send_at": auto_send_at,
            "status":       stmt.read::<String, _>(7)?,
        }));
    }
    Ok(out)
}

/// Read a single `chat_style` row from a prepared statement already
/// positioned at a row.  Column order:
/// 0=tone 1=formality 2=emoji_allowed 3=signature 4=extra_notes 5=updated_at
pub(super) fn read_style_row(stmt: &mut sqlite::Statement<'_>) -> Result<serde_json::Value, MemoryError> {
    Ok(serde_json::json!({
        "tone":          stmt.read::<Option<String>, _>(0)?,
        "formality":     stmt.read::<Option<String>, _>(1)?,
        "emoji_allowed": stmt.read::<i64, _>(2)? != 0,
        "signature":     stmt.read::<Option<String>, _>(3)?,
        "extra_notes":   stmt.read::<Option<String>, _>(4)?,
        "updated_at":    stmt.read::<String, _>(5)?,
    }))
}

/// Read a single `personas` row.  Column order matches `get_persona` / `list_personas`.
/// 0=slug 1=name 2=avatar_emoji 3=system_prompt 4=style_notes
/// 5=heartbeat_interval_secs 6=proactivity 7=rate_limit_per_hour
/// 8=created_at 9=updated_at 10=last_run_at 11=enabled 12=quiet_hours_disabled
pub(super) fn read_persona_row(stmt: &mut sqlite::Statement<'_>) -> Result<serde_json::Value, MemoryError> {
    let style_notes: Option<String> = match stmt.read::<sqlite::Value, _>(4)? {
        sqlite::Value::String(s) => Some(s),
        sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
    };
    let hb: Option<i64> = match stmt.read::<sqlite::Value, _>(5)? {
        sqlite::Value::Integer(v) => Some(v),
        sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::String(_) | sqlite::Value::Null => None,
    };
    let last_run: Option<String> = match stmt.read::<sqlite::Value, _>(10)? {
        sqlite::Value::String(s) => Some(s),
        sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
    };
    Ok(serde_json::json!({
        "slug":                    stmt.read::<String, _>(0)?,
        "name":                    stmt.read::<String, _>(1)?,
        "avatar_emoji":            stmt.read::<String, _>(2)?,
        "system_prompt":           stmt.read::<String, _>(3)?,
        "style_notes":             style_notes,
        "heartbeat_interval_secs": hb,
        "proactivity":             stmt.read::<String, _>(6)?,
        "rate_limit_per_hour":     stmt.read::<i64, _>(7)?,
        "created_at":              stmt.read::<String, _>(8)?,
        "updated_at":              stmt.read::<String, _>(9)?,
        "last_run_at":             last_run,
        "enabled":                 stmt.read::<i64, _>(11)? != 0,
        "quiet_hours_disabled":    stmt.read::<i64, _>(12)? != 0,
    }))
}

pub(super) fn collect_persona_facts(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        let cat: Option<String> = match stmt.read::<sqlite::Value, _>(2)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        out.push(serde_json::json!({
            "id":           stmt.read::<i64, _>(0)?,
            "persona_slug": stmt.read::<String, _>(1)?,
            "category":     cat,
            "fact_text":    stmt.read::<String, _>(3)?,
            "pinned":       stmt.read::<i64, _>(4)? != 0,
            "created_at":   stmt.read::<String, _>(5)?,
            "updated_at":   stmt.read::<String, _>(6)?,
        }));
    }
    Ok(out)
}

pub(super) fn collect_persona_audit(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        let ta: Option<String> = match stmt.read::<sqlite::Value, _>(5)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        let tc: Option<String> = match stmt.read::<sqlite::Value, _>(6)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        let pj: Option<String> = match stmt.read::<sqlite::Value, _>(7)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        let em: Option<String> = match stmt.read::<sqlite::Value, _>(9)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        out.push(serde_json::json!({
            "id":             stmt.read::<i64, _>(0)?,
            "persona_slug":   stmt.read::<String, _>(1)?,
            "occurred_at":    stmt.read::<String, _>(2)?,
            "actor":          stmt.read::<String, _>(3)?,
            "action":         stmt.read::<String, _>(4)?,
            "target_account": ta,
            "target_chat":    tc,
            "payload_json":   pj,
            "result":         stmt.read::<String, _>(8)?,
            "error_msg":      em,
        }));
    }
    Ok(out)
}
