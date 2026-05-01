//! Lemmy REST API v3 types and HTTP client.
//!
//! All types are intentionally kept internal to `poly-lemmy` so that
//! external crates stay isolated from Lemmy protocol details.

use chrono::{DateTime, Utc};
use poly_client::{
    Attachment, BackendType, Category, Channel, ChannelType, ClientError, ClientResult, Cursor,
    CursorKind, DmChannel, MenuTargetKind, Message, MessageContent, PresenceStatus, Reaction,
    Server, User, ViewRow,
};
use poly_host_bridge::http::{HttpClient, StatusCode};

/// Default User-Agent for Lemmy API requests.
pub const DEFAULT_CLIENT_VERSION: &str = "poly-lemmy/0.0.0";
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

// ── Request bodies ──────────────────────────────────────────────────────────

/// Login payload for `POST /api/v3/user/login`.
#[derive(Debug, Clone, Serialize)]
pub struct LoginRequest {
    pub username_or_email: String,
    pub password: String,
}

/// Comment create payload for `POST /api/v3/comment`.
#[derive(Debug, Clone, Serialize)]
pub struct CreateCommentRequest {
    pub content: String,
    pub post_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
}

/// `POST /api/v3/community/ban_user` — ban or unban a person from a community.
#[derive(Debug, Clone, Serialize)]
pub struct BanFromCommunityRequest {
    pub community_id: i64,
    pub person_id: i64,
    /// `true` to ban, `false` to unban.
    pub ban: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Unix timestamp (i64) when the ban expires. `None` = permanent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<i64>,
    pub remove_data: bool,
}

/// `POST /api/v3/post/remove` — remove a post as moderator.
#[derive(Debug, Clone, Serialize)]
pub struct RemovePostRequest {
    pub post_id: i64,
    pub removed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `POST /api/v3/comment/remove` — remove a comment as moderator.
#[derive(Debug, Clone, Serialize)]
pub struct RemoveCommentRequest {
    pub comment_id: i64,
    pub removed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `POST /api/v3/post` — create a new post in a community.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePostRequest {
    pub name: String,
    pub community_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Response from `POST /api/v3/post` — create post.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePostResponse {
    pub post_view: PostView,
}

// ── Response bodies ─────────────────────────────────────────────────────────

/// Response from `POST /api/v3/user/login`.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub jwt: Option<String>,
}

/// A Lemmy person (user).
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyPerson {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

/// A Lemmy community.
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyCommunity {
    pub id: i64,
    /// Short handle name (e.g. `"rust"`). Absent in some embedded responses
    /// (e.g. modlog entries) so we default to empty string.
    #[serde(default)]
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub icon: Option<String>,
    pub banner: Option<String>,
}

/// Aggregate counts for a community (subscribers, active users, etc.).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommunityCounts {
    #[serde(default)]
    pub subscribers: i64,
    #[serde(default)]
    pub users_active_week: i64,
}

/// A community view as returned in list responses.
#[derive(Debug, Clone, Deserialize)]
pub struct CommunityView {
    pub community: LemmyCommunity,
    /// Subscription state from Lemmy's `SubscribedType` enum — one of
    /// `"Subscribed"`, `"NotSubscribed"`, or `"Pending"`. Optional because
    /// unauthenticated list responses omit it.
    #[serde(default)]
    pub subscribed: Option<String>,
    /// Aggregate stats: subscriber count, active-user count, etc.
    #[serde(default)]
    pub counts: CommunityCounts,
}

/// Response from `GET /api/v3/community/list`.
#[derive(Debug, Clone, Deserialize)]
pub struct CommunityListResponse {
    pub communities: Vec<CommunityView>,
}

/// Post counts sub-object.
#[derive(Debug, Clone, Deserialize)]
pub struct PostCounts {
    pub upvotes: i64,
    pub downvotes: i64,
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub comments: i64,
}

/// A Lemmy post.
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyPost {
    pub id: i64,
    pub name: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub published: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ap_id: Option<String>,
    /// Preview thumbnail URL — set by pict-rs when the post URL's Open Graph
    /// image is available. Verified present in real Lemmy API responses:
    /// `curl https://lemmy.world/api/v3/post/list?limit=1` returns
    /// `"thumbnail_url": "https://lemmy.world/pictrs/image/<uuid>.png"`.
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

/// A post view as returned in list responses.
#[derive(Debug, Clone, Deserialize)]
pub struct PostView {
    pub post: LemmyPost,
    pub creator: LemmyPerson,
    pub counts: PostCounts,
    pub my_vote: Option<i32>,
}

/// Response from `GET /api/v3/post/list`.
#[derive(Debug, Clone, Deserialize)]
pub struct PostListResponse {
    pub posts: Vec<PostView>,
}

/// Comment counts sub-object.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentCounts {
    pub upvotes: i64,
    pub downvotes: i64,
}

/// A Lemmy comment.
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyComment {
    pub id: i64,
    pub content: String,
    pub published: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
}

/// A comment view as returned in list responses.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentView {
    pub comment: LemmyComment,
    pub creator: LemmyPerson,
    pub counts: CommentCounts,
    pub my_vote: Option<i32>,
}

/// Response from `GET /api/v3/comment/list`.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentListResponse {
    pub comments: Vec<CommentView>,
}

