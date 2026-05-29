//! SQLite backend — lightweight alternative for tests and development.
//!
//! Uses `tokio::sync::Mutex<sqlite::Connection>` for async-safe access.
//! All IDs use the `table:key` format for compatibility with the SurrealDB backend.

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use sqlite::State;
use tokio::sync::Mutex;
use tracing::info;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::{UserRecord, AuthChallenge, Device, FriendRequest, Server, Channel, Membership, Category, Message, Attachment};

/// Bundled parameters for [`Db::append_modlog`] — keeps the call sites
/// inside the `clippy::too_many_arguments` cap of 6.
#[derive(Debug, Clone, Copy)]
pub struct ModlogInsert<'a> {
    pub server_id: &'a str,
    pub actor_id: &'a str,
    pub target_id: Option<&'a str>,
    pub action: &'a str,
    pub reason: Option<&'a str>,
    pub channel_id: Option<&'a str>,
}

/// SQLite database handle.
#[derive(Clone)]
pub struct Db {
    inner: std::sync::Arc<Mutex<sqlite::Connection>>,
}

impl Db {
    // Caller awaits this via the db::init shim — keep async for the public API contract.
    #[allow(clippy::unused_async)]
    // Sequential migration steps make this inherently complex; splitting would obscure intent.
    #[allow(clippy::cognitive_complexity)]
    pub async fn init(config: &Config) -> anyhow::Result<Self> {
        let path = &config.db_path;
        let is_memory = path == ":memory:" || path.is_empty();
        if is_memory {
            info!("Opening SQLite in-memory database");
        } else {
            info!("Opening SQLite at {path}");
        }

        let conn = if is_memory {
            sqlite::open(":memory:")?
        } else {
            sqlite::open(path)?
        };

        if !is_memory {
            conn.execute("PRAGMA journal_mode=WAL")?;
        }
        conn.execute("PRAGMA foreign_keys=ON")?;
        conn.execute(SCHEMA)?;
        // Migration: add banner_url column if it doesn't exist yet.
        // SQLite doesn't support "ADD COLUMN IF NOT EXISTS", so we ignore the
        // error if the column already exists (duplicate column error).
        drop(conn.execute("ALTER TABLE server ADD COLUMN banner_url TEXT"));
        // Migration B-PS: add role column to membership if not present.
        drop(conn.execute("ALTER TABLE membership ADD COLUMN role TEXT NOT NULL DEFAULT 'member'"));
        // Migration B-PS: add timeout_until column to membership if not present.
        drop(conn.execute("ALTER TABLE membership ADD COLUMN timeout_until TEXT"));
        // Migration B-PS: add moderation columns to channel if not present.
        drop(conn.execute("ALTER TABLE channel ADD COLUMN topic TEXT"));
        drop(conn.execute("ALTER TABLE channel ADD COLUMN slow_mode_secs INTEGER NOT NULL DEFAULT 0"));
        drop(conn.execute("ALTER TABLE channel ADD COLUMN nsfw INTEGER NOT NULL DEFAULT 0"));
        // Migration B-PS: create bans and modlog tables.
        conn.execute(MODERATION_SCHEMA)?;
        // Migration: social ops — blocks, ignores, relationship meta, mutes, user invites.
        conn.execute(SOCIAL_SCHEMA)?;

        info!("SQLite schema applied");
        Ok(Self {
            inner: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    // ── Auth operations ──────────────────────────────────────────────────────

    pub async fn get_users_by_pubkey(&self, pubkey: &str) -> Result<Vec<UserRecord>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM user WHERE public_key = ?1", &[pubkey])
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM user WHERE username = ?1 LIMIT 1", &[username])
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM user WHERE email = ?1 LIMIT 1", &[email])
    }

    pub async fn create_user(
        &self,
        username: &str,
        email: &str,
        display_name: &str,
        public_key: &str,
    ) -> Result<Option<UserRecord>> {
        let conn = self.inner.lock().await;
        let id = new_id("user");
        let now = now_iso();
        exec_bind(&conn, "INSERT INTO user (id, username, email, display_name, public_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[&id, username, email, display_name, public_key, &now])?;
        query_one(&conn, "SELECT * FROM user WHERE id = ?1", &[&id])
    }

    pub async fn create_auth_challenge(&self, pubkey: &str, nonce: &str) -> Result<Option<AuthChallenge>> {
        let conn = self.inner.lock().await;
        let id = new_id("auth_challenge");
        let now = now_iso();
        let expires = expires_iso(60);
        exec_bind(&conn, "INSERT INTO auth_challenge (id, public_key, nonce, expires_at, used, created_at) VALUES (?1, ?2, ?3, ?4, 0, ?5)",
            &[&id, pubkey, nonce, &expires, &now])?;
        query_one(&conn, "SELECT * FROM auth_challenge WHERE id = ?1", &[&id])
    }

    pub async fn get_auth_challenge(&self, pubkey: &str, nonce: &str) -> Result<Option<AuthChallenge>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM auth_challenge WHERE public_key = ?1 AND nonce = ?2 AND used = 0 LIMIT 1", &[pubkey, nonce])
    }

    pub async fn mark_challenge_used(&self, id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "UPDATE auth_challenge SET used = 1 WHERE id = ?1", &[id])
    }

    pub async fn create_device(
        &self,
        owner_id: &str,
        name: &str,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<Option<Device>> {
        let conn = self.inner.lock().await;
        let id = new_id("device");
        let now = now_iso();
        let ua = user_agent.unwrap_or("");
        let ip_val = ip.unwrap_or("");
        exec_bind(&conn, "INSERT INTO device (id, owner, name, user_agent, ip, created_at, last_seen, revoked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            &[&id, owner_id, name, ua, ip_val, &now, &now])?;
        query_one(&conn, "SELECT * FROM device WHERE id = ?1", &[&id])
    }

    pub async fn list_devices(&self, owner_id: &str) -> Result<Vec<Device>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM device WHERE owner = ?1 ORDER BY last_seen DESC", &[owner_id])
    }

    pub async fn get_device(&self, id: &str) -> Result<Option<Device>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM device WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn revoke_device(&self, id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "UPDATE device SET revoked = 1 WHERE id = ?1", &[id])
    }

    pub async fn is_device_revoked(&self, device_id: &str) -> Result<bool> {
        let val: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn, "SELECT revoked FROM device WHERE id = ?1", &[device_id])?
        };
        Ok(val
            .as_ref()
            .and_then(|v| v.get("revoked"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    }

    pub async fn update_device_heartbeat(&self, device_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        drop(exec_bind(&conn, "UPDATE device SET last_seen = ?1 WHERE id = ?2", &[&now, device_id]));
        Ok(())
    }

    // ── User operations ──────────────────────────────────────────────────────

    pub async fn get_user(&self, id: &str) -> Result<Option<UserRecord>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM user WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn update_user(
        &self,
        id: &str,
        display_name: Option<String>,
        avatar_url: Option<String>,
    ) -> Result<Option<UserRecord>> {
        let conn = self.inner.lock().await;
        if let Some(dn) = &display_name {
            exec_bind(&conn, "UPDATE user SET display_name = ?1 WHERE id = ?2", &[dn.as_str(), id])?;
        }
        if let Some(av) = &avatar_url {
            exec_bind(&conn, "UPDATE user SET avatar_url = ?1 WHERE id = ?2", &[av.as_str(), id])?;
        }
        query_one(&conn, "SELECT * FROM user WHERE id = ?1", &[id])
    }

    // conn must span stmt preparation and iteration — cannot drop early.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn list_friends_raw(&self, user_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        let sql = "SELECT fr.id, \
                   uf.id AS from_id, uf.username AS from_username, uf.display_name AS from_display_name, uf.avatar_url AS from_avatar_url, \
                   ut.id AS to_id, ut.username AS to_username, ut.display_name AS to_display_name, ut.avatar_url AS to_avatar_url \
                   FROM friend_request fr \
                   JOIN user uf ON fr.\"from\" = uf.id \
                   JOIN user ut ON fr.\"to\" = ut.id \
                   WHERE fr.status = 'accepted' AND (fr.\"from\" = ?1 OR fr.\"to\" = ?1)";
        let mut stmt = conn.prepare(sql).map_err(|ref e| db_err(e))?;
        stmt.bind((1, user_id)).map_err(|ref e| db_err(e))?;
        let mut results = vec![];
        while matches!(stmt.next(), Ok(State::Row)) {
            let from_obj = serde_json::json!({
                "id": read_str(&stmt, "from_id"),
                "username": read_str(&stmt, "from_username"),
                "display_name": read_str(&stmt, "from_display_name"),
                "avatar_url": read_str_opt(&stmt, "from_avatar_url"),
            });
            let to_obj = serde_json::json!({
                "id": read_str(&stmt, "to_id"),
                "username": read_str(&stmt, "to_username"),
                "display_name": read_str(&stmt, "to_display_name"),
                "avatar_url": read_str_opt(&stmt, "to_avatar_url"),
            });
            results.push(serde_json::json!({ "from": from_obj, "to": to_obj }));
        }
        Ok(results)
    }

    pub async fn create_friend_request(&self, from_id: &str, to_id: &str) -> Result<Option<FriendRequest>> {
        let conn = self.inner.lock().await;
        let id = new_id("friend_request");
        let now = now_iso();
        exec_bind(&conn, "INSERT INTO friend_request (id, \"from\", \"to\", status, created_at) VALUES (?1, ?2, ?3, 'pending', ?4)",
            &[&id, from_id, to_id, &now])?;
        query_one(&conn, "SELECT * FROM friend_request WHERE id = ?1", &[&id])
    }

    pub async fn get_friend_request(&self, id: &str) -> Result<Option<FriendRequest>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM friend_request WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn update_friend_request_status(&self, id: &str, status: &str) -> Result<Option<FriendRequest>> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "UPDATE friend_request SET status = ?1 WHERE id = ?2", &[status, id])?;
        query_one(&conn, "SELECT * FROM friend_request WHERE id = ?1", &[id])
    }

    pub async fn remove_friend(&self, user_id: &str, target_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "DELETE FROM friend_request WHERE status = 'accepted' AND ((\"from\" = ?1 AND \"to\" = ?2) OR (\"from\" = ?2 AND \"to\" = ?1))",
            &[user_id, target_id])
    }

    // ── Server operations ────────────────────────────────────────────────────

    pub async fn list_servers_for_user(&self, user_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT s.* FROM server s JOIN membership m ON m.server = s.id WHERE m.user = ?1", &[user_id])
    }

    pub async fn create_server_record(
        &self,
        name: &str,
        icon_url: Option<&str>,
        owner_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        let id = new_id("server");
        let now = now_iso();
        let icon = icon_url.unwrap_or("");
        exec_bind(&conn, "INSERT INTO server (id, name, icon_url, owner, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[&id, name, icon, owner_id, &now])?;
        query_many(&conn, "SELECT * FROM server WHERE id = ?1", &[&id])
    }

    pub async fn get_server(&self, id: &str) -> Result<Option<Server>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM server WHERE id = ?1", &[id])
    }

    /// `banner_url`: `None` = don't touch; `Some(None)` = clear to NULL; `Some(Some(url))` = set.
    pub async fn update_server(
        &self,
        id: &str,
        name: Option<String>,
        icon_url: Option<String>,
        banner_url: Option<Option<String>>,
    ) -> Result<Option<Server>> {
        let conn = self.inner.lock().await;
        if let Some(n) = &name {
            exec_bind(&conn, "UPDATE server SET name = ?1 WHERE id = ?2", &[n.as_str(), id])?;
        }
        if let Some(ic) = &icon_url {
            exec_bind(&conn, "UPDATE server SET icon_url = ?1 WHERE id = ?2", &[ic.as_str(), id])?;
        }
        match banner_url {
            Some(Some(ref bn)) => {
                exec_bind(&conn, "UPDATE server SET banner_url = ?1 WHERE id = ?2", &[bn.as_str(), id])?;
            }
            Some(None) => {
                exec_bind(&conn, "UPDATE server SET banner_url = NULL WHERE id = ?1", &[id])?;
            }
            None => {}
        }
        query_one(&conn, "SELECT * FROM server WHERE id = ?1", &[id])
    }

    pub async fn delete_server_cascade(&self, id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        drop(exec_bind(&conn, "DELETE FROM reaction WHERE message IN (SELECT id FROM message WHERE channel IN (SELECT id FROM channel WHERE server = ?1))", &[id]));
        drop(exec_bind(&conn, "DELETE FROM attachment WHERE message IN (SELECT id FROM message WHERE channel IN (SELECT id FROM channel WHERE server = ?1))", &[id]));
        drop(exec_bind(&conn, "DELETE FROM message WHERE channel IN (SELECT id FROM channel WHERE server = ?1)", &[id]));
        drop(exec_bind(&conn, "DELETE FROM participant WHERE channel IN (SELECT id FROM channel WHERE server = ?1)", &[id]));
        drop(exec_bind(&conn, "DELETE FROM channel WHERE server = ?1", &[id]));
        drop(exec_bind(&conn, "DELETE FROM category WHERE server = ?1", &[id]));
        drop(exec_bind(&conn, "DELETE FROM membership WHERE server = ?1", &[id]));
        drop(exec_bind(&conn, "DELETE FROM invite WHERE server = ?1", &[id]));
        exec_bind(&conn, "DELETE FROM server WHERE id = ?1", &[id])
    }

    pub async fn get_server_members(&self, server_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT u.* FROM user u JOIN membership m ON m.user = u.id WHERE m.server = ?1", &[server_id])
    }

    pub async fn get_server_channels(&self, server_id: &str) -> Result<Vec<Channel>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM channel WHERE server = ?1 ORDER BY position", &[server_id])
    }

    pub async fn get_server_categories(&self, server_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM category WHERE server = ?1 ORDER BY position", &[server_id])
    }

    pub async fn create_membership(&self, user_id: &str, server_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let id = new_id("membership");
        let now = now_iso();
        exec_bind(&conn, "INSERT OR IGNORE INTO membership (id, user, server, joined_at) VALUES (?1, ?2, ?3, ?4)",
            &[&id, user_id, server_id, &now])
    }

    pub async fn get_membership(&self, user_id: &str, server_id: &str) -> Result<Option<Membership>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM membership WHERE user = ?1 AND server = ?2 LIMIT 1", &[user_id, server_id])
    }

    pub async fn delete_membership(&self, user_id: &str, server_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "DELETE FROM membership WHERE user = ?1 AND server = ?2", &[user_id, server_id])
    }

    pub async fn create_invite(&self, code: &str, server_id: &str, user_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let id = new_id("invite");
        let now = now_iso();
        exec_bind(&conn, "INSERT INTO invite (id, code, server, created_by, created_at, uses) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            &[&id, code, server_id, user_id, &now])
    }

    pub async fn get_valid_invite(&self, code: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        // Build query with now embedded since we need a 2-param query.
        let sql = format!(
            "SELECT * FROM invite WHERE code = ?1 AND (expires_at IS NULL OR expires_at > '{now}') LIMIT 1"
        );
        query_one(&conn, &sql, &[code])
    }

    // ── Channel operations ───────────────────────────────────────────────────

    pub async fn get_channel(&self, id: &str) -> Result<Option<Channel>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM channel WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn create_channel(
        &self,
        server_id: &str,
        category_id: Option<&str>,
        name: &str,
        kind: serde_json::Value,
        position: i64,
    ) -> Result<Option<Channel>> {
        let conn = self.inner.lock().await;
        let id = new_id("channel");
        let now = now_iso();
        let kind_str = kind.as_str().unwrap_or("text").to_owned();
        let pos_str = position.to_string();
        let cat_full = category_id.map(|c| ensure_prefix(c, "category"));
        match cat_full.as_deref() {
            Some(cat) => {
                exec_bind(&conn,
                    "INSERT INTO channel (id, server, category, name, kind, position, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    &[&id, server_id, cat, name, &kind_str, &pos_str, &now])?;
            }
            None => {
                exec_bind(&conn,
                    "INSERT INTO channel (id, server, category, name, kind, position, created_at) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)",
                    &[&id, server_id, name, &kind_str, &pos_str, &now])?;
            }
        }
        query_one(&conn, "SELECT * FROM channel WHERE id = ?1", &[&id])
    }

    pub async fn update_channel(
        &self,
        id: &str,
        name: Option<String>,
        category_id: Option<String>,
        position: Option<i64>,
    ) -> Result<Option<Channel>> {
        let conn = self.inner.lock().await;
        if let Some(n) = &name {
            exec_bind(&conn, "UPDATE channel SET name = ?1 WHERE id = ?2", &[n.as_str(), id])?;
        }
        if let Some(cat) = &category_id {
            exec_bind(&conn, "UPDATE channel SET category = ?1 WHERE id = ?2", &[cat.as_str(), id])?;
        }
        if let Some(pos) = position {
            let pos_str = pos.to_string();
            exec_bind(&conn, "UPDATE channel SET position = ?1 WHERE id = ?2", &[&pos_str, id])?;
        }
        query_one(&conn, "SELECT * FROM channel WHERE id = ?1", &[id])
    }

    pub async fn delete_channel(&self, id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "DELETE FROM channel WHERE id = ?1", &[id])
    }

    pub async fn create_category(&self, server_id: &str, name: &str, position: i64) -> Result<Option<Category>> {
        let conn = self.inner.lock().await;
        let id = new_id("category");
        let pos_str = position.to_string();
        exec_bind(&conn, "INSERT INTO category (id, server, name, position) VALUES (?1, ?2, ?3, ?4)",
            &[&id, server_id, name, &pos_str])?;
        query_one(&conn, "SELECT * FROM category WHERE id = ?1", &[&id])
    }

    pub async fn get_category(&self, id: &str) -> Result<Option<Category>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM category WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn update_category(
        &self,
        id: &str,
        name: Option<String>,
        position: Option<i64>,
    ) -> Result<Option<Category>> {
        let conn = self.inner.lock().await;
        if let Some(n) = &name {
            exec_bind(&conn, "UPDATE category SET name = ?1 WHERE id = ?2", &[n.as_str(), id])?;
        }
        if let Some(pos) = position {
            let pos_str = pos.to_string();
            exec_bind(&conn, "UPDATE category SET position = ?1 WHERE id = ?2", &[&pos_str, id])?;
        }
        query_one(&conn, "SELECT * FROM category WHERE id = ?1", &[id])
    }

    pub async fn delete_category(&self, id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        drop(exec_bind(&conn, "UPDATE channel SET category = NULL WHERE category = ?1", &[id]));
        exec_bind(&conn, "DELETE FROM category WHERE id = ?1", &[id])
    }

    pub async fn list_dms(&self, user_id: &str) -> Result<Vec<Channel>> {
        let conn = self.inner.lock().await;
        query_many(&conn,
            "SELECT c.* FROM channel c JOIN participant p ON p.channel = c.id WHERE p.user = ?1 AND c.server IS NULL",
            &[user_id])
    }

    pub async fn find_dm(&self, user_id: &str, other_id: &str) -> Result<Option<Channel>> {
        let conn = self.inner.lock().await;
        query_one(&conn,
            "SELECT c.* FROM channel c \
             WHERE c.server IS NULL \
             AND c.id IN (SELECT channel FROM participant WHERE user = ?1) \
             AND c.id IN (SELECT channel FROM participant WHERE user = ?2) \
             LIMIT 1",
            &[user_id, other_id])
    }

    pub async fn create_dm_channel(&self, name: &str) -> Result<Option<Channel>> {
        let conn = self.inner.lock().await;
        let id = new_id("channel");
        let now = now_iso();
        exec_bind(&conn,
            "INSERT INTO channel (id, server, category, name, kind, position, created_at) VALUES (?1, NULL, NULL, ?2, 'text', 0, ?3)",
            &[&id, name, &now])?;
        query_one(&conn, "SELECT * FROM channel WHERE id = ?1", &[&id])
    }

    pub async fn create_participants(&self, user_ids: &[&str], channel_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        for uid in user_ids {
            let id = new_id("participant");
            exec_bind(&conn,
                "INSERT OR IGNORE INTO participant (id, user, channel, added_at) VALUES (?1, ?2, ?3, ?4)",
                &[&id, uid, channel_id, &now])?;
        }
        Ok(())
    }

    pub async fn delete_participant(&self, user_id: &str, channel_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "DELETE FROM participant WHERE user = ?1 AND channel = ?2", &[user_id, channel_id])
    }

    pub async fn list_participants(&self, channel_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM participant WHERE channel = ?1", &[channel_id])
    }

    pub async fn is_participant(&self, user_id: &str, channel_id: &str) -> Result<bool> {
        let raw: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn,
                "SELECT id FROM participant WHERE user = ?1 AND channel = ?2 LIMIT 1",
                &[user_id, channel_id])?
        };
        Ok(raw.is_some())
    }

    pub async fn get_channel_server_id(&self, channel_id: &str) -> Result<Option<String>> {
        let raw: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn, "SELECT server FROM channel WHERE id = ?1 LIMIT 1", &[channel_id])?
        };
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("server"))
            .and_then(|v| v.as_str())
            .map(str::to_owned))
    }

    pub async fn is_server_owner(&self, server_id: &str, user_id: &str) -> Result<bool> {
        let raw: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn,
                "SELECT id FROM server WHERE id = ?1 AND owner = ?2 LIMIT 1",
                &[server_id, user_id])?
        };
        Ok(raw.is_some())
    }

    // ── Message operations ───────────────────────────────────────────────────

    // Two distinct SQL queries with different arg counts — map_or_else would be unreadable.
    #[allow(clippy::option_if_let_else)]
    pub async fn list_messages(
        &self,
        channel_id: &str,
        cursor: Option<&str>,
        limit: u8,
    ) -> Result<Vec<Message>> {
        let conn = self.inner.lock().await;
        if let Some(cur) = cursor {
            let sql = format!(
                "SELECT * FROM message WHERE channel = ?1 AND id < ?2 ORDER BY id DESC LIMIT {limit}"
            );
            query_many(&conn, &sql, &[channel_id, cur])
        } else {
            let sql = format!(
                "SELECT * FROM message WHERE channel = ?1 ORDER BY id DESC LIMIT {limit}"
            );
            query_many(&conn, &sql, &[channel_id])
        }
    }

    pub async fn create_message(
        &self,
        channel_id: &str,
        author_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<Option<Message>> {
        let conn = self.inner.lock().await;
        let id = new_id("message");
        let now = now_iso();
        let reply_full = reply_to.map(|rt| ensure_prefix(rt, "message"));
        match reply_full.as_deref() {
            Some(rt) => exec_bind(&conn,
                "INSERT INTO message (id, channel, author, content, reply_to, edited_at, deleted, created_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?6)",
                &[&id, channel_id, author_id, content, rt, &now])?,
            None => exec_bind(&conn,
                "INSERT INTO message (id, channel, author, content, reply_to, edited_at, deleted, created_at) VALUES (?1, ?2, ?3, ?4, NULL, NULL, 0, ?5)",
                &[&id, channel_id, author_id, content, &now])?,
        }
        query_one(&conn, "SELECT * FROM message WHERE id = ?1", &[&id])
    }

    pub async fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM message WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn edit_message(&self, id: &str, content: &str) -> Result<Option<Message>> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        exec_bind(&conn, "UPDATE message SET content = ?1, edited_at = ?2 WHERE id = ?3", &[content, &now, id])?;
        query_one(&conn, "SELECT * FROM message WHERE id = ?1", &[id])
    }

    pub async fn soft_delete_message(&self, id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "UPDATE message SET deleted = 1, content = '[deleted]' WHERE id = ?1", &[id])
    }

    pub async fn link_attachment_to_message(&self, attachment_id: &str, message_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "UPDATE attachment SET message = ?1 WHERE id = ?2", &[message_id, attachment_id])
    }

    pub async fn list_attachments_for_message(&self, message_id: &str) -> Result<Vec<Attachment>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM attachment WHERE message = ?1", &[message_id])
    }

    pub async fn add_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let id = new_id("reaction");
        exec_bind(&conn,
            "INSERT OR IGNORE INTO reaction (id, message, user, emoji) VALUES (?1, ?2, ?3, ?4)",
            &[&id, message_id, user_id, emoji])
    }

    pub async fn remove_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn, "DELETE FROM reaction WHERE message = ?1 AND user = ?2 AND emoji = ?3",
            &[message_id, user_id, emoji])
    }

    pub async fn list_reactions(&self, message_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        query_many(&conn, "SELECT * FROM reaction WHERE message = ?1", &[message_id])
    }

    // ── Upload operations ────────────────────────────────────────────────────

    pub async fn create_attachment(
        &self,
        uploaded_by: &str,
        filename: &str,
        storage_name: &str,
        mime_type: &str,
        size_bytes: u64,
    ) -> Result<Option<Attachment>> {
        let conn = self.inner.lock().await;
        let id = new_id("attachment");
        let now = now_iso();
        let sz_str = size_bytes.to_string();
        exec_bind(&conn,
            "INSERT INTO attachment (id, uploaded_by, message, filename, storage_name, mime_type, size_bytes, created_at) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)",
            &[&id, uploaded_by, filename, storage_name, mime_type, &sz_str, &now])?;
        query_one(&conn, "SELECT * FROM attachment WHERE id = ?1", &[&id])
    }

    pub async fn get_attachment(&self, id: &str) -> Result<Option<Attachment>> {
        let conn = self.inner.lock().await;
        query_one(&conn, "SELECT * FROM attachment WHERE id = ?1 LIMIT 1", &[id])
    }

    pub async fn get_message_channel_id(&self, message_id: &str) -> Result<Option<String>> {
        let raw: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn, "SELECT channel FROM message WHERE id = ?1 LIMIT 1", &[message_id])?
        };
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("channel"))
            .and_then(|v| v.as_str())
            .map(str::to_owned))
    }

    // ── Moderation operations ────────────────────────────────────────────────

    /// Get the role of a member in a server (None if not a member).
    pub async fn get_member_role(&self, server_id: &str, user_id: &str) -> Result<Option<String>> {
        let raw: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn,
                "SELECT role FROM membership WHERE user = ?1 AND server = ?2 LIMIT 1",
                &[user_id, server_id])?
        };
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("role"))
            .and_then(|v| v.as_str())
            .map(str::to_owned))
    }

    /// Set the role of a member in a server.
    pub async fn set_member_role(&self, server_id: &str, user_id: &str, role: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "UPDATE membership SET role = ?1 WHERE user = ?2 AND server = ?3",
            &[role, user_id, server_id])
    }

    /// Ban a member: insert into server_bans and remove from membership.
    pub async fn ban_member(
        &self,
        server_id: &str,
        user_id: &str,
        banned_by: &str,
        reason: Option<&str>,
        expires_at: Option<&str>,
    ) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        let reason_val = reason.unwrap_or("");
        // Upsert — replace existing ban if present.
        match expires_at {
            Some(exp) => exec_bind(&conn,
                "INSERT OR REPLACE INTO server_bans (server_id, user_id, banned_by, reason, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                &[server_id, user_id, banned_by, reason_val, exp, &now])?,
            None => exec_bind(&conn,
                "INSERT OR REPLACE INTO server_bans (server_id, user_id, banned_by, reason, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
                &[server_id, user_id, banned_by, reason_val, &now])?,
        }
        // Remove from server membership.
        drop(exec_bind(
            &conn,
            "DELETE FROM membership WHERE user = ?1 AND server = ?2",
            &[user_id, server_id],
        ));
        Ok(())
    }

    /// Unban a member.
    pub async fn unban_member(&self, server_id: &str, user_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "DELETE FROM server_bans WHERE server_id = ?1 AND user_id = ?2",
            &[server_id, user_id])
    }

    /// Check if a user is currently banned from a server (respects expires_at).
    pub async fn is_banned(&self, server_id: &str, user_id: &str) -> Result<bool> {
        let now = now_iso();
        let sql = format!(
            "SELECT server_id FROM server_bans WHERE server_id = ?1 AND user_id = ?2 AND (expires_at IS NULL OR expires_at > '{now}') LIMIT 1"
        );
        let raw: Option<serde_json::Value> = {
            let conn = self.inner.lock().await;
            query_one(&conn, &sql, &[server_id, user_id])?
        };
        Ok(raw.is_some())
    }

    /// List all active bans for a server (excluding expired).
    pub async fn list_bans(&self, server_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        let sql = format!(
            "SELECT * FROM server_bans WHERE server_id = ?1 AND (expires_at IS NULL OR expires_at > '{now}') ORDER BY created_at DESC"
        );
        query_many(&conn, &sql, &[server_id])
    }

    /// Append a modlog entry. See [`ModlogInsert`] for the full param shape.
    pub async fn append_modlog(&self, entry: ModlogInsert<'_>) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        let target = entry.target_id.unwrap_or("");
        let reason_val = entry.reason.unwrap_or("");
        let channel = entry.channel_id.unwrap_or("");
        exec_bind(&conn,
            "INSERT INTO server_modlog (server_id, actor_id, target_id, action, reason, channel_id, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[entry.server_id, entry.actor_id, target, entry.action, reason_val, channel, &now])
    }

    /// List modlog entries for a server, newest first.
    pub async fn list_modlog(&self, server_id: &str, limit: usize) -> Result<Vec<serde_json::Value>> {
        let conn = self.inner.lock().await;
        let lim = limit.to_string();
        let sql = format!(
            "SELECT * FROM server_modlog WHERE server_id = ?1 ORDER BY timestamp DESC LIMIT {lim}"
        );
        query_many(&conn, &sql, &[server_id])
    }

    /// Set a timeout on a member (stores expires_at in membership row).
    #[allow(clippy::option_if_let_else)]
    pub async fn set_member_timeout(&self, server_id: &str, user_id: &str, until: Option<&str>) -> Result<()> {
        let conn = self.inner.lock().await;
        match until {
            Some(exp) => exec_bind(&conn,
                "UPDATE membership SET timeout_until = ?1 WHERE user = ?2 AND server = ?3",
                &[exp, user_id, server_id]),
            None => exec_bind(&conn,
                "UPDATE membership SET timeout_until = NULL WHERE user = ?1 AND server = ?2",
                &[user_id, server_id]),
        }
    }

    /// Update channel moderation fields (topic, slow_mode_secs, nsfw).
    pub async fn update_channel_moderation(
        &self,
        channel_id: &str,
        topic: Option<&str>,
        slow_mode_secs: Option<u32>,
        nsfw: Option<bool>,
    ) -> Result<()> {
        let conn = self.inner.lock().await;
        if let Some(t) = topic {
            drop(exec_bind(
                &conn,
                "UPDATE channel SET topic = ?1 WHERE id = ?2",
                &[t, channel_id],
            ));
        }
        if let Some(sms) = slow_mode_secs {
            let s = sms.to_string();
            drop(exec_bind(
                &conn,
                "UPDATE channel SET slow_mode_secs = ?1 WHERE id = ?2",
                &[&s, channel_id],
            ));
        }
        if let Some(n) = nsfw {
            let n_str = if n { "1" } else { "0" };
            drop(exec_bind(
                &conn,
                "UPDATE channel SET nsfw = ?1 WHERE id = ?2",
                &[n_str, channel_id],
            ));
        }
        Ok(())
    }

    // ── Relationship operations ──────────────────────────────────────────────

    /// Block a user (upsert into user_blocks).
    // blocker_id / blocked_id are domain-meaningful paired names; rename would lose clarity.
    #[allow(clippy::similar_names)]
    pub async fn block_user(&self, blocker_id: &str, blocked_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        exec_bind(&conn,
            "INSERT OR REPLACE INTO user_blocks (blocker_id, blocked_id, created_at) VALUES (?1, ?2, ?3)",
            &[blocker_id, blocked_id, &now])
    }

    /// Unblock a user.
    #[allow(clippy::similar_names)]
    pub async fn unblock_user(&self, blocker_id: &str, blocked_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "DELETE FROM user_blocks WHERE blocker_id = ?1 AND blocked_id = ?2",
            &[blocker_id, blocked_id])
    }

    /// Ignore a user (upsert into user_ignores).
    #[allow(clippy::similar_names)]
    pub async fn ignore_user(&self, ignorer_id: &str, ignored_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        exec_bind(&conn,
            "INSERT OR REPLACE INTO user_ignores (ignorer_id, ignored_id, created_at) VALUES (?1, ?2, ?3)",
            &[ignorer_id, ignored_id, &now])
    }

    /// Unignore a user.
    #[allow(clippy::similar_names)]
    pub async fn unignore_user(&self, ignorer_id: &str, ignored_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "DELETE FROM user_ignores WHERE ignorer_id = ?1 AND ignored_id = ?2",
            &[ignorer_id, ignored_id])
    }

    /// Send a friend request by user ID (not username).
    pub async fn create_friend_request_by_id(&self, from_id: &str, to_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let id = new_id("friend_request");
        let now = now_iso();
        exec_bind(&conn,
            "INSERT OR IGNORE INTO friend_request (id, \"from\", \"to\", status, created_at) VALUES (?1, ?2, ?3, 'pending', ?4)",
            &[&id, from_id, to_id, &now])
    }

    /// Set or clear a nickname for a relationship (None clears).
    // Two distinct SQL statements — map_or_else form would be less readable than match.
    #[allow(clippy::option_if_let_else)]
    pub async fn set_relationship_nickname(&self, user_id: &str, target_id: &str, nickname: Option<&str>) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        match nickname {
            Some(n) => exec_bind(&conn,
                "INSERT INTO user_relationship_meta (user_id, target_id, nickname, updated_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(user_id, target_id) DO UPDATE SET nickname = excluded.nickname, updated_at = excluded.updated_at",
                &[user_id, target_id, n, &now]),
            None => exec_bind(&conn,
                "INSERT INTO user_relationship_meta (user_id, target_id, nickname, updated_at) VALUES (?1, ?2, NULL, ?3) \
                 ON CONFLICT(user_id, target_id) DO UPDATE SET nickname = NULL, updated_at = excluded.updated_at",
                &[user_id, target_id, &now]),
        }
    }

    /// Set or clear a private note about a user (None clears).
    #[allow(clippy::option_if_let_else)]
    pub async fn set_user_note(&self, user_id: &str, target_id: &str, note: Option<&str>) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        match note {
            Some(n) => exec_bind(&conn,
                "INSERT INTO user_relationship_meta (user_id, target_id, note, updated_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(user_id, target_id) DO UPDATE SET note = excluded.note, updated_at = excluded.updated_at",
                &[user_id, target_id, n, &now]),
            None => exec_bind(&conn,
                "INSERT INTO user_relationship_meta (user_id, target_id, note, updated_at) VALUES (?1, ?2, NULL, ?3) \
                 ON CONFLICT(user_id, target_id) DO UPDATE SET note = NULL, updated_at = excluded.updated_at",
                &[user_id, target_id, &now]),
        }
    }

    // ── Conversation lifecycle operations ────────────────────────────────────

    /// Mark a DM/group channel as hidden for the user.
    pub async fn close_dm_channel(&self, user_id: &str, channel_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "DELETE FROM participant WHERE user = ?1 AND channel = ?2",
            &[user_id, channel_id])
    }

    /// Mute a conversation until the given timestamp (or indefinitely if None).
    #[allow(clippy::option_if_let_else)]
    pub async fn mute_conversation(&self, user_id: &str, channel_id: &str, until: Option<&str>) -> Result<()> {
        let conn = self.inner.lock().await;
        let now = now_iso();
        match until {
            Some(u) => exec_bind(&conn,
                "INSERT INTO conversation_mutes (user_id, channel_id, muted_until, created_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(user_id, channel_id) DO UPDATE SET muted_until = excluded.muted_until, created_at = excluded.created_at",
                &[user_id, channel_id, u, &now]),
            None => exec_bind(&conn,
                "INSERT INTO conversation_mutes (user_id, channel_id, muted_until, created_at) VALUES (?1, ?2, NULL, ?3) \
                 ON CONFLICT(user_id, channel_id) DO UPDATE SET muted_until = NULL, created_at = excluded.created_at",
                &[user_id, channel_id, &now]),
        }
    }

    /// Unmute a conversation.
    pub async fn unmute_conversation(&self, user_id: &str, channel_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        exec_bind(&conn,
            "DELETE FROM conversation_mutes WHERE user_id = ?1 AND channel_id = ?2",
            &[user_id, channel_id])
    }

    /// Update group DM name and/or avatar_url. Skips fields that are None.
    // conn must span both optional exec_bind calls — cannot be dropped early.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn update_group_dm(&self, channel_id: &str, name: Option<&str>, avatar_url: Option<&str>) -> Result<()> {
        let conn = self.inner.lock().await;
        if let Some(n) = name {
            exec_bind(&conn, "UPDATE channel SET name = ?1 WHERE id = ?2", &[n, channel_id])?;
        }
        if let Some(av) = avatar_url {
            // avatar_url is stored in an optional column; add it if not present.
            drop(conn.execute("ALTER TABLE channel ADD COLUMN avatar_url TEXT"));
            exec_bind(&conn, "UPDATE channel SET avatar_url = ?1 WHERE id = ?2", &[av, channel_id])?;
        }
        Ok(())
    }

    // ── Server invite (user-targeted) ────────────────────────────────────────

    /// Record a server invite sent to a specific user via DM.
    // inviter_id / invitee_id are domain-meaningful paired names; rename would lose clarity.
    #[allow(clippy::similar_names)]
    pub async fn create_user_invite(&self, server_id: &str, inviter_id: &str, invitee_id: &str) -> Result<()> {
        let conn = self.inner.lock().await;
        let id = new_id("user_invite");
        let now = now_iso();
        exec_bind(&conn,
            "INSERT OR IGNORE INTO user_invites (id, server_id, inviter_id, invitee_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[&id, server_id, inviter_id, invitee_id, &now])
    }

    // ── Broadcast helpers ────────────────────────────────────────────────────

    pub async fn get_channel_member_ids(&self, channel_id: &str) -> Vec<String> {
        let conn = self.inner.lock().await;

        let server_members: Vec<String> = (|| -> std::result::Result<Vec<String>, sqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT m.user FROM membership m \
                 JOIN channel c ON c.server = m.server \
                 WHERE c.id = ?1 AND c.server IS NOT NULL"
            )?;
            stmt.bind((1, channel_id))?;
            let mut ids = vec![];
            while matches!(stmt.next(), Ok(State::Row)) {
                if let Ok(id) = stmt.read::<String, _>("user") {
                    ids.push(id);
                }
            }
            Ok(ids)
        })()
        .unwrap_or_default();

        let participants: Vec<String> = (|| -> std::result::Result<Vec<String>, sqlite::Error> {
            let mut stmt = conn.prepare("SELECT user FROM participant WHERE channel = ?1")?;
            stmt.bind((1, channel_id))?;
            let mut ids = vec![];
            while matches!(stmt.next(), Ok(State::Row)) {
                if let Ok(id) = stmt.read::<String, _>("user") {
                    ids.push(id);
                }
            }
            Ok(ids)
        })()
        .unwrap_or_default();

        let mut set = std::collections::HashSet::new();
        for id in server_members.into_iter().chain(participants) {
            set.insert(id);
        }
        set.into_iter().collect()
    }
}

