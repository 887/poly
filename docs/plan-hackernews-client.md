# Plan: Hacker News Client Backend

> **Created:** 2026-04-05
> **Status:** Not Started
> **Crate:** `poly-hackernews`
> **Goal:** Read-only (optionally authenticated) Hacker News backend implementing `ClientBackend`, surfacing HN stories and threaded comments as a forum-style channel in the Poly unified messenger UI.

---

## Overview

Hacker News is a link aggregator and discussion forum run by Y Combinator. Unlike
the other Poly backends (Stoat, Matrix, Discord, Teams), HN is **not a chat platform** --
it is a threaded forum with stories and nested comments. This backend maps HN's
data model onto the `ClientBackend` trait using the forum/post/thread paradigm:

- **One virtual server:** "Hacker News"
- **Categories with channels** representing the different story feeds (Top, New, Best, Ask HN, Show HN, Jobs)
- **Stories as messages** in each channel (forum post style)
- **Comments as threaded replies** (nested via the `kids` field)

The official Firebase API (`https://hacker-news.firebaseio.com/v0/`) is free, requires
no authentication for reading, and returns JSON. Authentication is optional and only
needed for voting and posting comments.

---

## 1. HN API Reference

Base URL: `https://hacker-news.firebaseio.com/v0/`

### 1.1 Story Feed Endpoints

| Endpoint | Returns | Description |
|----------|---------|-------------|
| `/v0/topstories.json` | `Vec<u64>` (up to 500 IDs) | Top-ranked stories |
| `/v0/newstories.json` | `Vec<u64>` (up to 500 IDs) | Newest stories |
| `/v0/beststories.json` | `Vec<u64>` (up to 500 IDs) | Best stories |
| `/v0/askstories.json` | `Vec<u64>` | Ask HN posts |
| `/v0/showstories.json` | `Vec<u64>` | Show HN posts |
| `/v0/jobstories.json` | `Vec<u64>` | Job postings |

### 1.2 Item Endpoint

`GET /v0/item/{id}.json`

Returns a single item (story, comment, job, poll, or pollopt). Fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `u64` | Unique item ID |
| `type` | `string` | `"story"`, `"comment"`, `"job"`, `"poll"`, `"pollopt"` |
| `by` | `string` | Username of the author |
| `time` | `u64` | Unix timestamp (seconds) |
| `text` | `string?` | HTML body (Ask HN text, comment text) |
| `url` | `string?` | URL for link stories |
| `title` | `string?` | Story/job title |
| `score` | `u64?` | Point count (stories/polls only) |
| `descendants` | `u64?` | Total comment count (stories only) |
| `kids` | `Vec<u64>?` | IDs of direct child comments |
| `parent` | `u64?` | Parent item ID (comments only) |
| `dead` | `bool?` | Whether the item is dead/flagged |
| `deleted` | `bool?` | Whether the item is deleted |

### 1.3 User Endpoint

`GET /v0/user/{username}.json`

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Username |
| `created` | `u64` | Account creation timestamp |
| `karma` | `u64` | Karma score |
| `about` | `string?` | HTML "about" text |
| `submitted` | `Vec<u64>?` | IDs of submitted items |

### 1.4 Live Data Endpoint

`GET /v0/updates.json`

Returns `{ items: Vec<u64>, profiles: Vec<string> }` -- recently changed items and
profiles. Useful for polling-based real-time updates.

### 1.5 Authentication (Optional)

HN has no official auth API. Authentication is done by:

1. `POST https://news.ycombinator.com/login` with form data `acct={username}&pw={password}`
2. Server returns a `user` cookie on success
3. Subsequent requests include this cookie for voting (`POST /vote`), commenting (`POST /comment`), etc.

This is fragile (HTML scraping territory) and should be treated as best-effort.

---

## 2. Mapping to ClientBackend

### 2.1 Server Structure

One virtual server with three categories:

```
Server: "Hacker News" (id: "hn")
  Category: "Stories" (id: "hn-stories")
    Channel: "Top"  (id: "hn-top",  endpoint: topstories.json)
    Channel: "New"  (id: "hn-new",  endpoint: newstories.json)
    Channel: "Best" (id: "hn-best", endpoint: beststories.json)
  Category: "Ask & Show" (id: "hn-askshow")
    Channel: "Ask HN"  (id: "hn-ask",  endpoint: askstories.json)
    Channel: "Show HN" (id: "hn-show", endpoint: showstories.json)
  Category: "Jobs" (id: "hn-jobs")
    Channel: "Jobs" (id: "hn-jobs-ch", endpoint: jobstories.json)
```

### 2.2 BackendType

Add `HackerNews` variant to `BackendType` enum in `clients/client/src/types.rs`:

```rust
pub enum BackendType {
    // ... existing variants ...
    /// Hacker News (read-only forum).
    HackerNews,
}
```

With `display_name() -> "Hacker News"`, `slug() -> "hackernews"`,
`from_slug("hackernews") -> Some(Self::HackerNews)`.

### 2.3 Stories as Messages

Each HN story maps to a `Message`:

| Message field | HN source | Notes |
|---------------|-----------|-------|
| `id` | `item.id.to_string()` | |
| `author.id` | `item.by` | Username is the ID |
| `author.display_name` | `item.by` | HN has no separate display names |
| `content` | `MessageContent::Text(formatted)` | See format below |
| `timestamp` | `DateTime::from_timestamp(item.time)` | |
| `attachments` | `vec![]` | URL could be modeled as attachment in future |
| `reactions` | Score as a pseudo-reaction | `[Reaction { emoji: "^", count: score, me: false }]` |
| `reply_to` | `None` | Stories are top-level |
| `edited` | `false` | HN does not expose edit history |

**Story message format:**

```
{title}
{url or self-text}

{score} points | {descendants} comments | by {author}
```

For Ask HN posts (no URL, have text body): title + HTML-stripped text.
For link posts: title + `({domain})` + URL.
For jobs: title + URL (no score/comments).

### 2.4 Comments as Messages

Comments are fetched via `get_messages(channel_id=post_id, ...)` where the "channel"
context shifts to a specific post's comment thread. This requires a two-level model:

- **Channel-level `get_messages`**: Returns stories (the "posts" in the channel feed)
- **Post-level comments**: Fetched by treating the post ID as a pseudo-channel

Implementation approach: use a channel ID convention:
- `hn-top`, `hn-new`, etc. = story feed channels (return stories as messages)
- `hn-post-{item_id}` = comment thread for a specific story (return comments as messages)

Comments map to `Message` with `reply_to` populated from the parent comment.

### 2.5 Users

HN users map directly:

| User field | HN source |
|------------|-----------|
| `id` | `user.id` (username) |
| `display_name` | `user.id` |
| `avatar_url` | `None` (HN has no avatars) |
| `presence` | `PresenceStatus::Offline` (HN has no presence) |
| `backend` | `BackendType::HackerNews` |

### 2.6 Trait Method Support Matrix

