//! # poly-client
//!
//! Shared messenger client trait and data types for Poly.
//!
//! This crate defines the [`ClientBackend`] trait that all messenger backend
//! implementations (Stoat, Matrix, Discord, Teams, Demo) must implement.
//! It also defines the shared data types used across all backends.

pub mod code_repo;
pub mod content_policy;
pub mod context_action;
pub mod dms_and_groups;
pub mod events;
pub mod forum;
pub mod discover;
pub mod messaging;
pub mod moderation;
pub mod server_admin;
pub mod settings;
pub mod social_graph;
pub mod threads;
pub mod types;
pub mod ui_surface;
pub mod view_descriptor;
pub mod voice_transport;
pub mod writable_messaging;

pub use code_repo::CodeRepoBackend;
pub use content_policy::ContentPolicyBackend;
pub use context_action::ContextActionBackend;
pub use dms_and_groups::DmsAndGroupsBackend;
pub use events::*;
pub use forum::ForumBackend;
pub use discover::DiscoverBackend;
pub use messaging::MessagingBackend;
pub use server_admin::ServerAdminBackend;
pub use settings::SettingsBackend;
pub use moderation::ModerationBackend;
pub use social_graph::SocialGraphBackend;
pub use threads::ThreadsBackend;
pub use types::*;
pub use ui_surface::*;
pub use view_descriptor::ViewDescriptorBackend;
pub use voice_transport::VoiceTransportBackend;
pub use writable_messaging::WritableMessagingBackend;

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

// ── IsBackend — unified backend trait (Phase H — H.4.f: ClientBackend deleted) ──
//
// `IsBackend` replaced the old `ClientBackend` god-trait in Phase H.4.
// Universal methods have sensible default impls; capability sub-traits
// (`MessagingBackend`, `ServerAdminBackend`, `DiscoverBackend`, etc.)
// carry the opt-in surface via `as_messaging()` / `as_server_admin()` etc.
// `Box<dyn IsBackend>` is the storage type throughout poly-core.