// ── SQLite helpers ──────────────────────────────────────────────────────────────

fn db_err(e: &sqlite::Error) -> AppError {
    AppError::Db(e.to_string())
}

fn new_id(table: &str) -> String {
    let key = uuid::Uuid::new_v4().to_string().replace('-', "");
    format!("{table}:{key}")
}

/// Clients commonly strip the `{table}:` prefix before sending an ID across
/// the wire (see `clients/server-client/src/http.rs`). Storage keys here are
/// always fully-qualified (`message:abc123`), so any reference we write
/// (reply_to, category, …) must be re-prefixed before FK-checked inserts.
fn ensure_prefix(id: &str, table: &str) -> String {
    if id.starts_with(&format!("{table}:")) {
        id.to_owned()
    } else {
        format!("{table}:{id}")
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn expires_iso(secs: i64) -> String {
    let now = Utc::now();
    now.checked_add_signed(chrono::Duration::seconds(secs))
        .unwrap_or(now)
        .to_rfc3339()
}

/// Execute a parameterized statement (no return value).
fn exec_bind(conn: &sqlite::Connection, sql: &str, binds: &[&str]) -> Result<()> {
    let mut stmt = conn.prepare(sql).map_err(|ref e| db_err(e))?;
    for (i, val) in binds.iter().enumerate() {
        stmt.bind((i.wrapping_add(1), *val)).map_err(|ref e| db_err(e))?;
    }
    // Drive the statement to completion.
    while matches!(stmt.next(), Ok(State::Row)) {}
    // Final step returns State::Done (not an error).
    Ok(())
}

/// Read a string column by name, returning empty string on failure.
fn read_str(stmt: &sqlite::Statement, col: &str) -> String {
    stmt.read::<String, _>(col).unwrap_or_default()
}

/// Read an optional string column (NULL → None).
fn read_str_opt(stmt: &sqlite::Statement, col: &str) -> Option<String> {
    stmt.read::<Option<String>, _>(col).ok().flatten()
}

/// Execute a query returning one row deserialized as T.
fn query_one<T: DeserializeOwned>(
    conn: &sqlite::Connection,
    sql: &str,
    binds: &[&str],
) -> Result<Option<T>> {
    let mut stmt = conn.prepare(sql).map_err(|ref e| db_err(e))?;
    for (i, val) in binds.iter().enumerate() {
        stmt.bind((i.wrapping_add(1), *val)).map_err(|ref e| db_err(e))?;
    }
    if matches!(stmt.next(), Ok(State::Row)) {
        let json = row_to_json(&stmt);
        serde_json::from_value(json)
            .map(Some)
            .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
    } else {
        Ok(None)
    }
}

/// Execute a query returning many rows deserialized as T.
fn query_many<T: DeserializeOwned>(
    conn: &sqlite::Connection,
    sql: &str,
    binds: &[&str],
) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql).map_err(|ref e| db_err(e))?;
    for (i, val) in binds.iter().enumerate() {
        stmt.bind((i.wrapping_add(1), *val)).map_err(|ref e| db_err(e))?;
    }
    let mut results = vec![];
    while matches!(stmt.next(), Ok(State::Row)) {
        let json = row_to_json(&stmt);
        let item: T = serde_json::from_value(json)
            .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))?;
        results.push(item);
    }
    Ok(results)
}

