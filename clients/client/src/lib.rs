//! # poly-client
//!
//! Shared messenger client trait and data types for Poly.
//!
//! This crate defines the [`ClientBackend`] trait that all messenger backend
//! implementations (Stoat, Matrix, Discord, Teams, Demo) must implement.
//! It also defines the shared data types used across all backends.

pub mod code_repo;
pub mod content_policy;
pub mod dms_and_groups;
pub mod events;
pub mod forum;
pub mod messaging;
pub mod moderation;
pub mod social_graph;
pub mod threads;
pub mod types;
pub mod ui_surface;

pub use code_repo::CodeRepoBackend;
pub use content_policy::ContentPolicyBackend;
pub use dms_and_groups::DmsAndGroupsBackend;
pub use events::*;
pub use forum::ForumBackend;
pub use messaging::MessagingBackend;
pub use moderation::ModerationBackend;
pub use social_graph::SocialGraphBackend;
pub use threads::ThreadsBackend;
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

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────
    // send_typing, send_reply_message, search_messages, get_pinned_messages,
    // set_message_pinned, get_channel_commands, get_available_emojis,
    // get_available_stickers → see clients/client/src/messaging.rs

    /// Get messages from a channel with query options.
    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>>;

    // --- Users (get_user + get_friends moved to SocialGraphBackend — H.3.b) ---

    /// Get members of a channel.
    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>>;

    // --- Groups (multi-user DMs) ---

    // --- Groups and DMs (H.3.c — moved to DmsAndGroupsBackend) ---

    // --- Notifications ---

    /// Get the user's notifications.
    async fn get_notifications(&self) -> ClientResult<Vec<Notification>>;

    // --- Social graph methods moved to SocialGraphBackend (H.3.b) ---
    // (respond_to_friend_request, unblock_user, block_user, ignore_user,
    //  unignore_user, add_friend, remove_friend, set_friend_nickname,
    //  set_user_note — all 9 moved to SocialGraphBackend)

    /// Accept or decline a pending server invite.
    ///
    /// Backends that do not support this action return `NotSupported`.
    async fn respond_to_server_invite(&self, server_id: &str, accept: bool) -> ClientResult<()> {
        let _ = (server_id, accept);
        Err(ClientError::NotSupported(
            "respond_to_server_invite".to_string(),
        ))
    }

    /// Mark a channel (server channel or DM) as read on the backend.
    ///
    /// Hosts call this alongside the local `mark_channel_as_read` so that
    /// the next `get_channels` / `get_dm_channels` refetch returns
    /// `unread_count = 0` for the channel — without it, the local clear is
    /// overwritten by the backend's stale unread snapshot.
    async fn mark_channel_read(&self, channel_id: &str) -> ClientResult<()> {
        let _ = channel_id;
        Err(ClientError::NotSupported("mark_channel_read".to_string()))
    }

    // --- Conversation lifecycle (H.3.c — moved to DmsAndGroupsBackend) ---

    /// Send a server invite to a specific user (DM-style invite).
    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()> {
        let _ = (server_id, user_id);
        Err(ClientError::NotSupported("invite_user_to_server".to_string()))
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

    // --- Presence (get_presence + set_presence moved to SocialGraphBackend — H.3.b) ---

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

    // --- Discover Communities (Phase E) ---

    /// Search for communities / subreddits matching `query`.
    ///
    /// `scope` is only meaningful for backends with
    /// [`CommunitySearchSupport::SubscribedLocalAll`] (Lemmy). Reddit ignores
    /// the scope and always searches across all of Reddit. `cursor` is the
    /// opaque pagination token returned by the previous call's
    /// `CommunityPage::next_cursor`; pass `None` for the first page.
    ///
    /// Default impl returns [`ClientError::NotSupported`]; Lemmy and Reddit
    /// override this.
    async fn search_communities(
        &self,
        query: &str,
        scope: CommunityScope,
        cursor: Option<String>,
    ) -> ClientResult<CommunityPage> {
        let _ = (query, scope, cursor);
        Err(ClientError::NotSupported("search_communities".to_string()))
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

    // --- Signup-link surface (plan-client-signup-link-surface Phase A) --------

    /// How this backend exposes account signup to users.
    ///
    /// `server_url` is passed for custom-server backends (Matrix, Stoat,
    /// Lemmy, Forgejo, GitHub Enterprise) so the signup URL can be
    /// parameterised. Hardcoded backends (Discord, Teams, …) ignore it.
    ///
    /// Default returns [`SignupMethod::NotSupported`]. Phase B overrides this
    /// per backend in each `clients/<backend>/src/lib.rs`.
    ///
    /// Sync — signup URL is static metadata; no I/O required.
    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        SignupMethod::NotSupported
    }

    // --- Client-config surface (plan-client-version-override-and-sandbox A) --

    /// Return the version string the plugin will advertise on the next
    /// outbound request.
    ///
    /// With no override set this returns `"poly/0.0.0"`. Phase B replaces
    /// this with a per-backend constant + host-stored override merge.
    ///
    /// Sync — version is an in-memory value; no I/O required.
    fn client_version(&self) -> String {
        "poly/0.0.0".to_string()
    }

    /// Set or clear the version override. `None` clears.
    ///
    /// Default returns `Err(NotSupported)` — Phase B wires this through
    /// `host-api.storage-set` in each backend.
    async fn set_client_version_override(
        &self,
        _override: Option<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_client_version_override".to_string(),
        ))
    }

    /// Return the full mechanism inventory for this backend.
    ///
    /// Empty list is legal (most backends in v1 have no mechanisms).
    /// Default returns `Ok(vec![])`.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        Ok(vec![])
    }

    /// Toggle one mechanism on or off by ID.
    ///
    /// Default returns `Err(NotSupported)` — Phase B wires this through
    /// `host-api.storage-set` in backends that declare mechanisms.
    async fn set_client_mechanism(
        &self,
        _id: &str,
        _enabled: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_client_mechanism".to_string(),
        ))
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

    // --- Code repository channels (H.2.a — CodeRepoBackend) ---

    /// Returns `Some(self)` if this backend implements [`CodeRepoBackend`].
    ///
    /// Override to `Some(self)` in backends that expose code-repository
    /// channels (`ChannelType::Code`).  Default: `None`.
    fn as_code_repo(&self) -> Option<&dyn CodeRepoBackend> {
        None
    }

    // --- Forum channels (H.2.b — ForumBackend) ---

    /// Returns `Some(self)` if this backend implements [`ForumBackend`].
    ///
    /// Override to `Some(self)` in backends that expose forum channels
    /// (`ChannelType::Forum`).  Default: `None`.
    fn as_forum(&self) -> Option<&dyn ForumBackend> {
        None
    }

    // --- Thread channels (H.2.c — ThreadsBackend) ---

    /// Returns `Some(self)` if this backend implements [`ThreadsBackend`].
    ///
    /// Override to `Some(self)` in backends that expose Discord-style thread
    /// channels (`ChannelType::Thread`).  Default: `None`.
    fn as_threads(&self) -> Option<&dyn ThreadsBackend> {
        None
    }

    // --- Moderation (H.3.a — ModerationBackend) ---

    /// Returns `Some(self)` if this backend implements [`ModerationBackend`].
    ///
    /// Override to `Some(self)` in backends that support server moderation
    /// (kick, ban, timeout, delete messages, roles, etc.).  Default: `None`.
    fn as_moderation(&self) -> Option<&dyn ModerationBackend> {
        None
    }

    // --- Social graph (H.3.b — SocialGraphBackend) ---

    /// Returns `Some(self)` if this backend implements [`SocialGraphBackend`].
    ///
    /// Override to `Some(self)` in backends that expose friend lists, presence,
    /// block/ignore, and user lookups.  Default: `None`.
    fn as_social_graph(&self) -> Option<&dyn SocialGraphBackend> {
        None
    }

    // --- DMs and groups (H.3.c — DmsAndGroupsBackend) ---

    /// Returns `Some(self)` if this backend implements [`DmsAndGroupsBackend`].
    ///
    /// Override to `Some(self)` in backends that expose direct messaging
    /// and group DM operations.  Default: `None`.
    fn as_dms_and_groups(&self) -> Option<&dyn DmsAndGroupsBackend> {
        None
    }

    // --- Messaging extras (H.4.a — MessagingBackend) ---

    /// Returns `Some(self)` if this backend implements [`MessagingBackend`].
    ///
    /// Override to `Some(self)` in backends that support typing indicators,
    /// reply threading, message search, pin management, and composer extras.
    /// Default: `None`.
    fn as_messaging(&self) -> Option<&dyn MessagingBackend> {
        None
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

    /// Phase D.3 of plan-solid-refactor-survey — backends with a
    /// `SettingsStorageCell` field override this to return a reference
    /// to it. The default returns a static empty cell so backends that
    /// genuinely have no settings (read-only feeds) accept the default.
    /// `get_setting_value` + `set_setting_value` then have working
    /// default impls that delegate to this accessor — eliminating ~12
    /// lines of identical boilerplate from each plugin.
    fn settings_storage(&self) -> &SettingsStorageCell {
        static EMPTY: std::sync::OnceLock<SettingsStorageCell> = std::sync::OnceLock::new();
        EMPTY.get_or_init(SettingsStorageCell::new)
    }

    /// D15 — read a JSON-encoded setting value from the plugin.
    ///
    /// Default impl: reads from `self.settings_storage()` and falls
    /// back to the declared `default_value` from `get_settings_sections`.
    /// Backends that need custom lookup logic (e.g. cross-scope coalescing)
    /// can override.
    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        if let Some(v) = self.settings_storage().get(scope, scope_id, key) {
            return Ok(v);
        }
        for section in self.get_settings_sections().await? {
            for field in section.fields {
                if field.key == key {
                    return Ok(field.default_value);
                }
            }
        }
        Err(ClientError::NotFound(format!("setting: {key}")))
    }

    /// D15 — write a JSON-encoded setting value via the plugin.
    ///
    /// Default impl: writes to `self.settings_storage()`. Override only
    /// for backends that need to push the change to a remote service.
    async fn set_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()> {
        self.settings_storage().set(scope, scope_id, key, value)
    }

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

