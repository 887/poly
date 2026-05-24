//! `IsBackend` trait impl for [`super::RedditBackend`].
//!
//! Carved out in SOLID-audit-reddit C.3.

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::*;
use std::pin::Pin;

use super::error::NS_CREDS;
use super::ids::{message_id_for_dm, sub_from_channel_id, sub_from_server_id};
use super::mapping::{
    build_sub_channel, build_sub_server, html_to_plain_text, raw_dm_to_dm_channel,
    raw_post_to_message, split_title_body,
};
use super::RedditBackend;
use crate::SortKind;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for RedditBackend {
    // ── Authentication ───────────────────────────────────────────────────────

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        match credentials {
            AuthCredentials::EmailPassword { email, password } => {
                // Reddit's "email" field carries the username.
                self.client
                    .login_with_password(&email, &password)
                    .await
                    .map_err(ClientError::from)?;
                let session = self.build_session(&email);
                self.session = Some(session.clone());
                Ok(session)
            }
            AuthCredentials::Token(cookie) => {
                self.client
                    .login_with_session_cookie(&cookie)
                    .map_err(ClientError::from)?;
                // Probe who we are.
                let username = self
                    .client
                    .is_logged_in()
                    .await
                    .map_err(ClientError::from)?
                    .unwrap_or_else(|| "me".to_string());
                let session = self.build_session(&username);
                self.session = Some(session.clone());
                Ok(session)
            }
            AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. } => Err(ClientError::NotSupported(
                NS_CREDS.to_string(),
            )),
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    // ── Servers (subreddits) ─────────────────────────────────────────────────

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        // Delegates to RedditClient::list_subscribed_subreddits which sends
        // the manual session header (X-Mock-Session for browser fetch +
        // Cookie for native). Calling self.client.http().get(...) directly
        // here would skip the auth header and always come back empty.
        let subs = self.client.list_subscribed_subreddits().await?;
        let account_id = self.account_id();
        let account_display_name = self.account_display_name();
        let bt = Self::backend_type();
        Ok(subs
            .iter()
            .map(|sub| {
                let mut server = build_sub_server(&sub.name, account_id, account_display_name, &bt);
                if let Some(url) = &sub.icon_url {
                    server.icon_url = Some(url.clone());
                }
                server
            })
            .collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let sub = sub_from_server_id(id)
            .ok_or_else(|| ClientError::NotFound(format!("server not found: {id}")))?;
        Ok(build_sub_server(
            sub,
            self.account_id(),
            self.account_display_name(),
            &Self::backend_type(),
        ))
    }

    // ── Channels ─────────────────────────────────────────────────────────────

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let sub = sub_from_server_id(server_id)
            .ok_or_else(|| ClientError::NotFound(format!("server not found: {server_id}")))?;
        Ok(vec![build_sub_channel(sub)])
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let sub = sub_from_channel_id(id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {id}")))?;
        Ok(build_sub_channel(sub))
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match &content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        };

        // Three channel-id shapes (mirrors get_messages):
        //   c_posts_<sub> — top-level submit (kind=self, title = first
        //                   non-empty line, body = remainder)
        //   hn-post-<id>  — top-level comment on the post (parent t3_<id>)
        //   dm_<dm_id>    — reply within an existing DM thread (Reddit
        //                   uses /api/comment with parent t4_<id>)
        let (placeholder_id, placeholder_prefix) =
            if let Some(sub) = sub_from_channel_id(channel_id) {
                let (title, body) = split_title_body(&text);
                let name = self
                    .client
                    .submit_self_post(sub, &title, body)
                    .await
                    .map_err(ClientError::from)?;
                let id = if name.is_empty() {
                    format!("t3_pending-{}", chrono::Utc::now().timestamp_millis())
                } else {
                    name
                };
                (id, "t3")
            } else if let Some(post_id) = channel_id.strip_prefix("hn-post-") {
                let bare = post_id.strip_prefix("t3_").unwrap_or(post_id);
                let parent = format!("t3_{bare}");
                self.client
                    .reply_comment(&parent, &text)
                    .await
                    .map_err(ClientError::from)?;
                (
                    format!("t1_pending-{}", chrono::Utc::now().timestamp_millis()),
                    "t1",
                )
            } else if let Some(dm_id) = channel_id.strip_prefix("dm_") {
                let parent = format!("t4_{dm_id}");
                self.client
                    .reply_comment(&parent, &text)
                    .await
                    .map_err(ClientError::from)?;
                (
                    format!("t4_pending-{}", chrono::Utc::now().timestamp_millis()),
                    "t4",
                )
            } else {
                return Err(ClientError::NotSupported(format!(
                    "send_message: unrecognised channel id `{channel_id}`"
                )));
            };
        let _ = placeholder_prefix; // documentation only

        let now = chrono::Utc::now();
        let account_display = self.account_display_name().to_string();
        let bt = Self::backend_type();
        Ok(Message {
            id: placeholder_id,
            author: User {
                id: self
                    .session
                    .as_ref()
                    .map_or("u_me".to_string(), |s| s.user.id.clone()),
                display_name: account_display,
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: bt,
            },
            content,
            timestamp: now,
            attachments: Vec::new(),
            reactions: Vec::new(),
            reply_to: None,
            edited: false,
            thread: None,
            preview_image_url: None,
        })
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let bt = Self::backend_type();

        // Three channel-id shapes:
        // 1. `c_posts_<sub>` — subreddit post-list (the standard channel)
        // 2. `hn-post-<post_id>` — ForumPostView's per-post comment fetch
        //    (the channel id is hard-coded to `hn-post-<pid>` in the
        //    shared forum_view.rs because HackerNews was the first
        //    forum-style backend; we accept the same shape here so
        //    Reddit posts open with their comments populated)
        // 3. anything else — return NotFound
        if let Some(sub) = sub_from_channel_id(channel_id) {
            let posts = self
                .client
                .list_subreddit(sub, SortKind::Hot)
                .await
                .map_err(ClientError::from)?;
            return Ok(posts.iter().map(|p| raw_post_to_message(p, &bt)).collect());
        }
        if let Some(post_id) = channel_id.strip_prefix("hn-post-") {
            // Fetch the post + its comment tree, flatten depth-first
            // into a Vec<Message>. The OP itself is included as the
            // first message so ForumPostView's lookup finds it.
            // Forum URLs carry the `t3_`-prefixed message id; the
            // RedditClient API expects a bare id, so strip if present.
            let bare_id = post_id.strip_prefix("t3_").unwrap_or(post_id);
            return self.fetch_post_thread_messages(bare_id, &bt).await;
        }

        // 3. `dm_<dm_id>` — single-message DM "thread" (Reddit DMs are
        //    not multi-message conversations server-side; the unified UI
        //    treats each DM as a one-message channel for now).
        if let Some(dm_id) = channel_id.strip_prefix("dm_") {
            let inbox = self.client.inbox().await.map_err(ClientError::from)?;
            let dm = inbox
                .into_iter()
                .find(|d| d.id == dm_id)
                .ok_or_else(|| ClientError::NotFound(format!("dm: {dm_id}")))?;
            let account_id = self
                .session
                .as_ref()
                .map_or("u_me", |s| s.user.id.as_str());
            let dm_chan = raw_dm_to_dm_channel(&dm, account_id, &bt);
            let body_plain = html_to_plain_text(&dm.body_html);
            let msg = Message {
                id: message_id_for_dm(&dm.id),
                author: dm_chan.user.clone(),
                content: MessageContent::Text(body_plain),
                timestamp: dm.timestamp,
                attachments: Vec::new(),
                reactions: Vec::new(),
                reply_to: None,
                edited: false,
                thread: None,
                preview_image_url: None,
            };
            return Ok(vec![msg]);
        }

        Err(ClientError::NotFound(format!("channel not found: {channel_id}")))
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    // ── Notifications ─────────────────────────────────────────────────────────

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // ── Voice / Settings / Views / Context: moved to C.1 sub-traits ──────────

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    // ── Real-time events ─────────────────────────────────────────────────────

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(stream::empty())
    }

    // ── Backend info ─────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Reddit"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            community_search: poly_client::CommunitySearchSupport::Single,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["old.reddit.com".to_string()],
            description:
                "Reddit client. Scrapes old.reddit.com HTML for posts and DMs. \
                 Anonymous and session-cookie auth supported."
                    .to_string(),
            homepage: Some("https://old.reddit.com".to_string()),
        }
    }

    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        SignupMethod::InApp("/signup/reddit".to_string())
    }

    /// Declares the `show-media-previews` mechanism, which controls whether
    /// image/video thumbnail previews are rendered next to forum post titles.
    ///
    /// Default ON. Set to `false` to hide all preview thumbnails.
    ///
    /// TODO: add `navigator.connection.effectiveType` auto-disable for
    /// `slow-2g`/`2g` connections when the Web Connection API is available in
    /// WASM contexts.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        let enabled = self.media_previews_enabled();
        Ok(vec![Mechanism {
            id: "show-media-previews".to_string(),
            name_key: "plugin-reddit-mechanism-show-media-previews-label".to_string(),
            enabled,
            requires_host_cap: None,
            description_key: Some(
                "plugin-reddit-mechanism-show-media-previews-desc".to_string(),
            ),
        }])
    }

    /// Toggle the `show-media-previews` mechanism.
    async fn set_client_mechanism(&self, id: &str, enabled: bool) -> ClientResult<()> {
        match id {
            "show-media-previews" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "show-media-previews",
                if enabled { "true" } else { "false" },
            ),
            _ => Err(ClientError::NotFound(format!("unknown mechanism: {id}"))),
        }
    }

    // ── Phase E: community search (moved to DiscoverBackend H.4.c) ────────────

    fn as_discover(&self) -> Option<&dyn poly_client::DiscoverBackend> {
        Some(self)
    }
}