/// Convert the current row of a Statement to a serde_json::Value (Object).
fn row_to_json(stmt: &sqlite::Statement) -> serde_json::Value {
    let names: Vec<String> = stmt.column_names().iter().map(String::from).collect();
    let mut map = serde_json::Map::new();
    for (i, name) in names.iter().enumerate() {
        let val = match stmt.column_type(i) {
            Ok(sqlite::Type::Integer) => {
                let v = stmt.read::<i64, _>(i).unwrap_or(0);
                if is_bool_column(name) {
                    serde_json::Value::Bool(v != 0)
                } else {
                    serde_json::Value::Number(v.into())
                }
            }
            Ok(sqlite::Type::Float) => {
                let v = stmt.read::<f64, _>(i).unwrap_or(0.0_f64);
                serde_json::Number::from_f64(v)
                    .map_or(serde_json::Value::Null, serde_json::Value::Number)
            }
            Ok(sqlite::Type::String) => {
                let v = stmt.read::<String, _>(i).unwrap_or_default();
                serde_json::Value::String(v)
            }
            Ok(sqlite::Type::Binary) => {
                let v = stmt.read::<Vec<u8>, _>(i).unwrap_or_default();
                serde_json::Value::String(hex::encode(v))
            }
            Ok(sqlite::Type::Null) | Err(_) => serde_json::Value::Null,
        };
        map.insert(name.clone(), val);
    }
    serde_json::Value::Object(map)
}