| Method | Support | Notes |
|--------|---------|-------|
| `authenticate` | Optional | Cookie-based HN login |
| `logout` | Optional | Clear stored cookie |
| `is_authenticated` | Yes | Whether cookie is present |
| `get_servers` | Yes | Returns single "Hacker News" server |
| `get_server` | Yes | Returns the HN server by ID |
| `get_channels` | Yes | Returns the 6 feed channels |
| `get_channel` | Yes | Returns a specific feed channel |
| `send_message` | Optional | Post comment (requires auth) |
| `send_reply_message` | Optional | Reply to comment (requires auth) |
| `get_messages` | Yes | Fetch stories or comments |
| `search_messages` | NotSupported | HN has Algolia search but different API |
| `get_pinned_messages` | NotSupported | No pins on HN |
| `get_channel_commands` | NotSupported | No slash commands |
| `get_available_emojis` | NotSupported | No custom emoji |
| `get_available_stickers` | NotSupported | No stickers |
| `set_message_pinned` | NotSupported | |
| `get_user` | Yes | Fetch user profile |
| `get_friends` | NotSupported | No friend system |
| `get_channel_members` | NotSupported | No channel membership |
| `get_groups` | NotSupported | No group DMs |
| `get_dm_channels` | NotSupported | No DMs |
| `get_notifications` | NotSupported | No notification system |
| `get_voice_participants` | NotSupported | No voice |
| `get_presence` | Stub | Always returns `Offline` |
| `set_presence` | NotSupported | |
| `event_stream` | Polling | Poll `/v0/updates.json` for changes |
| `backend_type` | Yes | `BackendType::HackerNews` |
| `backend_name` | Yes | `"Hacker News"` |

---

## 3. Architecture

### 3.1 Crate Structure

```
clients/hackernews/
  Cargo.toml
  src/
    lib.rs          -- HackerNewsBackend implementing ClientBackend
    api.rs          -- Firebase API client (HTTP + caching)
    types.rs        -- HN-specific deserialization types (HnItem, HnUser)
    mapping.rs      -- Convert HN types -> Poly types (Message, User, Server, etc.)
    cache.rs        -- In-memory TTL cache for items and feeds
    auth.rs         -- Optional cookie-based auth for voting/commenting
```

### 3.2 Dependencies

```toml
[package]
name = "poly-hackernews"
description = "Hacker News client for Poly"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["rlib"]

[dependencies]
poly-client = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
reqwest = { workspace = true }
tokio = { workspace = true }
chrono = { workspace = true }
futures = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt", "time", "net"] }
axum = { workspace = true }
serde_json = { workspace = true }

[lints]
workspace = true
```

No WASM guest build needed initially -- HN is read-only and lightweight enough to
run as a native-only client. WASM plugin support can be added later if needed.

### 3.3 API Client (`api.rs`)

```rust
pub struct HnApiClient {
    http: reqwest::Client,
    base_url: String,           // https://hacker-news.firebaseio.com/v0
    cache: HnCache,
    auth_cookie: Option<String>, // Optional HN login cookie
}
```

Key methods:

- `get_story_ids(feed: HnFeed) -> Vec<u64>` -- fetch + cache story ID lists
- `get_item(id: u64) -> Option<HnItem>` -- fetch single item with cache
- `get_items_batch(ids: &[u64]) -> Vec<HnItem>` -- parallel fetch with concurrency limit
- `get_user(username: &str) -> Option<HnUser>` -- fetch user profile
- `get_updates() -> HnUpdates` -- poll for changed items/profiles

### 3.4 Caching Strategy (`cache.rs`)

HN API returns item IDs separately from item data, requiring N+1 requests per feed.
Aggressive caching is essential:

| Data | TTL | Rationale |
|------|-----|-----------|
| Story ID lists (topstories, etc.) | 2 minutes | Feeds update frequently |
| Individual items (stories) | 5 minutes | Score/comment count changes |
| Individual items (comments) | 10 minutes | Comments change less often |
| User profiles | 30 minutes | Karma changes slowly |

Implementation: `HashMap<u64, (HnItem, Instant)>` with TTL-based expiry. No disk
persistence -- items are small and easily re-fetched.

Batch fetching: When loading a feed, fetch the first N story IDs (e.g., 30), then
parallel-fetch all items with a concurrency limit (e.g., 10 concurrent requests) to
avoid hammering the API.

### 3.5 Concurrency Limits

- Max 10 concurrent item fetches per batch request
- Min 100ms between feed ID list refreshes (debounce rapid channel switches)
- Respect HTTP 429 if returned (exponential backoff)

---

## 4. Implementation Plan

### 4.1 Scaffolding

