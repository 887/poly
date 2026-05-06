//! [`ClientBackend`] impl for [`RedditClient`].
//!
//! Wires the HTML-scraping Reddit client into the Poly trait surface so the
//! UI can render Reddit accounts alongside Lemmy, Discord, etc.
//!
//! # ID mapping
//!
//! | Reddit concept | Poly ID |
//! |---|---|
//! | Subreddit `<sub>` | Server `r_<sub>`, Channel `c_posts_<sub>` |
//! | Post `<id>` (t3) | Message `t3_<id>` |
//! | Comment `<id>` (t1) | Message `t1_<id>` |
//! | DM `<id>` (t4) | DmChannel `dm_<id>`, Message `t4_<id>` |
//! | Username `<name>` | User `u_<name>` |

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::*;
use std::pin::Pin;

use crate::{RedditClient, RedditError, SortKind};
use crate::parser::{RawComment, RawDm, RawPost, UserProfile};

/// Strip HTML tags + decode common entities from a reddit comment body.
///
/// Reddit's parser emits `body_html` already converted from markdown, but
/// `MessageContent::Text` is rendered as plain text by the chat view (no
/// HTML interpretation). This conversion gives readable text — paragraphs
/// joined with newlines, lists flattened, links shown as link text only
/// (URLs lost). Lossy but the right floor for the existing chat-view.
///
/// Future improvement: round-trip HTML → markdown so the chat-view's
/// markdown renderer can lay out lists / links / code blocks properly.
fn html_to_plain_text(html: &str) -> String {
    // Replace block-level closing tags with double-newline so paragraphs
    // and list items separate visually.
    let mut s = html.to_string();
    for close in ["</p>", "</li>", "</div>", "</blockquote>", "<br>", "<br/>", "<br />"] {
        s = s.replace(close, "\n\n");
    }
    // Strip remaining tags via a tiny state-machine.
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    // Decode the common HTML entities reddit emits.
    out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    // Collapse runs of 3+ newlines down to 2.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

/// Walk a comment tree depth-first and push each comment as a flat
/// Message into the output Vec. Used by `get_messages` for the
/// per-post comment-fetch route (`hn-post-<pid>`) so ForumPostView
/// can render the thread as a flat list (Message-level reply_to
/// threading is a separate, future pass).
fn flatten_comments_into_messages(
    comments: &[RawComment],
    backend: &BackendType,
    out: &mut Vec<Message>,
) {
    for c in comments {
        out.push(Message {
            id: format!("t1_{}", c.id),
            author: User {
                id: user_id_for_name(&c.author),
                display_name: c.author.clone(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: backend.clone(),
            },
            // body_html is reddit's pre-rendered HTML; the chat-view
            // renders MessageContent::Text as plain text (no HTML
            // interpretation), so strip tags + decode entities first.
            content: MessageContent::Text(html_to_plain_text(&c.body_html)),
            timestamp: c.timestamp,
            attachments: Vec::new(),
            reactions: Vec::new(),
            reply_to: None,
            edited: false,
            thread: None,
            preview_image_url: None,
        });
        if !c.replies.is_empty() {
            flatten_comments_into_messages(&c.replies, backend, out);
        }
    }
}

/// Recursively emit reddit comments as depth-indented sanitized HTML.
/// Used by `get_view_detail` to inline the comment thread under the
/// post body (TreeSpec-via-ViewRow doesn't support hierarchy yet).
fn render_comments_to_html(out: &mut String, comments: &[RawComment], depth: u32, max_depth: u32) {
    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let indent_px = depth.min(max_depth).saturating_mul(16);
    for comment in comments {
        out.push_str(&format!(
            "<div class=\"reddit-comment\" style=\"margin-left:{indent_px}px\">"
        ));
        out.push_str(&format!(
            "<div class=\"reddit-comment-meta\">u/{} · {} points</div>",
            html_escape(&comment.author),
            comment.score,
        ));
        // Body is already HTML-rendered by the parser (markdown → HTML by
        // reddit), so include verbatim — host's CustomBlock sanitizer
        // strips dangerous tags downstream.
        out.push_str(&format!(
            "<div class=\"reddit-comment-body\">{}</div>",
            comment.body_html,
        ));
        out.push_str("</div>");
        if depth < max_depth && !comment.replies.is_empty() {
            render_comments_to_html(out, &comment.replies, depth.saturating_add(1), max_depth);
        }
    }
}

// ─── ID helpers ─────────────────────────────────────────────────────────────

fn server_id_for_sub(sub: &str) -> String {
    format!("r_{sub}")
}

fn sub_from_server_id(id: &str) -> Option<&str> {
    id.strip_prefix("r_")
}

fn channel_id_for_sub(sub: &str) -> String {
    format!("c_posts_{sub}")
}

fn sub_from_channel_id(id: &str) -> Option<&str> {
    id.strip_prefix("c_posts_")
}

fn message_id_for_post(post_id: &str) -> String {
    format!("t3_{post_id}")
}

fn _message_id_for_comment(comment_id: &str) -> String {
    format!("t1_{comment_id}")
}

fn message_id_for_dm(dm_id: &str) -> String {
    format!("t4_{dm_id}")
}

fn dm_channel_id_for_dm(dm_id: &str) -> String {
    format!("dm_{dm_id}")
}

fn _dm_id_from_channel_id(id: &str) -> Option<&str> {
    id.strip_prefix("dm_")
}

fn user_id_for_name(name: &str) -> String {
    format!("u_{name}")
}

// ─── Error mapping ───────────────────────────────────────────────────────────

impl From<RedditError> for ClientError {
    fn from(e: RedditError) -> Self {
        match e {
            RedditError::LoggedOut => {
                ClientError::AuthFailed("Session cookie missing or expired".to_string())
            }
            RedditError::Status(401) | RedditError::Status(403) => {
                ClientError::AuthFailed(format!("HTTP {}", e))
            }
            RedditError::Status(404) => ClientError::NotFound(e.to_string()),
            RedditError::Http(s) => ClientError::Network(s),
            RedditError::Parse(p) => ClientError::Internal(p.to_string()),
            RedditError::Status(s) => ClientError::Network(format!("HTTP {s}")),
        }
    }
}

// ─── Mapping helpers ─────────────────────────────────────────────────────────

fn raw_post_to_message(post: &RawPost, backend: &BackendType) -> Message {
    let content = if let Some(body) = &post.body {
        let body_text = html_to_plain_text(body);
        if !body_text.is_empty() {
            MessageContent::Text(format!("{}\n\n{}", post.title, body_text))
        } else {
            MessageContent::Text(post.title.clone())
        }
    } else if let Some(url) = &post.url {
        MessageContent::Text(format!("{}\n\n{}", post.title, url))
    } else {
        MessageContent::Text(post.title.clone())
    };

    // Add an attachment for image previews so the message view can render them.
    // For video posts we use the preview thumbnail (if available) and mark the
    // attachment content-type as video/mp4 as a hint. Galleries get a single
    // cover-image attachment.
    let mut attachments = Vec::new();
    if let Some(ref preview) = post.preview_url {
        let (content_type, filename) = if post.is_video {
            ("video/mp4", "video_preview.jpg")
        } else {
            ("image/png", "preview.png")
        };
        attachments.push(Attachment::remote(
            format!("reddit-preview-{}", post.id),
            filename.to_string(),
            content_type.to_string(),
            preview.clone(),
            0,
        ));
    }

    Message {
        id: message_id_for_post(&post.id),
        author: User {
            id: user_id_for_name(&post.author),
            display_name: post.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        content,
        timestamp: post.timestamp,
        attachments,
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: post.preview_url.clone(),
    }
}

fn raw_dm_to_dm_channel(dm: &RawDm, account_id: &str, backend: &BackendType) -> DmChannel {
    let last_message = Message {
        id: message_id_for_dm(&dm.id),
        author: User {
            id: user_id_for_name(&dm.author),
            display_name: dm.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        content: MessageContent::Text(dm.subject.clone()),
        timestamp: dm.timestamp,
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: None,
    };

    DmChannel {
        id: dm_channel_id_for_dm(&dm.id),
        user: User {
            id: user_id_for_name(&dm.author),
            display_name: dm.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        last_message: Some(last_message),
        unread_count: 0,
        backend: backend.clone(),
        account_id: account_id.to_string(),
    }
}

fn user_profile_to_user(profile: &UserProfile, backend: &BackendType) -> User {
    User {
        id: user_id_for_name(&profile.name),
        display_name: profile.name.clone(),
        avatar_url: profile.avatar_url.clone(),
        presence: PresenceStatus::Offline,
        backend: backend.clone(),
    }
}

fn raw_post_to_viewrow(post: &RawPost, show_previews: bool) -> ViewRow {
    let secondary = format!("by u/{}", post.author);
    let preview_image_url = if show_previews { post.preview_url.clone() } else { None };

    ViewRow {
        id: message_id_for_post(&post.id),
        primary_text: post.title.clone(),
        secondary_text: Some(secondary),
        // SCORE: prefix is load-bearing for the forum-post-card render path in
        // list_body.rs — ListBodyRow renders the vote-card shape when it appears.
        meta_text: Some(format!("SCORE:{} · {} comments", post.score, post.comment_count)),
        icon: None,
        badge: None,
        context_menu_target_kind: MenuTargetKind::Message,
        preview_image_url,
        is_video: post.is_video,
    }
}

fn build_sub_server(
    sub: &str,
    account_id: &str,
    account_display_name: &str,
    backend: &BackendType,
) -> Server {
    Server {
        id: server_id_for_sub(sub),
        name: format!("r/{sub}"),
        icon_url: None,
        banner_url: None,
        categories: vec![Category {
            id: format!("cat_{sub}"),
            name: "Channels".to_string(),
            channel_ids: vec![channel_id_for_sub(sub)],
        }],
        backend: backend.clone(),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id: Some(channel_id_for_sub(sub)),
        description: None,
        star_count: None,
        language: None,
        forks_count: None,
        open_issues_count: None,
    }
}

fn build_sub_channel(sub: &str) -> Channel {
    Channel {
        id: channel_id_for_sub(sub),
        name: "posts".to_string(),
        channel_type: ChannelType::Forum,
        server_id: server_id_for_sub(sub),
        unread_count: 0,
        mention_count: 0,
        last_message_id: None,
        forum_tags: None,
        parent_channel_id: None,
        thread_metadata: None,
    }
}

// ─── Sort state helpers ───────────────────────────────────────────────────────

/// Stable string key used to persist a `SortKind` in `settings_storage`.
fn sort_kind_to_str(sort: SortKind) -> &'static str {
    match sort {
        SortKind::Hot => "hot",
        SortKind::New => "new",
        SortKind::Rising => "rising",
        SortKind::Controversial => "controversial",
        SortKind::Top => "top",
        SortKind::TopHour => "top-hour",
        SortKind::TopDay => "top-day",
        SortKind::TopWeek => "top-week",
        SortKind::TopMonth => "top-month",
        SortKind::TopYear => "top-year",
        SortKind::TopAll => "top-all",
    }
}

/// Parse a persisted sort key back into a `SortKind`.
///
/// Returns `SortKind::Hot` for unrecognised or absent values (safe default).
fn sort_kind_from_str(s: &str) -> SortKind {
    match s {
        "hot" => SortKind::Hot,
        "new" => SortKind::New,
        "rising" => SortKind::Rising,
        "controversial" => SortKind::Controversial,
        "top" => SortKind::Top,
        "top-hour" => SortKind::TopHour,
        "top-day" => SortKind::TopDay,
        "top-week" => SortKind::TopWeek,
        "top-month" => SortKind::TopMonth,
        "top-year" => SortKind::TopYear,
        "top-all" => SortKind::TopAll,
        _ => SortKind::Hot,
    }
}

// ─── State storage for session ───────────────────────────────────────────────

/// `ClientBackend` adapter wrapping a `RedditClient` + optional session.
pub struct RedditBackend {
    client: RedditClient,
    session: Option<Session>,
    /// In-memory settings storage (mirrors Lemmy's stub pattern, Phase 4).
    settings_storage: SettingsStorageCell,
}

impl RedditBackend {
    /// Create a new backend from an already-constructed `RedditClient`.
    pub fn new(client: RedditClient) -> Self {
        Self { client, session: None, settings_storage: SettingsStorageCell::new() }
    }

    /// Read the `show-media-previews` mechanism state.
    ///
    /// Defaults to `true` (previews shown) when the user has never toggled it.
    fn media_previews_enabled(&self) -> bool {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "show-media-previews")
            .is_none_or(|v| v != "false")
    }

    /// Read the current sort mode.
    ///
    /// Defaults to `SortKind::Hot` when the user has never chosen a sort.
    fn current_sort(&self) -> SortKind {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "current-sort")
            .as_deref()
            .map(sort_kind_from_str)
            .unwrap_or(SortKind::Hot)
    }

    fn backend_type() -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn account_id(&self) -> &str {
        self.session.as_ref().map_or("reddit-anon", |s| s.id.as_str())
    }

    fn account_display_name(&self) -> &str {
        self.session
            .as_ref()
            .map_or("Anonymous", |s| s.user.display_name.as_str())
    }

    /// Build a `Session` for the given username.
    fn build_session(&self, username: &str) -> Session {
        // `token` is what gets persisted to KV and replayed via
        // `authenticate(Token(t))` on next app boot. It MUST be the
        // session-cookie value captured during login_with_password, not
        // the bare username — otherwise restore re-authenticates with
        // a string the server doesn't recognise as a session.
        // Falls back to username if (somehow) login didn't capture a
        // session — caller can still re-login with password from the UI.
        let token = self
            .client
            .session_cookie_value()
            .unwrap_or_else(|| username.to_string());
        Session {
            id: format!("reddit-{username}"),
            user: User {
                id: user_id_for_name(username),
                display_name: username.to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: Self::backend_type(),
            },
            token,
            backend: Self::backend_type(),
            icon_emoji: Some("🤖".to_string()),
            instance_id: "old.reddit.com".to_string(),
            backend_url: Some(self.client.base_url().to_string()),
        }
    }
}

// ─── ClientBackend impl ───────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for RedditBackend {
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
                "Reddit only supports EmailPassword and Token credentials".to_string(),
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
        _channel_id: &str,
        _content: MessageContent,
    ) -> ClientResult<Message> {
        Err(ClientError::NotSupported(
            "reddit submit (top-level post) not yet implemented".to_string(),
        ))
    }

    async fn send_reply_message(
        &self,
        _channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match &content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        };

        // reply_to_message_id is t3_<id> for posts or t1_<id> for comments.
        let is_post = reply_to_message_id.starts_with("t3_");
        let is_comment = reply_to_message_id.starts_with("t1_");

        if is_post || is_comment {
            self.client
                .reply_comment(reply_to_message_id, &text)
                .await
                .map_err(ClientError::from)?;
        } else {
            return Err(ClientError::NotSupported(format!(
                "cannot reply to id: {reply_to_message_id}"
            )));
        }

        // Reddit's reply endpoint does not return the new comment ID.
        // Return a placeholder message so the host can show optimistic send.
        let now = chrono::Utc::now();
        let account_display = self.account_display_name().to_string();
        let bt = Self::backend_type();
        Ok(Message {
            id: format!("t1_pending-{}", now.timestamp_millis()),
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
            let (post, comments) = self
                .client
                .get_post(bare_id)
                .await
                .map_err(ClientError::from)?;

            // Always attempt the gallery JSON fetch — for a non-gallery
            // post it returns Ok(empty) cheaply; for a gallery post it
            // gives us the full ordered list of source URLs that the
            // HTML scrape doesn't expose. Append each as an Attachment
            // on the OP message so ForumThreadView renders the carousel.
            let gallery_urls: Vec<String> = if self.media_previews_enabled() {
                self.client.get_gallery_urls(bare_id).await.unwrap_or_default()
            } else {
                Vec::new()
            };

            let mut op_msg = raw_post_to_message(&post, &bt);
            if gallery_urls.len() >= 2 {
                op_msg.attachments.clear();
                for (i, url) in gallery_urls.iter().enumerate() {
                    op_msg.attachments.push(Attachment::remote(
                        format!("reddit-gallery-{bare_id}-{i}"),
                        format!("gallery_{i}.jpg"),
                        "image/jpeg".to_string(),
                        url.clone(),
                        0,
                    ));
                }
                if op_msg.preview_image_url.is_none() {
                    op_msg.preview_image_url = gallery_urls.first().cloned();
                }
            }

            let mut messages = Vec::new();
            messages.push(op_msg);
            flatten_comments_into_messages(&comments, &bt, &mut messages);
            return Ok(messages);
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

    // ── Voice ─────────────────────────────────────────────────────────────────

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(Vec::new())
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

    // ── D-series UI extension (stubs matching hackernews pattern) ────────────

    async fn get_context_menu_items(
        &self,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(Vec::new())
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown action: {action_id}")))
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        // Reddit has no per-server / per-channel settings exposed yet.
        Ok(Vec::new())
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
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

    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        let items = vec![
            SidebarItem {
                id: "sort-reddit-hot".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-hot".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-new".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-new".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-rising".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-rising".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-controversial".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-controversial".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-top".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            // "Top by time" sub-modes — nested under sort-reddit-top.
            SidebarItem {
                id: "sort-reddit-top-hour".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-hour".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-day".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-day".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-week".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-week".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-month".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-month".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-year".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-year".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-all".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-all".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
        ];
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::SortModes,
            sections: vec![SidebarSection {
                header_key: None,
                collapsible: false,
                default_collapsed: false,
                items,
            }],
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        let sort = match action_id {
            "sort-reddit-hot" => SortKind::Hot,
            "sort-reddit-new" => SortKind::New,
            "sort-reddit-rising" => SortKind::Rising,
            "sort-reddit-controversial" => SortKind::Controversial,
            "sort-reddit-top" => SortKind::Top,
            "sort-reddit-top-hour" => SortKind::TopHour,
            "sort-reddit-top-day" => SortKind::TopDay,
            "sort-reddit-top-week" => SortKind::TopWeek,
            "sort-reddit-top-month" => SortKind::TopMonth,
            "sort-reddit-top-year" => SortKind::TopYear,
            "sort-reddit-top-all" => SortKind::TopAll,
            _ => {
                return Err(ClientError::NotFound(format!(
                    "unknown sidebar action: {action_id}"
                )));
            }
        };
        self.settings_storage.set(
            SettingsScope::AccountGlobal,
            "",
            "current-sort",
            sort_kind_to_str(sort),
        )?;
        Ok(ActionOutcome::RefreshTarget)
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: None,
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("author".to_string()),
                    meta_field: Some("score-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 25,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        let sub = sub_from_channel_id(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {channel_id}")))?;

        let posts = self
            .client
            .list_subreddit(sub, self.current_sort())
            .await
            .map_err(ClientError::from)?;

        let show_previews = self.media_previews_enabled();

        let rows = posts
            .iter()
            .map(|p| raw_post_to_viewrow(p, show_previews))
            .collect();

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        // ViewRow ids are emitted as `t3_<post_id>` by raw_post_to_viewrow.
        let post_id = row_id
            .strip_prefix("t3_")
            .ok_or_else(|| ClientError::NotFound(format!("get_view_detail: not a t3_ row: {row_id}")))?;

        // Fetch the post + comment tree. Comments are rendered inline via
        // depth-indented HTML inside body_block (TreeSpec needs hierarchy
        // support on ViewRow that doesn't exist yet).
        let (post, comments) = self.client.get_post(post_id).await.map_err(ClientError::from)?;

        // Always try the gallery JSON — cheap (one extra request) and
        // robust against parser misclassification of is_gallery.
        // Empty Vec falls back to single-cover-image render below.
        let gallery_from_json = self
            .client
            .get_gallery_urls(post_id)
            .await
            .unwrap_or_default();

        // If the JSON gave us multiple URLs, that's a real gallery.
        // Otherwise fall back to the cover preview (single image post).
        let gallery_urls: Vec<String> = if gallery_from_json.len() >= 2 {
            gallery_from_json
        } else if let Some(ref preview) = post.preview_url {
            vec![preview.clone()]
        } else {
            Vec::new()
        };
        let is_real_gallery = gallery_urls.len() >= 2;

        fn html_escape(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        }

        let mut html = String::new();
        // Title heading.
        html.push_str(&format!("<h3>{}</h3>", html_escape(&post.title)));
        // Author + score line.
        html.push_str(&format!(
            "<p class=\"reddit-post-meta\">by u/{} · {} points · {} comments</p>",
            html_escape(&post.author),
            post.score,
            post.comment_count,
        ));
        // External URL link, if any (for non-image link posts).
        if let Some(ref url) = post.url
            && gallery_urls.is_empty()
        {
            let escaped = html_escape(url);
            html.push_str(&format!(
                "<p class=\"reddit-post-link\"><a href=\"{escaped}\">{escaped}</a></p>"
            ));
        }
        // Self-post body markdown (already HTML-rendered by parser).
        if let Some(ref body) = post.body
            && !body.is_empty()
        {
            html.push_str(&format!("<div class=\"reddit-post-body\">{body}</div>"));
        }
        // Gallery / single-image rendering. Multi-image (>=2) posts use a
        // scroll-snap carousel; single-image posts get a centered cover.
        if !gallery_urls.is_empty() {
            let wrapper_class = if is_real_gallery {
                "reddit-gallery reddit-gallery-carousel"
            } else {
                "reddit-gallery"
            };
            html.push_str(&format!("<div class=\"{wrapper_class}\">"));
            for (i, url) in gallery_urls.iter().enumerate() {
                let alt = if is_real_gallery {
                    format!("Gallery image {}/{}", i + 1, gallery_urls.len())
                } else {
                    "Post image".to_string()
                };
                html.push_str(&format!(
                    "<img class=\"reddit-gallery-item\" src=\"{}\" alt=\"{}\" loading=\"lazy\" />",
                    html_escape(url),
                    html_escape(&alt),
                ));
            }
            html.push_str("</div>");
            if is_real_gallery {
                html.push_str(&format!(
                    "<p class=\"reddit-gallery-count\">{} images — swipe / scroll to view</p>",
                    gallery_urls.len(),
                ));
            }
        }

        // Threaded comments rendered inline. Each RawComment becomes a
        // .reddit-comment block with depth-indented left margin (capped
        // at depth 8 to avoid runaway indentation).
        if !comments.is_empty() {
            html.push_str(&format!(
                "<h4 class=\"reddit-comments-heading\">Comments ({})</h4>",
                post.comment_count.min(9999),
            ));
            html.push_str("<div class=\"reddit-comments\">");
            render_comments_to_html(&mut html, &comments, 0, 8);
            html.push_str("</div>");
        }

        // Scoped stylesheet — flex strip for single images, scroll-snap
        // carousel for multi-image galleries, depth-indented comments.
        let stylesheet = Some(
            ".reddit-post-meta { color: var(--text-muted, #888); font-size: 0.85rem; }
             .reddit-post-body { margin: 12px 0; line-height: 1.5; }
             .reddit-post-link a { color: var(--text-link, #60a5fa); word-break: break-all; }
             .reddit-gallery {
                 display: flex;
                 gap: 8px;
                 margin-top: 12px;
                 align-items: flex-start;
             }
             .reddit-gallery-carousel {
                 overflow-x: auto;
                 scroll-snap-type: x mandatory;
                 scroll-behavior: smooth;
                 padding-bottom: 8px;
             }
             .reddit-gallery-carousel .reddit-gallery-item {
                 scroll-snap-align: center;
                 flex: 0 0 auto;
             }
             .reddit-gallery-item {
                 max-width: min(100%, 480px);
                 max-height: 540px;
                 object-fit: contain;
                 border-radius: 6px;
                 background: rgba(0, 0, 0, 0.3);
             }
             .reddit-gallery-count {
                 color: var(--text-muted, #888);
                 font-size: 0.8rem;
                 margin: 4px 0 0;
             }
             .reddit-comments-heading {
                 margin-top: 24px;
                 padding-top: 12px;
                 border-top: 1px solid var(--border-primary, #333);
             }
             .reddit-comments { display: flex; flex-direction: column; gap: 12px; }
             .reddit-comment {
                 padding: 8px 12px;
                 border-left: 2px solid var(--border-primary, #333);
                 background: rgba(255, 255, 255, 0.02);
                 border-radius: 0 4px 4px 0;
             }
             .reddit-comment-meta {
                 color: var(--text-muted, #888);
                 font-size: 0.78rem;
                 margin-bottom: 4px;
             }
             .reddit-comment-body { line-height: 1.45; }
             .reddit-comment-body p { margin: 4px 0; }"
                .to_string(),
        );

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html: html,
                stylesheet,
                max_height_px: None,
            },
            comments_section: None,
        })
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(Vec::new())
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(Vec::new())
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!(
            "unknown composer action: {action_id}"
        )))
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!(
            "unknown message action: {action_id}"
        )))
    }

    // ── Phase E: community search ─────────────────────────────────────────────

    async fn search_communities(
        &self,
        query: &str,
        _scope: poly_client::CommunityScope,
        cursor: Option<String>,
    ) -> poly_client::ClientResult<poly_client::CommunityPage> {
        let (subs, next_after) = self
            .client
            .search_subreddits(query, cursor.as_deref())
            .await
            .map_err(ClientError::from)?;

        let account_id = self.account_id().to_string();
        let account_display_name = self.account_display_name().to_string();
        let bt = Self::backend_type();

        let items = subs
            .into_iter()
            .map(|sub| {
                let mut server = build_sub_server(&sub.name, &account_id, &account_display_name, &bt);
                if let Some(url) = sub.icon_url {
                    server.icon_url = Some(url);
                }
                server
            })
            .collect();

        Ok(poly_client::CommunityPage {
            items,
            next_cursor: next_after,
        })
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for RedditBackend {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let name = id
            .strip_prefix("u_")
            .ok_or_else(|| ClientError::NotFound(format!("user not found: {id}")))?;

        let profile = self
            .client
            .get_user(name)
            .await
            .map_err(ClientError::from)?;

        Ok(user_profile_to_user(&profile, &Self::backend_type()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no friend system".to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no friend system".to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no friend system".to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no friend system".to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no user note system".to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit: block not supported via this interface".to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit: unblock not supported via this interface".to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no ignore concept".to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no ignore concept".to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no presence system".to_string()))
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// Reddit supports inbox messages as DMs. No group DMs.

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for RedditBackend {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let dms = self.client.inbox().await.map_err(ClientError::from)?;
        let account_id = self.account_id();
        let bt = Self::backend_type();
        Ok(dms
            .iter()
            .map(|dm| raw_dm_to_dm_channel(dm, account_id, &bt))
            .collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: not yet implemented for Reddit".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Reddit has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no group DMs".to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no group DMs".to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no group DMs".to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "close_dm_channel: not yet implemented for Reddit".to_string(),
        ))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no conversation mute API".to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no conversation mute API".to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no group DMs".to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Reddit has no group DMs".to_string()))
    }
}
