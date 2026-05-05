//! Persona CRUD, audit, and heartbeat helpers.
//!
//! `UpdatePersonaArgs` and `QueryPersonaAuditArgs` are builder structs that
//! replace the 10-parameter and 9-parameter positional signatures with
//! named-field structs.  The old positional methods remain for compatibility
//! with existing call sites; they forward to the builder path.

use super::helpers::{
    bind_opt_str, collect_persona_audit, collect_persona_facts,
    drain, now_iso8601, read_persona_row,
};
use super::{MemoryDb, MemoryError};

// ─── Builder structs ──────────────────────────────────────────────────────────

/// Named-field replacement for the 10-parameter `update_persona` signature.
#[derive(Default)]
pub struct UpdatePersonaArgs<'a> {
    pub name:                    Option<&'a str>,
    pub avatar_emoji:            Option<&'a str>,
    pub system_prompt:           Option<&'a str>,
    pub style_notes:             Option<Option<&'a str>>,
    pub heartbeat_interval_secs: Option<Option<i64>>,
    pub proactivity:             Option<&'a str>,
    pub rate_limit_per_hour:     Option<i64>,
    pub enabled:                 Option<bool>,
    pub last_run_at:             Option<Option<&'a str>>,
}

/// Named-field replacement for the 9-parameter `query_persona_audit` signature.
#[derive(Default)]
pub struct QueryPersonaAuditArgs<'a> {
    pub slug:           Option<&'a str>,
    pub action:         Option<&'a str>,
    pub actor:          Option<&'a str>,
    pub since:          Option<&'a str>,
    pub until:          Option<&'a str>,
    pub target_account: Option<&'a str>,
    pub target_chat:    Option<&'a str>,
    pub result:         Option<&'a str>,
    pub limit:          i64,
}

// ─── Persona CRUD ─────────────────────────────────────────────────────────────

