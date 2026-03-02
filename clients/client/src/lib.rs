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
}

/// Result type for client backend operations.
pub type ClientResult<T> = Result<T, ClientError>;

/// The core trait that all messenger backend clients must implement.
///
/// Each backend (Stoat, Matrix, Discord, Teams, Demo) implements this trait
/// to provide a unified API for the Poly UI layer.
// DECISION(D12): Demo client implements this trait for Phase 2 UI testing.
#[async_trait]
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

    /// Get messages from a channel with query options.
    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>>;

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

    // --- Direct Messages ---

    /// Get all DM channels.
    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>>;

    // --- Notifications ---

    /// Get the user's notifications.
    async fn get_notifications(&self) -> ClientResult<Vec<Notification>>;

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

    // --- Real-time events ---

    /// Get a stream of real-time events from the backend.
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>>;

    // --- Backend info ---

    /// The type of backend this client connects to.
    fn backend_type(&self) -> BackendType;

    /// Human-readable name for this backend.
    fn backend_name(&self) -> &str;
}
