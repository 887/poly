//! # poly-client
//!
//! Shared messenger client trait and data types for Poly.
//!
//! This crate defines the [`ClientBackend`] trait that all messenger backend
//! implementations (Stoat, Matrix, Discord, Teams, Demo) must implement.
//! It also defines the shared data types used across all backends.

pub mod events;
pub mod types;

pub use events::*;
pub use types::*;

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

    // --- Real-time events ---

    /// Get a stream of real-time events from the backend.
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>>;

    // --- Backend info ---

    /// The type of backend this client connects to.
    fn backend_type(&self) -> BackendType;

    /// Human-readable name for this backend.
    fn backend_name(&self) -> &str;
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
}

/// Type alias for the boxed-future authenticate fn stored in a `TestAccountEntry`.
///
/// Takes (base_url, username_or_token, password_or_empty) and returns a
/// pinned future. Each plugin implements this with their own auth logic.
pub type TestAuthFn = fn(
    String,
    String,
    String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<SignupCompleted, String>> + Send>,
>;

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
    /// Async auth function — wraps the plugin's actual auth, returns `SignupCompleted`.
    pub authenticate: TestAuthFn,
}
