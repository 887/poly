//! # poly-demo
//!
//! Demo/mock messenger client for Poly UI testing.
//!
//! Generates fake servers, channels, users, messages, and events
//! so the full UI can be developed and tested without connecting
//! to any real messenger backend.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements [`poly_client::IsBackend`]
//!   for direct linking into `poly-core`. This is the traditional path.
//! - **WASM plugin** (`--no-default-features`, target `wasm32-wasip2`): Exports
//!   the WIT `messenger-client` interface via `wit-bindgen`. Loaded at runtime
//!   by the plugin host in `poly-core`.
//!
//! DECISION(D21): WASM Plugin Backends.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "demo";

/// Public data module — demo data generators for testing.
pub mod data;

/// Per-flavour trait + three marker structs (Demo, DemoChat, DemoForum).
///
/// Only compiled under the `native` feature — the WASM plugin path goes
/// through `guest.rs` / `wit_bindings.rs` instead.
#[cfg(feature = "native")]
pub mod flavour;

/// WASM plugin guest implementation.
///
/// When compiled to `wasm32-wasip2`, this module exports the WIT
/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// `messenger-client` interface using `wit-bindgen`.
/// Only on WASI targets (not `wasm32-unknown-unknown` used by the web frontend).
#[cfg(target_os = "wasi")]
mod guest;

// ─── Native plugin metadata ─────────────────────────────────────────
//
// Mirrors the WIT `plugin-metadata.get-translations` interface for native
// (non-WASM) builds. The plugin-host calls `get-translations(locale)` on
// WASM components at instantiation time; for native backends poly-core calls
// this free function instead. The FTL paths are owned by this crate, not core.

/// Return the raw FTL translation source for the demo plugin.
///
/// Mirrors the WIT `plugin-metadata.get-translations(locale) → string` export.
/// Returns an empty string for unsupported locales so the host falls back to
/// English (the same contract as the WIT interface).
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

// ─── Native ClientBackend implementation ────────────────────────────
// One generic DemoClient<F: DemoFlavour> replaces the former
// DemoClient / DemoClient2 / DemoClient3 triplication (~1 600 LOC → ~400 LOC).
// Type aliases preserve the external names that consumers depend on.

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::Stream;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;

#[cfg(feature = "native")]
use flavour::DemoFlavour;

/// Generic demo backend parameterised over a [`DemoFlavour`].
///
/// All three demo accounts (Cat / Dog / Platypus-Forum) share this single
/// struct + `ClientBackend` impl. Per-flavour variation lives entirely in the
/// `F: DemoFlavour` associated functions so there is no branching inside
/// the methods.
///
/// Consumers should use the concrete type aliases: [`DemoClient`] (Cat),
/// [`DemoClient2`] (Dog), [`DemoClient3`] (Forum).
// DECISION(D12): Demo client created in Phase 2 alongside UI.
// Refactored in Phase C.1 of plan-solid-refactor-survey.md.
#[cfg(feature = "native")]
pub struct DemoClientGeneric<F: DemoFlavour> {
    authenticated: bool,
    session: Option<Session>,
    /// Pack C P18 — in-memory settings storage. Demo backends never persist
    /// across process restarts; this cell gives round-trip semantics within
    /// one session.
    settings_storage: SettingsStorageCell,
    /// Stored version override (None = return "poly-demo/0.0.0").
    version_override: std::sync::Mutex<Option<String>>,
    /// Zero-sized marker for the flavour type.
    _flavour: std::marker::PhantomData<F>,
}

#[cfg(feature = "native")]
impl<F: DemoFlavour> DemoClientGeneric<F> {
    /// Create a new demo client for flavour `F`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            authenticated: false,
            session: None,
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
            _flavour: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "native")]