// ── IsBackend — thin parent trait (Phase H) ──────────────────────────────────
//
// `IsBackend` is the future replacement for `ClientBackend` as the storage
// type (`Box<dyn IsBackend>`).  Right now it sits alongside `ClientBackend`
// and is implemented for free via the blanket impl below.  Capability
// sub-traits (`ContentPolicyBackend`, `CodeRepoBackend`, `ForumBackend`,
// `ThreadsBackend`, …) are being added in H.1-H.3; capability accessor
// methods (`as_content_policy`, `as_code_repo`, `as_forum`, `as_threads`, …)
// are added to `IsBackend` at the same time.
//
// H.0 defines only the universal surface — the methods every single backend
// has in common with no opt-out.

/// Thin parent trait shared by every Poly backend.
///
/// Every type that implements [`ClientBackend`] automatically gets
/// `IsBackend` via the blanket impl below — no code changes required in
/// individual backend crates.
///
/// This is the long-horizon storage type: after Phase H.4 ships,
/// `Box<dyn IsBackend>` replaces `Box<dyn ClientBackend>` everywhere.
/// For now, the trait is additive and the migration is H.1+.
///
/// # Capability accessors
///
/// Each accessor returns `None` by default.  A backend opts in to a
/// capability by implementing the corresponding sub-trait *and* overriding
/// the accessor to return `Some(self)`.
///
/// | accessor | sub-trait | phase |
/// |---|---|---|
/// | `as_content_policy` | [`ContentPolicyBackend`] | H.1 |
/// | `as_code_repo` | [`CodeRepoBackend`] | H.2.a |
/// | `as_forum` | [`ForumBackend`] | H.2.b |
/// | `as_threads` | [`ThreadsBackend`] | H.2.c |
/// | `as_moderation` | [`ModerationBackend`] | H.3.a |
/// | `as_social_graph` | [`SocialGraphBackend`] | H.3.b |
/// | `as_dms_and_groups` | [`DmsAndGroupsBackend`] | H.3.c |
/// | `as_messaging` | [`MessagingBackend`] | H.4.a |
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait IsBackend: Send + Sync {
    /// The type identifier (slug) for this backend.
    ///
    /// Returns the same value as [`ClientBackend::backend_type`].  Use
    /// `.slug()` on the returned `BackendType` to get the string slug
    /// (e.g. `"discord"`, `"matrix"`).
    fn backend_type(&self) -> BackendType;

    /// Runtime capability flags for this backend instance.
    ///
    /// The UI uses these flags to hide controls that don't apply (e.g. mic /
    /// speaker buttons for read-only news feeds).
    fn backend_capabilities(&self) -> BackendCapabilities;

    /// The version string the plugin advertises on outbound requests.
    fn client_version(&self) -> String;

    /// Authenticate with the backend using the provided credentials.
    ///
    /// Takes `&mut self` because most backends need to store the resulting
    /// session token in their own fields.
    async fn authenticate(
        &mut self,
        credentials: AuthCredentials,
    ) -> ClientResult<Session>;

    /// Log out and invalidate the current session.
    async fn logout(&mut self) -> ClientResult<()>;

    // --- Capability accessors (H.1+) ---

    /// Returns `Some(self)` if this backend implements [`ContentPolicyBackend`].
    ///
    /// Default: `None` (no backend currently opts in).
    fn as_content_policy(&self) -> Option<&dyn ContentPolicyBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`ModerationBackend`].
    ///
    /// Default: `None`.  Override in backends that support server moderation
    /// (kick, ban, timeout, delete messages, etc.) — currently `poly-discord`,
    /// `poly-matrix`, `poly-stoat`, `poly-lemmy`, `poly-server-client`,
    /// `poly-teams`, and `poly-forgejo`.
    fn as_moderation(&self) -> Option<&dyn ModerationBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`CodeRepoBackend`].
    ///
    /// Default: `None`.  Override in backends that expose code-repository
    /// channels (`ChannelType::Code`) — currently `poly-github` and `poly-forgejo`.
    fn as_code_repo(&self) -> Option<&dyn CodeRepoBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`ForumBackend`].
    ///
    /// Default: `None`.  Override in backends that expose forum channels
    /// (`ChannelType::Forum`) — currently `poly-discord` and `poly-lemmy`.
    fn as_forum(&self) -> Option<&dyn ForumBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`ThreadsBackend`].
    ///
    /// Default: `None`.  Override in backends that expose Discord-style thread
    /// channels (`ChannelType::Thread`) — currently `poly-discord` and WIT plugins.
    fn as_threads(&self) -> Option<&dyn ThreadsBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`SocialGraphBackend`].
    ///
    /// Default: `None`.  Override in backends that expose friend lists, presence,
    /// block/ignore, and user lookups — currently `poly-demo`, `poly-discord`,
    /// `poly-matrix`, `poly-server-client`, `poly-stoat`.
    fn as_social_graph(&self) -> Option<&dyn SocialGraphBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`DmsAndGroupsBackend`].
    ///
    /// Default: `None`.  Override in backends that expose DM channels and
    /// group DM operations.
    fn as_dms_and_groups(&self) -> Option<&dyn DmsAndGroupsBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`MessagingBackend`].
    ///
    /// Default: `None`.  Override in backends that support messaging extras:
    /// typing indicators, reply threading, message search, pin management,
    /// and composer extras (commands, emojis, stickers).
    /// Currently: `poly-demo`, `poly-discord`, `poly-matrix`, `poly-stoat`,
    /// `poly-teams`, `poly-server-client`, `poly-lemmy`.
    fn as_messaging(&self) -> Option<&dyn MessagingBackend> {
        None
    }
}

