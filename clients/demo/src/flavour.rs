// Explicit imports used across multiple fn bodies in this module.
#[cfg(feature = "native")]
use poly_client::{
    BackendType, CardSpec, ClientError, CustomBlock, DmChannel, Message, MessageContent,
    MenuTargetKind, PresenceStatus, SidebarDeclaration, SidebarItem, SidebarLayoutKind,
    SidebarRouteKind, SidebarSection, TreeSpec, User, ViewBody, ViewDescriptor, ViewDetail,
    ViewHeader, ViewKind, ViewRow, ViewRowsPage, ViewToolbar,
};

/// Per-flavour data-source bindings for the generic `DemoClient<F>`.
///
/// Each associated function/method on this trait provides the per-flavour
/// value for one logical "variation point" between the three demo accounts
/// (Cat / Dog / Platypus-Forum). Anything the three impls share identically
/// lives in the generic `DemoClient<F>` impl body, not here.
///
/// The trait surface is intentionally minimal: every method here is a genuine
/// variation point, not a default. If you add a hook that all three impls
/// would implement identically, it doesn't belong on the trait — put it in
/// the shared impl.
#[cfg(feature = "native")]
pub trait DemoFlavour: Send + Sync + 'static {
    // ─── identity ──────────────────────────────────────────────────────────

    /// The Dioxus `BackendType` slug for this flavour.
    /// Cat + Dog use `crate::SLUG` ("demo"); Forum uses `data::DEMO_FORUM_BACKEND`.
    fn backend_slug() -> &'static str;

    /// Human-readable name for the backend (shown in UI).
    fn backend_name() -> &'static str;

    /// `BackendCapabilities` bitfield for this flavour.
    fn capabilities() -> poly_client::BackendCapabilities;

    // ─── session + account ─────────────────────────────────────────────────

    fn session() -> poly_client::Session;

    fn account_id() -> &'static str;

    /// Saved-messages DM channel id (unique per account).
    fn saved_messages_dm_id() -> &'static str;

    // ─── data sources ──────────────────────────────────────────────────────

    fn servers() -> Vec<poly_client::Server>;

    fn channels(server_id: &str) -> Vec<poly_client::Channel>;

    fn messages(channel_id: &str, query: &poly_client::MessageQuery) -> Vec<poly_client::Message>;

    fn search_messages(
        query: &poly_client::MessageSearchQuery,
    ) -> Vec<poly_client::MessageSearchHit>;

    fn pinned_messages(channel_id: &str) -> Vec<poly_client::Message>;

    fn users() -> Vec<poly_client::User>;

    fn friends() -> Vec<poly_client::User>;

    fn channel_members(channel_id: &str) -> Vec<poly_client::User>;

    fn groups() -> Vec<poly_client::Group>;

    fn notifications() -> Vec<poly_client::Notification>;

    fn voice_participants(channel_id: &str) -> Vec<poly_client::VoiceParticipant>;

    fn dm_channels() -> Vec<poly_client::DmChannel>;

    fn open_dm_channel(
        user_id: &str,
    ) -> poly_client::ClientResult<poly_client::DmChannel>;

    fn send_message_for(
        channel_id: &str,
        content: poly_client::MessageContent,
    ) -> poly_client::Message;

    // ─── view layer ────────────────────────────────────────────────────────

    fn account_overview_view() -> poly_client::ClientResult<poly_client::ViewDescriptor>;

    fn channel_view(
        channel_id: &str,
    ) -> Result<poly_client::ViewDescriptor, poly_client::ClientError>;

    fn view_rows(
        channel_id: &str,
        tab_id: Option<&str>,
    ) -> Result<poly_client::ViewRowsPage, poly_client::ClientError>;

    fn view_detail(
        channel_id: &str,
        row_id: &str,
    ) -> Result<poly_client::ViewDetail, poly_client::ClientError>;

    // ─── sidebar ───────────────────────────────────────────────────────────

    fn sidebar_declaration() -> Result<poly_client::SidebarDeclaration, poly_client::ClientError>;

    /// Returns `Some(outcome)` if the action was handled; `None` if the
    /// generic impl should fall through to `NotFound`.
    fn invoke_sidebar_action(
        action_id: &str,
        settings: &poly_client::SettingsStorageCell,
    ) -> Option<Result<poly_client::ActionOutcome, poly_client::ClientError>>;

    // ─── search ────────────────────────────────────────────────────────────

    /// Returns `Some(page)` if this flavour supports community search,
    /// `None` to have the generic impl return `NotSupported`.
    fn search_communities(
        query: &str,
        scope: poly_client::CommunityScope,
        cursor: Option<String>,
    ) -> Option<poly_client::ClientResult<poly_client::CommunityPage>>;

    // ─── event stream ──────────────────────────────────────────────────────

    fn event_stream() -> std::pin::Pin<
        Box<dyn futures::stream::Stream<Item = poly_client::ClientEvent> + Send>,
    >;
}

