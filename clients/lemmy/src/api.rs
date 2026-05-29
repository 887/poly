//! Lemmy REST API v3 — internal HTTP layer.
//!
//! Re-exports are flattened so the rest of the crate keeps using
//! `crate::api::Foo` (no breaking-imports churn on the lib.rs split).
//!
//! Sub-modules:
//! - [`types`] — request/response DTOs + protocol constants.
//! - [`mapping`] — pure `LemmyType → poly_client::Type` mappers.
//! - [`client`] — `LemmyHttpClient` struct + session/UA state.
//! - [`endpoints`] — `impl LemmyHttpClient` with one fn per REST endpoint.

mod client;
mod endpoints;
mod mapping;
mod types;

pub use client::{LemmyHttpClient, LemmySession};
pub use mapping::{
    community_to_channel, cursor_to_page, map_comment_to_message, map_community_to_server,
    map_community_to_viewrow, map_person, map_pm_to_dm_channel, map_post_to_message,
    map_post_to_viewrow, next_page_cursor,
};
// lint-allow-unused: full DTO surface re-exported so callers keep using crate::api::Foo after split
#[allow(unused_imports)]
pub use types::{
    BanFromCommunityRequest, BanFromCommunityResponse, CommentCounts, CommentListResponse,
    CommentView, CommunityCounts, CommunityListResponse, CommunityView, CreateCommentRequest,
    CreatePostRequest, CreatePostResponse, DEFAULT_CLIENT_VERSION, GetModlogResponse,
    LemmyComment, LemmyCommunity, LemmyPerson, LemmyPost, LemmyPrivateMessage, LocalUserView,
    LoginRequest, LoginResponse, ModBanFromCommunity, ModBanFromCommunityView, ModRemoveComment,
    ModRemoveCommentView, ModRemovePost, ModRemovePostView, MyUserInfo, PersonView, PostCounts,
    PostListResponse, PostView, PrivateMessageListResponse, PrivateMessageView, RemoveCommentRequest,
    RemovePostRequest, SearchCommunitiesResponse, SiteResponse,
};

// lint-allow-unused: future-callable mapping helpers re-exported via crate::api surface
#[allow(unused_imports)]
pub use mapping::{humanize_age, post_is_video};
