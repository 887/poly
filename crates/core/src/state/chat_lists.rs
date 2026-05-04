//! `ChatLists` — reactive store for the sidebar list data.
//!
//! Holds the collections populated from all active backends:
//! servers, channels, DMs, groups, friends, notifications.
//!
//! Provided as `BatchedSignal<ChatLists>` at the `App` level
//! (Phase G.6 of plan-solid-refactor-survey.md).
//!
//! ## By-id shadows
//!
//! `servers_by_id`, `channels_by_id`, `dm_channels_by_id`, and
//! `groups_by_id` are index maps into the canonical Vecs. They MUST
//! stay in sync — always use the invariant-preserving setters below
//! (`set_servers`, `push_server`, etc.) instead of writing directly
//! into the Vec.

use poly_client::{Channel, DmChannel, Group, Notification, Server, User};
use std::collections::HashMap;

/// Reactive store for sidebar list collections.
///
/// Components that only render the server/channel/DM lists subscribe
/// to this signal and are not re-rendered when messages or account
/// preferences change.
#[derive(Debug, Clone, Default)]
pub struct ChatLists {
    /// All favorited/joined servers from all backends.
    pub servers: Vec<Server>,
    /// Channels for the currently selected server.
    pub channels: Vec<Channel>,
    /// DM channels from all backends.
    pub dm_channels: Vec<DmChannel>,
    /// Group chats from all backends.
    pub groups: Vec<Group>,
    /// Friends per account (account_id → friends list).
    pub friends: HashMap<String, Vec<User>>,
    /// Aggregated notifications from all backends.
    pub notifications: Vec<Notification>,

    // --- by-id shadows (filled atomically with their canonical Vec) ---
    /// Index shadow for `servers` — maps `server.id` → index in `self.servers`.
    pub servers_by_id: HashMap<String, usize>,
    /// Index shadow for `channels` — maps `channel.id` → index in `self.channels`.
    pub channels_by_id: HashMap<String, usize>,
    /// Index shadow for `dm_channels` — maps `dm.id` → index in `self.dm_channels`.
    pub dm_channels_by_id: HashMap<String, usize>,
    /// Index shadow for `groups` — maps `group.id` → index in `self.groups`.
    pub groups_by_id: HashMap<String, usize>,
}

impl ChatLists {
    // -------------------------------------------------------------------------
    // Invariant-preserving setters — ALWAYS use these instead of direct Vec writes
    // -------------------------------------------------------------------------

    /// Replace the entire servers list and rebuild the by-id index atomically.
    pub fn set_servers(&mut self, servers: Vec<Server>) {
        self.servers_by_id = servers
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();
        self.servers = servers;
    }

    /// Append one server and update the by-id index.
    pub fn push_server(&mut self, server: Server) {
        self.servers_by_id
            .insert(server.id.clone(), self.servers.len());
        self.servers.push(server);
    }

    /// Look up a server by its ID in O(1).
    #[must_use]
    pub fn server_by_id(&self, id: &str) -> Option<&Server> {
        self.servers_by_id.get(id).and_then(|i| self.servers.get(*i))
    }

    /// Mutable look up a server by its ID in O(1).
    #[must_use]
    pub fn server_by_id_mut(&mut self, id: &str) -> Option<&mut Server> {
        self.servers_by_id
            .get(id)
            .copied()
            .and_then(|i| self.servers.get_mut(i))
    }

    /// Replace the entire channels list and rebuild the by-id index atomically.
    pub fn set_channels(&mut self, channels: Vec<Channel>) {
        self.channels_by_id = channels
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), i))
            .collect();
        self.channels = channels;
    }

    /// Look up a channel by its ID in O(1).
    #[must_use]
    pub fn channel_by_id(&self, id: &str) -> Option<&Channel> {
        self.channels_by_id
            .get(id)
            .and_then(|i| self.channels.get(*i))
    }

    /// Replace the entire dm_channels list and rebuild the by-id index atomically.
    pub fn set_dm_channels(&mut self, dm_channels: Vec<DmChannel>) {
        self.dm_channels_by_id = dm_channels
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), i))
            .collect();
        self.dm_channels = dm_channels;
    }

    /// Look up a DM channel by its ID in O(1).
    #[must_use]
    pub fn dm_channel_by_id(&self, id: &str) -> Option<&DmChannel> {
        self.dm_channels_by_id
            .get(id)
            .and_then(|i| self.dm_channels.get(*i))
    }

    /// Replace the entire groups list and rebuild the by-id index atomically.
    pub fn set_groups(&mut self, groups: Vec<Group>) {
        self.groups_by_id = groups
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id.clone(), i))
            .collect();
        self.groups = groups;
    }

    /// Look up a group by its ID in O(1).
    #[must_use]
    pub fn group_by_id(&self, id: &str) -> Option<&Group> {
        self.groups_by_id
            .get(id)
            .and_then(|i| self.groups.get(*i))
    }
}