// ═══════════════════════════════════════════════════════════════════════════
// Cat (DemoClient — the original "demo" account)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "native")]
/// Marker type for the Cat / chat demo account (was `DemoClient`).
pub struct Demo;

#[cfg(feature = "native")]
impl DemoFlavour for Demo {
    fn backend_slug() -> &'static str {
        crate::SLUG
    }

    fn backend_name() -> &'static str {
        "Demo"
    }

    fn capabilities() -> poly_client::BackendCapabilities {
        poly_client::BackendCapabilities::FULL_SOCIAL_CHAT
    }

    fn session() -> poly_client::Session {
        crate::data::demo_session()
    }

    fn account_id() -> &'static str {
        crate::data::DEMO_ACCOUNT_ID
    }

    fn saved_messages_dm_id() -> &'static str {
        "dm-demo-saved-self"
    }

    fn servers() -> Vec<poly_client::Server> {
        crate::data::demo_servers()
    }

    fn channels(server_id: &str) -> Vec<poly_client::Channel> {
        crate::data::demo_channels(server_id)
    }

    fn messages(
        channel_id: &str,
        query: &poly_client::MessageQuery,
    ) -> Vec<poly_client::Message> {
        crate::data::demo_messages_query(channel_id, query)
    }

    fn search_messages(
        query: &poly_client::MessageSearchQuery,
    ) -> Vec<poly_client::MessageSearchHit> {
        crate::data::demo_search_messages(query)
    }

    fn pinned_messages(channel_id: &str) -> Vec<poly_client::Message> {
        crate::data::demo_pinned_messages(channel_id)
    }

    fn users() -> Vec<poly_client::User> {
        crate::data::demo_users()
    }

    fn friends() -> Vec<poly_client::User> {
        // Cat is friends with the eight stock demo people PLUS Dog so
        // the two demo accounts can be exercised end-to-end.
        let mut friends = crate::data::demo_users().into_iter().take(8).collect::<Vec<_>>();
        friends.push(crate::data::demo_dog_user());
        friends
    }

    fn channel_members(_channel_id: &str) -> Vec<poly_client::User> {
        crate::data::demo_users()
    }

    fn groups() -> Vec<poly_client::Group> {
        crate::data::demo_groups_v2()
    }

    fn notifications() -> Vec<poly_client::Notification> {
        crate::data::demo_notifications()
    }

    fn voice_participants(channel_id: &str) -> Vec<poly_client::VoiceParticipant> {
        crate::data::demo_voice_participants(channel_id)
    }

    fn dm_channels() -> Vec<poly_client::DmChannel> {
        crate::data::apply_local_read_state_dms(crate::data::demo_dm_channels())
    }

    fn open_dm_channel(user_id: &str) -> poly_client::ClientResult<poly_client::DmChannel> {
        crate::data::demo_dm_channels()
            .into_iter()
            .find(|dm| dm.user.id == user_id)
            .map_or_else(
                || crate::data::demo_empty_dm_channel_for_user(user_id, crate::data::DEMO_ACCOUNT_ID),
                Ok,
            )
    }

    fn send_message_for(
        channel_id: &str,
        content: poly_client::MessageContent,
    ) -> poly_client::Message {
        crate::data::demo_sent_message(channel_id, content)
    }

    fn account_overview_view() -> poly_client::ClientResult<poly_client::ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-demo-overview-title".to_string()),
                subtitle_key: Some("plugin-demo-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec { primary_field: "name".to_string() }),
        })
    }

    fn channel_view(_channel_id: &str) -> Result<poly_client::ViewDescriptor, poly_client::ClientError> {
        Err(poly_client::ClientError::NotSupported(
            "chat-only backend; no structured view".into(),
        ))
    }

    fn view_rows(
        channel_id: &str,
        _tab_id: Option<&str>,
    ) -> Result<poly_client::ViewRowsPage, poly_client::ClientError> {
        if channel_id.is_empty() || channel_id == "overview" {
            let rows = crate::data::demo_servers()
                .into_iter()
                .map(|s| {
                    let members = crate::data::demo_server_member_count(&s.id);
                    let unread = s.unread_count;
                    let mentions = s.mention_count;
                    let meta = if mentions > 0 {
                        format!("{members} members · {unread} unread · @{mentions} mentions")
                    } else {
                        format!("{members} members · {unread} unread")
                    };
                    ViewRow {
                        id: s.id.clone(),
                        primary_text: s.name.clone(),
                        secondary_text: Some(crate::data::demo_server_description(&s.id).to_string()),
                        meta_text: Some(meta),
                        icon: None,
                        badge: None,
                        context_menu_target_kind: MenuTargetKind::Server,
                        preview_image_url: None,
                        is_video: false,
                    }
                })
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }
        Err(ClientError::NotSupported("chat-only backend; no view rows".into()))
    }

    fn view_detail(
        _channel_id: &str,
        _row_id: &str,
    ) -> Result<poly_client::ViewDetail, poly_client::ClientError> {
        Err(poly_client::ClientError::NotSupported(
            "chat-only backend; no view detail".into(),
        ))
    }

    fn sidebar_declaration() -> Result<poly_client::SidebarDeclaration, poly_client::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: Vec::new(),
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: &str,
        _settings: &poly_client::SettingsStorageCell,
    ) -> Option<Result<poly_client::ActionOutcome, poly_client::ClientError>> {
        Some(Err(poly_client::ClientError::NotFound(format!(
            "unknown sidebar action: {action_id}"
        ))))
    }

    fn search_communities(
        _query: &str,
        _scope: poly_client::CommunityScope,
        _cursor: Option<String>,
    ) -> Option<poly_client::ClientResult<poly_client::CommunityPage>> {
        None
    }

    // lint-allow-unused: event-stream dispatch table — splitting loses cohesion
    #[allow(clippy::too_many_lines)]
    fn event_stream() -> std::pin::Pin<Box<dyn futures::stream::Stream<Item = poly_client::ClientEvent> + Send>> {
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(futures::stream::empty())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            use poly_client::{
                ClientEvent, Message, MessageContent, PresenceStatus,
            };
            let users = crate::data::demo_users();
            let server_channels = vec![
                "ch-general",
                "ch-off-topic",
                "ch-rust",
                "ch-dioxus",
                "ch-minecraft",
                "ch-valorant",
                "ch-recommendations",
            ];
            let dm_channels = vec!["dm-user-alice", "dm-user-bob", "dm-user-charlie"];
            let server_messages = vec![
                "That's a great point!",
                "I'll look into it. \u{1f527}",
                "Has anyone else seen this?",
                "Working on a fix now...",
                "brb",
                "lol nice one",
                "Can confirm, same issue here.",
                "\u{1f44d}",
                "Just pushed the fix!",
                "Who's up for a game tonight?",
                "This is so cool!",
                "Let's sync tomorrow morning.",
            ];
            let dm_messages = vec![
                "Hey, are you around?",
                "Did you see the latest update?",
                "Let's catch up soon!",
                "Thanks for the help earlier \u{1f64f}",
                "Check this out!",
                "I'll send you the file in a bit.",
                "Haha yeah exactly \u{1f61d}",
                "Makes sense, let's do it!",
            ];

            let stream = futures::stream::unfold(0u64, move |counter| {
                let users = users.clone();
                let server_channels = server_channels.clone();
                let dm_channels = dm_channels.clone();
                let server_messages = server_messages.clone();
                let dm_messages = dm_messages.clone();
                async move {
                    if users.is_empty() || server_channels.is_empty() {
                        return None;
                    }

                    let cu = usize::try_from(counter).unwrap_or(usize::MAX);

                    let delays = [4u64, 6, 8, 5, 7, 3];
                    let delay_secs = delays
                        .get(cu.checked_rem(delays.len()).unwrap_or(0))
                        .copied()
                        .unwrap_or(5);
                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

                    let user_idx = cu.checked_rem(users.len()).unwrap_or(0);
                    let user = users.get(user_idx)?;

                    let event = match counter.checked_rem(5).unwrap_or(0) {
                        0 | 3 => {
                            let ch_idx = cu.checked_rem(server_channels.len()).unwrap_or(0);
                            let channel_id = (*server_channels.get(ch_idx)?).to_string();
                            let msg_idx = cu
                                .checked_div(5)
                                .and_then(|v| v.checked_rem(server_messages.len()))
                                .unwrap_or(0);
                            let text = server_messages.get(msg_idx).copied().unwrap_or("...");
                            ClientEvent::MessageReceived {
                                channel_id,
                                message: Message {
                                    id: format!("msg-stream-{counter}"),
                                    author: user.clone(),
                                    content: MessageContent::Text(text.to_string()),
                                    timestamp: chrono::Utc::now(),
                                    attachments: vec![],
                                    reactions: vec![],
                                    reply_to: None,
                                    edited: false,
                                    thread: None,
                                    preview_image_url: None,
                                },
                            }
                        }
                        1 => {
                            let ch_idx = cu.checked_rem(server_channels.len()).unwrap_or(0);
                            let channel_id = (*server_channels.get(ch_idx)?).to_string();
                            ClientEvent::TypingStarted {
                                channel_id,
                                user_id: user.id.clone(),
                                timestamp: chrono::Utc::now(),
                            }
                        }
                        2 => {
                            let dm_idx = cu
                                .checked_div(2)
                                .and_then(|v| v.checked_rem(dm_channels.len()))
                                .unwrap_or(0);
                            let channel_id = (*dm_channels.get(dm_idx)?).to_string();
                            let dm_user_idx = cu
                                .checked_add(1)
                                .and_then(|v| v.checked_rem(users.len()))
                                .unwrap_or(0);
                            let dm_user = users.get(dm_user_idx)?;
                            let msg_idx = cu
                                .checked_div(3)
                                .and_then(|v| v.checked_rem(dm_messages.len()))
                                .unwrap_or(0);
                            let text = dm_messages.get(msg_idx).copied().unwrap_or("hey!");
                            ClientEvent::MessageReceived {
                                channel_id,
                                message: Message {
                                    id: format!("msg-stream-dm-{counter}"),
                                    author: dm_user.clone(),
                                    content: MessageContent::Text(text.to_string()),
                                    timestamp: chrono::Utc::now(),
                                    attachments: vec![],
                                    reactions: vec![],
                                    reply_to: None,
                                    edited: false,
                                    thread: None,
                                    preview_image_url: None,
                                },
                            }
                        }
                        _ => {
                            let statuses = [
                                PresenceStatus::Online,
                                PresenceStatus::Idle,
                                PresenceStatus::DoNotDisturb,
                                PresenceStatus::Online,
                            ];
                            let s_idx = cu
                                .checked_div(3)
                                .and_then(|v| v.checked_rem(statuses.len()))
                                .unwrap_or(0);
                            let status = statuses
                                .get(s_idx)
                                .copied()
                                .unwrap_or(PresenceStatus::Online);
                            ClientEvent::PresenceChanged {
                                user_id: user.id.clone(),
                                status,
                            }
                        }
                    };

                    Some((event, counter.saturating_add(1)))
                }
            });

            Box::pin(stream)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Dog (DemoChat — the "demo2" chat account)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "native")]