/// Known boolean columns — stored as INTEGER 0/1 in SQLite.
fn is_bool_column(name: &str) -> bool {
    matches!(name, "used" | "revoked" | "deleted" | "nsfw")
}

// Suppress unused warnings.
const _: fn() = || {
    let _: DateTime<Utc>;
};

/// Schema run on every startup after the base schema.
/// Uses CREATE TABLE IF NOT EXISTS so it is idempotent.
const MODERATION_SCHEMA: &str = "
-- Server bans
CREATE TABLE IF NOT EXISTS server_bans (
    server_id   TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    banned_by   TEXT NOT NULL,
    reason      TEXT,
    expires_at  TEXT,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (server_id, user_id)
);

-- Moderation log
CREATE TABLE IF NOT EXISTS server_modlog (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id   TEXT NOT NULL,
    actor_id    TEXT NOT NULL,
    target_id   TEXT,
    action      TEXT NOT NULL,
    reason      TEXT,
    channel_id  TEXT,
    timestamp   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_modlog_server ON server_modlog(server_id, timestamp DESC);
";

/// Schema for social features — blocks, ignores, relationship metadata, mutes, user invites.
/// Created on every startup (idempotent — all statements use IF NOT EXISTS).
const SOCIAL_SCHEMA: &str = "
-- Per-user blocks
CREATE TABLE IF NOT EXISTS user_blocks (
    blocker_id  TEXT NOT NULL,
    blocked_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (blocker_id, blocked_id)
);

-- Per-user ignores (quieter than block)
CREATE TABLE IF NOT EXISTS user_ignores (
    ignorer_id  TEXT NOT NULL,
    ignored_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (ignorer_id, ignored_id)
);

-- Per-relationship metadata (nickname, private note)
CREATE TABLE IF NOT EXISTS user_relationship_meta (
    user_id     TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    nickname    TEXT,
    note        TEXT,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (user_id, target_id)
);

-- Per-user conversation mutes
CREATE TABLE IF NOT EXISTS conversation_mutes (
    user_id     TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    muted_until TEXT,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (user_id, channel_id)
);

-- Server invites targeted at a specific user (DM-style invite)
CREATE TABLE IF NOT EXISTS user_invites (
    id          TEXT PRIMARY KEY,
    server_id   TEXT NOT NULL,
    inviter_id  TEXT NOT NULL,
    invitee_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_invites_invitee ON user_invites(invitee_id);
";

const SCHEMA: &str = "
-- Users
CREATE TABLE IF NOT EXISTS user (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    public_key TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_pubkey ON user(public_key);

-- Auth challenges
CREATE TABLE IF NOT EXISTS auth_challenge (
    id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    nonce TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

-- Devices
CREATE TABLE IF NOT EXISTS device (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL REFERENCES user(id),
    name TEXT NOT NULL,
    user_agent TEXT,
    ip TEXT,
    created_at TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0
);

-- Servers
CREATE TABLE IF NOT EXISTS server (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    icon_url TEXT,
    owner TEXT NOT NULL REFERENCES user(id),
    created_at TEXT NOT NULL
);

-- Memberships
CREATE TABLE IF NOT EXISTS membership (
    id TEXT PRIMARY KEY,
    user TEXT NOT NULL REFERENCES user(id),
    server TEXT NOT NULL REFERENCES server(id),
    joined_at TEXT NOT NULL,
    UNIQUE(user, server)
);

-- Invites
CREATE TABLE IF NOT EXISTS invite (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    server TEXT NOT NULL REFERENCES server(id),
    created_by TEXT NOT NULL REFERENCES user(id),
    created_at TEXT NOT NULL,
    expires_at TEXT,
    uses INTEGER NOT NULL DEFAULT 0,
    max_uses INTEGER
);

-- Categories
CREATE TABLE IF NOT EXISTS category (
    id TEXT PRIMARY KEY,
    server TEXT NOT NULL REFERENCES server(id),
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);

-- Channels
CREATE TABLE IF NOT EXISTS channel (
    id TEXT PRIMARY KEY,
    server TEXT REFERENCES server(id),
    category TEXT REFERENCES category(id),
    name TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'text',
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

-- Participants
CREATE TABLE IF NOT EXISTS participant (
    id TEXT PRIMARY KEY,
    user TEXT NOT NULL REFERENCES user(id),
    channel TEXT NOT NULL REFERENCES channel(id),
    added_at TEXT NOT NULL,
    UNIQUE(user, channel)
);

-- Messages
CREATE TABLE IF NOT EXISTS message (
    id TEXT PRIMARY KEY,
    channel TEXT NOT NULL REFERENCES channel(id),
    author TEXT NOT NULL REFERENCES user(id),
    content TEXT NOT NULL,
    reply_to TEXT REFERENCES message(id),
    edited_at TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

-- Reactions
CREATE TABLE IF NOT EXISTS reaction (
    id TEXT PRIMARY KEY,
    message TEXT NOT NULL REFERENCES message(id),
    user TEXT NOT NULL REFERENCES user(id),
    emoji TEXT NOT NULL,
    UNIQUE(message, user, emoji)
);

-- Friend requests
CREATE TABLE IF NOT EXISTS friend_request (
    id TEXT PRIMARY KEY,
    \"from\" TEXT NOT NULL REFERENCES user(id),
    \"to\" TEXT NOT NULL REFERENCES user(id),
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL
);

-- Voice sessions
CREATE TABLE IF NOT EXISTS voice_session (
    id TEXT PRIMARY KEY,
    user TEXT NOT NULL REFERENCES user(id),
    channel TEXT NOT NULL REFERENCES channel(id),
    joined_at TEXT NOT NULL,
    UNIQUE(user, channel)
);

-- Attachments
CREATE TABLE IF NOT EXISTS attachment (
    id TEXT PRIMARY KEY,
    uploaded_by TEXT NOT NULL REFERENCES user(id),
    message TEXT REFERENCES message(id),
    filename TEXT NOT NULL,
    storage_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
";