- [ ] **4.1.1** Create `clients/hackernews/` crate with `Cargo.toml`
- [ ] **4.1.2** Add `poly-hackernews` to workspace `Cargo.toml` members and `[workspace.dependencies]`
- [ ] **4.1.3** Add `HackerNews` variant to `BackendType` in `clients/client/src/types.rs`
- [ ] **4.1.4** Wire up feature flag in app crates (behind `hackernews` feature)

### 4.2 Core Types (`types.rs`)

- [ ] **4.2.1** Define `HnItem` -- deserialization struct matching the Firebase item schema
- [ ] **4.2.2** Define `HnUser` -- deserialization struct matching the Firebase user schema
- [ ] **4.2.3** Define `HnFeed` enum -- `Top`, `New`, `Best`, `Ask`, `Show`, `Jobs`
- [ ] **4.2.4** Define `HnUpdates` -- deserialization struct for `/v0/updates.json`
- [ ] **4.2.5** Define `HnItemType` enum -- `Story`, `Comment`, `Job`, `Poll`, `PollOpt`

### 4.3 API Client (`api.rs`)

- [ ] **4.3.1** `HnApiClient::new(base_url)` with `reqwest::Client` setup
- [ ] **4.3.2** `get_feed_ids(feed: HnFeed) -> ClientResult<Vec<u64>>` -- fetch story ID list
- [ ] **4.3.3** `get_item(id: u64) -> ClientResult<Option<HnItem>>` -- fetch single item (cache-aware)
- [ ] **4.3.4** `get_items_batch(ids: &[u64], limit: usize) -> ClientResult<Vec<HnItem>>` -- parallel fetch with concurrency cap
- [ ] **4.3.5** `get_user(username: &str) -> ClientResult<Option<HnUser>>` -- fetch user profile
- [ ] **4.3.6** `get_updates() -> ClientResult<HnUpdates>` -- poll for changes

### 4.4 Cache (`cache.rs`)

- [ ] **4.4.1** `HnCache` struct with `HashMap`-based TTL cache
- [ ] **4.4.2** `get_item(id) -> Option<HnItem>` -- return cached if not expired
- [ ] **4.4.3** `put_item(id, item)` -- insert with timestamp
- [ ] **4.4.4** `get_feed(feed) -> Option<Vec<u64>>` -- cached feed ID lists
- [ ] **4.4.5** `put_feed(feed, ids)` -- insert feed with timestamp
- [ ] **4.4.6** `invalidate_item(id)` -- remove on update notification
- [ ] **4.4.7** Periodic cleanup of expired entries (background task or lazy eviction)

### 4.5 Type Mapping (`mapping.rs`)

- [ ] **4.5.1** `hn_item_to_message(item: &HnItem) -> Message` -- story -> formatted message
- [ ] **4.5.2** `hn_comment_to_message(comment: &HnItem, parent_preview: Option<...>) -> Message` -- comment with reply preview
- [ ] **4.5.3** `hn_user_to_user(user: &HnUser) -> User` -- HN user -> Poly user
- [ ] **4.5.4** `build_server() -> Server` -- construct the static HN server with categories
- [ ] **4.5.5** `build_channels() -> Vec<Channel>` -- construct the 6 feed channels
- [ ] **4.5.6** `feed_for_channel(channel_id: &str) -> Option<HnFeed>` -- channel ID -> feed type
- [ ] **4.5.7** `format_story_text(item: &HnItem) -> String` -- render story as readable text
- [ ] **4.5.8** `strip_html(html: &str) -> String` -- strip HTML tags from comment/about text

### 4.6 Backend Implementation (`lib.rs`)

