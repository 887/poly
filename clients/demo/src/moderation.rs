//! In-memory moderation state for the demo backends.
//!
//! The demo backend is the *reference* `IsBackend` implementation, and the
//! per-slug capability tables declare `demo` / `poly` / `demo_forum` as
//! moderation-capable (`has_roles`, `has_kick`, `has_ban`, `has_timed_ban`,
//! `has_channel_mgmt`, `has_moderation_log`). Before this module existed the
//! declaration was a lie: `IsBackend::as_moderation()` fell through to the
//! `None` default, so the Roles / Bans / Mod-Log tabs and the Edit-Channel
//! dialog rendered but every fetch failed with `NotSupported`.
//!
//! Everything here is process-lifetime in-memory state (same contract as the
//! sent-message store in [`crate::data`]): demo backends never persist across
//! restarts, but every write round-trips within one session.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use async_trait::async_trait;
use poly_client::{
    BannedMember, Channel, ClientError, ClientResult, MemberPermissions, Message,
    ModerationAction, ModerationBackend, ModerationLogEntry, Role, UpdateChannelParams, User,
    WritableModerationBackend,
};

use crate::DemoClientGeneric;
use crate::flavour::DemoFlavour;

// ── In-memory store ────────────────────────────────────────────────────────

/// Mutable moderation state shared by all demo flavours.
#[derive(Default)]
struct ModerationState {
    /// `server_id` → current bans (includes active timeouts).
    bans: HashMap<String, Vec<BannedMember>>,
    /// `server_id` → moderation log, oldest first.
    log: HashMap<String, Vec<ModerationLogEntry>>,
    /// `channel_id` → renamed title (from `update_channel`).
    channel_names: HashMap<String, String>,
    /// `server_id` → explicit channel ordering (from `reorder_channels`).
    channel_order: HashMap<String, Vec<String>>,
    /// Message ids removed via `delete_message`.
    deleted_messages: HashSet<String>,
    /// Monotonic counter backing generated log-entry ids.
    next_log_id: u64,
}

fn store() -> &'static Mutex<ModerationState> {
    static STORE: OnceLock<Mutex<ModerationState>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(ModerationState::default()))
}

/// Run `f` against the shared state, mapping a poisoned mutex onto
/// `ClientError::Internal` rather than panicking (workspace policy: no
/// `unwrap`/`expect` on locks).
fn with_state<R>(f: impl FnOnce(&mut ModerationState) -> R) -> ClientResult<R> {
    match store().lock() {
        Ok(mut guard) => Ok(f(&mut guard)),
        Err(_) => Err(ClientError::Internal(
            "demo moderation store poisoned".to_string(),
        )),
    }
}

// ── Seed fixtures ──────────────────────────────────────────────────────────

/// Two fixture bans per server so the Bans tab has content on first open:
/// one permanent ban and one active timeout.
fn seed_bans() -> Vec<BannedMember> {
    vec![
        BannedMember {
            user_id: "user-mallory".to_string(),
            display_name: "Mallory".to_string(),
            avatar_url: None,
            reason: Some("Posting invite spam in every channel".to_string()),
            expires_at: None,
            banned_at: Some(crate::data::ago_hours(72).to_rfc3339()),
        },
        BannedMember {
            user_id: "user-trent".to_string(),
            display_name: "Trent".to_string(),
            avatar_url: None,
            reason: Some("Cooling-off period after a flame war".to_string()),
            expires_at: Some(crate::data::ago_hours(-24).to_rfc3339()),
            banned_at: Some(crate::data::ago_hours(2).to_rfc3339()),
        },
    ]
}

/// Seed `server_id` on first touch so reads are non-empty and later writes
/// mutate the same rows the UI already showed.
fn ensure_seeded(state: &mut ModerationState, server_id: &str, moderator: &User) {
    if state.bans.contains_key(server_id) {
        return;
    }
    let bans = seed_bans();
    let entries: Vec<ModerationLogEntry> = bans
        .iter()
        .map(|ban| ModerationLogEntry {
            id: format!("modlog-seed-{}-{}", server_id, ban.user_id),
            action: if ban.expires_at.is_some() {
                ModerationAction::MemberTimedOut
            } else {
                ModerationAction::MemberBanned
            },
            moderator: moderator.clone(),
            target_user_id: Some(ban.user_id.clone()),
            target_display_name: Some(ban.display_name.clone()),
            channel_id: None,
            message_id: None,
            reason: ban.reason.clone(),
            timestamp: ban
                .banned_at
                .clone()
                .unwrap_or_else(|| crate::data::ago_hours(72).to_rfc3339()),
        })
        .collect();
    state.bans.insert(server_id.to_string(), bans);
    state.log.insert(server_id.to_string(), entries);
}