/// Marker type for the Dog / chat demo account (was `DemoClient2`).
pub struct DemoChat;

#[cfg(feature = "native")]
impl DemoFlavour for DemoChat {
    fn backend_slug() -> &'static str {
        crate::SLUG
    }

    fn backend_name() -> &'static str {
        "Demo (Dog)"
    }

    fn capabilities() -> poly_client::BackendCapabilities {
        poly_client::BackendCapabilities::FULL_SOCIAL_CHAT
    }

    fn session() -> poly_client::Session {
        crate::data::demo2_session()
    }

    fn account_id() -> &'static str {
        crate::data::DEMO2_ACCOUNT_ID
    }

    fn saved_messages_dm_id() -> &'static str {
        "dm-demo2-saved-self"
    }

    fn servers() -> Vec<poly_client::Server> {
        crate::data::demo2_servers()
    }

    fn channels(server_id: &str) -> Vec<poly_client::Channel> {
        crate::data::demo2_channels(server_id)
    }

    fn messages(
        channel_id: &str,
        query: &poly_client::MessageQuery,
    ) -> Vec<poly_client::Message> {
        crate::data::demo2_messages_query(channel_id, query)
    }

    fn search_messages(
        query: &poly_client::MessageSearchQuery,
    ) -> Vec<poly_client::MessageSearchHit> {
        crate::data::demo2_search_messages(query)
    }

    fn pinned_messages(channel_id: &str) -> Vec<poly_client::Message> {
        crate::data::demo2_pinned_messages(channel_id)
    }

    fn users() -> Vec<poly_client::User> {
        crate::data::demo_users()
    }

    fn friends() -> Vec<poly_client::User> {
        // Dog account has a different friend circle plus Cat.
        let mut friends = crate::data::demo_users().into_iter().skip(2).take(6).collect::<Vec<_>>();
        friends.push(crate::data::demo_cat_user());
        friends
    }

    fn channel_members(_channel_id: &str) -> Vec<poly_client::User> {
        crate::data::demo_users().into_iter().take(6).collect()
    }

    fn groups() -> Vec<poly_client::Group> {
        crate::data::demo2_groups()
    }

    fn notifications() -> Vec<poly_client::Notification> {
        crate::data::demo2_notifications()
    }

    fn voice_participants(_channel_id: &str) -> Vec<poly_client::VoiceParticipant> {
        vec![]
    }

    fn dm_channels() -> Vec<poly_client::DmChannel> {
        // A subset of DMs from a different perspective
        let mut dms: Vec<DmChannel> = crate::data::demo_dm_channels()
            .into_iter()
            .take(3)
            .map(|mut dm| {
                dm.account_id = crate::data::DEMO2_ACCOUNT_ID.to_string();
                dm
            })
            .collect();

        // Add cross-account DM: dog sees cat
        dms.push(DmChannel {
            id: "dm-demo-cat".to_string(),
            user: User {
                id: "demo-cat-user".to_string(),
                display_name: "\u{1f431} Cat (demo)".to_string(),
                avatar_url: Some(crate::data::DEMO_CAT_AVATAR.to_string()),
                presence: PresenceStatus::Online,
                backend: BackendType::from(crate::SLUG),
            },
            last_message: Some(Message {
                id: "msg-dm-cat-latest".to_string(),
                author: User {
                    id: "demo-cat-user".to_string(),
                    display_name: "\u{1f431} Cat (demo)".to_string(),
                    avatar_url: Some(crate::data::DEMO_CAT_AVATAR.to_string()),
                    presence: PresenceStatus::Online,
                    backend: BackendType::from(crate::SLUG),
                },
                content: MessageContent::Text(
                    "fair! \u{1f639} but you have to admit the feature flag organization is *clean* even if it's stolen from my 2023 design"
                        .to_string(),
                ),
                timestamp: crate::data::ago_hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
                thread: None,
                preview_image_url: None,
            }),
            unread_count: 1,
            backend: BackendType::from(crate::SLUG),
            account_id: crate::data::DEMO2_ACCOUNT_ID.to_string(),
        });

        crate::data::apply_local_read_state_dms(dms)
    }

    fn open_dm_channel(user_id: &str) -> poly_client::ClientResult<poly_client::DmChannel> {
        // The DemoChat `get_dm_channels` may involve async in the generic impl,
        // but here we replicate the synchronous lookup from the dog DM list.
        Self::dm_channels()
            .into_iter()
            .find(|dm| dm.user.id == user_id)
            .map_or_else(
                || crate::data::demo_empty_dm_channel_for_user(user_id, crate::data::DEMO2_ACCOUNT_ID),
                Ok,
            )
    }

    fn send_message_for(
        channel_id: &str,
        content: poly_client::MessageContent,
    ) -> poly_client::Message {
        crate::data::demo_sent_message_for(channel_id, content, crate::data::demo2_session().user)
    }

    fn account_overview_view() -> poly_client::ClientResult<poly_client::ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-demo-overview-title".to_string()),
                subtitle_key: Some("plugin-demo-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec { primary_field: "name".to_string() }),
        })
    }

    fn channel_view(_channel_id: &str) -> Result<poly_client::ViewDescriptor, poly_client::ClientError> {
        Err(poly_client::ClientError::NotSupported(
            "chat-only backend; no structured view".into(),
        ))
    }

    fn view_rows(
        channel_id: &str,
        _tab_id: Option<&str>,
    ) -> Result<poly_client::ViewRowsPage, poly_client::ClientError> {
        if channel_id.is_empty() || channel_id == "overview" {
            let rows = crate::data::demo2_servers()
                .into_iter()
                .map(|s| {
                    let members = crate::data::demo_server_member_count(&s.id);
                    let unread = s.unread_count;
                    let mentions = s.mention_count;
                    let meta = if mentions > 0 {
                        format!("{members} members · {unread} unread · @{mentions} mentions")
                    } else {
                        format!("{members} members · {unread} unread")
                    };
                    ViewRow {
                        id: s.id.clone(),
                        primary_text: s.name.clone(),
                        secondary_text: Some(crate::data::demo_server_description(&s.id).to_string()),
                        meta_text: Some(meta),
                        icon: None,
                        badge: None,
                        context_menu_target_kind: MenuTargetKind::Server,
                        preview_image_url: None,
                        is_video: false,
                    }
                })
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }
        Err(ClientError::NotSupported("chat-only backend; no view rows".into()))
    }

    fn view_detail(
        _channel_id: &str,
        _row_id: &str,
    ) -> Result<poly_client::ViewDetail, poly_client::ClientError> {
        Err(poly_client::ClientError::NotSupported(
            "chat-only backend; no view detail".into(),
        ))
    }

    fn sidebar_declaration() -> Result<poly_client::SidebarDeclaration, poly_client::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: Vec::new(),
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: &str,
        _settings: &poly_client::SettingsStorageCell,
    ) -> Option<Result<poly_client::ActionOutcome, poly_client::ClientError>> {
        Some(Err(poly_client::ClientError::NotFound(format!(
            "unknown sidebar action: {action_id}"
        ))))
    }

    fn search_communities(
        _query: &str,
        _scope: poly_client::CommunityScope,
        _cursor: Option<String>,
    ) -> Option<poly_client::ClientResult<poly_client::CommunityPage>> {
        None
    }

    fn event_stream() -> std::pin::Pin<Box<dyn futures::stream::Stream<Item = poly_client::ClientEvent> + Send>> {
        // Demo2 emits no live events for simplicity.
        Box::pin(futures::stream::empty())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Forum / Platypus (DemoForum — the "demo_forum" account)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "native")]