impl<F: DemoFlavour> Default for DemoClientGeneric<F> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> IsBackend for DemoClientGeneric<F> {
    async fn authenticate(&mut self, _credentials: AuthCredentials) -> ClientResult<Session> {
        let session = F::session();
        self.session = Some(session.clone());
        self.authenticated = true;
        Ok(session)
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        self.authenticated = false;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Ok(F::servers())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        F::servers()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("Server {id}")))
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(data::apply_local_read_state(F::channels(server_id)))
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        for server in F::servers() {
            for channel in F::channels(&server.id) {
                if channel.id == id {
                    return Ok(channel);
                }
            }
        }
        Err(ClientError::NotFound(format!("Channel {id}")))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Ok(F::messages(channel_id, &query))
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    // ── Writable messaging (trait-split — plan-trait-split-readable-vs-writable) ─

    fn as_writable_messaging(&self) -> Option<&dyn poly_client::WritableMessagingBackend> {
        Some(self)
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(F::channel_members(channel_id))
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(F::notifications())
    }

    fn as_voice_transport(&self) -> Option<&dyn poly_client::VoiceTransportBackend> {
        Some(self)
    }

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    fn as_context_action(&self) -> Option<&dyn poly_client::ContextActionBackend> {
        Some(self)
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        F::event_stream()
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from(F::backend_slug())
    }

    fn backend_name(&self) -> &str {
        F::backend_name()
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        F::capabilities()
    }

    // --- C.1 — UI surface / settings / views / context-actions moved below ---

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| "poly-demo/0.0.0".to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        Ok(())
    }

    fn as_server_admin(&self) -> Option<&dyn poly_client::ServerAdminBackend> {
        Some(self)
    }

    fn as_discover(&self) -> Option<&dyn poly_client::DiscoverBackend> {
        Some(self)
    }
}

// ── H.4.b — ServerAdminBackend ────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::ServerAdminBackend for DemoClientGeneric<F> {
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported("demo: create_server".to_string()))
    }

    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported("demo: create_channel".to_string()))
    }

    async fn update_server_banner(
        &self,
        _server_id: &str,
        _banner_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("demo: update_server_banner".to_string()))
    }

    async fn mark_channel_read(&self, channel_id: &str) -> ClientResult<()> {
        data::mark_channel_read_local(channel_id);
        Ok(())
    }

    async fn respond_to_server_invite(&self, _server_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("demo: respond_to_server_invite".to_string()))
    }

    async fn invite_user_to_server(&self, _server_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("demo: invite_user_to_server".to_string()))
    }
}