/// Read-only role list. The demo user is always the owner.
fn demo_roles() -> Vec<Role> {
    vec![
        Role {
            id: "role-owner".to_string(),
            name: "Owner".to_string(),
            color: Some("#f2a33c".to_string()),
            permissions: owner_permissions(),
            position: 3,
        },
        Role {
            id: "role-moderator".to_string(),
            name: "Moderator".to_string(),
            color: Some("#5865f2".to_string()),
            permissions: MemberPermissions {
                manage_channels: true,
                kick_members: true,
                ban_members: true,
                manage_messages: true,
                timeout_members: true,
                display_role: "Moderator".to_string(),
                power_level: Some(50),
                ..MemberPermissions::default()
            },
            position: 2,
        },
        Role {
            id: "role-member".to_string(),
            name: "Member".to_string(),
            color: None,
            permissions: MemberPermissions {
                display_role: "Member".to_string(),
                power_level: Some(0),
                ..MemberPermissions::default()
            },
            position: 1,
        },
    ]
}

fn owner_permissions() -> MemberPermissions {
    MemberPermissions {
        manage_server: true,
        manage_channels: true,
        manage_roles: true,
        kick_members: true,
        ban_members: true,
        manage_messages: true,
        timeout_members: true,
        display_role: "Owner".to_string(),
        power_level: Some(100),
    }
}

// ── Log helpers ────────────────────────────────────────────────────────────

/// Everything needed to append one moderation-log entry.
///
/// A struct rather than eight positional parameters — `clippy::too_many_arguments`
/// is denied workspace-wide and the call sites read better this way.
struct LogRecord<'a> {
    server_id: &'a str,
    action: ModerationAction,
    moderator: &'a User,
    target_user_id: Option<String>,
    channel_id: Option<String>,
    message_id: Option<String>,
    reason: Option<String>,
}

fn append_log(state: &mut ModerationState, record: LogRecord<'_>) {
    let id = state.next_log_id;
    state.next_log_id = state.next_log_id.saturating_add(1);
    let entry = ModerationLogEntry {
        id: format!("modlog-{id}"),
        action: record.action,
        moderator: record.moderator.clone(),
        target_display_name: record.target_user_id.clone(),
        target_user_id: record.target_user_id,
        channel_id: record.channel_id,
        message_id: record.message_id,
        reason: record.reason,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    state
        .log
        .entry(record.server_id.to_string())
        .or_default()
        .push(entry);
}

// ── Projections consumed by the generic `IsBackend` impl ───────────────────

/// Drop messages removed via `delete_message` so `get_messages` reflects the
/// moderation action instead of silently re-serving the deleted row.
pub(crate) fn filter_deleted(messages: Vec<Message>) -> Vec<Message> {
    let Ok(guard) = store().lock() else {
        return messages;
    };
    if guard.deleted_messages.is_empty() {
        return messages;
    }
    messages
        .into_iter()
        .filter(|m| !guard.deleted_messages.contains(&m.id))
        .collect()
}

/// Apply `update_channel` renames and `reorder_channels` ordering so channel
/// management round-trips within the session.
pub(crate) fn apply_channel_overrides(mut channels: Vec<Channel>) -> Vec<Channel> {
    let Ok(guard) = store().lock() else {
        return channels;
    };
    for channel in &mut channels {
        if let Some(name) = guard.channel_names.get(&channel.id) {
            channel.name.clone_from(name);
        }
    }
    let order = channels
        .first()
        .and_then(|c| guard.channel_order.get(&c.server_id))
        .cloned();
    if let Some(order) = order {
        channels.sort_by_key(|c| {
            order
                .iter()
                .position(|id| id == &c.id)
                .unwrap_or(usize::MAX)
        });
    }
    channels
}

/// Resolve the owning server of `channel_id` by walking the flavour fixtures.
fn server_of_channel<F: DemoFlavour>(channel_id: &str) -> Option<String> {
    F::servers().into_iter().find_map(|server| {
        F::channels(&server.id)
            .into_iter()
            .any(|c| c.id == channel_id)
            .then_some(server.id)
    })
}

// ── ModerationBackend (reads + writable accessor) ──────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> ModerationBackend for DemoClientGeneric<F> {
    async fn get_my_permissions(
        &self,
        _server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        Ok(owner_permissions())
    }

    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            state.bans.get(server_id).cloned().unwrap_or_default()
        })
    }

    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            state.log.get(server_id).map_or_else(Vec::new, |entries| {
                entries.iter().rev().take(limit).cloned().collect()
            })
        })
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Ok(demo_roles())
    }

    fn as_writable_moderation(&self) -> Option<&dyn WritableModerationBackend> {
        Some(self)
    }
}

