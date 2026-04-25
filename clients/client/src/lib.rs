//! # poly-client
//!
//! Shared messenger client trait and data types for Poly.
//!
//! This crate defines the [`ClientBackend`] trait that all messenger backend
//! implementations (Stoat, Matrix, Discord, Teams, Demo) must implement.
//! It also defines the shared data types used across all backends.

pub mod events;
pub mod types;
pub mod ui_surface;

pub use events::*;
pub use types::*;
pub use ui_surface::*;

use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

/// Errors that can occur in client backend operations.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Authentication failed.
    #[error("authentication failed: {0}")]
    AuthFailed(String),

    /// Network error.
    #[error("network error: {0}")]
    Network(String),

    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Rate limited by the server.
    #[error("rate limited, retry after {retry_after_ms}ms")]
    RateLimited {
        /// Milliseconds to wait before retrying.
        retry_after_ms: u64,
    },

    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Internal or unexpected error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Operation not supported by this backend.
    #[error("not supported: {0}")]
    NotSupported(String),
}

/// Result type for client backend operations.
pub type ClientResult<T> = Result<T, ClientError>;

/// The core trait that all messenger backend clients must implement.
///
/// Each backend (Stoat, Matrix, Discord, Teams, Demo) implements this trait
/// to provide a unified API for the Poly UI layer.
// DECISION(D12): Demo client implements this trait for Phase 2 UI testing.
//
// On WASM, reqwest's futures are !Send (they use Rc<RefCell<>> internally).
// We use ?Send on the async_trait to avoid requiring Send-able futures on WASM.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ClientBackend: Send + Sync {
    // --- Authentication ---

    /// Authenticate with the backend using the provided credentials.
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session>;

    /// Log out and invalidate the current session.
    async fn logout(&mut self) -> ClientResult<()>;

    /// Check if the client is currently authenticated.
    fn is_authenticated(&self) -> bool;

    // --- Servers / Communities ---

    /// Get all servers/communities the user has joined.
    async fn get_servers(&self) -> ClientResult<Vec<Server>>;

    /// Get a specific server by ID.
    async fn get_server(&self, id: &str) -> ClientResult<Server>;

    // --- Channels ---

    /// Get all channels in a server.
    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>>;

    /// Get a specific channel by ID.
    async fn get_channel(&self, id: &str) -> ClientResult<Channel>;

    // --- Messages ---

    /// Send a message to a channel.
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message>;

    /// Broadcast a typing indicator for the given channel.
    ///
    /// Fire-and-forget — callers should not block on the result. Backends that
    /// do not support typing indicators return [`ClientError::NotSupported`].
    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        let _ = channel_id;
        Err(ClientError::NotSupported("send_typing".to_string()))
    }

    /// Send a reply to an existing message.
    ///
    /// Default implementation falls back to [`ClientBackend::send_message`]
    /// for backends that do not yet expose reply semantics natively.
    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let _ = reply_to_message_id;
        self.send_message(channel_id, content).await
    }

    /// Get messages from a channel with query options.
    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>>;

    /// Search messages using the backend's native search implementation.
    ///
    /// Backends that do not support search should return the default
    /// `Err(ClientError::NotSupported(...))` provided below.
    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        let _ = query;
        Err(ClientError::NotSupported("search_messages".to_string()))
    }

    /// Get pinned messages for a channel.
    ///
    /// Backends that do not support pins should return an empty list or the
    /// default implementation below.
    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
        let _ = channel_id;
        Ok(Vec::new())
    }

    /// Get slash commands available in a channel.
    ///
    /// Returns app/bot-provided commands valid for `channel_id`. The UI layer
    /// prepends built-in Poly commands (shrug, me, tableflip, …) before showing
    /// the autocomplete popup, so backends do not need to include those.
    ///
    /// Backends that do not support slash commands should return an empty list.
    async fn get_channel_commands(&self, channel_id: &str) -> ClientResult<Vec<ChatCommand>> {
        let _ = channel_id;
        Ok(Vec::new())
    }

    /// Get the custom emoji usable in a channel.
    async fn get_available_emojis(&self, channel_id: &str) -> ClientResult<Vec<CustomEmoji>> {
        let _ = channel_id;
        Ok(Vec::new())
    }

    /// Get the stickers usable in a channel.
    async fn get_available_stickers(&self, channel_id: &str) -> ClientResult<Vec<StickerItem>> {
        let _ = channel_id;
        Ok(Vec::new())
    }

    /// Pin or unpin a message in a channel.
    ///
    /// Backends that do not support pin mutation should return the default
    /// `Err(ClientError::NotSupported(...))` provided below.
    async fn set_message_pinned(
        &self,
        channel_id: &str,
        message_id: &str,
        pinned: bool,
    ) -> ClientResult<()> {
        let _ = (channel_id, message_id, pinned);
        Err(ClientError::NotSupported("set_message_pinned".to_string()))
    }

    // --- Users ---

    /// Get a user by ID.
    async fn get_user(&self, id: &str) -> ClientResult<User>;

    /// Get the authenticated user's friend list.
    async fn get_friends(&self) -> ClientResult<Vec<User>>;

    /// Get members of a channel.
    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>>;

    // --- Groups (multi-user DMs) ---

    /// Get all group chats.
    async fn get_groups(&self) -> ClientResult<Vec<Group>>;

    /// Remove a user from a group DM.
    ///
    /// Backends that do not support removing members should return the
    /// default `Err(ClientError::NotSupported(...))` provided below.
    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        let _ = (group_id, user_id);
        Err(ClientError::NotSupported("remove_group_member".to_string()))
    }

    /// Add a user to a group DM.
    ///
    /// Backends that do not support adding members should return the
    /// default `Err(ClientError::NotSupported(...))` provided below.
    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        let _ = (group_id, user_id);
        Err(ClientError::NotSupported("add_group_member".to_string()))
    }

    // --- Direct Messages ---

    /// Get all DM channels.
    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>>;

    /// Open or create a DM channel with the target user.
    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        let _ = user_id;
        Err(ClientError::NotSupported(
            "open_direct_message_channel".to_string(),
        ))
    }

    /// Open the authenticated user's Saved Messages / self-DM channel.
    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel".to_string(),
        ))
    }

    // --- Notifications ---

    /// Get the user's notifications.
    async fn get_notifications(&self) -> ClientResult<Vec<Notification>>;

    /// Accept or reject a pending friend request.
    ///
    /// `user_id` is the ID of the user who sent the request.
    /// `accept` is `true` to accept, `false` to reject.
    ///
    /// Backends that do not support this action return `NotSupported`.
    async fn respond_to_friend_request(&self, user_id: &str, accept: bool) -> ClientResult<()> {
        let _ = (user_id, accept);
        Err(ClientError::NotSupported(
            "respond_to_friend_request".to_string(),
        ))
    }

    /// Accept or decline a pending server invite.
    ///
    /// Backends that do not support this action return `NotSupported`.
    async fn respond_to_server_invite(&self, server_id: &str, accept: bool) -> ClientResult<()> {
        let _ = (server_id, accept);
        Err(ClientError::NotSupported(
            "respond_to_server_invite".to_string(),
        ))
    }

    // --- Content & Social Policy ---

    /// Get the account's content and social policy settings.
    ///
    /// Returns [`ClientError::NotSupported`] if the backend does not expose
    /// content policy settings — the UI will fall back to locally-stored defaults.
    async fn get_content_policy(&self) -> ClientResult<ContentPolicy> {
        Err(ClientError::NotSupported("get_content_policy".to_string()))
    }

    /// Update the account's content and social policy settings.
    ///
    /// Backends that do not support this action return `NotSupported`.
    async fn set_content_policy(&self, policy: ContentPolicy) -> ClientResult<()> {
        let _ = policy;
        Err(ClientError::NotSupported("set_content_policy".to_string()))
    }

    /// Get the list of users blocked by the authenticated user.
    ///
    /// Returns an empty list if the backend does not track blocks.
    async fn get_blocked_users(&self) -> ClientResult<Vec<BlockedUser>> {
        Ok(Vec::new())
    }

    /// Unblock a previously blocked user.
    ///
    /// Backends that do not support unblocking return `NotSupported`.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        let _ = user_id;
        Err(ClientError::NotSupported("unblock_user".to_string()))
    }

    // --- Presence ---

    // --- Voice / Video ---

    /// Get the current voice participants in a voice or video channel.
    ///
    /// Returns the list of users currently connected to the channel.
    /// Returns an empty list for backends where voice participant tracking is
    /// not available or the channel is not a voice/video channel.
    async fn get_voice_participants(&self, channel_id: &str)
    -> ClientResult<Vec<VoiceParticipant>>;

    // --- Presence ---

    /// Get a user's online presence status.
    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus>;

    /// Set the authenticated user's presence status.
    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()>;

    // --- Moderation (optional capability — Wave 1 scaffolding) ---

    /// Get the calling user's effective permissions in a server (and optionally
    /// a specific channel).
    ///
    /// Backends that do not expose a permission model return `NotSupported`.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let _ = (server_id, channel_id);
        Err(ClientError::NotSupported("get_my_permissions".to_string()))
    }

    /// Kick a member from a server.
    ///
    /// Backends that do not support kick return `NotSupported`.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let _ = (server_id, member_id, reason);
        Err(ClientError::NotSupported("kick_member".to_string()))
    }

    /// Permanently ban a member from a server.
    ///
    /// Use `timeout_member` for temporary suspensions. Backends that do not
    /// support permanent bans return `NotSupported`.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        let _ = (server_id, member_id, reason, delete_message_history_secs);
        Err(ClientError::NotSupported("ban_member".to_string()))
    }

    /// Lift a ban for a member.
    ///
    /// Backends that do not support bans return `NotSupported`.
    async fn unban_member(
        &self,
        server_id: &str,
        member_id: &str,
    ) -> ClientResult<()> {
        let _ = (server_id, member_id);
        Err(ClientError::NotSupported("unban_member".to_string()))
    }

    /// Temporarily suspend a member until `until`.
    ///
    /// This maps to Discord's `communication_disabled_until`, Stoat's native
    /// timeout field, or Lemmy's `expires`-bearing ban — each backend uses its
    /// own native primitive. Backends that do not support timed suspensions
    /// return `NotSupported`.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let _ = (server_id, member_id, until, reason);
        Err(ClientError::NotSupported("timeout_member".to_string()))
    }

    /// Remove a timeout / suspension from a member.
    ///
    /// Backends that do not support timeouts return `NotSupported`.
    async fn untimeout_member(
        &self,
        server_id: &str,
        member_id: &str,
    ) -> ClientResult<()> {
        let _ = (server_id, member_id);
        Err(ClientError::NotSupported("untimeout_member".to_string()))
    }

    /// Get the list of banned members for a server.
    ///
    /// Backends that do not support bans return `NotSupported`.
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let _ = server_id;
        Err(ClientError::NotSupported("get_bans".to_string()))
    }

    /// Delete a single message by ID.
    ///
    /// The caller should already have verified the user has `manage_messages`
    /// permission or is the message author. Backends that do not support
    /// message deletion return `NotSupported`.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let _ = (channel_id, message_id);
        Err(ClientError::NotSupported("delete_message".to_string()))
    }

    /// Update channel settings (name, topic, slow-mode, nsfw, position).
    ///
    /// Only fields set to `Some` are changed. Backends that do not support
    /// channel editing return `NotSupported`.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        let _ = (channel_id, update);
        Err(ClientError::NotSupported("update_channel".to_string()))
    }

    /// Reorder channels within a server.
    ///
    /// `ordering` is the desired channel-ID order (all channels, including
    /// those not being moved). Backends that do not support reordering return
    /// `NotSupported`.
    async fn reorder_channels(
        &self,
        server_id: &str,
        ordering: Vec<String>,
    ) -> ClientResult<()> {
        let _ = (server_id, ordering);
        Err(ClientError::NotSupported("reorder_channels".to_string()))
    }

    /// Fetch recent moderation log entries for a server.
    ///
    /// `limit` caps the number of entries returned. Backends that do not
    /// expose a moderation log return `NotSupported`.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        let _ = (server_id, limit);
        Err(ClientError::NotSupported("get_moderation_log".to_string()))
    }

    /// Fetch the role list for a server.
    ///
    /// Returns roles sorted by position (ascending). Backends that do not
    /// expose roles return `NotSupported`.
    async fn get_server_roles(&self, server_id: &str) -> ClientResult<Vec<Role>> {
        let _ = server_id;
        Err(ClientError::NotSupported("get_server_roles".to_string()))
    }

    // --- Server management (optional capability) ---

    /// Create a new server/guild in this backend.
    ///
    /// Returns the newly created [`Server`].
    ///
    /// Backends that do not support server creation should return the default
    /// `Err(ClientError::NotSupported(...))` provided here.
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported("create_server".to_string()))
    }

    /// Create a new channel inside a server.
    ///
    /// `server_id` is the backend-specific ID of the parent server.
    /// `name` is the channel display name.
    /// `channel_type` selects Text, Voice, or Video.
    ///
    /// Returns the newly created [`Channel`].
    ///
    /// Backends that do not support channel creation should return the default
    /// `Err(ClientError::NotSupported(...))` provided here.
    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported("create_channel".to_string()))
    }

    /// Update the banner image URL for a server.
    ///
    /// `server_id` is the backend-specific server/guild/community ID.
    /// `banner_url` is a URL string pointing to the new banner image, or `None`
    /// to clear the banner.
    ///
    /// ## Format contract
    ///
    /// Pass a publicly accessible URL. For Discord, this must be a base64 data
    /// URI (`data:image/png;base64,…`) because the Discord API does not accept
    /// remote URLs; the real-Discord path is therefore not implementable without
    /// a binary upload step (noted out-of-scope). The Spacebar/test-server path
    /// accepts a URL string for test convenience.
    ///
    /// Backends that do not support banner updates return
    /// [`ClientError::NotSupported`] — including backends where the banner is
    /// a local-only `AppSettings` override (Matrix, Teams, Demo).
    async fn update_server_banner(
        &self,
        _server_id: &str,
        _banner_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("update_server_banner".to_string()))
    }

    // --- Real-time events ---

    /// Get a stream of real-time events from the backend.
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>>;

    // --- Backend info ---

    /// The type of backend this client connects to.
    fn backend_type(&self) -> BackendType;

    /// Human-readable name for this backend.
    fn backend_name(&self) -> &str;

    /// Capability flags describing what this backend supports.
    ///
    /// The UI uses these flags to hide controls that don't apply (e.g. mic /
    /// speaker buttons for read-only news feeds, DM picker for backends with
    /// no DMs). Default returns [`BackendCapabilities::READ_ONLY_FEED`] —
    /// the safe minimum; richer backends opt in explicitly.
    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::READ_ONLY_FEED
    }

    /// Self-declared plugin manifest. Purely informational.
    ///
    /// Native (in-tree) backends return [`PluginManifest::default`]. WASM
    /// plugins override this with the manifest exported via the WIT
    /// `get-plugin-manifest` function so the settings UI can display what
    /// the plugin says it will access.
    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest::default()
    }

    // --- Code repository channels ---

    /// List entries at the given path within a code-type channel.
    ///
    /// `path` is repo-relative; an empty string means the repo root. Backends
    /// that do not have code channels should return the default
    /// `Err(ClientError::NotSupported(...))` provided here.
    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>> {
        let _ = (channel_id, path);
        Err(ClientError::NotSupported("list_files".to_string()))
    }

    /// Read the raw bytes of a file in a code-type channel.
    ///
    /// Backends that do not have code channels should return the default
    /// `Err(ClientError::NotSupported(...))` provided here.
    async fn read_file(&self, channel_id: &str, path: &str) -> ClientResult<FileContent> {
        let _ = (channel_id, path);
        Err(ClientError::NotSupported("read_file".to_string()))
    }

    // --- Forum channels and threads ---

    /// Get forum posts (threads) in a forum channel.
    ///
    /// Posts are sorted according to `sort`. `limit` caps the number returned;
    /// `None` uses the backend default.
    ///
    /// Backends that do not support forum channels return `NotSupported`.
    async fn get_forum_posts(
        &self,
        forum_channel_id: &str,
        sort: ForumSortOrder,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>> {
        let _ = (forum_channel_id, sort, limit);
        Err(ClientError::NotSupported(
            "get_forum_posts not implemented".into(),
        ))
    }

    /// Get all active (non-archived) threads in a server.
    ///
    /// Backends that do not support threads return `NotSupported`.
    async fn get_active_threads(&self, server_id: &str) -> ClientResult<Vec<ThreadInfo>> {
        let _ = server_id;
        Err(ClientError::NotSupported(
            "get_active_threads not implemented".into(),
        ))
    }

    /// Get archived threads for a parent channel (text or forum).
    ///
    /// `limit` caps the number returned; `None` uses the backend default.
    /// Backends that do not support threads return `NotSupported`.
    async fn get_archived_threads(
        &self,
        parent_channel_id: &str,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ThreadInfo>> {
        let _ = (parent_channel_id, limit);
        Err(ClientError::NotSupported(
            "get_archived_threads not implemented".into(),
        ))
    }

    /// Create a new forum post (thread) in a forum channel.
    ///
    /// `title` is the post/thread name, `body` is the starter message text,
    /// and `tags` is the list of tag IDs to apply.
    ///
    /// Returns the newly-created [`ForumPost`] on success.
    /// Backends that do not support forum post creation return
    /// [`ClientError::NotSupported`].
    async fn create_forum_post(
        &self,
        forum_channel_id: &str,
        title: &str,
        body: &str,
        tags: Vec<String>,
    ) -> ClientResult<ForumPost> {
        let _ = (forum_channel_id, title, body, tags);
        Err(ClientError::NotSupported(
            "create_forum_post not implemented".into(),
        ))
    }

    // --- Client-provided UI surface (WP 1 / plan-client-ui-surface) ----
    //
    // Per D9 these methods have **no default implementation** — every
    // backend is required to implement them (explicit empty list for
    // backends that have nothing to contribute).

    /// D11 — return plugin-declared context menu items for `target`.
    ///
    /// Called by the host every time a context menu opens (D24, no
    /// caching). Merge with host-universal items happens in the host.
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>>;

    /// D14 / D22 — dispatch a plugin action. Unknown ids return
    /// `ClientError::NotFound(action_id)`.
    async fn invoke_context_action(
        &self,
        action_id: &str,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome>;

    /// D16 — poll a pending async action for its final outcome.
    async fn poll_action(&self, handle: PendingHandle) -> ClientResult<ActionOutcome>;

    /// D11 / D18 — every settings section this plugin contributes across
    /// every scope. Host filters by scope at render time.
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>>;

    /// D15 — read a JSON-encoded setting value from the plugin.
    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String>;

    /// D15 — write a JSON-encoded setting value via the plugin.
    async fn set_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()>;

    /// D5 / D19 — plugin's current sidebar declaration. Host re-calls on
    /// receipt of [`ClientEvent::SidebarInvalidated`].
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration>;

    /// D14 / D25 — dispatch a sidebar-item click.
    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome>;

    /// Fetch the account-level overview view descriptor.
    ///
    /// Each backend supplies its own per-account "overview" view rendered
    /// at `/{backend}/{instance}/{account}/overview`. This is the default
    /// landing page for every account unless the backend declares a
    /// different `landing` capability. Plugin-defined content: repo grids
    /// for forge backends, community / server cards for chat backends,
    /// curated story lists for read-only feeds.
    ///
    /// The default impl returns a generic CardBody descriptor so the host
    /// always has something to render. Phase 2 of the overview plan
    /// replaces the default with a backend-specific impl in each
    /// `clients/<name>/src/lib.rs`.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("overview-default-title".to_string()),
                subtitle_key: Some("overview-default-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    /// D5 — fetch a channel's non-chat view descriptor.
    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor>;

    /// D23 — paged data feed. `cursor == None` for the first page.
    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage>;

    /// D5 — detail payload for `split` views.
    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail>;

    /// D8 — composer-toolbar buttons for the given channel.
    async fn get_composer_buttons(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<ComposerButton>>;

    /// D8 — per-message actions, merged into the message hover/overflow menu.
    async fn get_message_actions(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<Vec<MenuItem>>;

    /// D14 / D25 — dispatch a composer button action.
    async fn invoke_composer_action(
        &self,
        action_id: &str,
        channel_id: &str,
    ) -> ClientResult<ActionOutcome>;

    /// D14 / D25 — dispatch a per-message action.
    async fn invoke_message_action(
        &self,
        action_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<ActionOutcome>;
}

// ── Signup plugin interface ──────────────────────────────────────────────────

/// Host-provided context passed to a signup page component when it renders.
///
/// The host (poly-core) populates this struct from its own state before
/// calling the plugin's render function.  Plugins use what they need.
///
/// Adding fields here is backwards-compatible — existing plugins ignore
/// unknown fields.
#[derive(Clone, Debug)]
pub struct SignupContext {
    /// The local Ed25519 private key, if one has been generated.
    ///
    /// Used by backends that authenticate via challenge-response
    /// (e.g. Poly Server).  Backends that use passwords or OAuth may
    /// ignore this field.
    pub private_key: Option<Vec<u8>>,

    /// i18n lookup function for the current locale.
    ///
    /// Resolves FTL message keys — including plugin-registered ones — to
    /// translated strings in the currently active locale.  The host
    /// points this at `poly_core::i18n::t` at context creation time.
    ///
    /// Using a function pointer instead of depending on `poly-core` keeps
    /// the `poly-client` crate free of UI framework dependencies.
    ///
    /// Falls back to returning the key unchanged when not set (e.g. tests).
    pub t: fn(&str) -> String,

    /// Navigate back to the signup backend picker.
    ///
    /// Called when the user clicks a "← Back" link in the signup form.
    /// The host sets this to navigate to `Route::SignupPicker`; tests use a no-op.
    pub navigate_back: fn(),
}

fn _default_t(key: &str) -> String {
    key.to_string()
}

fn _default_navigate_back() {}

impl PartialEq for SignupContext {
    fn eq(&self, other: &Self) -> bool {
        // Function pointers (`t`, `navigate_back`) are set once at context
        // creation and never change per-session.  Comparing them by address is
        // unreliable across codegen units, so we treat them as always equal and
        // only diff the meaningful runtime field: `private_key`.
        self.private_key == other.private_key
    }
}

impl Default for SignupContext {
    fn default() -> Self {
        Self {
            private_key: None,
            t: _default_t,
            navigate_back: _default_navigate_back,
        }
    }
}

/// Returned by a signup page component when authentication succeeds.
///
/// The host receives this via the `on_complete` callback, wraps `backend`
/// in `Arc<tokio::sync::RwLock<...>>`, commits it to `ClientManager` and
/// `ChatData`, then navigates to the new account's home.
pub struct SignupCompleted {
    /// The authenticated session returned by the backend.
    pub session: Session,
    /// The authenticated backend, ready to serve requests.
    ///
    /// The host wraps this in `BackendHandle = Arc<RwLock<Box<dyn ClientBackend>>>`.
    pub backend: Box<dyn ClientBackend + Send + Sync>,
    /// OAuth2 refresh token (backends that issue one set this). Persisted with
    /// the `AccountToken` so silent reauth survives restarts.
    pub refresh_token: Option<String>,
    /// RFC3339 UTC timestamp at which the access token expires.
    pub token_expires_at: Option<String>,
    /// Space-separated OAuth scopes the token was granted.
    pub scope: Option<String>,
}

impl SignupCompleted {
    /// Build a legacy-shaped completion (no OAuth metadata) — most signup
    /// flows (Bearer tokens, email+password against test servers) use this.
    pub fn new(session: Session, backend: Box<dyn ClientBackend + Send + Sync>) -> Self {
        Self {
            session,
            backend,
            refresh_token: None,
            token_expires_at: None,
            scope: None,
        }
    }
}

/// Type alias for the boxed-future authenticate fn stored in a `TestAccountEntry`.
///
/// Takes (base_url, username_or_token, password_or_empty) and returns a
/// pinned future. Each plugin implements this with their own auth logic.
pub type TestAuthFn = fn(
    String,
    String,
    String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SignupCompleted, String>>>>;

/// A single pre-configured test account for local development.
///
/// Registered by each native plugin via [`ClientManager::register_test_account`].
/// The Test Accounts panel reads these at runtime — core has no compile-time
/// knowledge of which plugins provide test accounts.
#[derive(Clone, Copy, Debug)]
pub struct TestAccountEntry {
    /// Animal emoji icon (e.g. "🦉").
    pub icon: &'static str,
    /// Display name shown in the card (e.g. "Owl").
    pub label: &'static str,
    /// Backend/server description shown as subtitle (e.g. "Matrix — localhost:9100").
    pub server_label: &'static str,
    /// Base URL of the test server.
    pub base_url: &'static str,
    /// Username, email, or token (first credential).
    pub username: &'static str,
    /// Password or empty string for token-only backends.
    pub password: &'static str,
    /// Backend slug (e.g. "discord", "matrix") — matches the plugin's
    /// `BACKEND_SLUG`. Used to synthesize an offline `Session` when
    /// auto-signin fails (server unreachable), so the account still
    /// appears in the sidebar.
    pub backend_slug: &'static str,
    /// Async auth function — wraps the plugin's actual auth, returns `SignupCompleted`.
    pub authenticate: TestAuthFn,
}
