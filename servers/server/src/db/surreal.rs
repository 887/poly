//! SurrealDB backend.
//!
//! The connection is driven by `Config::surreal_url`:
//! - `ws://host:port` or `wss://host:port` — connects to a running SurrealDB daemon.
//! - `surrealkv://./path/to/data` — embedded SurrealKV (no external process).
//!
//! Both modes use the same `Surreal<Any>` handle so all query code is shared.

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Root;
use surrealdb::types::{Number, RecordIdKey, Value};
use surrealdb::IndexedResults;
use tracing::info;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::*;

/// SurrealDB database handle.
#[derive(Clone)]
pub struct Db {
    inner: Surreal<Any>,
}

impl Db {
    pub async fn init(config: &Config) -> anyhow::Result<Self> {
        info!("Connecting to SurrealDB at {}", config.surreal_url);
        let inner = surrealdb::engine::any::connect(&config.surreal_url).await?;
        // Only sign in for remote connections; embedded SurrealKV handles auth internally.
        if config.surreal_url.starts_with("ws://") || config.surreal_url.starts_with("wss://") {
            inner.signin(Root { username: &config.surreal_user, password: &config.surreal_pass }).await?;
        }
        inner.use_ns("poly").use_db("server").await?;
        inner.query(SCHEMA).await?.check()?;
        info!("Database schema applied");
        Ok(Self { inner })
    }

    // ── Auth operations ──────────────────────────────────────────────────────