// ── WritableModerationBackend ──────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> WritableModerationBackend for DemoClientGeneric<F> {
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            append_log(
                state,
                LogRecord {
                    server_id,
                    action: ModerationAction::MemberKicked,
                    moderator: &moderator,
                    target_user_id: Some(member_id.to_string()),
                    channel_id: None,
                    message_id: None,
                    reason: reason.map(str::to_string),
                },
            );
        })
    }

    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            upsert_ban(state, server_id, member_id, reason, None);
            append_log(
                state,
                LogRecord {
                    server_id,
                    action: ModerationAction::MemberBanned,
                    moderator: &moderator,
                    target_user_id: Some(member_id.to_string()),
                    channel_id: None,
                    message_id: None,
                    reason: reason.map(str::to_string),
                },
            );
        })
    }

    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            if !remove_ban(state, server_id, member_id) {
                return Err(ClientError::NotFound(format!(
                    "demo: {member_id} is not banned in {server_id}"
                )));
            }
            append_log(
                state,
                LogRecord {
                    server_id,
                    action: ModerationAction::MemberUnbanned,
                    moderator: &moderator,
                    target_user_id: Some(member_id.to_string()),
                    channel_id: None,
                    message_id: None,
                    reason: None,
                },
            );
            Ok(())
        })?
    }

    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            upsert_ban(state, server_id, member_id, reason, Some(until.to_rfc3339()));
            append_log(
                state,
                LogRecord {
                    server_id,
                    action: ModerationAction::MemberTimedOut,
                    moderator: &moderator,
                    target_user_id: Some(member_id.to_string()),
                    channel_id: None,
                    message_id: None,
                    reason: reason.map(str::to_string),
                },
            );
        })
    }

    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        let moderator = F::session().user;
        with_state(|state| {
            ensure_seeded(state, server_id, &moderator);
            let is_timeout = state.bans.get(server_id).is_some_and(|bans| {
                bans.iter()
                    .any(|b| b.user_id == member_id && b.expires_at.is_some())
            });
            if !is_timeout {
                return Err(ClientError::NotFound(format!(
                    "demo: {member_id} has no active timeout in {server_id}"
                )));
            }
            let _removed: bool = remove_ban(state, server_id, member_id);
            append_log(
                state,
                LogRecord {
                    server_id,
                    action: ModerationAction::Other("member_untimed_out".to_string()),
                    moderator: &moderator,
                    target_user_id: Some(member_id.to_string()),
                    channel_id: None,
                    message_id: None,
                    reason: None,
                },
            );
            Ok(())
        })?
    }

    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        let moderator = F::session().user;
        let server_id = server_of_channel::<F>(channel_id);
        with_state(|state| {
            let _inserted: bool = state.deleted_messages.insert(message_id.to_string());
            if let Some(server_id) = server_id.as_deref() {
                append_log(
                    state,
                    LogRecord {
                        server_id,
                        action: ModerationAction::MessageDeleted,
                        moderator: &moderator,
                        target_user_id: None,
                        channel_id: Some(channel_id.to_string()),
                        message_id: Some(message_id.to_string()),
                        reason: None,
                    },
                );
            }
        })
    }

    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        let moderator = F::session().user;
        let server_id = server_of_channel::<F>(channel_id);
        with_state(|state| {
            if let Some(name) = update.name.clone() {
                let _prev: Option<String> =
                    state.channel_names.insert(channel_id.to_string(), name);
            }
            if let Some(server_id) = server_id.as_deref() {
                append_log(
                    state,
                    LogRecord {
                        server_id,
                        action: ModerationAction::ChannelUpdated,
                        moderator: &moderator,
                        target_user_id: None,
                        channel_id: Some(channel_id.to_string()),
                        message_id: None,
                        reason: update.topic.clone(),
                    },
                );
            }
        })
    }

    async fn reorder_channels(&self, server_id: &str, ordering: Vec<String>) -> ClientResult<()> {
        let moderator = F::session().user;
        with_state(|state| {
            let _prev: Option<Vec<String>> =
                state.channel_order.insert(server_id.to_string(), ordering);
            append_log(
                state,
                LogRecord {
                    server_id,
                    action: ModerationAction::ChannelUpdated,
                    moderator: &moderator,
                    target_user_id: None,
                    channel_id: None,
                    message_id: None,
                    reason: None,
                },
            );
        })
    }
}

/// Insert or replace the ban row for `member_id`.
fn upsert_ban(
    state: &mut ModerationState,
    server_id: &str,
    member_id: &str,
    reason: Option<&str>,
    expires_at: Option<String>,
) {
    let entry = BannedMember {
        user_id: member_id.to_string(),
        display_name: member_id.to_string(),
        avatar_url: None,
        reason: reason.map(str::to_string),
        expires_at,
        banned_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    let bans = state.bans.entry(server_id.to_string()).or_default();
    bans.retain(|b| b.user_id != member_id);
    bans.push(entry);
}

/// Remove the ban row for `member_id`; returns `false` if there was none.
fn remove_ban(state: &mut ModerationState, server_id: &str, member_id: &str) -> bool {
    let Some(bans) = state.bans.get_mut(server_id) else {
        return false;
    };
    let before = bans.len();
    bans.retain(|b| b.user_id != member_id);
    bans.len() != before
}