/// Unified trait shared by every Poly backend.
///
/// Every backend crate implements this trait directly.
/// `Box<dyn IsBackend>` is the storage type in [`ClientManager`].
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
/// | `as_server_admin` | [`ServerAdminBackend`] | H.4.b |
/// | `as_discover` | [`DiscoverBackend`] | H.4.c |
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait IsBackend: Send + Sync {
    /// The type identifier (slug) for this backend.
    ///
    /// Use `.slug()` on the returned `BackendType` to get the string slug
    /// (e.g. `"discord"`, `"matrix"`).
    fn backend_type(&self) -> BackendType;

    /// Runtime capability flags for this backend instance.
    ///
    /// The UI uses these flags to hide controls that don't apply (e.g. mic /
    /// speaker buttons for read-only news feeds).
    /// Default: [`BackendCapabilities::READ_ONLY_FEED`].
    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::READ_ONLY_FEED
    }

    /// Human-readable display name for this backend instance.
    ///
    /// Shown in account headings, settings, and debug logs.
    fn backend_name(&self) -> &str;

    /// The version string the plugin advertises on outbound requests.
    ///
    /// Default: `"poly/0.0.0"`.
    fn client_version(&self) -> String {
        "poly/0.0.0".to_string()
    }

    /// Check if the client currently holds a valid authenticated session.
    ///
    /// Default: `false`.
    fn is_authenticated(&self) -> bool {
        false
    }

    /// How this backend exposes account signup to users.
    ///
    /// `server_url` is passed for custom-server backends (Matrix, Stoat,
    /// Lemmy, Forgejo, GitHub Enterprise) so the signup URL can be
    /// parameterised. Hardcoded backends (Discord, Teams, …) ignore it.
    ///
    /// Default: [`SignupMethod::NotSupported`].
    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        SignupMethod::NotSupported
    }

    /// Set or clear the version override. `None` clears.
    ///
    /// Default: `Err(NotSupported)`.
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
    /// Empty list is legal (most backends have no mechanisms).
    /// Default: `Ok(vec![])`.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        Ok(vec![])
    }

    /// Toggle one mechanism on or off by ID.
    ///
    /// Default: `Err(NotSupported)`.
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
    /// `get-plugin-manifest` function.
    ///
    /// Default: [`PluginManifest::default()`].
    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest::default()
    }

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

    /// Returns `Some(self)` if this backend implements
    /// [`WritableMessagingBackend`] (i.e. accepts outbound
    /// `send_message` calls on at least some channels).
    ///
    /// Default: `None`.  Override in writable backends.  Read-only
    /// feeds (`poly-forgejo`, future read-only news backends) leave
    /// this as `None` and `send_message` returns `NotSupported`.
    ///
    /// Plan: `plan-trait-split-readable-vs-writable.md` Phase B.2.
    fn as_writable_messaging(&self) -> Option<&dyn WritableMessagingBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`ServerAdminBackend`].
    ///
    /// Default: `None`.  Override in backends that support server management
    /// (create/modify servers and channels, mark-read, invite).
    /// Currently: `poly-demo`, `poly-discord`, `poly-lemmy`, `poly-matrix`,
    /// `poly-server-client`.
    fn as_server_admin(&self) -> Option<&dyn ServerAdminBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`DiscoverBackend`].
    ///
    /// Default: `None`.  Override in backends that support community search.
    /// Currently: `poly-lemmy`, `poly-reddit`.
    fn as_discover(&self) -> Option<&dyn DiscoverBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`VoiceTransportBackend`].
    ///
    /// Default: `None`.  Override in backends that carry voice / DM-call
    /// transport (Discord gateway op 4 / op 13, Stoat WS, Matrix RTC).
    /// Phase C.1 — ISP split.
    fn as_voice_transport(&self) -> Option<&dyn VoiceTransportBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`SettingsBackend`].
    ///
    /// Default: `None`.  Override in backends that declare their own
    /// settings sections / persistent settings cells.
    /// Phase C.1 — ISP split.
    fn as_settings(&self) -> Option<&dyn SettingsBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`ViewDescriptorBackend`].
    ///
    /// Default: `None`.  Override in backends that drive the
    /// plugin-controlled UI surface (sidebar, account overview, channel
    /// views, paged data feeds).
    /// Phase C.1 — ISP split.
    fn as_view_descriptor(&self) -> Option<&dyn ViewDescriptorBackend> {
        None
    }

    /// Returns `Some(self)` if this backend implements [`ContextActionBackend`].
    ///
    /// Default: `None`.  Override in backends that contribute context-menu
    /// items, composer buttons, or per-message actions.
    /// Phase C.1 — ISP split.
    fn as_context_action(&self) -> Option<&dyn ContextActionBackend> {
        None
    }

    // --- Servers / Communities (H.4.e) ---

    /// Get all servers/communities the user has joined.
    ///
    /// Default: `Ok(vec![])` — backends with no server concept return empty.
    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Ok(vec![])
    }

    /// Get a specific server by ID.
    ///
    /// Default: `Err(NotFound)`.
    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        Err(ClientError::NotFound(format!("server: {id}")))
    }

    // --- Channels (H.4.e) ---

    /// Get all channels in a server.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_channels(&self, _server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(vec![])
    }

    /// Get a specific channel by ID.
    ///
    /// Default: `Err(NotFound)`.
    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        Err(ClientError::NotFound(format!("channel: {id}")))
    }

    // --- Messages (H.4.e) ---

    /// Send a message to a channel.
    ///
    /// Plan-trait-split: this method is now a default-delegating shim
    /// that consults [`Self::as_writable_messaging`] and forwards to
    /// [`WritableMessagingBackend::send_message`] when `Some`,
    /// otherwise returns `Err(NotSupported)`.  Existing call sites
    /// (`crates/core/`, `mcp/chat-mcp/`) continue to compile through
    /// this shim; new code should prefer the capability-dispatch form
    /// `if let Some(wm) = backend.as_writable_messaging() { ... }`
    /// for clearer error UX.
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        match self.as_writable_messaging() {
            Some(wm) => wm.send_message(channel_id, content).await,
            None => Err(ClientError::NotSupported("send_message".to_string())),
        }
    }

    /// Get messages from a channel with query options.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_messages(
        &self,
        _channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Ok(vec![])
    }

    // --- Users / Members (H.4.e) ---

    /// Get members of a channel.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    // --- Notifications (H.4.e) ---

    /// Get the user's notifications.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    // --- Voice / Video (H.4.e) ---

    /// Get the current voice participants in a voice or video channel.
    ///
    /// Phase C.1 — default delegates to [`Self::as_voice_transport`] when
    /// `Some`, otherwise returns `Ok(vec![])`.
    async fn get_voice_participants(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        match self.as_voice_transport() {
            Some(vt) => vt.get_voice_participants(channel_id).await,
            None => Ok(vec![]),
        }
    }

    /// C.1 — Signal the backend that the local user is joining a voice channel.
    ///
    /// Phase C.1 — default delegates to [`Self::as_voice_transport`] when
    /// `Some`, otherwise `Ok(())` (pseudo-backend fallback).
    async fn join_voice_channel_transport(
        &self,
        server_id: &str,
        channel_id: &str,
    ) -> ClientResult<()> {
        match self.as_voice_transport() {
            Some(vt) => vt.join_voice_channel_transport(server_id, channel_id).await,
            None => Ok(()),
        }
    }

    /// D.2 / D.5 — Initiate a DM call via backend transport (real signaling).
    ///
    /// Phase C.1 — default delegates to [`Self::as_voice_transport`] when
    /// `Some`, otherwise `Err(NotSupported)`.
    async fn start_dm_call_transport(&self, dm_channel_id: &str) -> ClientResult<()> {
        match self.as_voice_transport() {
            Some(vt) => vt.start_dm_call_transport(dm_channel_id).await,
            None => Err(ClientError::NotSupported("start_dm_call_transport".into())),
        }
    }

    /// C.5 — Toggle the local user's mute / deafen state on the backend.
    ///
    /// Phase C.1 — default delegates to [`Self::as_voice_transport`] when
    /// `Some`, otherwise `Ok(())` (pseudo-backend fallback).
    async fn set_voice_mute(
        &self,
        server_id: &str,
        channel_id: &str,
        self_mute: bool,
        self_deaf: bool,
    ) -> ClientResult<()> {
        match self.as_voice_transport() {
            Some(vt) => {
                vt.set_voice_mute(server_id, channel_id, self_mute, self_deaf)
                    .await
            }
            None => Ok(()),
        }
    }

    // --- Real-time events (H.4.e) ---

    /// Get a stream of real-time events from the backend.
    ///
    /// Default: an empty stream that never yields.
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(futures::stream::empty())
    }

    // --- D9 UI surface methods (H.4.e) ---

    /// D11 — return plugin-declared context menu items for `target`.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Ok(vec![])`.
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match self.as_context_action() {
            Some(ca) => ca.get_context_menu_items(target, target_id).await,
            None => Ok(vec![]),
        }
    }

    /// D14 / D22 — dispatch a plugin action.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Err(NotFound(action_id))`.
    async fn invoke_context_action(
        &self,
        action_id: &str,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match self.as_context_action() {
            Some(ca) => ca.invoke_context_action(action_id, target, target_id).await,
            None => Err(ClientError::NotFound(action_id.to_string())),
        }
    }

    /// D16 — poll a pending async action for its final outcome.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Err(NotSupported)`.
    async fn poll_action(&self, handle: PendingHandle) -> ClientResult<ActionOutcome> {
        match self.as_context_action() {
            Some(ca) => ca.poll_action(handle).await,
            None => Err(ClientError::NotSupported("poll_action".to_string())),
        }
    }

    /// D11 / D18 — every settings section this plugin contributes.
    ///
    /// Phase C.1 — default delegates to [`Self::as_settings`] when `Some`,
    /// otherwise `Ok(vec![])`.
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        match self.as_settings() {
            Some(s) => s.get_settings_sections().await,
            None => Ok(vec![]),
        }
    }

    /// Storage cell for backend-local settings.
    ///
    /// Phase C.1 — default delegates to [`Self::as_settings`] when `Some`,
    /// otherwise returns a static empty cell.
    fn settings_storage(&self) -> &SettingsStorageCell {
        if let Some(s) = self.as_settings() {
            return s.settings_storage();
        }
        static EMPTY: std::sync::OnceLock<SettingsStorageCell> = std::sync::OnceLock::new();
        EMPTY.get_or_init(SettingsStorageCell::new)
    }

    /// D15 — read a JSON-encoded setting value.
    ///
    /// Phase C.1 — default delegates to [`Self::as_settings`] when `Some`
    /// (the sub-trait default reads through `settings_storage`); otherwise
    /// reads from the IsBackend's storage cell with the same fallback chain.
    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        if let Some(s) = self.as_settings() {
            return s.get_setting_value(scope, scope_id, key).await;
        }
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

    /// D15 — write a JSON-encoded setting value.
    ///
    /// Phase C.1 — default delegates to [`Self::as_settings`] when `Some`,
    /// otherwise writes to the IsBackend's storage cell.
    async fn set_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()> {
        if let Some(s) = self.as_settings() {
            return s.set_setting_value(scope, scope_id, key, value).await;
        }
        self.settings_storage().set(scope, scope_id, key, value)
    }

    /// D5 / D19 — plugin's current sidebar declaration.
    ///
    /// Phase C.1 — default delegates to [`Self::as_view_descriptor`] when
    /// `Some`, otherwise returns an empty Custom layout.
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        match self.as_view_descriptor() {
            Some(vd) => vd.get_sidebar_declaration().await,
            None => Ok(SidebarDeclaration {
                layout: SidebarLayoutKind::Custom,
                sections: vec![],
                header_block: None,
            }),
        }
    }

    /// D14 / D25 — dispatch a sidebar-item click.
    ///
    /// Phase C.1 — default delegates to [`Self::as_view_descriptor`] when
    /// `Some`, otherwise `Err(NotFound(action_id))`.
    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        match self.as_view_descriptor() {
            Some(vd) => vd.invoke_sidebar_action(action_id).await,
            None => Err(ClientError::NotFound(action_id.to_string())),
        }
    }

    /// Fetch the account-level overview view descriptor.
    ///
    /// Phase C.1 — default delegates to [`Self::as_view_descriptor`] when
    /// `Some`, otherwise returns the generic CardGrid descriptor.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        match self.as_view_descriptor() {
            Some(vd) => vd.get_account_overview_view().await,
            None => Ok(ViewDescriptor {
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
            }),
        }
    }

    /// D5 — fetch a channel's non-chat view descriptor.
    ///
    /// Phase C.1 — default delegates to [`Self::as_view_descriptor`] when
    /// `Some`, otherwise `Err(NotSupported)`.
    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        match self.as_view_descriptor() {
            Some(vd) => vd.get_channel_view(channel_id).await,
            None => Err(ClientError::NotSupported("get_channel_view".to_string())),
        }
    }

    /// D23 — paged data feed.
    ///
    /// Phase C.1 — default delegates to [`Self::as_view_descriptor`] when
    /// `Some`, otherwise returns an empty page.
    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        match self.as_view_descriptor() {
            Some(vd) => {
                vd.get_view_rows(channel_id, cursor, sort_id, filter_id, tab_id)
                    .await
            }
            None => Ok(ViewRowsPage {
                rows: vec![],
                next_cursor: None,
            }),
        }
    }

    /// D5 — detail payload for `split` views.
    ///
    /// Phase C.1 — default delegates to [`Self::as_view_descriptor`] when
    /// `Some`, otherwise `Err(NotSupported)`.
    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        match self.as_view_descriptor() {
            Some(vd) => vd.get_view_detail(channel_id, row_id).await,
            None => Err(ClientError::NotSupported("get_view_detail".to_string())),
        }
    }

    /// D8 — composer-toolbar buttons for the given channel.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Ok(vec![])`.
    async fn get_composer_buttons(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<ComposerButton>> {
        match self.as_context_action() {
            Some(ca) => ca.get_composer_buttons(channel_id).await,
            None => Ok(vec![]),
        }
    }

    /// D8 — per-message actions, merged into the message hover/overflow menu.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Ok(vec![])`.
    async fn get_message_actions(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match self.as_context_action() {
            Some(ca) => ca.get_message_actions(channel_id, message_id).await,
            None => Ok(vec![]),
        }
    }

    /// D14 / D25 — dispatch a composer button action.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Err(NotFound(action_id))`.
    async fn invoke_composer_action(
        &self,
        action_id: &str,
        channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match self.as_context_action() {
            Some(ca) => ca.invoke_composer_action(action_id, channel_id).await,
            None => Err(ClientError::NotFound(action_id.to_string())),
        }
    }

    /// D14 / D25 — dispatch a per-message action.
    ///
    /// Phase C.1 — default delegates to [`Self::as_context_action`] when
    /// `Some`, otherwise `Err(NotFound(action_id))`.
    async fn invoke_message_action(
        &self,
        action_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match self.as_context_action() {
            Some(ca) => {
                ca.invoke_message_action(action_id, channel_id, message_id)
                    .await
            }
            None => Err(ClientError::NotFound(action_id.to_string())),
        }
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
    /// The host wraps this in `BackendHandle = Arc<RwLock<Box<dyn IsBackend>>>`.
    pub backend: Box<dyn IsBackend>,
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
    pub fn new(session: Session, backend: Box<dyn IsBackend>) -> Self {
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