/// A private message.
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyPrivateMessage {
    pub id: i64,
    pub content: String,
    pub creator_id: i64,
    pub published: DateTime<Utc>,
    pub read: bool,
}

/// A private message view.
#[derive(Debug, Clone, Deserialize)]
pub struct PrivateMessageView {
    pub private_message: LemmyPrivateMessage,
    pub creator: LemmyPerson,
    pub recipient: LemmyPerson,
}

/// Response from `GET /api/v3/private_message/list`.
#[derive(Debug, Clone, Deserialize)]
pub struct PrivateMessageListResponse {
    pub private_messages: Vec<PrivateMessageView>,
}

/// Site info response (used to get current user).
#[derive(Debug, Clone, Deserialize)]
pub struct SiteResponse {
    pub my_user: Option<MyUserInfo>,
}

/// Current user info block from site response.
#[derive(Debug, Clone, Deserialize)]
pub struct MyUserInfo {
    pub local_user_view: LocalUserView,
}

/// Local user view.
#[derive(Debug, Clone, Deserialize)]
pub struct LocalUserView {
    pub person: LemmyPerson,
}

/// Response from `POST /api/v3/community/ban_user`.
///
/// Fields kept for full protocol fidelity — `banned_person` is decoded but
/// not currently consumed by the caller (the return value is discarded).
#[derive(Debug, Clone, Deserialize)]
pub struct BanFromCommunityResponse {
    #[allow(dead_code)] // lint-allow-unused: kept for protocol-fidelity decode
    pub banned_person: PersonView,
    #[allow(dead_code)] // lint-allow-unused: kept for protocol-fidelity decode
    pub banned: bool,
}

/// A person view (person + counts).
#[derive(Debug, Clone, Deserialize)]
pub struct PersonView {
    #[allow(dead_code)] // lint-allow-unused: kept for protocol-fidelity decode
    pub person: LemmyPerson,
}

/// A single `ModBanFromCommunity` entry in the modlog.
#[derive(Debug, Clone, Deserialize)]
pub struct ModBanFromCommunityView {
    pub mod_ban_from_community: ModBanFromCommunity,
    pub moderator: Option<LemmyPerson>,
    pub banned_person: LemmyPerson,
    /// Community context decoded for completeness; not currently read.
    #[allow(dead_code)] // lint-allow-unused: kept for protocol-fidelity decode
    pub community: LemmyCommunity,
}

/// The core modlog record for a community ban event.
#[derive(Debug, Clone, Deserialize)]
pub struct ModBanFromCommunity {
    pub id: i64,
    pub when_: DateTime<Utc>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub banned: bool,
    #[serde(default)]
    pub expires: Option<DateTime<Utc>>,
}

/// A single `ModRemovePost` entry in the modlog.
#[derive(Debug, Clone, Deserialize)]
pub struct ModRemovePostView {
    pub mod_remove_post: ModRemovePost,
    pub moderator: Option<LemmyPerson>,
    pub post: LemmyPost,
    pub community: LemmyCommunity,
}

/// Core modlog record for a post-remove event.
#[derive(Debug, Clone, Deserialize)]
pub struct ModRemovePost {
    pub id: i64,
    pub when_: DateTime<Utc>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // lint-allow-unused: kept for protocol-fidelity decode
    pub removed: bool,
}

/// A single `ModRemoveComment` entry in the modlog.
#[derive(Debug, Clone, Deserialize)]
pub struct ModRemoveCommentView {
    pub mod_remove_comment: ModRemoveComment,
    pub moderator: Option<LemmyPerson>,
    pub comment: LemmyComment,
    pub commenter: LemmyPerson,
    pub community: LemmyCommunity,
}

