//! Lemmy REST API v3 request/response DTO types.
//!
//! All types are intentionally kept internal to `poly-lemmy` so that
//! external crates stay isolated from Lemmy protocol details.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Default User-Agent for Lemmy API requests.
pub const DEFAULT_CLIENT_VERSION: &str = "poly-lemmy/0.0.0";

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

/// Response from `GET /api/v3/search?type_=Communities` (Phase E).
///
/// The search endpoint wraps results under a `communities` key just like
/// the community-list endpoint. We define a separate type so callers can
/// distinguish the two response shapes if Lemmy ever diverges.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchCommunitiesResponse {
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
    /// Video embed URL set by Lemmy when the post links to a video host
    /// (YouTube, PeerTube, etc.). Present in the real Lemmy v3 API but
    /// absent for non-video posts. We surface this to drive `is_video`
    /// detection without URL-extension heuristics.
    #[serde(default)]
    pub embed_video_url: Option<String>,
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
    // lint-allow-unused: kept for protocol-fidelity decode
    #[allow(dead_code)]
    pub banned_person: PersonView,
    // lint-allow-unused: kept for protocol-fidelity decode
    #[allow(dead_code)]
    pub banned: bool,
}

/// A person view (person + counts).
#[derive(Debug, Clone, Deserialize)]
pub struct PersonView {
    // lint-allow-unused: kept for protocol-fidelity decode
    #[allow(dead_code)]
    pub person: LemmyPerson,
}

/// A single `ModBanFromCommunity` entry in the modlog.
#[derive(Debug, Clone, Deserialize)]
pub struct ModBanFromCommunityView {
    pub mod_ban_from_community: ModBanFromCommunity,
    pub moderator: Option<LemmyPerson>,
    pub banned_person: LemmyPerson,
    /// Community context decoded for completeness; not currently read.
    // lint-allow-unused: kept for protocol-fidelity decode
    #[allow(dead_code)]
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
    // lint-allow-unused: kept for protocol-fidelity decode
    #[allow(dead_code)]
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
    // lint-allow-unused: kept for protocol-fidelity decode
    #[allow(dead_code)]
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