- [ ] **4.6.1** `HackerNewsBackend` struct with `HnApiClient`, auth state
- [ ] **4.6.2** `authenticate()` -- optional cookie-based login via `news.ycombinator.com`
- [ ] **4.6.3** `logout()` -- clear stored cookie
- [ ] **4.6.4** `is_authenticated()` -- check cookie presence
- [ ] **4.6.5** `get_servers()` -- return `vec![build_server()]`
- [ ] **4.6.6** `get_server(id)` -- return the HN server if `id == "hn"`
- [ ] **4.6.7** `get_channels(server_id)` -- return the 6 feed channels
- [ ] **4.6.8** `get_channel(id)` -- return specific feed channel
- [ ] **4.6.9** `get_messages(channel_id, query)` -- fetch stories for feed channels, comments for post channels
- [ ] **4.6.10** `send_message()` -- post comment (requires auth, else `NotSupported`)
- [ ] **4.6.11** `send_reply_message()` -- reply to comment (requires auth, else `NotSupported`)
- [ ] **4.6.12** `get_user(id)` -- fetch HN user profile
- [ ] **4.6.13** `get_presence()` -- always return `Offline`
- [ ] **4.6.14** `event_stream()` -- poll-based stream using `/v0/updates.json`
- [ ] **4.6.15** `backend_type()` / `backend_name()` -- return `HackerNews` / `"Hacker News"`
- [ ] **4.6.16** All unsupported methods return `NotSupported` or empty defaults

### 4.7 Auth (`auth.rs`)

- [ ] **4.7.1** `HnAuth` struct holding cookie and username
- [ ] **4.7.2** `login(username, password) -> Result<HnAuth>` -- POST to `news.ycombinator.com/login`
- [ ] **4.7.3** `post_comment(parent_id, text, cookie) -> Result<()>` -- POST comment via web form
- [ ] **4.7.4** `vote(item_id, direction, cookie) -> Result<()>` -- POST vote via web form
- [ ] **4.7.5** Cookie extraction from HTTP response headers

---

## 5. Session & Account Model

### 5.1 Session

```rust
Session {
    id: "hn-anonymous" or "hn-{username}",
    user: User { id: username or "anonymous", ... },
    token: "" (anonymous) or cookie string (authenticated),
    backend: BackendType::HackerNews,
    icon_emoji: Some("Y"),
    instance_id: "news.ycombinator.com",
    backend_url: Some("https://hacker-news.firebaseio.com"),
}
```

### 5.2 Anonymous Mode

HN works fully without authentication. The backend should support an "anonymous"
login flow:

- `authenticate(AuthCredentials::Token("anonymous"))` -> creates a read-only session
- Or: the backend works without calling `authenticate()` at all (always returns data)
- This is the primary use case -- most users will just read HN, not post

### 5.3 Authenticated Mode

- `authenticate(AuthCredentials::EmailPassword { email: username, password })` -> attempts HN login
- `email` field repurposed as "username" since HN uses usernames, not emails
- On success, stores the `user` cookie for subsequent write operations
- Authenticated users can: vote on stories, post comments, reply to comments

---

## 6. Message Rendering

### 6.1 Story Display Format

Stories should render in the message view with a forum-post style:

```
[Title of the Story](https://example.com)  (example.com)
--
142 points | 73 comments | posted by dang | 3 hours ago
```

For Ask HN (self-text posts):

```
Ask HN: What's the best way to learn Rust?
--
I've been programming in Python for 5 years and want to try systems programming.
What resources do you recommend?
--
89 points | 45 comments | posted by rustcurious | 6 hours ago
```

For Jobs:

```
YC Startup (YC W26) is hiring a founding engineer
https://example.com/jobs/123
```

### 6.2 Comment Display Format

Comments render as plain text (HTML stripped) in the threaded message view.
`reply_to` is populated so the UI shows the parent comment context.

### 6.3 Score as Reaction

Story scores are mapped to a `Reaction`:

```rust
Reaction {
    emoji: "^",       // Upvote arrow
    count: item.score,
    me: false,         // Would need auth + scraping to determine
}
```

This displays the point count inline with the familiar upvote metaphor.