// Blanket implementation: every `ClientBackend` automatically is an `IsBackend`.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<T: ClientBackend + ?Sized> IsBackend for T {
    #[inline]
    fn backend_type(&self) -> BackendType {
        ClientBackend::backend_type(self)
    }

    #[inline]
    fn backend_capabilities(&self) -> BackendCapabilities {
        ClientBackend::backend_capabilities(self)
    }

    #[inline]
    fn client_version(&self) -> String {
        ClientBackend::client_version(self)
    }

    #[inline]
    async fn authenticate(
        &mut self,
        credentials: AuthCredentials,
    ) -> ClientResult<Session> {
        ClientBackend::authenticate(self, credentials).await
    }

    #[inline]
    async fn logout(&mut self) -> ClientResult<()> {
        ClientBackend::logout(self).await
    }

    #[inline]
    fn as_moderation(&self) -> Option<&dyn ModerationBackend> {
        ClientBackend::as_moderation(self)
    }

    #[inline]
    fn as_code_repo(&self) -> Option<&dyn CodeRepoBackend> {
        ClientBackend::as_code_repo(self)
    }

    #[inline]
    fn as_forum(&self) -> Option<&dyn ForumBackend> {
        ClientBackend::as_forum(self)
    }

    #[inline]
    fn as_threads(&self) -> Option<&dyn ThreadsBackend> {
        ClientBackend::as_threads(self)
    }

    #[inline]
    fn as_social_graph(&self) -> Option<&dyn SocialGraphBackend> {
        ClientBackend::as_social_graph(self)
    }

    #[inline]
    fn as_dms_and_groups(&self) -> Option<&dyn DmsAndGroupsBackend> {
        ClientBackend::as_dms_and_groups(self)
    }

    #[inline]
    fn as_messaging(&self) -> Option<&dyn MessagingBackend> {
        ClientBackend::as_messaging(self)
    }
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
    #[must_use]
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