// ── H.4.c — DiscoverBackend ──────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::DiscoverBackend for DemoClientGeneric<F> {
    async fn search_communities(
        &self,
        query: &str,
        scope: CommunityScope,
        cursor: Option<String>,
    ) -> ClientResult<CommunityPage> {
        F::search_communities(query, scope, cursor)
            .unwrap_or_else(|| Err(ClientError::NotSupported("community search not supported".into())))
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────
//
// Demo provides stub implementations for social graph methods: `get_user` and
// `get_friends` return fixture data; `respond_to_friend_request` returns success
// so the notifications UI can exercise that flow.

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::SocialGraphBackend for DemoClientGeneric<F> {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        F::users()
            .into_iter()
            .find(|u| u.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("User {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(F::friends())
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        // Demo client: accept/deny is handled by host-side state updates after a successful
        // backend response. Return success so the notifications UI can exercise that flow.
        Ok(())
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Ok(())
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Ok(())
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Online)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<F: DemoFlavour> poly_client::DmsAndGroupsBackend for DemoClientGeneric<F> {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(F::groups())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(F::dm_channels())
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        F::open_dm_channel(user_id)
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        let session = self.session.clone().unwrap_or_else(F::session);
        Ok(DmChannel {
            id: F::saved_messages_dm_id().to_string(),
            user: session.user,
            last_message: None,
            unread_count: 0,
            backend: BackendType::from(F::backend_slug()),
            account_id: F::account_id().to_string(),
        })
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Ok(())
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Ok(())
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Ok(())
    }
}

// ── H.4.a — MessagingBackend ──────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<F: DemoFlavour> poly_client::WritableMessagingBackend for DemoClientGeneric<F> {
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        Ok(F::send_message_for(channel_id, content))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<F: DemoFlavour> poly_client::MessagingBackend for DemoClientGeneric<F> {
    async fn send_typing(&self, _channel_id: &str) -> ClientResult<()> {
        Ok(()) // Demo silently accepts typing indicators.
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        Ok(data::demo_sent_reply_message(channel_id, reply_to_message_id, content))
    }

    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Ok(F::search_messages(&query))
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
        Ok(F::pinned_messages(channel_id))
    }

    async fn set_message_pinned(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _pinned: bool,
    ) -> ClientResult<()> {
        Ok(()) // Demo: accept pin/unpin without persisting.
    }

    async fn get_channel_commands(&self, channel_id: &str) -> ClientResult<Vec<ChatCommand>> {
        Ok(data::demo_channel_commands(channel_id))
    }

    async fn get_available_emojis(&self, channel_id: &str) -> ClientResult<Vec<CustomEmoji>> {
        Ok(data::demo_available_emojis(channel_id))
    }

    async fn get_available_stickers(&self, channel_id: &str) -> ClientResult<Vec<StickerItem>> {
        Ok(data::demo_available_stickers(channel_id))
    }
}

// ─── Public type aliases — preserve the API that consumers depend on ────────

// ─── Public type aliases ──────────────────────────────────────────────────────

/// Demo client for the Cat / chat account (the original "demo" account).
///
/// Externally: `poly_demo::DemoClient::new()`.
#[cfg(feature = "native")]
pub type DemoClient = DemoClientGeneric<flavour::Demo>;

/// Demo client for the Dog / chat account (the "demo2" account).
///
/// Externally: `poly_demo::DemoClient2::new()`.
#[cfg(feature = "native")]
pub type DemoClient2 = DemoClientGeneric<flavour::DemoChat>;

/// Demo client for the Platypus / forum account (the "demo_forum" account).
///
/// Externally: `poly_demo::DemoClient3::new()`.
#[cfg(feature = "native")]
pub type DemoClient3 = DemoClientGeneric<flavour::DemoForum>;

// ── C.1 — VoiceTransportBackend ──────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::VoiceTransportBackend for DemoClientGeneric<F> {
    async fn get_voice_participants(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(F::voice_participants(channel_id))
    }
}

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::SettingsBackend for DemoClientGeneric<F> {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::AccountGlobal,
            section_key: "preferences".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "regenerate-on-start".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
                    extra: String::new(),
                },
                SettingDescriptor {
                    key: "message-count".to_string(),
                    kind: SettingKind::Slider,
                    default_value: "50".to_string(),
                    extra: "{\"min\":10,\"max\":500,\"step\":10}".to_string(),
                },
            ],
            info_block: None,
        }])
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::ViewDescriptorBackend for DemoClientGeneric<F> {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        F::sidebar_declaration()
    }

    async fn invoke_sidebar_action(
        &self,
        action_id: &str,
    ) -> ClientResult<ActionOutcome> {
        F::invoke_sidebar_action(action_id, &self.settings_storage)
            .unwrap_or_else(|| Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}"))))
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        F::account_overview_view()
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        F::channel_view(channel_id)
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        F::view_rows(channel_id, tab_id)
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        F::view_detail(channel_id, row_id)
    }
}

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DemoFlavour> poly_client::ContextActionBackend for DemoClientGeneric<F> {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }
        Ok(vec![MenuItem {
            id: "regenerate-demo-data".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-demo-menu-regenerate-demo-data-label".to_string(),
            icon: None,
            item_variant: MenuItemVariant::Normal,
            shortcut: None,
            block: None,
        }])
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "regenerate-demo-data" => Ok(ActionOutcome::Noop),
            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }
}