---

## 7. Event Stream (Polling)

HN has no WebSocket or SSE API. Real-time updates use polling:

1. Poll `/v0/updates.json` every 30 seconds
2. For each changed item ID in the response, check if it's in the cache
3. If cached, re-fetch and emit `ClientEvent::MessageEdited` (score/comment count changed)
4. For new items that are children of cached stories, emit `ClientEvent::MessageReceived`

This is low-fidelity compared to WebSocket backends but sufficient for a forum-style
feed where real-time updates are not critical.

```rust
fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
    let client = self.api.clone();
    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Ok(updates) = client.get_updates().await {
                for item_id in updates.items {
                    // Check if item is in cache, re-fetch, emit events
                }
            }
        }
    };
    Box::pin(stream)
}
```

---

## 8. Pagination

### 8.1 Story Feeds

The API returns up to 500 story IDs per feed. Pagination maps to `MessageQuery`:

- **Initial load** (`query.limit = Some(30)`, no before/after): Fetch first 30 IDs from the feed, batch-fetch items
- **Load more** (`query.before = Some(last_story_id)`): Find the position of `last_story_id` in the cached ID list, fetch the next N items
- **Refresh** (`query.after = Some(first_story_id)`): Re-fetch the feed ID list, return any new stories before the given ID

### 8.2 Comment Threads

Comment pagination is tree-based:

- **Initial load**: Fetch the story item, get its `kids` (top-level comment IDs), batch-fetch those
- **Expand thread**: For each comment with `kids`, fetch child comments on demand
- **Flat vs threaded**: Initially present comments in chronological order with `reply_to` set. The UI's existing threaded reply rendering handles the visual nesting.

---

## 9. Error Handling

| Scenario | Handling |
|----------|----------|
| Item returns `null` (deleted) | Skip, do not include in results |
| Item has `deleted: true` | Show `[deleted]` placeholder message |
| Item has `dead: true` | Show `[flagged]` placeholder message (or hide) |
| Network timeout | Return `ClientError::Network` |
| Rate limited (429) | Return `ClientError::RateLimited` with backoff |
| Invalid item ID | Return `ClientError::NotFound` |
| Auth cookie expired | Clear cookie, return `ClientError::AuthFailed` |

---

## 10. Testing Strategy

### 10.1 Unit Tests

- Type deserialization: parse sample JSON from the Firebase API
- Mapping: verify `hn_item_to_message` produces correct `Message` fields
- Cache: TTL expiry, invalidation, feed caching
- Channel ID parsing: `feed_for_channel`, post channel detection
- HTML stripping: `strip_html` handles HN's common HTML patterns (`<p>`, `<a>`, `<i>`, `<code>`, `<pre>`)

### 10.2 Mock Server Tests

Use `axum` to create a mock Firebase API server (same pattern as `poly-stoat` tests):

```rust
async fn mock_hn_server() -> (SocketAddr, JoinHandle<()>) {
    let app = Router::new()
        .route("/v0/topstories.json", get(|| async { Json(vec![1, 2, 3]) }))
        .route("/v0/item/:id.json", get(mock_item))
        .route("/v0/user/:id.json", get(mock_user));
    // ...
}
```

Test scenarios:
- Fetch top stories and verify message list
- Fetch comments for a story and verify threading
- Handle deleted/dead items gracefully
- Pagination (before/after/limit)
- Cache hit/miss behavior
- Concurrent batch fetching
- Anonymous vs authenticated session creation

### 10.3 Integration Tests (Optional)

Since the HN API is free and public, integration tests can hit the real API:

```rust
#[tokio::test]
#[ignore] // Only run manually or in CI with network
async fn live_fetch_top_stories() {
    let backend = HackerNewsBackend::new();
    let messages = backend.get_messages("hn-top", MessageQuery { limit: Some(5), ..Default::default() }).await.unwrap();
    assert!(!messages.is_empty());
}
```

