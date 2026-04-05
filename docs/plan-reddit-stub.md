# Plan — Reddit Client Stub

> **Created:** 2026-04-05
> **Status:** Skeleton only (no implementation beyond `NotSupported`)
> **Crate:** `poly-reddit` (`clients/reddit/`)
> **Goal:** Provide a compilable `ClientBackend` stub for Reddit so the backend type exists in the codebase. Not intended to be functional without someone bringing their own API access.

---

## Background

Reddit killed third-party API access for most apps in mid-2023. The free tier is
extremely limited and the paid tier is prohibitively expensive for a client app.
Bot/script accounts still get access but only for their own content. This stub
exists so that:

1. The `BackendType::Reddit` variant is available for routing, UI, and future work.
2. If Reddit ever re-opens API access, or someone has credentials, they can flesh
   out the implementation without scaffolding work.
3. The conceptual data mapping (subreddits to servers, posts to messages) is
   documented for reference.

---

## Data Model Mapping

| Reddit Concept | Poly Concept | Notes |
|---|---|---|
| Subreddit | `Server` | `id` = subreddit name (e.g. `"rust"`), `name` = `"r/rust"` |
| Subreddit icon | `Server.icon_url` | Community icon / `icon_img` field |
| Post (submission) | Forum post / top-level `Message` | `id` = Reddit `t3_` ID |
| Comment | `Message` (threaded reply) | `id` = Reddit `t1_` ID |
| User | `User` | `id` = Reddit username, no real-time presence |
| DM (private message) | `DmChannel` + `Message` | Reddit `/message/inbox` |
| Multireddit | `Category` | Optional grouping of subreddits |
| Post flair | Tag / label on forum post | Per-subreddit flair list |
| User flair | Display suffix on `User.display_name` | |
| Karma | Not mapped | No Poly equivalent |

---

## Implementation Checklist

### 1. Crate Setup

- [ ] **1.1** Create `clients/reddit/Cargo.toml` mirroring `clients/discord/Cargo.toml` structure
- [ ] **1.2** Create `clients/reddit/src/lib.rs` with `RedditClient` struct
- [ ] **1.3** Add WIT bindings module (`wit_bindings.rs`) gated behind `cfg(target_os = "wasi")`
- [ ] **1.4** Add guest module (`guest.rs`) gated behind `cfg(target_os = "wasi")`

### 2. BackendType Variant

- [ ] **2.1** Add `Reddit` variant to `BackendType` in `clients/client/src/types.rs`
- [ ] **2.2** Add `display_name()` -> `"Reddit"`, `slug()` -> `"reddit"`, `from_slug("reddit")` arms

### 3. ClientBackend Stub

- [ ] **3.1** Implement `ClientBackend` for `RedditClient` with every method returning `ClientError::NotSupported("Reddit API access unavailable")`
- [ ] **3.2** `backend_type()` -> `BackendType::Reddit`
- [ ] **3.3** `backend_name()` -> `"Reddit"`
- [ ] **3.4** `event_stream()` -> empty stream

### 4. Workspace Integration

- [ ] **4.1** Add `poly-reddit` to workspace `Cargo.toml` members
- [ ] **4.2** Verify `cargo check -p poly-reddit` passes
- [ ] **4.3** Do NOT add Reddit to the signup picker UI or client manager — it stays dormant

---

## What This Does NOT Include

- No test server (`servers/test-reddit/`)
- No REST client or OAuth flow
- No real API calls of any kind
- No signup page component
- No entry in the signup picker UI

If someone wants to build this out, they would need:
- A Reddit app registration (https://www.reddit.com/prefs/apps)
- OAuth2 PKCE flow for user authentication
- `reqwest` calls to `https://oauth.reddit.com/` endpoints
- Rate limit handling (60 req/min for OAuth clients)
