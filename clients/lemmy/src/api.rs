//! Lemmy REST API v3 types and HTTP client.
//!
//! All types are intentionally kept internal to `poly-lemmy` so that
//! external crates stay isolated from Lemmy protocol details.

use chrono::{DateTime, Utc};
use poly_client::{
    Attachment, BackendType, Category, Channel, ChannelType, ClientError, ClientResult, DmChannel,
    Message, MessageContent, PresenceStatus, Reaction, Server, User,
};
use poly_host_bridge::http::{HttpClient, StatusCode};
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
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub banner: Option<String>,
}

/// A community view as returned in list responses.
#[derive(Debug, Clone, Deserialize)]
pub struct CommunityView {
    pub community: LemmyCommunity,
}

/// Response from `GET /api/v3/community/list`.
#[derive(Debug, Clone, Deserialize)]
pub struct CommunityListResponse {
    pub communities: Vec<CommunityView>,
}

/// Post counts sub-object.
#[derive(Debug, Clone, Deserialize)]
pub struct PostCounts {
    pub score: i64,
    pub upvotes: i64,
    pub downvotes: i64,
    pub comments: i64,
}

/// A Lemmy post.
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyPost {
    pub id: i64,
    pub name: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub creator_id: i64,
    pub published: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
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
    pub score: i64,
    pub upvotes: i64,
    pub downvotes: i64,
}

/// A Lemmy comment.
#[derive(Debug, Clone, Deserialize)]
pub struct LemmyComment {
    pub id: i64,
    pub content: String,
    pub creator_id: i64,
    pub post_id: i64,
    pub path: String,
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
    pub recipient_id: i64,
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
            count: counts.upvotes.max(0) as u32,
            me: view.my_vote == Some(1),
        },
        Reaction {
            emoji: "downvote".to_string(),
            count: counts.downvotes.max(0) as u32,
            me: view.my_vote == Some(-1),
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
    }
}

/// Map a `CommentView` to a Poly `Message`.
pub fn map_comment_to_message(view: &CommentView) -> Message {
    let comment = &view.comment;
    let creator = &view.creator;
    let counts = &view.counts;

    let reactions = vec![
        Reaction {
            emoji: "upvote".to_string(),
            count: counts.upvotes.max(0) as u32,
            me: view.my_vote == Some(1),
        },
        Reaction {
            emoji: "downvote".to_string(),
            count: counts.downvotes.max(0) as u32,
            me: view.my_vote == Some(-1),
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
            .expect("session lock poisoned")
            .is_some()
    }

    /// Retrieve the stored session, if any.
    pub fn session(&self) -> Option<LemmySession> {
        self.session
            .read()
            .expect("session lock poisoned")
            .clone()
    }

    /// Store a session JWT after successful login.
    pub fn set_session(&self, session: LemmySession) {
        *self.session.write().expect("session lock poisoned") = Some(session);
    }

    /// Clear the stored session.
    pub fn clear_session(&self) {
        *self.session.write().expect("session lock poisoned") = None;
    }

    /// Build the full URL for an API path (e.g. `/api/v3/user/login`).
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Return the current JWT or an AuthFailed error.
    fn jwt(&self) -> ClientResult<String> {
        self.session()
            .map(|s| s.jwt)
            .ok_or_else(|| ClientError::AuthFailed("Lemmy client is not authenticated".to_string()))
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

    /// `GET /api/v3/post/list?community_id={id}&sort=Hot&limit=20`
    pub async fn fetch_posts(&self, community_id: i64) -> ClientResult<PostListResponse> {
        let jwt = self.jwt()?;
        let url = self.url(&format!(
            "/api/v3/post/list?community_id={community_id}&sort=Hot&limit=20"
        ));
        let resp = self
            .http
            .get(url)
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
}