These should be `#[ignore]`-gated so they don't run in normal `cargo test`.

---

## 11. UI Considerations

### 11.1 Server Sidebar

- Server icon: orange "Y" on white background (or the `icon_emoji: Some("Y")`)
- Server banner: HN orange gradient (`#ff6600`)
- Channel list shows the 6 feeds with unread indicators based on "new stories since last visit" heuristic

### 11.2 Message View

- Stories render as forum posts (title-prominent, metadata line below)
- Clicking a story opens its comment thread (navigates to `hn-post-{id}` pseudo-channel)
- Comments render as threaded messages using the existing reply-chain UI
- External URLs open in the system browser
- Score shown as upvote count (reaction badge)

### 11.3 Composer

- **Anonymous mode**: Composer hidden or shows "Log in to comment"
- **Authenticated mode**: Standard text composer for posting comments
- No file attachments, no emoji picker, no stickers
- No slash commands

### 11.4 User Profile

- Display: username, karma, account age, "about" text
- No avatar (use default/generated avatar)
- No presence indicator
- Link to `https://news.ycombinator.com/user?id={username}`

---

## 12. Future Enhancements (Out of Scope for Initial Implementation)

- [ ] **Algolia search integration** -- HN has a powerful search API at `hn.algolia.com/api/v1/search` that could power `search_messages()`
- [ ] **Favorites/saved stories** -- requires auth, scraping the user's favorites page
- [ ] **Upvote/downvote** -- requires auth + parsing vote links from HTML pages
- [ ] **Story submission** -- POST new stories via authenticated web forms
- [ ] **Poll support** -- map HN polls to a custom message format
- [ ] **User karma history** -- track karma changes over time
- [ ] **"Who is hiring" thread parsing** -- special handling for monthly hiring threads
- [ ] **Push notifications via polling** -- notify on replies to user's comments
- [ ] **WASM plugin build** -- package as WASM component for plugin architecture parity

---

## 13. Workspace Integration Checklist

- [ ] Add `"clients/hackernews"` to `Cargo.toml` workspace members list
- [ ] Add `poly-hackernews = { path = "clients/hackernews" }` to `[workspace.dependencies]`
- [ ] Add `HackerNews` variant to `BackendType` enum with `display_name`, `slug`, `from_slug`
- [ ] Add `hackernews` feature flag to app crates (`apps/web`, `apps/desktop`, etc.)
- [ ] Add signup/login page component for HN (simple username/password form or "Browse Anonymously" button)
- [ ] Register `HackerNewsBackend` in `ClientManager` account creation flow
- [ ] Add HN icon asset (orange Y) to `assets/`
- [ ] Add i18n strings for HN-specific UI labels

---

## 14. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Firebase API rate limits | Feed loading fails | Aggressive caching, batch requests with concurrency limit |
| HN login form changes | Auth breaks | Auth is optional; core read path unaffected |
| No WebSocket = stale data | Users see old scores/comments | 30s polling + manual refresh button |
| N+1 request pattern | Slow initial load | Parallel batch fetch (10 concurrent), progressive rendering |
| HTML in comments | XSS risk in message rendering | Strip HTML to plain text; render code blocks specially |
| Dead/deleted items | Confusing gaps in threads | Show placeholder messages rather than hiding |
| Forum model != chat model | Awkward UX mapping | Use forum-post styling, not chat bubble styling |

---

## 15. Estimated Effort

| Task | Estimate |
|------|----------|
| Scaffolding (4.1) | 1 hour |
| Core types (4.2) | 1 hour |
| API client + cache (4.3, 4.4) | 3-4 hours |
| Type mapping (4.5) | 2 hours |
| Backend implementation (4.6) | 3-4 hours |
| Auth (4.7) | 2 hours |
| Tests (mock server + unit) | 3-4 hours |
| UI integration + signup page | 2-3 hours |
| **Total** | **~17-20 hours** |