/// Marker type for the Platypus / forum demo account (was `DemoClient3`).
pub struct DemoForum;

#[cfg(feature = "native")]
impl DemoFlavour for DemoForum {
    fn backend_slug() -> &'static str {
        crate::data::DEMO_FORUM_BACKEND
    }

    fn backend_name() -> &'static str {
        "Demo Forum (Platypus)"
    }

    fn capabilities() -> poly_client::BackendCapabilities {
        poly_client::BackendCapabilities::MESSAGING_NO_SOCIAL
    }

    fn session() -> poly_client::Session {
        crate::data::demo3_session()
    }

    fn account_id() -> &'static str {
        crate::data::DEMO3_ACCOUNT_ID
    }

    fn saved_messages_dm_id() -> &'static str {
        "dm-demo3-saved-self"
    }

    fn servers() -> Vec<poly_client::Server> {
        crate::data::demo3_servers()
    }

    fn channels(server_id: &str) -> Vec<poly_client::Channel> {
        crate::data::demo3_channels(server_id)
    }

    fn messages(
        channel_id: &str,
        _query: &poly_client::MessageQuery,
    ) -> Vec<poly_client::Message> {
        // DM channels
        let dm_msgs = crate::data::demo3_dm_messages(channel_id);
        if !dm_msgs.is_empty() {
            return dm_msgs;
        }
        // Thread comments — channel_id is a post ID or the "hn-post-<id>"
        // pseudo-channel that ForumPostView uses to fetch comments.
        let stripped = channel_id.strip_prefix("hn-post-").unwrap_or(channel_id);
        if stripped.starts_with("fpost-") {
            let comments = crate::data::demo3_post_comments(stripped);
            if !comments.is_empty() {
                return comments;
            }
        }
        // Forum post list
        crate::data::demo3_messages(channel_id)
    }

    fn search_messages(
        _query: &poly_client::MessageSearchQuery,
    ) -> Vec<poly_client::MessageSearchHit> {
        vec![]
    }

    fn pinned_messages(_channel_id: &str) -> Vec<poly_client::Message> {
        vec![]
    }

    fn users() -> Vec<poly_client::User> {
        vec![]
    }

    fn friends() -> Vec<poly_client::User> {
        vec![]
    }

    fn channel_members(_channel_id: &str) -> Vec<poly_client::User> {
        vec![]
    }

    fn groups() -> Vec<poly_client::Group> {
        vec![]
    }

    fn notifications() -> Vec<poly_client::Notification> {
        crate::data::demo3_notifications()
    }

    fn voice_participants(_channel_id: &str) -> Vec<poly_client::VoiceParticipant> {
        vec![]
    }

    fn dm_channels() -> Vec<poly_client::DmChannel> {
        crate::data::apply_local_read_state_dms(crate::data::demo3_dm_channels())
    }

    fn open_dm_channel(user_id: &str) -> poly_client::ClientResult<poly_client::DmChannel> {
        crate::data::demo3_dm_channels()
            .into_iter()
            .find(|dm| dm.user.id == user_id)
            .ok_or_else(|| poly_client::ClientError::NotFound(format!("DM user {user_id}")))
    }

    fn send_message_for(
        channel_id: &str,
        content: poly_client::MessageContent,
    ) -> poly_client::Message {
        crate::data::demo_sent_message(channel_id, content)
    }

    fn account_overview_view() -> poly_client::ClientResult<poly_client::ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-demo-forum-overview-title".to_string()),
                subtitle_key: Some("plugin-demo-forum-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec { primary_field: "name".to_string() }),
        })
    }

    fn channel_view(_channel_id: &str) -> Result<poly_client::ViewDescriptor, poly_client::ClientError> {
        Ok(ViewDescriptor {
            kind: ViewKind::Tree,
            header: Some(ViewHeader {
                title_key: Some("plugin-demo-view-posts-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![],
                filter_options: vec![],
                tabs: vec![],
                action_items: vec![],
            }),
            body: ViewBody::TreeBody(TreeSpec {
                root_page_size: 25,
                max_depth: 8,
            }),
        })
    }

    fn view_rows(
        channel_id: &str,
        tab_id: Option<&str>,
    ) -> Result<poly_client::ViewRowsPage, poly_client::ClientError> {
        if channel_id.is_empty() || channel_id == "overview" {
            let rows = crate::data::demo3_servers()
                .into_iter()
                .map(|s| {
                    let members = crate::data::demo_server_member_count(&s.id);
                    let posts_count = crate::data::demo3_messages(
                        s.categories
                            .first()
                            .and_then(|c| c.channel_ids.first())
                            .map_or("", String::as_str),
                    )
                    .len();
                    let meta = format!("{members} subscribers · {posts_count} posts");
                    ViewRow {
                        id: s.id.clone(),
                        primary_text: s.name.clone(),
                        secondary_text: Some(crate::data::demo_server_description(&s.id).to_string()),
                        meta_text: Some(meta),
                        icon: None,
                        badge: None,
                        context_menu_target_kind: MenuTargetKind::Server,
                        preview_image_url: None,
                        is_video: false,
                    }
                })
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }
        let subscribed_posts = crate::data::demo3_messages(channel_id);
        let posts: Vec<Message> = match tab_id.unwrap_or("subscribed") {
            "local" => subscribed_posts
                .into_iter()
                .filter(|m| m.author.display_name.contains("(demo_forum)"))
                .collect(),
            "all" => {
                let mut combined = subscribed_posts;
                combined.extend(crate::data::demo_federated_posts(channel_id));
                combined
            }
            _ => subscribed_posts,
        };
        let rows = posts
            .into_iter()
            .map(|msg| {
                let body = match &msg.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::WithAttachments { text, .. } => text.clone(),
                };
                let score = crate::data::forum_post_score(&msg);
                let comment_count = crate::data::demo3_post_comments(&msg.id).len();
                let age = crate::data::forum_humanize_age(msg.timestamp);
                ViewRow {
                    id: msg.id.clone(),
                    primary_text: body,
                    secondary_text: Some(format!("by {}", msg.author.display_name)),
                    meta_text: Some(format!("SCORE:{score} · {comment_count} comments · {age}")),
                    icon: None,
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Message,
                    preview_image_url: None,
                    is_video: false,
                }
            })
            .collect();
        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    fn view_detail(
        channel_id: &str,
        row_id: &str,
    ) -> Result<poly_client::ViewDetail, poly_client::ClientError> {
        fn html_escape(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        }
        let body_html = crate::data::demo3_messages(channel_id)
            .into_iter()
            .find(|msg| msg.id == row_id)
            .map_or_else(
                || format!("<p>(post {} not found)</p>", html_escape(row_id)),
                |msg| match msg.content {
                    MessageContent::Text(t) | MessageContent::WithAttachments { text: t, .. } => {
                        format!("<p>{}</p>", html_escape(&t))
                    }
                },
            );
        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html: body_html,
                stylesheet: None,
                max_height_px: None,
            },
            comments_section: Some(TreeSpec {
                root_page_size: 25,
                max_depth: 8,
            }),
        })
    }

    fn sidebar_declaration() -> Result<poly_client::SidebarDeclaration, poly_client::ClientError> {
        let mk = |id: &str, label_key: &str, parent: Option<&str>| SidebarItem {
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
            label_key: label_key.to_string(),
            icon: None,
            badge: None,
            route_kind: SidebarRouteKind::Channel,
        };
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::SortModes,
            sections: vec![SidebarSection {
                header_key: None,
                collapsible: false,
                default_collapsed: false,
                items: vec![
                    mk("sort-hot", "ui-sidebar-sort-hot", None),
                    mk("sort-active", "ui-sidebar-sort-active", None),
                    mk("sort-new", "ui-sidebar-sort-new", None),
                    mk("sort-old", "ui-sidebar-sort-old", None),
                    mk("sort-most-comments", "ui-sidebar-sort-most-comments", None),
                    mk("sort-new-comments", "ui-sidebar-sort-new-comments", None),
                    mk("sort-top", "ui-sidebar-sort-top", None),
                    mk("sort-top-day", "ui-sidebar-sort-top-day", Some("sort-top")),
                    mk("sort-top-week", "ui-sidebar-sort-top-week", Some("sort-top")),
                    mk("sort-top-month", "ui-sidebar-sort-top-month", Some("sort-top")),
                    mk("sort-top-year", "ui-sidebar-sort-top-year", Some("sort-top")),
                    mk("sort-top-all", "ui-sidebar-sort-top-all", Some("sort-top")),
                ],
            }],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: &str,
        settings: &poly_client::SettingsStorageCell,
    ) -> Option<Result<poly_client::ActionOutcome, poly_client::ClientError>> {
        if action_id.starts_with("sort-") {
            drop(settings.set(
                poly_client::SettingsScope::AccountGlobal,
                "",
                "current-sort",
                action_id,
            ));
            return Some(Ok(poly_client::ActionOutcome::RefreshTarget));
        }
        Some(Err(poly_client::ClientError::NotFound(format!(
            "unknown sidebar action: {action_id}"
        ))))
    }

    fn search_communities(
        query: &str,
        _scope: poly_client::CommunityScope,
        _cursor: Option<String>,
    ) -> Option<poly_client::ClientResult<poly_client::CommunityPage>> {
        let needle = query.trim().to_lowercase();
        let items: Vec<poly_client::Server> = crate::data::demo3_discover_servers()
            .into_iter()
            .filter(|s| needle.is_empty() || s.name.to_lowercase().contains(&needle))
            .collect();
        Some(Ok(poly_client::CommunityPage { items, next_cursor: None }))
    }

    fn event_stream() -> std::pin::Pin<Box<dyn futures::stream::Stream<Item = poly_client::ClientEvent> + Send>> {
        Box::pin(futures::stream::empty())
    }
}