impl MemoryDb {
    #[allow(clippy::too_many_arguments)]
    pub fn create_persona(
        &self,
        slug: &str,
        name: &str,
        avatar_emoji: &str,
        system_prompt: &str,
        style_notes: Option<&str>,
        heartbeat_interval_secs: Option<i64>,
        proactivity: &str,
        rate_limit_per_hour: i64,
    ) -> Result<String, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO personas
                (slug,name,avatar_emoji,system_prompt,style_notes,
                 heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                 created_at,updated_at,enabled)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1)"
        )?;
        stmt.bind((1, slug))?;
        stmt.bind((2, name))?;
        stmt.bind((3, avatar_emoji))?;
        stmt.bind((4, system_prompt))?;
        match style_notes {
            Some(v) => stmt.bind((5, v))?,
            None    => stmt.bind((5, sqlite::Value::Null))?,
        }
        match heartbeat_interval_secs {
            Some(v) => stmt.bind((6, v))?,
            None    => stmt.bind((6, sqlite::Value::Null))?,
        }
        stmt.bind((7, proactivity))?;
        stmt.bind((8, rate_limit_per_hour))?;
        stmt.bind((9, now.as_str()))?;
        stmt.bind((10, now.as_str()))?;
        drain(&mut stmt)?;
        Ok(slug.to_string())
    }

    pub fn get_persona(&self, slug: &str) -> Result<Option<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT slug,name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    created_at,updated_at,last_run_at,enabled,
                    COALESCE(quiet_hours_disabled,0)
             FROM personas WHERE slug=?1"
        )?;
        stmt.bind((1, slug))?;
        if stmt.next()? == sqlite::State::Row {
            Ok(Some(read_persona_row(&mut stmt)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_personas(&self) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT slug,name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    created_at,updated_at,last_run_at,enabled,
                    COALESCE(quiet_hours_disabled,0)
             FROM personas ORDER BY name"
        )?;
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            out.push(read_persona_row(&mut stmt)?);
        }
        Ok(out)
    }

    /// Partial-update a persona using positional args (backward-compatible).
    /// New call sites should prefer `update_persona_with`.
    #[allow(clippy::too_many_arguments)]
    pub fn update_persona(
        &self,
        slug: &str,
        name: Option<&str>,
        avatar_emoji: Option<&str>,
        system_prompt: Option<&str>,
        style_notes: Option<Option<&str>>,
        heartbeat_interval_secs: Option<Option<i64>>,
        proactivity: Option<&str>,
        rate_limit_per_hour: Option<i64>,
        enabled: Option<bool>,
        last_run_at: Option<Option<&str>>,
    ) -> Result<bool, MemoryError> {
        self.update_persona_with(slug, UpdatePersonaArgs {
            name,
            avatar_emoji,
            system_prompt,
            style_notes,
            heartbeat_interval_secs,
            proactivity,
            rate_limit_per_hour,
            enabled,
            last_run_at,
        })
    }

    pub fn update_persona_with(&self, slug: &str, args: UpdatePersonaArgs<'_>) -> Result<bool, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;

        let mut sel = db.prepare(
            "SELECT name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    enabled,last_run_at
             FROM personas WHERE slug=?1"
        )?;
        sel.bind((1, slug))?;
        if sel.next()? != sqlite::State::Row {
            return Ok(false);
        }
        let cur_name:    String         = sel.read::<String, _>(0)?;
        let cur_emoji:   String         = sel.read::<String, _>(1)?;
        let cur_prompt:  String         = sel.read::<String, _>(2)?;
        let cur_notes:   Option<String> = match sel.read::<sqlite::Value, _>(3)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        let cur_hb: Option<i64> = match sel.read::<sqlite::Value, _>(4)? {
            sqlite::Value::Integer(v) => Some(v),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::String(_) | sqlite::Value::Null => None,
        };
        let cur_pro:     String         = sel.read::<String, _>(5)?;
        let cur_rl:      i64            = sel.read::<i64, _>(6)?;
        let cur_enabled: i64            = sel.read::<i64, _>(7)?;
        let cur_lr: Option<String> = match sel.read::<sqlite::Value, _>(8)? {
            sqlite::Value::String(s) => Some(s),
            sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
        };
        drop(sel);

        let fin_name   = args.name.map_or(cur_name, std::string::ToString::to_string);
        let fin_emoji  = args.avatar_emoji.map_or(cur_emoji, std::string::ToString::to_string);
        let fin_prompt = args.system_prompt.map_or(cur_prompt, std::string::ToString::to_string);
        let fin_notes: Option<String> = match args.style_notes {
            Some(Some(v)) => Some(v.to_string()),
            Some(None)    => None,
            None          => cur_notes,
        };
        let fin_hb: Option<i64> = match args.heartbeat_interval_secs {
            Some(Some(v)) => Some(v),
            Some(None)    => None,
            None          => cur_hb,
        };
        let fin_pro = args.proactivity.map_or(cur_pro, std::string::ToString::to_string);
        let fin_rl  = args.rate_limit_per_hour.unwrap_or(cur_rl);
        let fin_en  = args.enabled.map_or(cur_enabled, |b| if b { 1_i64 } else { 0_i64 });
        let fin_lr: Option<String> = match args.last_run_at {
            Some(Some(v)) => Some(v.to_string()),
            Some(None)    => None,
            None          => cur_lr,
        };

        let mut stmt = db.prepare(
            "UPDATE personas SET
                name=?1, avatar_emoji=?2, system_prompt=?3, style_notes=?4,
                heartbeat_interval_secs=?5, proactivity=?6, rate_limit_per_hour=?7,
                enabled=?8, last_run_at=?9, updated_at=?10
             WHERE slug=?11"
        )?;
        stmt.bind((1, fin_name.as_str()))?;
        stmt.bind((2, fin_emoji.as_str()))?;
        stmt.bind((3, fin_prompt.as_str()))?;
        match &fin_notes {
            Some(v) => stmt.bind((4, v.as_str()))?,
            None    => stmt.bind((4, sqlite::Value::Null))?,
        }
        match fin_hb {
            Some(v) => stmt.bind((5, v))?,
            None    => stmt.bind((5, sqlite::Value::Null))?,
        }
        stmt.bind((6, fin_pro.as_str()))?;
        stmt.bind((7, fin_rl))?;
        stmt.bind((8, fin_en))?;
        match &fin_lr {
            Some(v) => stmt.bind((9, v.as_str()))?,
            None    => stmt.bind((9, sqlite::Value::Null))?,
        }
        stmt.bind((10, now.as_str()))?;
        stmt.bind((11, slug))?;
        drain(&mut stmt)?;

        let mut chk = db.prepare("SELECT changes()")?;
        if chk.next()? == sqlite::State::Row {
            Ok(chk.read::<i64, _>(0)? > 0)
        } else {
            Ok(false)
        }
    }

    pub fn set_persona_quiet_hours_disabled(
        &self,
        slug: &str,
        disabled: bool,
    ) -> Result<(), MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "UPDATE personas SET quiet_hours_disabled=?1, updated_at=?2 WHERE slug=?3"
        )?;
        stmt.bind((1, if disabled { 1_i64 } else { 0_i64 }))?;
        stmt.bind((2, now.as_str()))?;
        stmt.bind((3, slug))?;
        drain(&mut stmt)
    }

    pub fn delete_persona(&self, slug: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        db.execute("PRAGMA foreign_keys = ON")?;
        let mut stmt = db.prepare("DELETE FROM personas WHERE slug=?1")?;
        stmt.bind((1, slug))?;
        drain(&mut stmt)
    }

    // ─── persona_sources ──────────────────────────────────────────────────────

    pub fn add_persona_source(
        &self,
        persona_slug: &str,
        account_id: &str,
        selector_kind: &str,
        selector_value: Option<&str>,
        include: bool,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT OR IGNORE INTO persona_sources
                (persona_slug,account_id,selector_kind,selector_value,include,created_at)
             VALUES(?1,?2,?3,?4,?5,?6)"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, selector_kind))?;
        match selector_value {
            Some(v) => stmt.bind((4, v))?,
            None    => stmt.bind((4, sqlite::Value::Null))?,
        }
        stmt.bind((5, if include { 1_i64 } else { 0_i64 }))?;
        stmt.bind((6, now.as_str()))?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn list_persona_sources(
        &self,
        persona_slug: &str,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,persona_slug,account_id,selector_kind,selector_value,include,created_at
             FROM persona_sources WHERE persona_slug=?1 ORDER BY id"
        )?;
        stmt.bind((1, persona_slug))?;
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            let sv: Option<String> = match stmt.read::<sqlite::Value, _>(4)? {
                sqlite::Value::String(s) => Some(s),
                sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
            };
            out.push(serde_json::json!({
                "id":             stmt.read::<i64, _>(0)?,
                "persona_slug":   stmt.read::<String, _>(1)?,
                "account_id":     stmt.read::<String, _>(2)?,
                "selector_kind":  stmt.read::<String, _>(3)?,
                "selector_value": sv,
                "include":        stmt.read::<i64, _>(5)? != 0,
                "created_at":     stmt.read::<String, _>(6)?,
            }));
        }
        Ok(out)
    }

    pub fn remove_persona_source(&self, source_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM persona_sources WHERE id=?1")?;
        stmt.bind((1, source_id))?;
        drain(&mut stmt)
    }

    // ─── persona_tool_whitelist ───────────────────────────────────────────────

    pub fn add_persona_tool(&self, persona_slug: &str, tool_name: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT OR IGNORE INTO persona_tool_whitelist(persona_slug,tool_name)
             VALUES(?1,?2)"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, tool_name))?;
        drain(&mut stmt)
    }

    pub fn remove_persona_tool(
        &self,
        persona_slug: &str,
        tool_name: &str,
    ) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM persona_tool_whitelist WHERE persona_slug=?1 AND tool_name=?2"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, tool_name))?;
        drain(&mut stmt)
    }

    pub fn list_persona_tools(&self, persona_slug: &str) -> Result<Vec<String>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT tool_name FROM persona_tool_whitelist
             WHERE persona_slug=?1 ORDER BY tool_name"
        )?;
        stmt.bind((1, persona_slug))?;
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            out.push(stmt.read::<String, _>(0)?);
        }
        Ok(out)
    }

    // ─── persona_facts ────────────────────────────────────────────────────────

    pub fn add_persona_fact(
        &self,
        persona_slug: &str,
        category: Option<&str>,
        fact_text: &str,
        pinned: bool,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO persona_facts
                (persona_slug,category,fact_text,pinned,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)"
        )?;
        stmt.bind((1, persona_slug))?;
        match category {
            Some(v) => stmt.bind((2, v))?,
            None    => stmt.bind((2, sqlite::Value::Null))?,
        }
        stmt.bind((3, fact_text))?;
        stmt.bind((4, if pinned { 1_i64 } else { 0_i64 }))?;
        stmt.bind((5, now.as_str()))?;
        stmt.bind((6, now.as_str()))?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn list_persona_facts(
        &self,
        persona_slug: &str,
        pinned_only: bool,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let sql = if pinned_only {
            "SELECT id,persona_slug,category,fact_text,pinned,created_at,updated_at
             FROM persona_facts WHERE persona_slug=?1 AND pinned=1 ORDER BY id"
        } else {
            "SELECT id,persona_slug,category,fact_text,pinned,created_at,updated_at
             FROM persona_facts WHERE persona_slug=?1 ORDER BY id"
        };
        let mut stmt = db.prepare(sql)?;
        stmt.bind((1, persona_slug))?;
        collect_persona_facts(&mut stmt)
    }

    pub fn remove_persona_fact(&self, fact_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM persona_facts WHERE id=?1")?;
        stmt.bind((1, fact_id))?;
        drain(&mut stmt)
    }

    pub fn forget_all_persona_facts(&self, persona_slug: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM persona_facts WHERE persona_slug=?1")?;
        stmt.bind((1, persona_slug))?;
        drain(&mut stmt)
    }

    // ─── persona_outbound_allowlist ───────────────────────────────────────────

    pub fn set_persona_outbound_allow(
        &self,
        persona_slug: &str,
        account_id: &str,
        chat_id: &str,
        max_messages_per_day: i64,
    ) -> Result<(), MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO persona_outbound_allowlist
                (persona_slug,account_id,chat_id,max_messages_per_day,created_at)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(persona_slug,account_id,chat_id) DO UPDATE SET
                max_messages_per_day = excluded.max_messages_per_day"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, chat_id))?;
        stmt.bind((4, max_messages_per_day))?;
        stmt.bind((5, now.as_str()))?;
        drain(&mut stmt)
    }

    pub fn remove_persona_outbound_allow(
        &self,
        persona_slug: &str,
        account_id: &str,
        chat_id: &str,
    ) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM persona_outbound_allowlist
             WHERE persona_slug=?1 AND account_id=?2 AND chat_id=?3"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, chat_id))?;
        drain(&mut stmt)
    }

    pub fn list_persona_outbound_allows(
        &self,
        persona_slug: &str,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT persona_slug,account_id,chat_id,max_messages_per_day,created_at
             FROM persona_outbound_allowlist
             WHERE persona_slug=?1 ORDER BY account_id,chat_id"
        )?;
        stmt.bind((1, persona_slug))?;
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            out.push(serde_json::json!({
                "persona_slug":         stmt.read::<String, _>(0)?,
                "account_id":           stmt.read::<String, _>(1)?,
                "chat_id":              stmt.read::<String, _>(2)?,
                "max_messages_per_day": stmt.read::<i64, _>(3)?,
                "created_at":           stmt.read::<String, _>(4)?,
            }));
        }
        Ok(out)
    }

    // ─── persona_audit ────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub fn record_persona_audit(
        &self,
        persona_slug: &str,
        actor: &str,
        action: &str,
        target_account: Option<&str>,
        target_chat: Option<&str>,
        payload_json: Option<&str>,
        result: &str,
        error_msg: Option<&str>,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO persona_audit
                (persona_slug,occurred_at,actor,action,
                 target_account,target_chat,payload_json,result,error_msg)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, now.as_str()))?;
        stmt.bind((3, actor))?;
        stmt.bind((4, action))?;
        bind_opt_str(&mut stmt, 5, target_account)?;
        bind_opt_str(&mut stmt, 6, target_chat)?;
        bind_opt_str(&mut stmt, 7, payload_json)?;
        stmt.bind((8, result))?;
        bind_opt_str(&mut stmt, 9, error_msg)?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn list_persona_audit(
        &self,
        persona_slug: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,persona_slug,occurred_at,actor,action,
                    target_account,target_chat,payload_json,result,error_msg
             FROM persona_audit WHERE persona_slug=?1
             ORDER BY occurred_at DESC LIMIT ?2"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, limit))?;
        collect_persona_audit(&mut stmt)
    }

    /// Backward-compatible positional entry point. New call sites should
    /// prefer `query_persona_audit_with(QueryPersonaAuditArgs { … })`.
    // poly-lint: query builder uses bounded Vec-index arithmetic for SQL positional params.
    #[allow(clippy::too_many_arguments, clippy::arithmetic_side_effects)]
    pub fn query_persona_audit(
        &self,
        slug:           Option<&str>,
        action:         Option<&str>,
        actor:          Option<&str>,
        since:          Option<&str>,
        until:          Option<&str>,
        target_account: Option<&str>,
        target_chat:    Option<&str>,
        result:         Option<&str>,
        limit:          i64,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        self.query_persona_audit_with(QueryPersonaAuditArgs {
            slug, action, actor, since, until, target_account, target_chat, result, limit,
        })
    }

    // poly-lint: query builder uses bounded Vec-index arithmetic for SQL positional params.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn query_persona_audit_with(&self, args: QueryPersonaAuditArgs<'_>) -> Result<Vec<serde_json::Value>, MemoryError> {
        let mut clauses: Vec<&str> = Vec::new();
        let mut vals:    Vec<sqlite::Value> = Vec::new();

        macro_rules! push_filter {
            ($opt:expr, $col:expr) => {
                if let Some(v) = $opt {
                    let _idx = clauses.len() + 1;
                    clauses.push(concat!($col, " = ?"));
                    vals.push(sqlite::Value::String(v.to_string()));
                }
            };
            (ge, $opt:expr, $col:expr) => {
                if let Some(v) = $opt {
                    clauses.push(concat!($col, " >= ?"));
                    vals.push(sqlite::Value::String(v.to_string()));
                }
            };
            (le, $opt:expr, $col:expr) => {
                if let Some(v) = $opt {
                    clauses.push(concat!($col, " <= ?"));
                    vals.push(sqlite::Value::String(v.to_string()));
                }
            };
        }

        push_filter!(args.slug,           "persona_slug");
        push_filter!(args.action,         "action");
        push_filter!(args.actor,          "actor");
        push_filter!(ge, args.since,      "occurred_at");
        push_filter!(le, args.until,      "occurred_at");
        push_filter!(args.target_account, "target_account");
        push_filter!(args.target_chat,    "target_chat");
        push_filter!(args.result,         "result");

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = clauses
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let tail = c.trim_end_matches('?');
                    format!("{tail}?{}", i + 1)
                })
                .collect();
            format!("WHERE {}", parts.join(" AND "))
        };

        let limit_pos = vals.len() + 1;
        let cols = "id,persona_slug,occurred_at,actor,action,\
                    target_account,target_chat,payload_json,result,error_msg";
        // poly-lint: allow cross-persona-memory — query_persona_audit intentionally
        // supports cross-persona reads when slug filter is absent (ops/audit export).
        let sql = format!("SELECT {cols} FROM persona_audit {where_sql} ORDER BY occurred_at DESC LIMIT ?{limit_pos}"); // poly-lint: allow cross-persona-memory — see comment above

        let db = self.lock()?;
        let mut stmt = db.prepare(&sql)?;
        for (i, v) in vals.iter().enumerate() {
            stmt.bind((i + 1, v.clone()))?;
        }
        stmt.bind((limit_pos, args.limit))?;
        collect_persona_audit(&mut stmt)
    }

    pub fn export_persona_audit(
        &self,
        slug: &str,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,persona_slug,occurred_at,actor,action,\
             target_account,target_chat,payload_json,result,error_msg \
             FROM persona_audit WHERE persona_slug=?1 \
             ORDER BY occurred_at ASC"
        )?;
        stmt.bind((1, slug))?;
        collect_persona_audit(&mut stmt)
    }

    pub fn list_personas_for_heartbeat(&self) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT slug,name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    created_at,updated_at,last_run_at,enabled,
                    COALESCE(quiet_hours_disabled,0)
             FROM personas
             WHERE enabled=1 AND heartbeat_interval_secs IS NOT NULL
             ORDER BY name"
        )?;
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            out.push(read_persona_row(&mut stmt)?);
        }
        Ok(out)
    }

    pub fn count_persona_audit_since(
        &self,
        persona_slug: &str,
        cutoff_iso8601: &str,
    ) -> Result<i64, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT COUNT(*) FROM persona_audit
             WHERE persona_slug=?1
               AND occurred_at > ?2
               AND action IN ('draft_create','notify','outbound_send')"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, cutoff_iso8601))?;
        if stmt.next()? == sqlite::State::Row {
            Ok(stmt.read::<i64, _>(0)?)
        } else {
            Ok(0)
        }
    }

    pub fn count_outbound_sends_today(
        &self,
        persona_slug: &str,
        account_id: &str,
        chat_id: &str,
    ) -> Result<i64, MemoryError> {
        let today = {
            let ts = now_iso8601();
            format!("{}T00:00:00Z", ts.get(..10).unwrap_or(""))
        };
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT COUNT(*) FROM persona_audit
             WHERE persona_slug=?1
               AND target_account=?2
               AND target_chat=?3
               AND action='outbound_send'
               AND occurred_at >= ?4"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, chat_id))?;
        stmt.bind((4, today.as_str()))?;
        if stmt.next()? == sqlite::State::Row {
            Ok(stmt.read::<i64, _>(0)?)
        } else {
            Ok(0)
        }
    }

    pub fn check_persona_outbound_cap(
        &self,
        persona_slug: &str,
        account_id: &str,
        chat_id: &str,
    ) -> Result<Option<(i64, i64)>, MemoryError> {
        let max_per_day = {
            let db = self.lock()?;
            let mut stmt = db.prepare(
                "SELECT max_messages_per_day FROM persona_outbound_allowlist
                 WHERE persona_slug=?1 AND account_id=?2 AND chat_id=?3"
            )?;
            stmt.bind((1, persona_slug))?;
            stmt.bind((2, account_id))?;
            stmt.bind((3, chat_id))?;
            if stmt.next()? == sqlite::State::Row {
                stmt.read::<i64, _>(0)?
            } else {
                return Ok(None);
            }
        };
        let sends_today = self.count_outbound_sends_today(persona_slug, account_id, chat_id)?;
        Ok(Some((max_per_day, sends_today)))
    }

    pub fn update_persona_last_run_at(&self, slug: &str) -> Result<(), MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "UPDATE personas SET last_run_at=?1 WHERE slug=?2"
        )?;
        stmt.bind((1, now.as_str()))?;
        stmt.bind((2, slug))?;
        drain(&mut stmt)
    }

    pub fn prune_persona_audit_before(&self, cutoff_iso8601: &str) -> Result<u64, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM persona_audit WHERE occurred_at < ?1" // poly-lint: allow cross-persona-memory — time-based housekeeping prune; intentionally crosses personas.
        )?;
        stmt.bind((1, cutoff_iso8601))?;
        drain(&mut stmt)?;

        let mut chk = db.prepare("SELECT changes()")?;
        if chk.next()? == sqlite::State::Row {
            Ok(u64::try_from(chk.read::<i64, _>(0)?).unwrap_or(0))
        } else {
            Ok(0)
        }
    }
}