    pub async fn get_users_by_pubkey(&self, pubkey: &str) -> Result<Vec<UserRecord>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM user WHERE public_key = $pk ORDER BY created_at ASC")
                .bind(("pk", pubkey.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM user WHERE username = $u LIMIT 1")
                .bind(("u", username.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM user WHERE email = $e LIMIT 1")
                .bind(("e", email.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_user(
        &self,
        username: &str,
        email: &str,
        display_name: &str,
        public_key: &str,
    ) -> Result<Option<UserRecord>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE user CONTENT { \
                      username: $u, email: $e, display_name: $d, \
                      public_key: $pk, created_at: time::now() \
                    } RETURN *",
                )
                .bind(("u", username.to_owned()))
                .bind(("e", email.to_owned()))
                .bind(("d", display_name.to_owned()))
                .bind(("pk", public_key.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_auth_challenge(&self, pubkey: &str, nonce: &str) -> Result<Option<AuthChallenge>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE auth_challenge CONTENT { \
                      public_key: $pk, nonce: $n, \
                      expires_at: time::now() + 60s, used: false, \
                      created_at: time::now() \
                    } RETURN *",
                )
                .bind(("pk", pubkey.to_owned()))
                .bind(("n", nonce.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_auth_challenge(&self, pubkey: &str, nonce: &str) -> Result<Option<AuthChallenge>> {
        take_one(
            &mut self.inner
                .query(
                    "SELECT * FROM auth_challenge \
                     WHERE public_key = $pk AND nonce = $n AND used = false \
                     LIMIT 1",
                )
                .bind(("pk", pubkey.to_owned()))
                .bind(("n", nonce.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn mark_challenge_used(&self, id: &str) -> Result<()> {
        self.inner
            .query("UPDATE type::record($id) SET used = true")
            .bind(("id", id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn create_device(
        &self,
        owner_id: &str,
        name: &str,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<Option<Device>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE device CONTENT { \
                      owner: type::record($uid), \
                      name: $name, \
                      user_agent: $ua, \
                      ip: $ip, \
                      created_at: time::now(), \
                      last_seen: time::now(), \
                      revoked: false \
                    } RETURN *",
                )
                .bind(("uid", owner_id.to_owned()))
                .bind(("name", name.to_owned()))
                .bind(("ua", user_agent.map(str::to_owned)))
                .bind(("ip", ip.map(str::to_owned)))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn list_devices(&self, owner_id: &str) -> Result<Vec<Device>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM device WHERE owner = type::record($id) ORDER BY last_seen DESC")
                .bind(("id", owner_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_device(&self, id: &str) -> Result<Option<Device>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn revoke_device(&self, id: &str) -> Result<()> {
        self.inner
            .query("UPDATE type::record($id) SET revoked = true")
            .bind(("id", id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn is_device_revoked(&self, device_id: &str) -> Result<bool> {
        let raw: Option<serde_json::Value> = take_one(
            &mut self.inner
                .query("SELECT revoked FROM type::record($id)")
                .bind(("id", device_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )?;
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("revoked"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub async fn update_device_heartbeat(&self, device_id: &str) -> Result<()> {
        let _ = self.inner
            .query("UPDATE type::record($id) SET last_seen = time::now()")
            .bind(("id", device_id.to_owned()))
            .await;
        Ok(())
    }

    // ── User operations ──────────────────────────────────────────────────────

    pub async fn get_user(&self, id: &str) -> Result<Option<UserRecord>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn update_user(
        &self,
        id: &str,
        display_name: Option<String>,
        avatar_url: Option<String>,
    ) -> Result<Option<UserRecord>> {
        take_one(
            &mut self.inner
                .query(
                    "UPDATE type::record($id) MERGE { \
                      display_name: $dn ?? display_name, \
                      avatar_url: $av ?? avatar_url \
                    } RETURN *",
                )
                .bind(("id", id.to_owned()))
                .bind(("dn", display_name))
                .bind(("av", avatar_url))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn list_friends_raw(&self, user_id: &str) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query(
                    "SELECT from.*, to.* FROM friend_request \
                     WHERE status = 'accepted' \
                       AND (from = type::record($uid) OR to = type::record($uid)) \
                     FETCH from, to",
                )
                .bind(("uid", user_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_friend_request(&self, from_id: &str, to_id: &str) -> Result<Option<FriendRequest>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE friend_request CONTENT { \
                      `from`: type::record($from), `to`: type::record($to), \
                      status: 'pending', created_at: time::now() \
                    } RETURN *",
                )
                .bind(("from", from_id.to_owned()))
                .bind(("to", to_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_friend_request(&self, id: &str) -> Result<Option<FriendRequest>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn update_friend_request_status(&self, id: &str, status: &str) -> Result<Option<FriendRequest>> {
        take_one(
            &mut self.inner
                .query("UPDATE type::record($id) SET status = $s RETURN *")
                .bind(("id", id.to_owned()))
                .bind(("s", status.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn remove_friend(&self, user_id: &str, target_id: &str) -> Result<()> {
        self.inner
            .query(
                "DELETE friend_request WHERE status = 'accepted' AND \
                 ((`from` = type::record($me) AND `to` = type::record($them)) OR \
                  (`from` = type::record($them) AND `to` = type::record($me)))",
            )
            .bind(("me", user_id.to_owned()))
            .bind(("them", target_id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    // ── Server operations ────────────────────────────────────────────────────

    pub async fn list_servers_for_user(&self, user_id: &str) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query("SELECT server.* FROM membership WHERE user = type::record($uid) FETCH server")
                .bind(("uid", user_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_server_record(
        &self,
        name: &str,
        icon_url: Option<&str>,
        owner_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query(
                    "CREATE server CONTENT { \
                      name: $name, icon_url: $icon, \
                      owner: type::record($owner), \
                      created_at: time::now() \
                    } RETURN *",
                )
                .bind(("name", name.to_owned()))
                .bind(("icon", icon_url.map(str::to_owned)))
                .bind(("owner", owner_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_server(&self, id: &str) -> Result<Option<Server>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id)")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn update_server(
        &self,
        id: &str,
        name: Option<String>,
        icon_url: Option<String>,
    ) -> Result<Option<Server>> {
        take_one(
            &mut self.inner
                .query(
                    "UPDATE type::record($id) MERGE { \
                      name: $name ?? name, \
                      icon_url: $icon ?? icon_url \
                    } RETURN *",
                )
                .bind(("id", id.to_owned()))
                .bind(("name", name))
                .bind(("icon", icon_url))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn delete_server_cascade(&self, id: &str) -> Result<()> {
        // 7-step cascade delete.
        let _ = self.inner
            .query("DELETE message WHERE channel.server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        let _ = self.inner
            .query("DELETE reaction WHERE message.channel.server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        let _ = self.inner
            .query("DELETE attachment WHERE message.channel.server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        let _ = self.inner
            .query("DELETE channel WHERE server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        let _ = self.inner
            .query("DELETE category WHERE server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        let _ = self.inner
            .query("DELETE membership WHERE server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        let _ = self.inner
            .query("DELETE invite WHERE server = type::record($sid)")
            .bind(("sid", id.to_owned()))
            .await;
        self.inner
            .query("DELETE type::record($id)")
            .bind(("id", id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn get_server_members(&self, server_id: &str) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query("SELECT user.* FROM membership WHERE server = type::record($id) FETCH user")
                .bind(("id", server_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_server_channels(&self, server_id: &str) -> Result<Vec<Channel>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM channel WHERE server = type::record($id) ORDER BY position")
                .bind(("id", server_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_server_categories(&self, server_id: &str) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM category WHERE server = type::record($id) ORDER BY position")
                .bind(("id", server_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_membership(&self, user_id: &str, server_id: &str) -> Result<()> {
        self.inner
            .query(
                "CREATE membership CONTENT { \
                  user: type::record($uid), \
                  server: type::record($sid), \
                  joined_at: time::now() \
                }",
            )
            .bind(("uid", user_id.to_owned()))
            .bind(("sid", server_id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn get_membership(&self, user_id: &str, server_id: &str) -> Result<Option<Membership>> {
        take_one(
            &mut self.inner
                .query(
                    "SELECT * FROM membership \
                     WHERE user = type::record($uid) AND server = type::record($sid) \
                     LIMIT 1",
                )
                .bind(("uid", user_id.to_owned()))
                .bind(("sid", server_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn delete_membership(&self, user_id: &str, server_id: &str) -> Result<()> {
        self.inner
            .query(
                "DELETE membership \
                 WHERE user = type::record($uid) AND server = type::record($sid)",
            )
            .bind(("uid", user_id.to_owned()))
            .bind(("sid", server_id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn create_invite(&self, code: &str, server_id: &str, user_id: &str) -> Result<()> {
        self.inner
            .query(
                "CREATE invite CONTENT { \
                  code: $code, server: type::record($sid), \
                  created_by: type::record($uid), \
                  created_at: time::now(), uses: 0 \
                }",
            )
            .bind(("code", code.to_owned()))
            .bind(("sid", server_id.to_owned()))
            .bind(("uid", user_id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn get_valid_invite(&self, code: &str) -> Result<Option<serde_json::Value>> {
        take_one(
            &mut self.inner
                .query(
                    "SELECT * FROM invite \
                     WHERE code = $code \
                     AND (expires_at IS NONE OR expires_at > time::now()) \
                     LIMIT 1",
                )
                .bind(("code", code.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    // ── Channel operations ───────────────────────────────────────────────────

    pub async fn get_channel(&self, id: &str) -> Result<Option<Channel>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_channel(
        &self,
        server_id: &str,
        category_id: Option<&str>,
        name: &str,
        kind: serde_json::Value,
        position: i64,
    ) -> Result<Option<Channel>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE channel CONTENT { \
                      server: type::record($sid), \
                      category: IF $cat != NONE THEN type::record($cat) ELSE NONE END, \
                      name: $name, \
                      kind: $kind, \
                      position: $pos, \
                      created_at: time::now() \
                    } RETURN *",
                )
                .bind(("sid", server_id.to_owned()))
                .bind(("cat", category_id.map(|c| format!("category:{c}"))))
                .bind(("name", name.to_owned()))
                .bind(("kind", kind))
                .bind(("pos", position))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn update_channel(
        &self,
        id: &str,
        name: Option<String>,
        category_id: Option<String>,
        position: Option<i64>,
    ) -> Result<Option<Channel>> {
        take_one(
            &mut self.inner
                .query(
                    "UPDATE type::record($id) MERGE { \
                      name: $nm ?? name, \
                      category: IF $cat != NONE THEN type::record($cat) ELSE category END, \
                      position: $pos ?? position \
                    } RETURN *",
                )
                .bind(("id", id.to_owned()))
                .bind(("nm", name))
                .bind(("cat", category_id.map(|c| format!("category:{c}"))))
                .bind(("pos", position))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn delete_channel(&self, id: &str) -> Result<()> {
        self.inner
            .query("DELETE type::record($id)")
            .bind(("id", id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn create_category(&self, server_id: &str, name: &str, position: i64) -> Result<Option<Category>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE category CONTENT { \
                      server: type::record($sid), name: $name, position: $pos \
                    } RETURN *",
                )
                .bind(("sid", server_id.to_owned()))
                .bind(("name", name.to_owned()))
                .bind(("pos", position))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_category(&self, id: &str) -> Result<Option<Category>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn update_category(
        &self,
        id: &str,
        name: Option<String>,
        position: Option<i64>,
    ) -> Result<Option<Category>> {
        take_one(
            &mut self.inner
                .query(
                    "UPDATE type::record($id) MERGE { \
                      name: $nm ?? name, position: $pos ?? position \
                    } RETURN *",
                )
                .bind(("id", id.to_owned()))
                .bind(("nm", name))
                .bind(("pos", position))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn delete_category(&self, id: &str) -> Result<()> {
        self.inner
            .query(
                "UPDATE channel SET category = NONE WHERE category = type::record($id); \
                 DELETE type::record($id)",
            )
            .bind(("id", id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_dms(&self, user_id: &str) -> Result<Vec<Channel>> {
        take_many(
            &mut self.inner
                .query(
                    "SELECT * FROM channel WHERE \
                     id IN (SELECT VALUE channel FROM participant WHERE user = type::record($uid)) \
                     AND server IS NONE",
                )
                .bind(("uid", user_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn find_dm(&self, user_id: &str, other_id: &str) -> Result<Option<Channel>> {
        take_one(
            &mut self.inner
                .query(
                    "SELECT * FROM channel WHERE server IS NONE \
                     AND id IN (SELECT VALUE channel FROM participant WHERE user = type::record($me)) \
                     AND id IN (SELECT VALUE channel FROM participant WHERE user = type::record($them)) \
                     LIMIT 1",
                )
                .bind(("me", user_id.to_owned()))
                .bind(("them", other_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_dm_channel(&self, name: &str) -> Result<Option<Channel>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE channel CONTENT { \
                      server: NONE, category: NONE, name: $name, \
                      kind: 'text', position: 0, created_at: time::now() \
                    } RETURN *",
                )
                .bind(("name", name.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn create_participants(&self, user_ids: &[&str], channel_id: &str) -> Result<()> {
        for uid in user_ids {
            self.inner
                .query(
                    "CREATE participant CONTENT { \
                      user: type::record($uid), channel: type::record($ch), added_at: time::now() \
                    }",
                )
                .bind(("uid", uid.to_string()))
                .bind(("ch", channel_id.to_owned()))
                .await
                .map_err(db_err)?
                .check()
                .map_err(db_err)?;
        }
        Ok(())
    }

    pub async fn delete_participant(&self, user_id: &str, channel_id: &str) -> Result<()> {
        self.inner
            .query(
                "DELETE participant \
                 WHERE user = type::record($uid) AND channel = type::record($ch)",
            )
            .bind(("uid", user_id.to_owned()))
            .bind(("ch", channel_id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_participants(&self, channel_id: &str) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM participant WHERE channel = type::record($ch)")
                .bind(("ch", channel_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn is_participant(&self, user_id: &str, channel_id: &str) -> Result<bool> {
        let raw: Option<serde_json::Value> = take_one(
            &mut self.inner
                .query(
                    "SELECT * FROM participant \
                     WHERE channel = type::record($ch) AND user = type::record($uid) LIMIT 1",
                )
                .bind(("ch", channel_id.to_owned()))
                .bind(("uid", user_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )?;
        Ok(raw.is_some())
    }

    pub async fn get_channel_server_id(&self, channel_id: &str) -> Result<Option<String>> {
        let raw: Option<serde_json::Value> = take_one(
            &mut self.inner
                .query("SELECT server FROM type::record($ch) LIMIT 1")
                .bind(("ch", channel_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )?;
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("server"))
            .and_then(|v| v.as_str())
            .map(str::to_owned))
    }

    pub async fn is_server_owner(&self, server_id: &str, user_id: &str) -> Result<bool> {
        let raw: Option<serde_json::Value> = take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($sid) WHERE owner = type::record($uid) LIMIT 1")
                .bind(("sid", server_id.to_owned()))
                .bind(("uid", user_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )?;
        Ok(raw.is_some())
    }

    // ── Message operations ───────────────────────────────────────────────────

    pub async fn list_messages(
        &self,
        channel_id: &str,
        cursor: Option<&str>,
        limit: u8,
    ) -> Result<Vec<Message>> {
        match cursor {
            Some(cur) => take_many(
                &mut self.inner
                    .query(
                        "SELECT * FROM message \
                         WHERE channel = type::record($ch) AND id < type::record($cursor) \
                         ORDER BY id DESC LIMIT $lim",
                    )
                    .bind(("ch", channel_id.to_owned()))
                    .bind(("cursor", cur.to_owned()))
                    .bind(("lim", limit))
                    .await
                    .map_err(db_err)?,
                0,
            ),
            None => take_many(
                &mut self.inner
                    .query(
                        "SELECT * FROM message WHERE channel = type::record($ch) \
                         ORDER BY id DESC LIMIT $lim",
                    )
                    .bind(("ch", channel_id.to_owned()))
                    .bind(("lim", limit))
                    .await
                    .map_err(db_err)?,
                0,
            ),
        }
    }

    pub async fn create_message(
        &self,
        channel_id: &str,
        author_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<Option<Message>> {
        take_one(
            &mut self.inner
                .query(
                    "CREATE message CONTENT { \
                      channel: type::record($ch), \
                      author: type::record($author), \
                      content: $content, \
                      reply_to: IF $reply_to != NONE THEN type::record($reply_to) ELSE NONE END, \
                      edited_at: NONE, \
                      deleted: false, \
                      created_at: time::now() \
                    } RETURN *",
                )
                .bind(("ch", channel_id.to_owned()))
                .bind(("author", author_id.to_owned()))
                .bind(("content", content.to_owned()))
                .bind(("reply_to", reply_to.map(|r| format!("message:{r}"))))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_message(&self, id: &str) -> Result<Option<Message>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn edit_message(&self, id: &str, content: &str) -> Result<Option<Message>> {
        take_one(
            &mut self.inner
                .query("UPDATE type::record($id) SET content = $c, edited_at = time::now() RETURN *")
                .bind(("id", id.to_owned()))
                .bind(("c", content.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn soft_delete_message(&self, id: &str) -> Result<()> {
        self.inner
            .query("UPDATE type::record($id) SET deleted = true, content = '[deleted]'")
            .bind(("id", id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn link_attachment_to_message(&self, attachment_id: &str, message_id: &str) -> Result<()> {
        self.inner
            .query("UPDATE type::record($id) SET message = type::record($mid)")
            .bind(("id", attachment_id.to_owned()))
            .bind(("mid", message_id.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_attachments_for_message(&self, message_id: &str) -> Result<Vec<Attachment>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM attachment WHERE message = type::record($mid)")
                .bind(("mid", message_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn add_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<()> {
        self.inner
            .query(
                "IF (SELECT count() FROM reaction WHERE message = type::record($mid) \
                    AND user = type::record($uid) AND emoji = $em GROUP ALL)[0].count == 0 { \
                  CREATE reaction CONTENT { \
                    message: type::record($mid), user: type::record($uid), emoji: $em \
                  } \
                }",
            )
            .bind(("mid", message_id.to_owned()))
            .bind(("uid", user_id.to_owned()))
            .bind(("em", emoji.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn remove_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<()> {
        self.inner
            .query(
                "DELETE reaction WHERE message = type::record($mid) \
                 AND user = type::record($uid) AND emoji = $em",
            )
            .bind(("mid", message_id.to_owned()))
            .bind(("uid", user_id.to_owned()))
            .bind(("em", emoji.to_owned()))
            .await
            .map_err(db_err)?
            .check()
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_reactions(&self, message_id: &str) -> Result<Vec<serde_json::Value>> {
        take_many(
            &mut self.inner
                .query("SELECT * FROM reaction WHERE message = type::record($mid)")
                .bind(("mid", message_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
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
        take_one(
            &mut self.inner
                .query(
                    "CREATE attachment CONTENT { \
                      uploaded_by: type::record($uid), \
                      message: NONE, \
                      filename: $fn, \
                      storage_name: $sn, \
                      mime_type: $mt, \
                      size_bytes: $sz, \
                      created_at: time::now() \
                    } RETURN *",
                )
                .bind(("uid", uploaded_by.to_owned()))
                .bind(("fn", filename.to_owned()))
                .bind(("sn", storage_name.to_owned()))
                .bind(("mt", mime_type.to_owned()))
                .bind(("sz", size_bytes))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_attachment(&self, id: &str) -> Result<Option<Attachment>> {
        take_one(
            &mut self.inner
                .query("SELECT * FROM type::record($id) LIMIT 1")
                .bind(("id", id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )
    }

    pub async fn get_message_channel_id(&self, message_id: &str) -> Result<Option<String>> {
        let raw: Option<serde_json::Value> = take_one(
            &mut self.inner
                .query("SELECT channel FROM type::record($id) LIMIT 1")
                .bind(("id", message_id.to_owned()))
                .await
                .map_err(db_err)?,
            0,
        )?;
        Ok(raw
            .as_ref()
            .and_then(|v| v.get("channel"))
            .and_then(|v| v.as_str())
            .map(str::to_owned))
    }

    // ── Broadcast helpers ────────────────────────────────────────────────────

    /// Get all user IDs who should receive events for a channel
    /// (union of server members and direct participants).
    pub async fn get_channel_member_ids(&self, channel_id: &str) -> Vec<String> {
        let server_members: Vec<String> = self.inner
            .query(
                "SELECT VALUE user FROM membership WHERE server = \
                 (SELECT server FROM type::record($ch) LIMIT 1)[0].server",
            )
            .bind(("ch", channel_id.to_owned()))
            .await
            .ok()
            .map(|mut r| {
                let vals: Vec<Value> = r.take(0).unwrap_or_default();
                vals.into_iter()
                    .filter_map(|v| {
                        if let Value::RecordId(rid) = v {
                            Some(record_id_to_string(&rid))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let participants: Vec<String> = self.inner
            .query("SELECT VALUE user FROM participant WHERE channel = type::record($ch)")
            .bind(("ch", channel_id.to_owned()))
            .await
            .ok()
            .map(|mut r| {
                let vals: Vec<Value> = r.take(0).unwrap_or_default();
                vals.into_iter()
                    .filter_map(|v| {
                        if let Value::RecordId(rid) = v {
                            Some(record_id_to_string(&rid))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut set = std::collections::HashSet::new();
        for id in server_members.into_iter().chain(participants) {
            set.insert(id);
        }
        set.into_iter().collect()
    }
}

// ── SurrealDB helpers ──────────────────────────────────────────────────────────

fn db_err(e: surrealdb::Error) -> AppError {
    AppError::Db(e.to_string())
}

fn record_id_to_string(rid: &surrealdb::types::RecordId) -> String {
    let table = rid.table.as_str();
    let key = match &rid.key {
        RecordIdKey::Number(n) => n.to_string(),
        RecordIdKey::String(s) => s.clone(),
        _ => format!("{:?}", rid.key),
    };
    format!("{table}:{key}")
}

fn value_to_json(value: Value) -> serde_json::Value {
    match value {
        Value::None | Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Number(n) => match n {
            Number::Int(i) => serde_json::Value::Number(i.into()),
            Number::Float(f) => serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Number::Decimal(d) => serde_json::Value::String(d.to_string()),
        },
        Value::String(s) => serde_json::Value::String(s),
        Value::Datetime(dt) => serde_json::Value::String(dt.to_rfc3339()),
        Value::RecordId(rid) => serde_json::Value::String(record_id_to_string(&rid)),
        Value::Object(obj) => {
            let map = obj
                .into_inner()
                .into_iter()
                .map(|(k, v)| (k, value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(value_to_json).collect()),
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

fn take_one<T: DeserializeOwned>(
    response: &mut IndexedResults,
    index: usize,
) -> Result<Option<T>> {
    let value: Value = response.take(index).map_err(db_err)?;
    match &value {
        Value::None | Value::Null => return Ok(None),
        Value::Array(arr) if arr.is_empty() => return Ok(None),
        _ => {}
    }
    let single = match value {
        Value::Array(mut arr) => {
            if arr.is_empty() {
                return Ok(None);
            }
            arr.swap_remove(0)
        }
        v => v,
    };
    if matches!(&single, Value::None | Value::Null) {
        return Ok(None);
    }
    let json = value_to_json(single);
    serde_json::from_value(json)
        .map(Some)
        .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
}

fn take_many<T: DeserializeOwned>(
    response: &mut IndexedResults,
    index: usize,
) -> Result<Vec<T>> {
    let value: Value = response.take(index).map_err(db_err)?;
    match value {
        Value::None | Value::Null => Ok(vec![]),
        Value::Array(arr) => arr
            .into_iter()
            .filter(|v| !matches!(v, Value::None | Value::Null))
            .map(|v| {
                let json = value_to_json(v);
                serde_json::from_value(json)
                    .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
            })
            .collect(),
        v => {
            let json = value_to_json(v);
            serde_json::from_value(json)
                .map(|item| vec![item])
                .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
        }
    }
}

// Suppress unused warnings for types used only via Db methods.
const _: fn() = || {
    let _: DateTime<Utc>;
};

const SCHEMA: &str = r#"
-- Users
DEFINE TABLE OVERWRITE user SCHEMAFULL;
DEFINE FIELD OVERWRITE username       ON user TYPE string;
DEFINE FIELD OVERWRITE email          ON user TYPE string;
DEFINE FIELD OVERWRITE display_name   ON user TYPE string;
DEFINE FIELD OVERWRITE avatar_url     ON user TYPE option<string>;
DEFINE FIELD OVERWRITE public_key     ON user TYPE string;
DEFINE FIELD OVERWRITE created_at     ON user TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE user_username  ON user COLUMNS username UNIQUE;
DEFINE INDEX OVERWRITE user_email     ON user COLUMNS email UNIQUE;
DEFINE INDEX OVERWRITE user_pubkey    ON user COLUMNS public_key;

-- Auth challenges (short-lived nonces for Ed25519 challenge-response signin)
DEFINE TABLE OVERWRITE auth_challenge SCHEMAFULL;
DEFINE FIELD OVERWRITE public_key   ON auth_challenge TYPE string;
DEFINE FIELD OVERWRITE nonce        ON auth_challenge TYPE string;
DEFINE FIELD OVERWRITE expires_at   ON auth_challenge TYPE datetime;
DEFINE FIELD OVERWRITE used         ON auth_challenge TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE created_at   ON auth_challenge TYPE datetime DEFAULT time::now();

-- Devices
DEFINE TABLE OVERWRITE device SCHEMAFULL;
DEFINE FIELD OVERWRITE owner       ON device TYPE record<user>;
DEFINE FIELD OVERWRITE name        ON device TYPE string;
DEFINE FIELD OVERWRITE user_agent  ON device TYPE option<string>;
DEFINE FIELD OVERWRITE ip          ON device TYPE option<string>;
DEFINE FIELD OVERWRITE created_at  ON device TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE last_seen   ON device TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE revoked     ON device TYPE bool DEFAULT false;

-- Servers (guilds)
DEFINE TABLE OVERWRITE server SCHEMAFULL;
DEFINE FIELD OVERWRITE name        ON server TYPE string;
DEFINE FIELD OVERWRITE icon_url    ON server TYPE option<string>;
DEFINE FIELD OVERWRITE owner       ON server TYPE record<user>;
DEFINE FIELD OVERWRITE created_at  ON server TYPE datetime DEFAULT time::now();

-- Memberships (user <-> server)
DEFINE TABLE OVERWRITE membership SCHEMAFULL;
DEFINE FIELD OVERWRITE user       ON membership TYPE record<user>;
DEFINE FIELD OVERWRITE server     ON membership TYPE record<server>;
DEFINE FIELD OVERWRITE joined_at  ON membership TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE membership_unique ON membership COLUMNS user, server UNIQUE;

-- Invite codes
DEFINE TABLE OVERWRITE invite SCHEMAFULL;
DEFINE FIELD OVERWRITE code        ON invite TYPE string;
DEFINE FIELD OVERWRITE server      ON invite TYPE record<server>;
DEFINE FIELD OVERWRITE created_by  ON invite TYPE record<user>;
DEFINE FIELD OVERWRITE created_at  ON invite TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE expires_at  ON invite TYPE option<datetime>;
DEFINE FIELD OVERWRITE uses        ON invite TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE max_uses    ON invite TYPE option<int>;
DEFINE INDEX OVERWRITE invite_code ON invite COLUMNS code UNIQUE;

-- Categories
DEFINE TABLE OVERWRITE category SCHEMAFULL;
DEFINE FIELD OVERWRITE server    ON category TYPE record<server>;
DEFINE FIELD OVERWRITE name      ON category TYPE string;
DEFINE FIELD OVERWRITE position  ON category TYPE int DEFAULT 0;

-- Channels
DEFINE TABLE OVERWRITE channel SCHEMAFULL;
DEFINE FIELD OVERWRITE server    ON channel TYPE option<record<server>>;
DEFINE FIELD OVERWRITE category  ON channel TYPE option<record<category>>;
DEFINE FIELD OVERWRITE name      ON channel TYPE string;
DEFINE FIELD OVERWRITE kind      ON channel TYPE string;
DEFINE FIELD OVERWRITE position  ON channel TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE created_at ON channel TYPE datetime DEFAULT time::now();

-- Participants (DM / group channel members)
DEFINE TABLE OVERWRITE participant SCHEMAFULL;
DEFINE FIELD OVERWRITE user     ON participant TYPE record<user>;
DEFINE FIELD OVERWRITE channel  ON participant TYPE record<channel>;
DEFINE FIELD OVERWRITE added_at ON participant TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE participant_unique ON participant COLUMNS user, channel UNIQUE;

-- Messages
DEFINE TABLE OVERWRITE message SCHEMAFULL;
DEFINE FIELD OVERWRITE channel   ON message TYPE record<channel>;
DEFINE FIELD OVERWRITE author    ON message TYPE record<user>;
DEFINE FIELD OVERWRITE content   ON message TYPE string;
DEFINE FIELD OVERWRITE reply_to  ON message TYPE option<record<message>>;
DEFINE FIELD OVERWRITE edited_at ON message TYPE option<datetime>;
DEFINE FIELD OVERWRITE deleted   ON message TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE created_at ON message TYPE datetime DEFAULT time::now();

-- Reactions
DEFINE TABLE OVERWRITE reaction SCHEMAFULL;
DEFINE FIELD OVERWRITE message  ON reaction TYPE record<message>;
DEFINE FIELD OVERWRITE user     ON reaction TYPE record<user>;
DEFINE FIELD OVERWRITE emoji    ON reaction TYPE string;
DEFINE INDEX OVERWRITE reaction_unique ON reaction COLUMNS message, user, emoji UNIQUE;

-- Friend requests
DEFINE TABLE OVERWRITE friend_request SCHEMAFULL;
DEFINE FIELD OVERWRITE "from"    ON friend_request TYPE record<user>;
DEFINE FIELD OVERWRITE "to"      ON friend_request TYPE record<user>;
DEFINE FIELD OVERWRITE status    ON friend_request TYPE string DEFAULT 'pending';
DEFINE FIELD OVERWRITE created_at ON friend_request TYPE datetime DEFAULT time::now();

-- Voice sessions (ephemeral — cleared on restart via REMOVE TABLE then DEFINE)
DEFINE TABLE OVERWRITE voice_session SCHEMAFULL;
DEFINE FIELD OVERWRITE user       ON voice_session TYPE record<user>;
DEFINE FIELD OVERWRITE channel    ON voice_session TYPE record<channel>;
DEFINE FIELD OVERWRITE joined_at  ON voice_session TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE voice_session_unique ON voice_session COLUMNS user, channel UNIQUE;

-- Attachments (file uploads)
DEFINE TABLE OVERWRITE attachment SCHEMAFULL;
DEFINE FIELD OVERWRITE uploaded_by   ON attachment TYPE record<user>;
DEFINE FIELD OVERWRITE message       ON attachment TYPE option<record<message>>;
DEFINE FIELD OVERWRITE filename      ON attachment TYPE string;
DEFINE FIELD OVERWRITE storage_name  ON attachment TYPE string;
DEFINE FIELD OVERWRITE mime_type     ON attachment TYPE string;
DEFINE FIELD OVERWRITE size_bytes    ON attachment TYPE int;
DEFINE FIELD OVERWRITE created_at    ON attachment TYPE datetime DEFAULT time::now();
"#;
