//! # poly-stoat
//!
//! Stoat (formerly Revolt) messenger client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using the Stoat REST and
//! WebSocket APIs from `developers.stoat.chat`.
//!
//! This crate is included in `poly-core` when the `stoat` feature is enabled.

// TODO(phase-3.1): Implement Stoat client

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::*;
use std::pin::Pin;

/// Stoat (Revolt) messenger client.
pub struct StoatClient {
    // TODO(phase-3.1): Add REST client, WS connection, session state
}

impl StoatClient {
    /// Create a new Stoat client.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for StoatClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClientBackend for StoatClient {
    async fn authenticate(&mut self, _credentials: AuthCredentials) -> ClientResult<Session> {
        Err(ClientError::Internal(
            "Stoat client not yet implemented".into(),
        ))
    }

    async fn logout(&mut self) -> ClientResult<()> {
        Err(ClientError::Internal(
            "Stoat client not yet implemented".into(),
        ))
    }

    fn is_authenticated(&self) -> bool {
        false
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Ok(vec![])
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        Err(ClientError::NotFound(format!("Server {id}")))
    }

    async fn get_channels(&self, _server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(vec![])
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        Err(ClientError::NotFound(format!("Channel {id}")))
    }

    async fn send_message(
        &self,
        _channel_id: &str,
        _content: MessageContent,
    ) -> ClientResult<Message> {
        Err(ClientError::Internal(
            "Stoat client not yet implemented".into(),
        ))
    }

    async fn get_messages(
        &self,
        _channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Ok(vec![])
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        Err(ClientError::NotFound(format!("User {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(vec![])
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(stream::empty())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Stoat
    }

    fn backend_name(&self) -> &str {
        "Stoat"
    }
}
