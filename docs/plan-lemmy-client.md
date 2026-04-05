# Plan — Lemmy Client Backend

> **Created:** 2026-04-05
> **Status:** Planning
> **Crate:** `poly-lemmy`
> **Goal:** Full Lemmy client backend implementing `ClientBackend`, supporting subscribed communities as servers, forum-style posts and threaded comments, voting, private messages, and multi-instance accounts.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Lemmy API Reference](#2-lemmy-api-reference)
3. [Mapping Lemmy Concepts to Poly Types](#3-mapping-lemmy-concepts-to-poly-types)
4. [ClientBackend Trait Implementation](#4-clientbackend-trait-implementation)
5. [New Poly Types Required](#5-new-poly-types-required)
6. [Authentication](#6-authentication)
7. [File Structure](#7-file-structure)
8. [Crate Dependencies](#8-crate-dependencies)
9. [Test Server](#9-test-server)
10. [Implementation Phases](#10-implementation-phases)
11. [Special Considerations](#11-special-considerations)
12. [Open Questions](#12-open-questions)

---

## 1. Overview

Lemmy is a federated link aggregator / forum built on ActivityPub. Users subscribe to communities on one or more instances. Each community has posts (link, text, or image), and each post has threaded comments with upvote/downvote scoring. Lemmy also supports private messages between users.

Poly maps Lemmy into its unified messenger model:

| Lemmy Concept | Poly Concept |
|---|---|
| Instance (e.g. `lemmy.ml`) | Account instance (like a Matrix homeserver) |
| Subscribed community | Server |
| Community feed | Single implicit forum channel per server |
| Post | Forum post (new type) |
| Comment thread | Threaded messages under a post |
| Private message | DM channel |
| Upvote/downvote | Vote action (new trait method) |

This is the first backend that uses a **forum model** rather than a real-time chat model. The key architectural difference: messages are not a flat chronological stream in a channel but a two-level hierarchy (posts, then threaded comments under each post). This requires extending the `ClientBackend` trait and `poly-client` types.

---

## 2. Lemmy API Reference

### 2.1 Base URL

All Lemmy instances expose a REST API at:

```
https://{instance}/api/v3/
```

Examples: `https://lemmy.ml/api/v3/`, `https://lemmy.world/api/v3/`

### 2.2 Authentication

**Login:**
```
POST /api/v3/user/login
Body: { "username_or_email": "alice", "password": "hunter2" }
Response: { "jwt": "eyJ...", "registration_created": false, "verify_email_sent": false }
```

The JWT is sent on subsequent requests as:
```
Authorization: Bearer {jwt}
```

**Get current user:**
```
GET /api/v3/site
Header: Authorization: Bearer {jwt}
Response includes: my_user.local_user_view.person { id, name, display_name, avatar, ... }
```

### 2.3 Communities

**List subscribed communities:**
```
GET /api/v3/community/list?type_=Subscribed&limit=50&page=1
Response: { "communities": [ { "community": { ... }, "subscribed": "Subscribed", "counts": { ... } } ] }
```

**Get single community:**
```
GET /api/v3/community?id={community_id}
GET /api/v3/community?name={name}  (for federated: name@instance)
```

Community fields of interest:
- `id` (integer) — community ID
- `name` — machine name (e.g. `linux`)
- `title` — display name (e.g. `Linux`)
- `description` — sidebar markdown
- `icon` — avatar URL
- `banner` — banner image URL
- `nsfw` — whether community is NSFW
- `subscribers` — subscriber count
- `posts` — post count
- `comments` — comment count

### 2.4 Posts

**List posts in a community:**
```
GET /api/v3/post/list?community_id={id}&sort={sort}&limit=20&page=1
```

Sort types: `Active`, `Hot`, `New`, `Old`, `Scaled`, `Controversial`, `TopHour`, `TopSixHour`, `TopTwelveHour`, `TopDay`, `TopWeek`, `TopMonth`, `TopThreeMonths`, `TopSixMonths`, `TopNineMonths`, `TopYear`, `TopAll`, `MostComments`, `NewComments`

**Get single post:**
```
GET /api/v3/post?id={post_id}
Response: { "post_view": { ... }, "community_view": { ... }, "cross_posts": [ ... ] }
```

Post fields:
- `id` (integer)
- `name` — post title
- `body` — optional markdown body (text posts)
- `url` — optional link URL (link posts)
- `thumbnail_url` — auto-generated thumbnail
- `embed_title`, `embed_description`, `embed_video_url` — link preview metadata
- `nsfw` — whether post is NSFW
- `published` — ISO 8601 timestamp
- `updated` — optional edit timestamp
- `creator_id` — author user ID

Post counts (in `PostView.counts`):
- `score` — upvotes minus downvotes
- `upvotes`, `downvotes`
- `comments` — comment count

Post state (in `PostView`):
- `my_vote` — current user's vote: `1`, `-1`, or `null`
- `saved` — whether user bookmarked the post

**Create a post:**
```
POST /api/v3/post
Body: {
  "name": "Post title",
  "body": "Optional markdown body",
  "url": "https://example.com",  // optional
  "community_id": 123,
  "nsfw": false
}
```

### 2.5 Comments

**List comments on a post:**
```
GET /api/v3/comment/list?post_id={id}&sort={sort}&limit=50&page=1
```

Comment sort types: `Hot`, `Top`, `New`, `Old`, `Controversial`

The API returns a **flat list** with `parent_id` fields. The client builds the tree structure.

Comment fields:
- `id` (integer)
- `content` — markdown body
- `creator_id` — author user ID
- `post_id` — parent post
- `parent_id` — parent comment ID (null for top-level)
- `path` — materialized path string like `"0.123.456.789"` (root is `0`, then comment IDs)
- `published`, `updated` — timestamps
- `deleted`, `removed` — moderation state

Comment counts:
- `score`, `upvotes`, `downvotes`
- `child_count` — number of direct children

Comment state:
- `my_vote` — `1`, `-1`, or `null`
- `saved` — bookmarked

**Create a comment:**
```
POST /api/v3/comment
Body: {
  "content": "Comment body markdown",
  "post_id": 123,
  "parent_id": 456  // optional, for replies to other comments
}
```

### 2.6 Voting

**Vote on a post:**
```
POST /api/v3/post/like
Body: { "post_id": 123, "score": 1 }   // 1 = upvote, -1 = downvote, 0 = remove vote
```

**Vote on a comment:**
```
POST /api/v3/comment/like
Body: { "comment_id": 456, "score": 1 }
```

### 2.7 Users

**Get user profile:**
```
GET /api/v3/user?person_id={id}
GET /api/v3/user?username={name}  (for federated: name@instance)
```

User fields:
- `id` (integer)
- `name` — username
- `display_name` — optional display name
- `avatar` — avatar URL
- `banner` — profile banner URL
- `bio` — markdown bio
- `published` — account creation date

### 2.8 Private Messages

**List private messages:**
```
GET /api/v3/private_message/list?limit=50&page=1
Response: { "private_messages": [ { "private_message": { ... }, "creator": { ... }, "recipient": { ... } } ] }
```

**Send private message:**
```
POST /api/v3/private_message
Body: { "content": "Message text", "recipient_id": 123 }
```

### 2.9 Search

```
GET /api/v3/search?q={query}&type_={type}&sort={sort}&listing_type={listing}&community_id={id}&limit=20&page=1
```

Type values: `All`, `Comments`, `Posts`, `Communities`, `Users`, `Url`

### 2.10 WebSocket / Real-Time

Lemmy v0.19+ removed the WebSocket API. Real-time updates require polling or using the `/api/v3/comment/list?sort=New` endpoint. Future versions may add SSE or a new streaming endpoint. For now, the Lemmy backend uses **periodic polling** for new content (configurable interval, default 30s).

---

## 3. Mapping Lemmy Concepts to Poly Types

### 3.1 Community -> Server

```rust
// Lemmy CommunityView -> poly_client::Server
Server {
    id: format!("lemmy-community-{}", community.id),
    name: community.title,                    // display name
    icon_url: community.icon,                 // community avatar
    banner_url: community.banner,             // community banner
    categories: vec![Category {
        id: "posts".into(),
        name: "Posts".into(),
        channel_ids: vec![format!("lemmy-feed-{}", community.id)],
    }],
    backend: BackendType::Lemmy,
    unread_count: 0,                          // Lemmy has no unread tracking
    mention_count: 0,
    account_id: account_id.clone(),
    account_display_name: user.display_name,
}
```

### 3.2 Community Feed -> Channel

Each community maps to exactly one channel (the community post feed):

```rust
Channel {
    id: format!("lemmy-feed-{}", community.id),
    name: community.title,
    channel_type: ChannelType::Forum,         // NEW — see section 5
    server_id: format!("lemmy-community-{}", community.id),
    unread_count: 0,
    mention_count: 0,
    last_message_id: None,                    // or latest post ID
}
```

### 3.3 Post -> ForumPost (new type)

Posts do not map cleanly to `Message` because they have a title, URL, thumbnail, vote score, and comment count. A new `ForumPost` type is needed (section 5).

### 3.4 Comment -> Message

Comments map to `Message` with extensions for threading:

```rust
Message {
    id: format!("lemmy-comment-{}", comment.id),
    author: map_user(creator),
    content: MessageContent::Text(comment.content),
    timestamp: comment.published,
    attachments: vec![],
    reactions: vec![
        // Map upvotes/downvotes as pseudo-reactions for display
        Reaction { emoji: "upvote".into(), count: counts.upvotes, me: my_vote == Some(1) },
        Reaction { emoji: "downvote".into(), count: counts.downvotes, me: my_vote == Some(-1) },
    ],
    reply_to: parent_comment_preview,         // from parent_id
    edited: comment.updated.is_some(),
}
```

### 3.5 User -> User

```rust
User {
    id: format!("lemmy-user-{}", person.id),
    display_name: person.display_name.unwrap_or(person.name),
    avatar_url: person.avatar,
    presence: PresenceStatus::Offline,        // Lemmy has no presence
    backend: BackendType::Lemmy,
}
```

### 3.6 Private Message -> DmChannel + Message

Private messages group by conversation partner into `DmChannel`:

```rust
DmChannel {
    id: format!("lemmy-dm-{}", other_user.id),
    user: map_user(other_user),
    last_message: Some(map_pm_to_message(latest_pm)),
    unread_count: unread_count,
    backend: BackendType::Lemmy,
    account_id: account_id.clone(),
}
```

---

## 4. ClientBackend Trait Implementation

### 4.1 Fully Implemented Methods

| Method | Lemmy API Call | Notes |
|---|---|---|
| `authenticate()` | `POST /user/login` + `GET /site` | Store JWT, fetch user profile |
| `logout()` | Clear stored JWT | Lemmy has no server-side logout |
| `is_authenticated()` | Check if JWT is stored | Local check |
| `get_servers()` | `GET /community/list?type_=Subscribed` | Map communities to servers |
| `get_server()` | `GET /community?id=X` | Single community |
| `get_channels()` | Return single implicit forum channel | One channel per community |
| `get_channel()` | Return implicit forum channel | Derived from community ID |
| `send_message()` | `POST /comment` | Create comment on a post |
| `send_reply_message()` | `POST /comment` with `parent_id` | Reply to existing comment |
| `get_messages()` | `GET /comment/list?post_id=X` | Flat list, client builds tree |
| `get_user()` | `GET /user?person_id=X` | User profile |
| `get_dm_channels()` | `GET /private_message/list` | Group by conversation partner |
| `get_notifications()` | `GET /user/replies` + `GET /user/mentions` | Mentions and replies |
| `search_messages()` | `GET /search?type_=Comments` | Backend search |

### 4.2 New Methods (trait extensions needed)

| Method | Lemmy API Call | Notes |
|---|---|---|
| `get_forum_posts()` | `GET /post/list?community_id=X` | List posts with sort/filter |
| `get_forum_post()` | `GET /post?id=X` | Single post with cross-posts |
| `create_forum_post()` | `POST /post` | Create text/link/image post |
| `vote()` | `POST /post/like` or `/comment/like` | Upvote/downvote/unvote |

These will be added to `ClientBackend` as default-implemented methods returning `NotSupported`, so existing backends are unaffected.

### 4.3 NotSupported Methods

| Method | Reason |
|---|---|
| `get_friends()` | Lemmy has no friend system; returns empty `Vec` |
| `get_groups()` | Lemmy has no group DMs; returns empty `Vec` |
| `get_voice_participants()` | No voice/video; returns empty `Vec` |
| `get_presence()` | No presence system; returns `PresenceStatus::Offline` |
| `set_presence()` | No presence system; returns `NotSupported` |
| `create_server()` | Community creation exists but is admin-gated on most instances; defer |
| `create_channel()` | No channel concept; returns `NotSupported` |
| `get_pinned_messages()` | Lemmy has "featured" posts but no pinned comments; partial support later |
| `add_group_member()` / `remove_group_member()` | No groups; returns `NotSupported` |

### 4.4 Event Stream

Since Lemmy has no WebSocket API (removed in v0.19), `event_stream()` returns a polling-based stream:

```rust
fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
    // Poll /comment/list?sort=New and /private_message/list every N seconds.
    // Emit ClientEvent::MessageReceived for new comments/PMs.
    // Configurable interval (default 30s, min 10s).
    let interval = self.poll_interval;
    // ... polling stream implementation
}
```

---

## 5. New Poly Types Required

### 5.1 BackendType::Lemmy

Add `Lemmy` variant to `BackendType` in `clients/client/src/types.rs`:

```rust
pub enum BackendType {
    Stoat,
    Matrix,
    Discord,
    Teams,
    Demo,
    Poly,
    Lemmy,  // NEW
}
```

With `display_name() -> "Lemmy"`, `slug() -> "lemmy"`, `from_slug("lemmy") -> Some(Self::Lemmy)`.

### 5.2 ChannelType::Forum

Add `Forum` variant to `ChannelType`:

```rust
pub enum ChannelType {
    Text,
    Voice,
    Video,
    Forum,  // NEW — post-based channel with threaded comments
}
```

### 5.3 ForumPost Type

New type in `clients/client/src/types.rs`:

```rust
/// A forum post (used by Lemmy, potentially Reddit-like backends).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumPost {
    /// Backend-specific post ID.
    pub id: String,
    /// Post title.
    pub title: String,
    /// Post author.
    pub author: User,
    /// Post body (markdown). None for link-only posts.
    pub body: Option<String>,
    /// Link URL for link posts. None for text-only posts.
    pub url: Option<String>,
    /// Thumbnail URL (auto-generated for link posts, or uploaded image).
    pub thumbnail_url: Option<String>,
    /// Link embed metadata.
    pub embed: Option<LinkEmbed>,
    /// When the post was created.
    pub timestamp: DateTime<Utc>,
    /// Whether the post has been edited.
    pub edited: bool,
    /// Upvote count.
    pub upvotes: u32,
    /// Downvote count.
    pub downvotes: u32,
    /// Net score (upvotes - downvotes).
    pub score: i32,
    /// Current user's vote: 1, -1, or 0 (no vote).
    pub my_vote: i8,
    /// Number of comments on this post.
    pub comment_count: u32,
    /// Whether the post is marked NSFW.
    pub nsfw: bool,
    /// Whether the current user has saved/bookmarked this post.
    pub saved: bool,
    /// Whether the post is pinned/featured in the community.
    pub pinned: bool,
    /// Cross-post IDs (same URL posted in other communities).
    pub cross_post_ids: Vec<String>,
    /// Which backend this post is from.
    pub backend: BackendType,
}

/// Link embed metadata for link posts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkEmbed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub video_url: Option<String>,
}
```

### 5.4 ForumPostQuery Type

```rust
/// Query options for fetching forum posts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumPostQuery {
    /// Sort order for posts.
    pub sort: ForumSortType,
    /// Maximum number of posts to return.
    pub limit: Option<u32>,
    /// Page number for pagination (1-indexed).
    pub page: Option<u32>,
}

/// Sort types for forum posts and comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ForumSortType {
    #[default]
    Active,
    Hot,
    New,
    Old,
    Scaled,
    Controversial,
    TopHour,
    TopSixHour,
    TopTwelveHour,
    TopDay,
    TopWeek,
    TopMonth,
    TopThreeMonths,
    TopSixMonths,
    TopNineMonths,
    TopYear,
    TopAll,
    MostComments,
    NewComments,
}

/// Sort types for comments specifically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CommentSortType {
    #[default]
    Hot,
    Top,
    New,
    Old,
    Controversial,
}
```

### 5.5 VoteTarget Type

```rust
/// Target of a vote action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteTarget {
    /// Vote on a forum post.
    Post(String),
    /// Vote on a comment/message.
    Comment(String),
}
```

### 5.6 New ClientBackend Methods

Add to the `ClientBackend` trait with default `NotSupported` implementations:

```rust
/// Get forum posts in a channel (for forum-type channels).
async fn get_forum_posts(
    &self,
    channel_id: &str,
    query: ForumPostQuery,
) -> ClientResult<Vec<ForumPost>> {
    let _ = (channel_id, query);
    Err(ClientError::NotSupported("get_forum_posts".to_string()))
}

/// Get a single forum post by ID.
async fn get_forum_post(&self, post_id: &str) -> ClientResult<ForumPost> {
    let _ = post_id;
    Err(ClientError::NotSupported("get_forum_post".to_string()))
}

/// Create a new forum post in a channel.
async fn create_forum_post(
    &self,
    channel_id: &str,
    title: &str,
    body: Option<&str>,
    url: Option<&str>,
) -> ClientResult<ForumPost> {
    let _ = (channel_id, title, body, url);
    Err(ClientError::NotSupported("create_forum_post".to_string()))
}

/// Upvote, downvote, or remove a vote on a post or comment.
/// `score` is 1 (upvote), -1 (downvote), or 0 (remove vote).
async fn vote(&self, target: VoteTarget, score: i8) -> ClientResult<()> {
    let _ = (target, score);
    Err(ClientError::NotSupported("vote".to_string()))
}
```

---

## 6. Authentication

### 6.1 Credentials

Lemmy uses `AuthCredentials::EmailPassword` with one twist: the email field is actually `username_or_email` (Lemmy accepts either). The instance URL must be provided separately.

Option A: Reuse `EmailPassword` as-is, document that `email` field accepts usernames too.
Option B: Add a new `AuthCredentials::LemmyLogin` variant.

**Recommended: Option A** to avoid proliferating credential variants. The signup page for Lemmy will have an "Instance URL" field that is stored in the `Session::backend_url` field, and a "Username or Email" label on the email input.

### 6.2 Session

```rust
Session {
    id: format!("lemmy-{}-{}", instance_host, user_id),
    user: mapped_user,
    token: jwt_string,
    backend: BackendType::Lemmy,
    icon_emoji: None,
    instance_id: instance_host,              // e.g. "lemmy.ml"
    backend_url: Some(instance_base_url),    // e.g. "https://lemmy.ml"
}
```

### 6.3 Token Lifecycle

- JWT is stored in the session and sent as `Authorization: Bearer {jwt}` on every request
- Lemmy JWTs have no standard expiry mechanism exposed to clients; if a request returns 401, the client should prompt re-authentication
- No server-side logout endpoint; `logout()` simply clears the stored JWT

### 6.4 Multi-Instance Support

Users can add multiple Lemmy accounts on different instances. Each account has its own:
- `backend_url` (the instance base URL)
- `instance_id` (derived from the hostname)
- JWT token

The `LemmyHttpClient` is instantiated per-account, each pointing at a different base URL.

---

## 7. File Structure

### 7.1 Client Crate

```
clients/lemmy/
  Cargo.toml
  src/
    lib.rs           — LemmyClient struct, ClientBackend impl
    api.rs           — Lemmy REST API response types (serde Deserialize)
    http.rs          — LemmyHttpClient (reqwest transport, auth header injection)
    config.rs        — LemmyConfig (base URL normalization, instance ID derivation)
    mapping.rs       — Convert Lemmy API types -> Poly types (Server, Channel, Message, etc.)
    comment_tree.rs  — Build threaded comment tree from flat list with path/parent_id
  locales/
    en/plugin.ftl    — English translations for Lemmy signup/UI strings
    de/plugin.ftl
    fr/plugin.ftl
    es/plugin.ftl
```

### 7.2 Test Server

```
servers/test-lemmy/
  Cargo.toml
  src/
    main.rs          — axum server entry point, CLI args, router
    routes.rs        — Lemmy API v3 route handlers (login, communities, posts, comments, PMs, votes)
    state.rs         — In-memory state (users, communities, posts, comments, votes, PMs)
```

### 7.3 WASM Plugin (deferred)

WASM guest implementation (`guest.rs`, `wit_bindings.rs`) follows the same pattern as Stoat/Matrix but is deferred until the native implementation is stable.

---

## 8. Crate Dependencies

### 8.1 `poly-lemmy/Cargo.toml`

```toml
[package]
name = "poly-lemmy"
description = "Lemmy client for Poly — builds as native lib or WASM plugin"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[features]
default = ["native"]
native = [
    "dep:async-trait",
    "dep:futures",
    "dep:tokio",
    "dep:serde",
    "dep:serde_json",
    "dep:chrono",
    "dep:uuid",
    "dep:thiserror",
    "dep:tracing",
    "dep:reqwest",
]

[dependencies]
poly-client = { workspace = true }

# Native-only dependencies (feature-gated)
serde = { workspace = true, optional = true }
serde_json = { workspace = true, optional = true }
reqwest = { workspace = true, optional = true }
tokio = { workspace = true, optional = true }
chrono = { workspace = true, optional = true }
uuid = { workspace = true, optional = true, features = ["js"] }
futures = { workspace = true, optional = true }
thiserror = { workspace = true, optional = true }
tracing = { workspace = true, optional = true }
async-trait = { workspace = true, optional = true }

# WASM plugin guest bindings (WASI targets only)
[target.'cfg(target_os = "wasi")'.dependencies]
wit-bindgen = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
axum = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["macros", "rt", "time", "net"] }

[lints]
workspace = true
```

### 8.2 `servers/test-lemmy/Cargo.toml`

```toml
[package]
name = "test-lemmy"
description = "Mock Lemmy API server for integration testing"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
poly-test-common = { workspace = true }
axum = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["full"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }

[lints]
workspace = true
```

---

## 9. Test Server

### 9.1 Architecture

`test-lemmy` follows the same pattern as `test-stoat`, `test-matrix`, etc.:

- Uses `poly-test-common` for port binding, CLI args, health/reset/seed routes, and auth state
- Axum-based, with in-memory state protected by `Arc<RwLock<...>>`
- Exposes `POST /seed`, `POST /reset`, `POST /reseed` lifecycle endpoints
- JWT auth via `Authorization: Bearer {token}` header validation

### 9.2 Seed Data

When seeded, the test server creates:

- 2 users: `alice` (password: `password`) and `bob` (password: `password`)
- 3 communities: `!rust@localhost`, `!linux@localhost`, `!gaming@localhost`
- `alice` is subscribed to all 3; `bob` is subscribed to `rust` and `linux`
- 5 posts per community (mix of text, link, and image posts)
- 3-8 comments per post with threaded structure (1-2 levels deep)
- Votes: some posts and comments have pre-seeded votes from both users
- 2 private messages between alice and bob

### 9.3 Implemented Routes

| Route | Handler |
|---|---|
| `POST /api/v3/user/login` | Validate credentials, return JWT |
| `GET /api/v3/site` | Return site info with current user |
| `GET /api/v3/community/list` | List communities with subscription filter |
| `GET /api/v3/community` | Get single community by `id` or `name` |
| `GET /api/v3/post/list` | List posts with `community_id`, `sort`, pagination |
| `GET /api/v3/post` | Get single post by `id` |
| `POST /api/v3/post` | Create post (auth required) |
| `GET /api/v3/comment/list` | List comments with `post_id`, `sort`, pagination |
| `POST /api/v3/comment` | Create comment (auth required) |
| `POST /api/v3/post/like` | Vote on post (auth required) |
| `POST /api/v3/comment/like` | Vote on comment (auth required) |
| `GET /api/v3/user` | Get user profile |
| `GET /api/v3/private_message/list` | List PMs (auth required) |
| `POST /api/v3/private_message` | Send PM (auth required) |
| `GET /api/v3/search` | Search posts/comments |

### 9.4 Integration Tests

Located in `clients/lemmy/tests/integration.rs`, following the Stoat pattern:

- Spin up `test-lemmy` on a random port
- Seed data via `POST /seed`
- Run `LemmyClient` methods against it
- Assert Poly types are correctly mapped
- Reset between test groups via `POST /reseed`

Test cases:
- `lemmy_authenticate_success` / `lemmy_authenticate_wrong_password`
- `lemmy_get_servers_subscribed`
- `lemmy_get_channels_returns_single_forum_channel`
- `lemmy_get_forum_posts_sorted`
- `lemmy_get_messages_comment_tree`
- `lemmy_send_message_creates_comment`
- `lemmy_create_forum_post`
- `lemmy_vote_post` / `lemmy_vote_comment`
- `lemmy_get_dm_channels`
- `lemmy_send_private_message`
- `lemmy_get_user`
- `lemmy_search_messages`
- `lemmy_logout_clears_token`

---

## 10. Implementation Phases

### Phase 1: Foundation (poly-client types + config)

- [ ] **1.1** Add `BackendType::Lemmy` to `clients/client/src/types.rs`
- [ ] **1.2** Add `ChannelType::Forum` to `clients/client/src/types.rs`
- [ ] **1.3** Add `ForumPost`, `ForumPostQuery`, `ForumSortType`, `CommentSortType`, `LinkEmbed`, `VoteTarget` types
- [ ] **1.4** Add `get_forum_posts()`, `get_forum_post()`, `create_forum_post()`, `vote()` default methods to `ClientBackend`
- [ ] **1.5** Create `clients/lemmy/Cargo.toml`
- [ ] **1.6** Create `clients/lemmy/src/config.rs` — `LemmyConfig` with base URL normalization and instance ID derivation
- [ ] **1.7** Create `clients/lemmy/src/http.rs` — `LemmyHttpClient` with JWT auth header injection
- [ ] **1.8** Add `poly-lemmy` to workspace `Cargo.toml`

### Phase 2: API Types + Mapping

- [ ] **2.1** Create `clients/lemmy/src/api.rs` — all Lemmy API response types (`LemmyCommunityView`, `LemmyPostView`, `LemmyCommentView`, `LemmyPersonView`, `LemmyPrivateMessageView`, etc.)
- [ ] **2.2** Create `clients/lemmy/src/mapping.rs` — conversion functions: `community_to_server()`, `community_to_channel()`, `post_to_forum_post()`, `comment_to_message()`, `person_to_user()`, `pm_to_dm_channel()`
- [ ] **2.3** Create `clients/lemmy/src/comment_tree.rs` — `build_comment_tree()` function that takes flat comment list and returns ordered messages with `reply_to` populated

### Phase 3: Core ClientBackend Implementation

- [ ] **3.1** Create `clients/lemmy/src/lib.rs` — `LemmyClient` struct with `LemmyHttpClient`
- [ ] **3.2** Implement `authenticate()`, `logout()`, `is_authenticated()`
- [ ] **3.3** Implement `get_servers()`, `get_server()`
- [ ] **3.4** Implement `get_channels()`, `get_channel()` (single implicit forum channel per community)
- [ ] **3.5** Implement `get_forum_posts()`, `get_forum_post()`, `create_forum_post()`
- [ ] **3.6** Implement `get_messages()` (comment list for a post), `send_message()` (create comment), `send_reply_message()`
- [ ] **3.7** Implement `vote()`
- [ ] **3.8** Implement `get_user()`
- [ ] **3.9** Implement `get_dm_channels()`, `send_message()` on DM channels (private messages)
- [ ] **3.10** Implement `get_notifications()` (user replies + mentions)
- [ ] **3.11** Implement `search_messages()`
- [ ] **3.12** Implement `event_stream()` with polling
- [ ] **3.13** Stub remaining methods with `NotSupported` or empty returns

### Phase 4: Test Server

- [ ] **4.1** Create `servers/test-lemmy/` scaffold with `poly-test-common`
- [ ] **4.2** Implement seed data generation (users, communities, posts, comments, votes, PMs)
- [ ] **4.3** Implement auth routes (`/user/login`, JWT validation middleware)
- [ ] **4.4** Implement community routes (`/community/list`, `/community`)
- [ ] **4.5** Implement post routes (`/post/list`, `/post`, `POST /post`)
- [ ] **4.6** Implement comment routes (`/comment/list`, `POST /comment`)
- [ ] **4.7** Implement vote routes (`/post/like`, `/comment/like`)
- [ ] **4.8** Implement user, PM, and search routes
- [ ] **4.9** Write integration tests in `clients/lemmy/tests/integration.rs`

### Phase 5: UI Integration

- [ ] **5.1** Forum post list view component (title, score, comment count, thumbnail)
- [ ] **5.2** Forum post detail view with threaded comment tree
- [ ] **5.3** Upvote/downvote UI controls on posts and comments
- [ ] **5.4** Post creation dialog (title, body, URL fields)
- [ ] **5.5** Sort/filter controls in the server banner area
- [ ] **5.6** Lemmy signup page component
- [ ] **5.7** Localization (FTL files for en/de/fr/es)

### Phase 6: Polish

- [ ] **6.1** WASM guest implementation (`guest.rs`, `wit_bindings.rs`)
- [ ] **6.2** Plugin host E2E tests (`crates/plugin-host-tests/tests/client_e2e/lemmy.rs`)
- [ ] **6.3** Markdown rendering for post bodies and comments
- [ ] **6.4** NSFW content gating (respect `ContentPolicy` settings)
- [ ] **6.5** Cross-post display (link to same-URL posts in other communities)
- [ ] **6.6** Saved/bookmarked posts view

---

## 11. Special Considerations

### 11.1 Forum vs Chat Model

This is the first non-chat backend. Key differences from chat backends:

- **No real-time stream**: Lemmy has no WebSocket. Polling is the only option for "live" updates.
- **Two-level hierarchy**: Posts contain comments, unlike flat message lists in channels.
- **Vote-driven ordering**: Default sort is by score, not chronological. The UI must show sort controls.
- **Post titles**: Messages don't have titles; forum posts do. The channel view must render differently for `ChannelType::Forum`.
- **No typing indicators**: Lemmy has no typing events. `TypingStarted` events are never emitted.
- **No presence**: All users show as `Offline`. The presence dot should be hidden for Lemmy users.

### 11.2 Comment Tree Building

Lemmy returns comments as a flat list with a `path` field (materialized path like `"0.123.456.789"`). The `comment_tree.rs` module must:

1. Parse the path string to determine parent-child relationships
2. Sort comments within each level according to the requested sort type
3. Flatten back into a depth-annotated list for rendering (or provide tree structure)
4. Populate `reply_to` on each `Message` with the parent comment's preview

The path-based approach is more reliable than `parent_id` alone because it encodes the full ancestry, enabling efficient subtree extraction.

### 11.3 ID Namespacing

Lemmy uses integer IDs that could collide with other backends. All Lemmy IDs are prefixed:

- Communities: `lemmy-community-{id}`
- Channels: `lemmy-feed-{id}` (same numeric ID as community)
- Posts: `lemmy-post-{id}`
- Comments: `lemmy-comment-{id}`
- Users: `lemmy-user-{id}`
- DM channels: `lemmy-dm-{user_id}`
- Private messages: `lemmy-pm-{id}`

### 11.4 Multi-Instance Federation

Lemmy is federated. A user on `lemmy.ml` can subscribe to communities on `lemmy.world`. The Lemmy API handles this transparently — the home instance proxies federated content. Poly does not need to manage federation directly; each account talks only to its home instance.

However, usernames should display in `user@instance` format when the user is from a different instance than the account's home instance (the `actor_id` URL reveals the origin instance).

### 11.5 Rate Limiting

Lemmy instances apply rate limits (configurable per instance, typical defaults):

- Login: 5 per minute
- Post creation: 6 per 10 minutes
- Comment creation: 6 per minute
- Search: 60 per minute
- General reads: 1000 per minute

The HTTP client should detect `429 Too Many Requests` responses and map them to `ClientError::RateLimited` with the `Retry-After` header value.

### 11.6 Server Banner Area

When a Lemmy community is selected, the server banner area at the top of the channel list should show:

- Community banner image (if available)
- Community subscriber count
- Sort order selector (Hot, New, Active, Top, etc.)
- Community description/sidebar toggle

### 11.7 Post Types

Lemmy supports three post types that render differently:

| Type | Detection | Display |
|---|---|---|
| **Text post** | `url` is None, `body` is Some | Title + body preview |
| **Link post** | `url` is Some | Title + link domain + thumbnail + embed preview |
| **Image post** | `url` is Some and points to an image | Title + inline image preview |

The UI must detect the post type from the `url` and `body` fields and render appropriately.

### 11.8 Markdown

Both post bodies and comments are markdown. Lemmy uses a slightly extended CommonMark:

- Standard markdown (bold, italic, links, images, code blocks, blockquotes)
- Spoiler tags: `::: spoiler Title\nHidden text\n:::`
- Community links: `!community@instance`
- User mentions: `@user@instance`

The existing Poly markdown renderer needs to handle spoiler syntax and Lemmy-style mentions.

### 11.9 No Voice/Video

Lemmy has no voice or video features. The `get_voice_participants()` method returns an empty `Vec`, and voice-related UI elements are hidden for Lemmy servers.

### 11.10 Content Moderation (Future)

Lemmy has a rich moderation system (ban, remove, lock, feature, report). These are not in scope for the initial implementation but the API types should be designed to accommodate them later:

- `PostView` includes `removed`, `locked`, `featured_community`, `featured_local`
- `CommentView` includes `removed`, `deleted`
- Moderation actions: `POST /post/remove`, `POST /comment/remove`, `POST /community/ban_user`

---

## 12. Open Questions

1. **How should `get_messages()` work for forum channels?** Currently it returns a flat `Vec<Message>`. For comments, we need either:
   - (a) Return flat list ordered by path (indentation communicated via a new field on `Message`), or
   - (b) Return flat list and let the UI use `reply_to` chains to reconstruct the tree, or
   - (c) Add a `depth: Option<u32>` field to `Message` for forum backends.
   - **Recommendation: Option (c)** — add an optional `depth` field to `Message` that forum backends populate. Chat backends leave it as `None`.

2. **Should `get_messages()` require a post context for forum channels?** Currently `get_messages(channel_id, query)` has no way to specify "get comments for post X in channel Y". Options:
   - (a) Encode the post ID in the channel_id: `lemmy-feed-123:post-456`
   - (b) Add an optional `post_id` field to `MessageQuery`
   - (c) Have the UI call `get_forum_posts()` first, then a separate comments method
   - **Recommendation: Option (b)** — add `post_id: Option<String>` to `MessageQuery`. For forum channels, this is required; for chat channels, it's ignored.

3. **Polling interval**: What should the default polling interval be? Lemmy instances have rate limits, and aggressive polling wastes bandwidth on a platform with low activity velocity.
   - **Recommendation**: 30 seconds default, configurable per-account, minimum 10 seconds.

4. **Community creation**: Should `create_server()` be implemented? Most instances restrict community creation to admins or trusted users. Some open instances allow anyone.
   - **Recommendation**: Defer to Phase 6. When implemented, detect the instance's policy from `GET /site` response (`community_creation_admin_only` field).

5. **Saved posts**: Lemmy has a "saved posts" feature (like bookmarks). Should this map to a virtual server/channel?
   - **Recommendation**: Add as a "Saved" virtual channel within each Lemmy account, similar to "Saved Messages" in other backends.