/// Core modlog record for a comment-remove event.
#[derive(Debug, Clone, Deserialize)]
pub struct ModRemoveComment {
    pub id: i64,
    pub when_: DateTime<Utc>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // lint-allow-unused: kept for protocol-fidelity decode
    pub removed: bool,
}

/// Response from `GET /api/v3/modlog`.
///
/// Only the arrays we consume are decoded; unknown keys are ignored.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GetModlogResponse {
    #[serde(default)]
    pub banned_from_community: Vec<ModBanFromCommunityView>,
    #[serde(default)]
    pub removed_posts: Vec<ModRemovePostView>,
    #[serde(default)]
    pub removed_comments: Vec<ModRemoveCommentView>,
}

// ── Conversion helpers ───────────────────────────────────────────────────────

/// Map a `LemmyPerson` to a Poly `User`.
pub fn map_person(person: &LemmyPerson) -> User {
    User {
        id: format!("lemmy-user-{}", person.id),
        display_name: person
            .display_name
            .clone()
            .unwrap_or_else(|| person.name.clone()),
        avatar_url: person.avatar.clone(),
        presence: PresenceStatus::Offline,
        backend: BackendType::from("lemmy"),
    }
}

/// Map a `CommunityView` to a Poly `Server`.
pub fn map_community_to_server(view: &CommunityView, account_id: &str, account_display_name: &str) -> Server {
    let community = &view.community;
    let channel_id = format!("lemmy-feed-{}", community.id);
    Server {
        id: format!("lemmy-community-{}", community.id),
        name: community.title.clone(),
        icon_url: community.icon.clone(),
        banner_url: community.banner.clone(),
        categories: vec![Category {
            id: "posts".to_string(),
            name: "Posts".to_string(),
            channel_ids: vec![channel_id],
        }],
        backend: BackendType::from("lemmy"),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id: None,
        description: None,
        star_count: None,
        language: None,
        forks_count: None,
        open_issues_count: None,
    }
}

/// Map a `CommunityView` to a Poly `ViewRow` for the account overview card grid.
///
/// - `primary_text`   — community title (display name)
/// - `secondary_text` — short handle name (`!rust@lemmy.example.com` style) or description
/// - `meta_text`      — `"X subscribers · Y active · Z unread"`
pub fn map_community_to_viewrow(view: &CommunityView, unread: u32) -> ViewRow {
    let community = &view.community;
    let counts = &view.counts;

    let secondary = community
        .description
        .as_deref()
        .filter(|d| !d.is_empty()).map_or_else(|| community.name.clone(), std::string::ToString::to_string);

    let meta = format!(
        "{} subscribers · {} active · {} unread",
        counts.subscribers, counts.users_active_week, unread,
    );

    ViewRow {
        id: format!("lemmy-community-{}", community.id),
        primary_text: community.title.clone(),
        secondary_text: Some(secondary),
        meta_text: Some(meta),
        icon: community.icon.clone(),
        badge: if unread > 0 { Some(unread.to_string()) } else { None },
        context_menu_target_kind: MenuTargetKind::Server,
        preview_image_url: None,
    }
}

/// Map a community ID to its implicit forum `Channel`.
pub fn community_to_channel(community: &LemmyCommunity) -> Channel {
    Channel {
        id: format!("lemmy-feed-{}", community.id),
        name: community.title.clone(),
        channel_type: ChannelType::Forum,
        server_id: format!("lemmy-community-{}", community.id),
        unread_count: 0,
        mention_count: 0,
        last_message_id: None,
        forum_tags: None,
        parent_channel_id: None,
        thread_metadata: None,
    }
}

/// Map a `PostView` to a Poly `Message`.
///
/// The post title becomes the message content. URL and body are appended as
/// attachments (body as inline text attachment, URL as a remote attachment).
pub fn map_post_to_message(view: &PostView) -> Message {
    let post = &view.post;
    let creator = &view.creator;
    let counts = &view.counts;

    let mut content_text = post.name.clone();
    if let Some(body) = &post.body {
        content_text.push('\n');
        content_text.push_str(body);
    }

    let mut attachments = Vec::new();
    if let Some(url) = &post.url {
        attachments.push(Attachment::remote(
            format!("lemmy-post-url-{}", post.id),
            "link".to_string(),
            "text/uri-list".to_string(),
            url.clone(),
            0,
        ));
    }

    let reactions = vec![
        Reaction {
            emoji: "upvote".to_string(),
            count: u32::try_from(counts.upvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(1_i32),
        },
        Reaction {
            emoji: "downvote".to_string(),
            count: u32::try_from(counts.downvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(-1_i32),
        },
    ];

    Message {
        id: format!("lemmy-post-{}", post.id),
        author: map_person(creator),
        content: MessageContent::Text(content_text),
        timestamp: post.published,
        attachments,
        reactions,
        reply_to: None,
        edited: post.updated.is_some(),
        thread: None,
        preview_image_url: post.thumbnail_url.clone(),
    }
}

/// Format an approximate age like "3h" / "2d" / "5m" from a publish time.
///
/// Pure fn — takes `now` explicitly so tests can pin the clock.
pub fn humanize_age(published: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = now
        .signed_duration_since(published)
        .num_seconds()
        .max(0);
    // lint-allow-unused: time-bucket boundaries; truncation is the desired display semantic
    #[allow(clippy::integer_division)]
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

/// Map a `PostView` to a Poly `ViewRow` (Pack E.1).
///
/// Pure mapping — no I/O. Used by `get_view_rows`. The `SCORE:` prefix on
/// `meta_text` is load-bearing: ListBody/TreeBody render the vote-card
/// shape when it appears (per Pack A).
pub fn map_post_to_viewrow(view: &PostView, now: DateTime<Utc>, render_previews: bool) -> ViewRow {
    let post = &view.post;
    let creator = &view.creator;
    let counts = &view.counts;

    let id = post.ap_id.clone().unwrap_or_else(|| post.id.to_string());
    let secondary = format!("by {}", creator.display_name.clone().unwrap_or_else(|| creator.name.clone()));
    let meta = format!(
        "SCORE:{} · {} comments · {}",
        counts.score,
        counts.comments,
        humanize_age(post.published, now)
    );

    let preview_image_url = if render_previews {
        post.thumbnail_url.clone()
    } else {
        None
    };

    ViewRow {
        id,
        primary_text: post.name.clone(),
        secondary_text: Some(secondary),
        meta_text: Some(meta),
        icon: None,
        badge: None,
        context_menu_target_kind: MenuTargetKind::Message,
        preview_image_url,
    }
}

/// Build a next-page `Cursor` for offset-paginated Lemmy endpoints.
pub fn next_page_cursor(current_page: u32, page_size: usize, rows_returned: usize) -> Option<Cursor> {
    if rows_returned < page_size {
        return None;
    }
    Some(Cursor {
        kind: CursorKind::Offset,
        value: current_page.saturating_add(1).to_string(),
    })
}

/// Parse a Lemmy view cursor (offset-based) back into a 1-indexed page number.
pub fn cursor_to_page(cursor: Option<&Cursor>) -> u32 {
    cursor
        .and_then(|c| match c.kind {
            CursorKind::Offset => c.value.parse::<u32>().ok(),
            CursorKind::Timestamp | CursorKind::Id | CursorKind::Opaque => None,
        })
        .unwrap_or(1)
}

/// Map a `CommentView` to a Poly `Message`.
pub fn map_comment_to_message(view: &CommentView) -> Message {
    let comment = &view.comment;
    let creator = &view.creator;
    let counts = &view.counts;

    let reactions = vec![
        Reaction {
            emoji: "upvote".to_string(),
            count: u32::try_from(counts.upvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(1_i32),
        },
        Reaction {
            emoji: "downvote".to_string(),
            count: u32::try_from(counts.downvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(-1_i32),
        },
    ];

    Message {
        id: format!("lemmy-comment-{}", comment.id),
        author: map_person(creator),
        content: MessageContent::Text(comment.content.clone()),
        timestamp: comment.published,
        attachments: vec![],
        reactions,
        reply_to: None,
        edited: comment.updated.is_some(),
        thread: None,
        preview_image_url: None, // comments do not have preview thumbnails
    }
}

/// Map a `PrivateMessageView` to a Poly `DmChannel`.
///
/// `my_user_id` is the authenticated user's Lemmy integer ID, used to
/// identify which side of the conversation is the "other" user.
pub fn map_pm_to_dm_channel(
    view: &PrivateMessageView,
    my_user_id: i64,
    account_id: &str,
) -> DmChannel {
    let other = if view.creator.id == my_user_id {
        &view.recipient
    } else {
        &view.creator
    };

    let last_msg = map_pm_to_message(view, my_user_id);

    DmChannel {
        id: format!("lemmy-dm-{}", other.id),
        user: map_person(other),
        last_message: Some(last_msg),
        unread_count: u32::from(!view.private_message.read),
        backend: BackendType::from("lemmy"),
        account_id: account_id.to_string(),
    }
}

/// Map a single `PrivateMessageView` to a Poly `Message`.
pub fn map_pm_to_message(view: &PrivateMessageView, my_user_id: i64) -> Message {
    let pm = &view.private_message;
    let author = if pm.creator_id == my_user_id {
        &view.creator
    } else {
        &view.creator
    };

    Message {
        id: format!("lemmy-pm-{}", pm.id),
        author: map_person(author),
        content: MessageContent::Text(pm.content.clone()),
        timestamp: pm.published,
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: None, // private messages do not have preview thumbnails
    }
}

// ── HTTP client ──────────────────────────────────────────────────────────────

/// Stored session state for the Lemmy HTTP client.
#[derive(Debug, Clone)]
pub struct LemmySession {
    /// Bearer JWT.
    pub jwt: String,
    /// Authenticated user's integer ID (from `/api/v3/site`).
    pub user_id: i64,
    /// Authenticated user's display name.
    pub user_display_name: String,
    /// Authenticated user's avatar URL.
    pub user_avatar_url: Option<String>,
}

/// Low-level Lemmy REST API client.
pub struct LemmyHttpClient {
    base_url: String,
    http: HttpClient,
    session: Arc<RwLock<Option<LemmySession>>>,
    user_agent: Arc<RwLock<String>>,
}

impl LemmyHttpClient {
    /// Create a new client pointing at `base_url` (e.g. `https://lemmy.ml`).
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut url = base_url.into();
        // Strip trailing slash so we can always append `/api/v3/...`
        if url.ends_with('/') {
            url.pop();
        }
        Self {
            base_url: url,
            http: HttpClient::new(),
            session: Arc::new(RwLock::new(None)),
            user_agent: Arc::new(RwLock::new(DEFAULT_CLIENT_VERSION.to_string())),
        }
    }

    /// The configured base URL (no trailing slash).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Whether a session JWT is currently stored.
    pub fn is_authenticated(&self) -> bool {
        self.session
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Retrieve the stored session, if any.
    pub fn session(&self) -> Option<LemmySession> {
        self.session
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Store a session JWT after successful login.
    pub fn set_session(&self, session: LemmySession) {
        *self.session.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
    }

    /// Clear the stored session.
    pub fn clear_session(&self) {
        *self.session.write().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// Update the User-Agent string.
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut guard) = self.user_agent.write() {
            *guard = ua;
        }
    }

    fn ua(&self) -> String {
        self.user_agent
            .read()
            .ok().map_or_else(|| DEFAULT_CLIENT_VERSION.to_string(), |g| g.clone())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Return the current JWT or an AuthFailed error.
    fn jwt(&self) -> ClientResult<String> {
        self.session()
            .map(|s| s.jwt)
            .ok_or_else(|| ClientError::AuthFailed("Lemmy client is not authenticated".to_string()))
    }


    /// POST with UA header injected.
    #[allow(dead_code)] // lint-allow-unused: helper kept for upcoming UA-aware routes
    async fn http_post<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        auth: Option<&str>,
    ) -> ClientResult<T> {
        let mut req = self
            .http
            .post(self.url(path))
            .header("User-Agent", self.ua())
            .json(body);
        if let Some(jwt) = auth {
            req = req.header("Authorization", format!("Bearer {jwt}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ClientError::Network(format!("{path} returned HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// GET with UA header injected.
    #[allow(dead_code)] // lint-allow-unused: helper kept for upcoming UA-aware routes
    async fn http_get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        auth: Option<&str>,
    ) -> ClientResult<T> {
        let mut req = self
            .http
            .get(self.url(path))
            .header("User-Agent", self.ua());
        if let Some(jwt) = auth {
            req = req.header("Authorization", format!("Bearer {jwt}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ClientError::Network(format!("{path} returned HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// `POST /api/v3/user/login`
    pub async fn login(&self, username: &str, password: &str) -> ClientResult<LoginResponse> {
        let body = LoginRequest {
            username_or_email: username.to_string(),
            password: password.to_string(),
        };
        let resp = self
            .http
            .post(self.url("/api/v3/user/login"))
            .header("User-Agent", self.ua())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::AuthFailed(format!(
                "Login failed: HTTP {}",
                resp.status()
            )));
        }

        resp.json::<LoginResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/site` — fetch current user info.
    pub async fn fetch_site(&self) -> ClientResult<SiteResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .http
            .get(self.url("/api/v3/site"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/site returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<SiteResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/community/list?type_=Subscribed&limit=50`
    pub async fn fetch_subscribed_communities(&self) -> ClientResult<CommunityListResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .http
            .get(self.url("/api/v3/community/list?type_=Subscribed&limit=50"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/community/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CommunityListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/community?id={id}`
    pub async fn fetch_community(&self, community_id: i64) -> ClientResult<CommunityView> {
        let jwt = self.jwt()?;
        let url = self.url(&format!("/api/v3/community?id={community_id}"));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(format!(
                "community {community_id} not found"
            )));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/community returned HTTP {}",
                resp.status()
            )));
        }

        // The single-community response wraps in `community_view`
        #[derive(Deserialize)]
        struct SingleCommunityResponse {
            community_view: CommunityView,
        }

        resp.json::<SingleCommunityResponse>()
            .await
            .map(|r| r.community_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/post/list` with explicit sort / page / limit.
    ///
    /// `sort` is passed straight through to Lemmy (`Hot`, `New`, `Top`, …).
    pub async fn fetch_posts_paged(
        &self,
        community_id: i64,
        sort: &str,
        page: u32,
        limit: u32,
    ) -> ClientResult<PostListResponse> {
        let jwt = self.jwt()?;
        // Title-case the sort id so we accept both "hot" and "Hot" from the toolbar.
        let sort_param = {
            let mut chars = sort.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => "Hot".to_string(),
            }
        };
        let url = self.url(&format!(
            "/api/v3/post/list?community_id={community_id}&sort={sort_param}&page={page}&limit={limit}"
        ));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/post/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<PostListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/post?id={id}` — fetch a single post by id.
    pub async fn fetch_post(&self, post_id: i64) -> ClientResult<PostView> {
        let jwt = self.jwt()?;
        let url = self.url(&format!("/api/v3/post?id={post_id}"));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(format!("post {post_id} not found")));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/post returned HTTP {}",
                resp.status()
            )));
        }

        #[derive(Deserialize)]
        struct SinglePostResponse {
            post_view: PostView,
        }

        resp.json::<SinglePostResponse>()
            .await
            .map(|r| r.post_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/post/list?community_id={id}&sort=Hot&limit=20`
    pub async fn fetch_posts(&self, community_id: i64) -> ClientResult<PostListResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/post/list?community_id={community_id}&sort=Hot&limit=20"
        ));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/post/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<PostListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/comment/list?post_id={id}&sort=Hot&limit=50`
    pub async fn fetch_comments(&self, post_id: i64) -> ClientResult<CommentListResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/comment/list?post_id={post_id}&sort=Hot&limit=50"
        ));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/comment/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CommentListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/private_message/list?limit=50`
    pub async fn fetch_private_messages(&self) -> ClientResult<PrivateMessageListResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .http
            .get(self.url("/api/v3/private_message/list?limit=50"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/private_message/list returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<PrivateMessageListResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/post` — create a new post in a community (C.7).
    pub async fn create_post(
        &self,
        community_id: i64,
        title: &str,
        body: Option<&str>,
        url: Option<&str>,
    ) -> ClientResult<PostView> {
        let jwt = self.jwt()?;
        let req = CreatePostRequest {
            name: title.to_string(),
            community_id,
            body: body.filter(|s| !s.is_empty()).map(str::to_string),
            url: url.filter(|s| !s.is_empty()).map(str::to_string),
        };

        let resp = self
            .http
            .post(self.url("/api/v3/post"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&req)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/post returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CreatePostResponse>()
            .await
            .map(|r| r.post_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/comment` — create a new comment on a post.
    pub async fn create_comment(
        &self,
        post_id: i64,
        content: &str,
        parent_id: Option<i64>,
    ) -> ClientResult<CommentView> {
        let jwt = self.jwt()?;
        let body = CreateCommentRequest {
            content: content.to_string(),
            post_id,
            parent_id,
        };

        let resp = self
            .http
            .post(self.url("/api/v3/comment"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/comment returned HTTP {}",
                resp.status()
            )));
        }

        #[derive(Deserialize)]
        struct CommentResponse {
            comment_view: CommentView,
        }

        resp.json::<CommentResponse>()
            .await
            .map(|r| r.comment_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/community/ban_user` — ban or unban a person from a community.
    pub async fn ban_from_community(
        &self,
        req: BanFromCommunityRequest,
    ) -> ClientResult<BanFromCommunityResponse> {
        let jwt = self.jwt()?;
        let resp = self
            .http
            .post(self.url("/api/v3/community/ban_user"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&req)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::PermissionDenied(
                "ban_from_community: permission denied".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/community/ban_user returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<BanFromCommunityResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `POST /api/v3/post/remove` — remove a post as moderator.
    pub async fn remove_post(
        &self,
        post_id: i64,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let jwt = self.jwt()?;
        let body = RemovePostRequest {
            post_id,
            removed: true,
            reason: reason.map(str::to_string),
        };
        let resp = self
            .http
            .post(self.url("/api/v3/post/remove"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::PermissionDenied(
                "remove_post: permission denied".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/post/remove returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// `POST /api/v3/comment/remove` — remove a comment as moderator.
    pub async fn remove_comment(
        &self,
        comment_id: i64,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let jwt = self.jwt()?;
        let body = RemoveCommentRequest {
            comment_id,
            removed: true,
            reason: reason.map(str::to_string),
        };
        let resp = self
            .http
            .post(self.url("/api/v3/comment/remove"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if resp.status() == StatusCode::FORBIDDEN {
            return Err(ClientError::PermissionDenied(
                "remove_comment: permission denied".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "POST /api/v3/comment/remove returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// `GET /api/v3/modlog?community_id={id}&type_=ModBanFromCommunity` — ban history only.
    pub async fn get_modlog_bans(
        &self,
        community_id: i64,
    ) -> ClientResult<Vec<ModBanFromCommunityView>> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/modlog?community_id={community_id}&type_=ModBanFromCommunity"
        ));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/modlog (bans) returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<GetModlogResponse>()
            .await
            .map(|r| r.banned_from_community)
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `GET /api/v3/modlog?community_id={id}` — fetch moderation log for a community.
    pub async fn get_modlog(&self, community_id: i64) -> ClientResult<GetModlogResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/modlog?community_id={community_id}&type_=All"
        ));
        let resp = self
            .http
            .get(url)
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET /api/v3/modlog returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<GetModlogResponse>()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    /// `PUT /api/v3/community` — update a community (EditCommunity).
    ///
    /// `banner` is a URL string pointing to a previously-uploaded pictrs image
    /// (or any public URL for test purposes). Pass `None` to clear the banner.
    pub async fn put_community(
        &self,
        community_id: i64,
        banner: Option<&str>,
    ) -> ClientResult<CommunityView> {
        let jwt = self.jwt()?;
        let body = serde_json::json!({
            "community_id": community_id,
            "banner": banner,
            "auth": jwt,
        });
        let resp = self
            .http
            .put(self.url("/api/v3/community"))
            .header("User-Agent", self.ua())
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "PUT /api/v3/community returned HTTP {}",
                resp.status()
            )));
        }

        #[derive(Deserialize)]
        struct EditCommunityResponse {
            community_view: CommunityView,
        }

        resp.json::<EditCommunityResponse>()
            .await
            .map(|r| r.community_view)
            .map_err(|e| ClientError::Network(e.to_string()))
    }
}

// ── Unit tests (Pack E.1 layer-a) ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

    use super::*;
    use chrono::TimeZone;

    /// Parse the checked-in Lemmy post-list fixture and exercise the pure
    /// `map_post_to_viewrow` mapping. NO NETWORK.
    #[test]
    fn map_post_to_viewrow_from_fixture() {
        let raw = include_str!("../tests/fixtures/post_list.json");
        let resp: PostListResponse =
            serde_json::from_str(raw).expect("fixture must deserialize as PostListResponse");

        assert_eq!(resp.posts.len(), 2);

        // Pin the clock so humanize_age output is deterministic.
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();

        let row0 = map_post_to_viewrow(&resp.posts[0], now, true);
        assert_eq!(row0.id, "https://lemmy.example.com/post/101");
        assert_eq!(row0.primary_text, "Rust 2025 edition is here");
        assert_eq!(row0.secondary_text.as_deref(), Some("by Alice A."));
        let meta = row0.meta_text.expect("meta required");
        assert!(meta.starts_with("SCORE:42"), "meta must lead with SCORE:42, got {meta}");
        assert!(meta.contains("12 comments"), "meta must include comment count: {meta}");
        assert!(meta.contains("2h"), "meta must include humanized age 2h: {meta}");
        assert_eq!(row0.context_menu_target_kind, MenuTargetKind::Message);

        // Row 1: creator has no display_name → falls back to `name`.
        let row1 = map_post_to_viewrow(&resp.posts[1], now, true);
        assert_eq!(row1.secondary_text.as_deref(), Some("by bob"));
        let meta1 = row1.meta_text.expect("meta required");
        assert!(meta1.starts_with("SCORE:128"));
        assert!(meta1.contains("5 comments"));
    }

    #[test]
    fn humanize_age_buckets() {
        let base = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        assert_eq!(
            humanize_age(base - chrono::Duration::seconds(30), base),
            "30s"
        );
        assert_eq!(
            humanize_age(base - chrono::Duration::minutes(5), base),
            "5m"
        );
        assert_eq!(humanize_age(base - chrono::Duration::hours(3), base), "3h");
        assert_eq!(humanize_age(base - chrono::Duration::days(2), base), "2d");
    }

    #[test]
    fn cursor_round_trip_offset() {
        let c = Cursor {
            kind: CursorKind::Offset,
            value: "3".to_string(),
        };
        assert_eq!(cursor_to_page(Some(&c)), 3);
        assert_eq!(cursor_to_page(None), 1);

        // Full page → next cursor advances.
        let next = next_page_cursor(3, 25, 25).expect("full page must produce next cursor");
        assert_eq!(next.value, "4");
        assert_eq!(next.kind, CursorKind::Offset);

        // Short page → no next cursor.
        assert!(next_page_cursor(3, 25, 10).is_none());
    }

    /// Verify that `LemmyPost.thumbnail_url` propagates to `ViewRow.preview_image_url`
    /// through `map_post_to_viewrow` when `render_previews` is true, and is suppressed
    /// when `render_previews` is false.
    ///
    /// Also verifies propagation through `map_post_to_message.preview_image_url`.
    #[test]
    fn thumbnail_url_propagates_to_preview_image_url() {
        let raw = include_str!("../tests/fixtures/post_list.json");
        let resp: PostListResponse =
            serde_json::from_str(raw).expect("fixture must deserialize");

        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();

        // Post 0 has thumbnail_url set in the fixture.
        let view0 = &resp.posts[0];
        assert_eq!(
            view0.post.thumbnail_url.as_deref(),
            Some("https://lemmy.example.com/pictrs/image/test-preview.png"),
            "fixture thumbnail_url must deserialize correctly"
        );

        // render_previews = true: preview_image_url is populated on the ViewRow.
        let row_on = map_post_to_viewrow(view0, now, true);
        assert_eq!(
            row_on.preview_image_url.as_deref(),
            Some("https://lemmy.example.com/pictrs/image/test-preview.png"),
            "render_previews=true must propagate thumbnail_url to ViewRow.preview_image_url"
        );

        // render_previews = false: preview_image_url is suppressed on the ViewRow.
        let row_off = map_post_to_viewrow(view0, now, false);
        assert_eq!(
            row_off.preview_image_url,
            None,
            "render_previews=false must suppress preview_image_url even when thumbnail_url is set"
        );

        // map_post_to_message always propagates thumbnail_url → preview_image_url
        // (the mechanism check lives in get_view_rows, not in the message mapper).
        let msg = map_post_to_message(view0);
        assert_eq!(
            msg.preview_image_url.as_deref(),
            Some("https://lemmy.example.com/pictrs/image/test-preview.png"),
            "map_post_to_message must propagate thumbnail_url to Message.preview_image_url"
        );

        // Post 1 has no thumbnail_url — preview_image_url must be None.
        let view1 = &resp.posts[1];
        assert!(view1.post.thumbnail_url.is_none(), "post[1] has no thumbnail_url in fixture");
        let row1 = map_post_to_viewrow(view1, now, true);
        assert_eq!(
            row1.preview_image_url,
            None,
            "absent thumbnail_url must produce None preview_image_url"
        );
    }
}

