## Status: IN PROGRESS — Phase A (a459cea2) + F.2 (a2c95418) + B (a6e2f5c3) + D-anonymous (commit pending) shipped. Anonymous read flows are usable end-to-end. Phase C (cookie auth) optional next; Phase E (writes) blocked on figuring out a working compose endpoint.

## Real-world findings from F.2 fixture capture (2026-05-02)

Findings vs the original write-up. Reddit has tightened old.reddit.com
surface meaningfully since the plan was drafted:

1. **`.compact` URL suffix is gone** — `/r/<sub>/<sort>/` 301-redirects
   to the non-`.compact` URL. Drop every `.compact` reference. The
   non-`.compact` HTML is functionally identical for parsing.
2. **`/login` is now the shreddit React app** — "Welcome to Reddit",
   `class="theme-beta"`, 385KB of JS-app HTML with no scrapable form. Kept
   the file as `login_redirect.html` to drive the **LoggedOut detector**
   (parser returns `LoggedOut` if response contains either the legacy
   `.login-form` selector OR the modern `class="theme-beta"` shreddit
   marker).
3. **`/api/login/<username>` returns 403** — legacy form-POST login is
   locked. **Password login is dead from the public surface.** Phase C
   must drop the password path entirely; bring-your-own-cookie is the
   **only** viable auth flow.
4. **`/api/compose` and `/api/sendmessage` BOTH 404** — the legacy
   write-side endpoints for direct messages are gone too. Phase E.3 (DM
   compose, the *primary use case* for this backend) cannot use these
   form-POST endpoints. Three options to investigate:
   - **OAuth bearer token flow** via `/api/v1/access_token` (would shift
     auth from cookie-paste to a real OAuth dance, much more complex)
   - **Scrape the modern shreddit `/message/compose/` form action + JS**
     (fragile; new-reddit form changes weekly)
   - **Use the stealth path:** new reddit's GraphQL endpoint
     `https://www.reddit.com/svc/shreddit/graphql` with the `token_v2`
     cookie that's set alongside `reddit_session`
5. **Brand-new accounts get HIDDEN from `/subreddits/mine/` HTML**
   (anti-spam delay) — but `/subreddits/mine/subscriber/.json` shows the
   subs immediately. Phase D.1 should fall back to the JSON endpoint when
   the HTML returns 0 subscribed subs (or just always use JSON for that
   endpoint — no parsing benefit from HTML).
6. **`/api/subscribe`** with `X-Modhash` + `X-Requested-With: XMLHttpRequest`
   headers + `uh=<modhash>` form data **DOES still work** — returns
   `200 {}` and the subscription registers (verified via
   `/r/<sub>/about.json` `user_is_subscriber: true`). So at least one
   write-side legacy endpoint survives.

Bonus useful endpoint discovered: `GET /api/me.json` returns JSON without
auth (anonymous loid + experiment flags). Useful as an auth-state probe in
Phase C.

### Fixture inventory (`clients/reddit/tests/fixtures/`)

Public (no auth needed):
- `r_rust_hot.html`, `r_rust_new.html`, `r_rust_top.html` — 3 sort listings, 25/25/18 posts each
- `comments_t3_14921t7.html` — 211 nested comments + 1 OP, ideal stress test for the threading parser
- `user_overview.html` — 25 posts/comments by a real user
- `login_redirect.html` — the new shreddit React app (negative fixture for LoggedOut detector)

Auth-gated (captured via throwaway `sheep` account, sanitized):
- `api_me_sheep.json` — populated `/api/me.json` response (logged-in user data)
- `frontpage_logged_in.html` — `old.reddit.com/` for an authed user (sidebar + nav state)
- `inbox_empty.html` — `/message/inbox/` with no DMs (empty-state fixture)
- `subreddits_mine_empty.html` — `/subreddits/mine/` HTML for new account (anti-spam hide; shows what the page looks like when reddit declines to display subs)
- `subreddits_mine_populated.json` — `/subreddits/mine/subscriber/.json` showing 2 actual subs (r/rust, r/programming) — JSON branch for the parser

Deferred (need a populated state we couldn't easily produce):
- `inbox_with_dms.html` — needs another account messaging us, OR working compose endpoint
- `message_thread_t4_*.html` — needs a populated DM thread
- `subreddits_mine_populated.html` — needs aged account past the anti-spam window
- Real `/api/compose` success response — needs working write endpoint (see finding #4)

**Sanitization applied to every auth-gated fixture:** username, user-id,
loid, modhash, csrf_token, anonbox email, all replaced with `sheep` /
`SANITIZED` placeholders. Verified zero leakage with a 4-needle probe
(`PreviousCurve2228`, `2djlt5h8c4`, `bbbs7hvgrr`, `bafqw`).

# Plan: Reddit Backend (HTML scraping, dev-plugins gated)

> **Created:** 2026-04-05 (rewritten 2026-05-02)
> **Crate:** `poly-reddit` (`clients/reddit/`)
> **Test backend:** `poly-test-reddit` (`servers/test-reddit/`)
> **Goal:** Real, functional Reddit backend that scrapes `old.reddit.com` HTML.
> **Gating:** `dev-plugins` Cargo feature, identical pattern to Discord/Teams
> in `crates/core/src/bundled_plugins.rs:174` (Discord) and `:185` (Teams).

---

## Background — why HTML scraping, why old.reddit, why dev-plugins

Reddit killed third-party API access mid-2023. Free tier is throttled to
uselessness, paid tier is per-request priced for enterprise scale. OAuth still
exists but for app-registered bots, not user clients. The remaining viable
path is the same one RES, every old browser extension, and every defunct
mobile client used: scrape `old.reddit.com` HTML.

**Why old.reddit.com specifically:**
- Stable HTML structure that hasn't changed materially since 2018.
- No client-side React rendering — everything is server-rendered HTML.
- Reddit explicitly maintains it (login page links to it, user prefs toggle
  it as default UI).
- If Reddit ever breaks old.reddit's HTML, we know it's their decision, not
  our parser drifting.
- New reddit (`www.reddit.com`) requires JS execution + GraphQL endpoints
  that change weekly.

**Why dev-plugins gating:** scraping is a TOS gray area. Discord + Teams are
already dev-plugins-gated for the same reason (TOS / API access concerns).
The user explicitly called this out as the gating model.

**The primary use case is DMs.** "Hey, here's my Signal/Matrix handle, let's
move off Reddit" — that's the killer feature. Subreddit browsing is a nice
side effect.

---

## Data model mapping

| Reddit concept | Poly concept | Notes |
|---|---|---|
| Subreddit | `Server` | `id` = `r_<subreddit>`, `name` = `r/<subreddit>` |
| Subreddit icon | `Server.icon_url` | `community_icon` or `icon_img` from `/about.json`-equivalent HTML scrape |
| Subreddit (single channel) | `Channel` | One per subreddit, `id` = `c_posts`, `name` = `posts`. **Sort is a UI dropdown (hot/new/top/rising), not separate channels** — mirrors Lemmy. |
| Post (submission) | Forum post / top-level `Message` | `id` = Reddit `t3_xxx` ID |
| Comment | Threaded reply `Message` | `id` = Reddit `t1_xxx` ID, parent = `t3_` or `t1_` |
| User | `User` | `id` = `u_<username>`, no real-time presence |
| DM (private message) | `DmChannel` + `Message` | Reddit `/message/inbox` and `/message/messages/<id>` |
| Modmail | Out of scope | (mention in docs but not implementing) |
| Multireddit | `Category` | `id` = `m_<owner>_<name>`, optional grouping |
| Post flair | Tag on forum post | Per-subreddit flair list, scraped from `/about/edit` if mod, otherwise from post listing |
| User flair | Suffix on `User.display_name` | E.g. `"username · :rust: Rustacean"` |
| Karma | Not mapped | No Poly equivalent |
| Awards | Not mapped | Reddit awards UI is dead post-2023 |

---

## Phases

### Phase A — Crate scaffold + feature gating — ✅ shipped in commit a459cea2

- [x] **A.1** Created `clients/reddit/Cargo.toml` mirroring
      `clients/lemmy/Cargo.toml` (closer peer than discord — same forum +
      DMs profile). Deps: `scraper` 0.20 (HTML), `reqwest` (HTTP, with
      `cookies` feature), `regex` 1, `chrono`, `serde`, `tracing`,
      `async-trait`, `poly-host-bridge`. Crate type `cdylib + rlib`,
      native feature gated.
- [x] **A.2** Created `clients/reddit/src/lib.rs` with `RedditClient`
      struct holding `http: reqwest::Client` (with cookie jar) and
      `base_url: String`. Constructor returns `Result<Self,
      reqwest::Error>` to surface TLS-backend init failures rather than
      `unwrap` (workspace lints `unwrap_used = "deny"`). Modhash field
      deferred to Phase C — workspace lint `feedback_cfg_gating.md` says
      add fields when they're used, not now-with-`#[allow(dead_code)]`.
- [ ] **A.3** ~~Add WIT bindings module + guest module~~ — **deferred to
      its own future phase**. Reason: discord and lemmy DO have
      `cfg(target_os = "wasi")` `wit_bindings.rs` + `guest.rs` modules,
      but they're full plugin guest implementations (hundreds of lines
      mirroring the `messenger-plugin` WIT world). For Phase A scaffold
      this is overscope. The native `RedditClient` ships as the canonical
      surface; WASI plugin packaging can come back as its own phase if
      anyone needs reddit-as-WASM-plugin (currently no backend ships
      that way in production — they're all native crates loaded by
      poly-core's feature gates).
- [x] **A.4** ~~Add `Reddit` variant to `BackendType` enum~~ — **plan was
      wrong**. `BackendType` is a type alias for `BackendId` which is
      `struct BackendId(String)` (clients/client/src/types.rs:12), a
      string newtype not an enum. No variant to add — `BackendId::new("reddit")`
      and `from_slug("reddit")` work directly. Instead added `"reddit"`
      arm to `capabilities_for_slug` in clients/client/src/types.rs:96
      (forum + DMs profile, mirrors lemmy without moderation flags
      because reddit's mod actions live behind modtools we don't
      scrape).
- [x] **A.5** Registered in `crates/core/src/bundled_plugins.rs:197`
      under `#[cfg(feature = "reddit")]` block, identical pattern to
      Discord (line 174) and Teams (line 185). Icon 🤖 (the snoo
      stand-in), name+desc keys `plugin-reddit-signup-{name,desc}`.
- [x] **A.6** Added `reddit = ["dep:poly-reddit"]` feature + optional
      `poly-reddit` dep to `crates/core/Cargo.toml`. **NOT in default
      features** — same model as `discord` and `teams` (non-default
      per-plugin features). There's no `dev-plugins` umbrella feature
      in this codebase; the original plan's
      `dev-plugins = ["discord", "teams", "reddit"]` was speculative
      and not how the existing gating works.
- [x] **A.7** Added `clients/reddit` to root `Cargo.toml`
      workspace.members + `poly-reddit = { path = "clients/reddit" }`
      to workspace.dependencies. Verified `cargo check -p poly-reddit`,
      `cargo check -p poly-core --features reddit`, AND
      `cargo check -p poly-core` (default, no reddit) all pass clean.

### Phase B — HTML parser layer — ✅ shipped (commit pending)

- [x] **B.1** Created `clients/reddit/src/parser/` module tree (mod.rs +
      subreddit.rs + post.rs + inbox.rs + user.rs), wired via `pub mod
      parser;` under `#[cfg(feature = "native")]` in lib.rs.
- [x] **B.2** `parser::subreddit::parse_listing` — extracts every
      `div.thing[data-fullname^="t3_"]` from a listing. Uses `data-*`
      attribute access (`data-fullname`, `data-author`, `data-subreddit`,
      `data-score`, `data-timestamp` epoch-ms, `data-permalink`,
      `data-comments-count`, `data-url`) — more stable than text
      scraping since these attrs are part of the public DOM contract.
      Title from `a.title`, body from `div.usertext-body div.md`.
- [x] **B.3** `parser::post::parse_post_page` — OP extracted via the
      same `t3_` machinery, then walks `div.commentarea > div.sitetable
      > div.thing.comment[data-fullname^="t1_"]` for top-level comments
      and recurses on `:scope > div.child > div.listing >
      div.thing.comment[data-fullname^="t1_"]` for replies. **Key
      finding from real fixture:** the bare `div.thing[data-fullname^="t1_"]`
      selector also matches `morechildren` "load more" placeholders
      (24 of 211 t1_ containers in the test fixture). Adding `.comment`
      class filter (`div.thing.comment[...]`) excludes them cleanly —
      placeholders don't have the `comment` class.
- [x] **B.4** `parser::inbox::parse_inbox` — extracts every
      `div.message[data-fullname^="t4_"]`. Empty inbox returns
      `Ok(Vec::new())` not an error. Subject from `a.subject`, body from
      `div.md`, timestamp from `time.live-timestamp[datetime=...]`.
- [x] **B.5** `parser::user::parse_user_overview` — extracts the user's
      name from `<title>overview for <name></title>`, optional avatar
      from `img.profile-img`, and a mixed `Vec<UserOverviewItem>` of
      `Post(RawPost)` and `Comment(RawComment)` entries. Comments on
      overview pages are flat (no reply nesting), so a local
      `parse_user_comment` helper avoids pulling the full recursive
      machinery.
- [x] **B.6** `ParseError` enum with variants: `LoggedOut`,
      `MissingElement(&'static str)`, `MalformedInt(&'static str)`,
      `MalformedTimestamp(String)`. **LoggedOut detector** matches BOTH
      legacy `.login-form` AND modern shreddit (`class="theme-beta"` +
      `<title>Welcome to Reddit</title>` together) — the cheap byte-level
      check runs as the first step of every parser. Per-helper
      `data_attr` and `parse_timestamp_ms` (handles both epoch-ms and
      RFC-3339 datestring) live in `mod.rs` as crate-private utilities.
- [x] **B.7** Per-parser unit tests against fixtures. **15 passing
      tests, 0 failures:**
      - 6 unit tests in `mod.rs` (LoggedOut detection ×3,
        `parse_html` short-circuit ×1, timestamp parser ×2)
      - `tests/parser_subreddit.rs` ×4 (hot/new/top + LoggedOut
        short-circuit on `login_redirect.html`)
      - `tests/parser_post.rs` ×2 (OP + threaded comments + timestamp/
        score sanity for the 211-comment fixture; got 170 real comments
        after filtering 24 morechildren placeholders, which is within
        the expected 150-200 band)
      - `tests/parser_inbox.rs` ×2 (empty inbox + LoggedOut)
      - `tests/parser_user.rs` ×1 (name + recent items extraction)
      The `inbox_with_dms.html` fixture is still deferred (need
      working compose endpoint or a second account to populate).

### Phase C — Cookie auth + modhash (REVISED post-F.2: cookie-only)

Per F.2 findings: `/api/login/<username>` returns 403 → password-based login
is dead from the public surface. Only viable auth path is bring-your-own-
cookie. C.1 (password login) is **REMOVED**.

- [ ] **C.1** ~~Password login~~ — **REMOVED**. `/api/login/<u>` is locked
      (403 Forbidden as of 2026-05-02). Document this in
      `docs/dev/test-backends.md` Reddit section.
- [ ] **C.2** Implement `RedditClient::login_with_cookie(reddit_session: String) -> Result<()>`:
      set the cookie directly into the `reqwest::cookie_store`, then GET
      `https://old.reddit.com/api/me.json` to verify the cookie is live
      (anon response has empty `data.name`; live cookie response has
      populated user fields). Extract modhash from the response if present
      (`data.modhash`).
- [ ] **C.3** Modhash refresh: every authenticated HTML response contains
      `<input name="uh" value="...">` somewhere — parser updates the cached
      modhash on every fetch. Required for any state-mutating POST.
- [ ] **C.4** Persist `reddit_session` cookie in `poly_kv` under
      `client.reddit.<account_id>.session_cookie`. Restore on
      `RedditClient::resume(account_id)`.
- [ ] **C.5** Auth-state detection: parser layer (Phase B.6) returns
      `LoggedOut` when response HTML contains EITHER the legacy `.login-form`
      selector OR the modern shreddit marker `class="theme-beta"` AND
      `<title>Welcome to Reddit</title>`. `RedditClient` surfaces this as
      `ClientError::SessionExpired` so the UI prompts for a fresh cookie.
- [ ] **C.6** Rate limit handling: respect `X-Ratelimit-Remaining` (seen
      in F.2 capture as `99.0`), `X-Ratelimit-Reset` response headers.
      Sleep + retry once on `429`.

### Phase D-anonymous — Read flows (no-auth subset) — ✅ shipped (commit pending)

The first three read methods need no auth at all — they were the first
shippable subset of Phase D. Cookie-required reads (subscribed list, DMs,
inbox) stay in the unticked Phase D list below for when Phase C lands.

- [x] **D-anon.1** `RedditClient::list_subreddit(subreddit, sort: SortKind)`
      — `GET /r/<sub>/<sort>/`, parses via `parser::subreddit::parse_listing`.
      `SortKind` enum covers Hot / New / Top / Rising / Controversial.
- [x] **D-anon.2** `RedditClient::get_post(post_id)` — `GET /comments/<id>/`,
      reqwest follows the 301 reddit issues to add the canonical slug.
      Parses via `parser::post::parse_post_page`.
- [x] **D-anon.3** `RedditClient::get_user(username)` — `GET /user/<u>/`,
      parses via `parser::user::parse_user_overview`.
- [x] **D-anon.4** `RedditError` enum (Http / Parse / LoggedOut /
      Status(u16)) with `From<reqwest::Error>` and `From<ParseError>`.
- [x] **D-anon.5** `tests/integration_anonymous_read.rs` — 4 live wire
      tests against `old.reddit.com`, all `#[ignore]`'d so CI doesn't
      depend on the live internet. Manual run:
      `cargo test -p poly-reddit --test integration_anonymous_read -- --ignored`.
      Verified live: r/rust hot listing fetches + parses ✓
- [x] **D-anon.6** Bumped default UA to a real Firefox UA string —
      `Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0`.
      The bare `poly-reddit/0.1` string was getting rate-limited.
      Regression-guarded by a unit test.

### Phase D — Read flows (auth-gated subset, requires Phase C)

- [ ] **D.1** `get_servers()` → return user's subscribed subreddits scraped
      from `https://old.reddit.com/subreddits/mine/` (the standard
      HTML view is smaller). Each becomes a `Server { id: "r_<sub>", … }`.
- [ ] **D.2** `get_channels(server_id)` → return single `Channel { id:
      "c_posts", name: "posts" }`. (Sort is UI-side, not channel-side.)
- [ ] **D.3** `get_messages(server_id, channel_id, sort: ChannelSort)` →
      GET `https://old.reddit.com/r/<sub>/<sort>/` where `sort ∈
      {hot, new, top, rising, controversial}`, parse via
      `parser::subreddit::parse_listing`, convert to forum-style
      `Vec<Message>`.
- [ ] **D.4** `get_message_thread(server_id, channel_id, post_id)` → GET
      `https://old.reddit.com/r/<sub>/comments/<post_id>/`, parse via
      `parser::post::parse_post_page`, return OP as parent + comments as
      threaded replies.
- [ ] **D.5** `get_dm_channels()` → GET
      `https://old.reddit.com/message/inbox/`, parse via
      `parser::inbox::parse_inbox`, group messages by counterparty into
      `DmChannel`s.
- [ ] **D.6** `get_dm_messages(dm_id)` → GET
      `https://old.reddit.com/message/messages/<dm_id>/`, parse the
      thread.
- [ ] **D.7** Avatar resolution: `User.avatar_url` → resolve from
      `https://old.reddit.com/user/<u>/about.json`-equivalent HTML scrape;
      cache per-session in `Mutex<HashMap<String, String>>`.
- [ ] **D.8** Pagination: subreddit listings have `<a class="next-button">
      href="...?after=t3_xxx">`. Surface as `next_cursor: Option<String>` on
      the message list response.

### Phase E — Write flows (post, comment, DM, vote)

- [ ] **E.1** `send_message(server_id, channel_id, content)` → if
      `channel_id == "c_posts"` POST to
      `https://oauth.reddit.com/api/submit` form-encoded `sr=<sub>,
      kind=self, title=<first-line-of-content>, text=<rest>, uh=<modhash>`.
      (Or scrape the submit form from `/r/<sub>/submit` and POST to its
      action URL — pick whichever is more stable; document the call site.)
- [ ] **E.2** `send_message_reply(parent_message_id, content)` → POST to
      `https://old.reddit.com/api/comment` with `thing_id=<t1_or_t3>,
      text=<content>, uh=<modhash>`.
- [ ] **E.3** `send_dm(recipient_username, subject, body)` → POST to
      `https://old.reddit.com/api/compose` with `to=<user>, subject=<>,
      text=<>, uh=<modhash>`. **This is the primary use case** — message
      someone to suggest moving the conversation off Reddit.
- [ ] **E.4** `send_dm_reply(dm_thread_id, content)` → POST to
      `https://old.reddit.com/api/comment` with `thing_id=<t4_>,
      text=<content>, uh=<modhash>` (DMs reuse the comment endpoint with
      `t4_` prefix).
- [ ] **E.5** Vote: POST to `https://old.reddit.com/api/vote` with
      `id=<t3_or_t1>, dir=<-1|0|1>, uh=<modhash>`. Map to a "reaction" UI
      action on Poly's side (👍 = +1, 👎 = -1, click-again = 0).
- [ ] **E.6** Edit / delete own post or comment: POST to
      `/api/editusertext` and `/api/del` respectively.
- [ ] **E.7** Mark DM read: POST to `/api/read_message` with `id=<t4_>`.

### Phase F — Heavyweight test backend (`servers/test-reddit/`)

User chose heavyweight: full HTML fixture replay. The reason is forensic —
if Reddit changes old.reddit.com markup, our parser tests catch the drift
in CI before any user notices the production breakage.

**Test animals (with emoji):** 🐑 `sheep` and 🐋 `walrus`. Both already exist
in `clients/demo/assets/{sheep,walrus}.{png,svg}` and follow the established
animal-mapping convention from `docs/dev/test-backends.md` and the avatar
table in CLAUDE.md. `sheep` is the test username with subscribed subreddits
(r/rust, r/programming) and a populated DM inbox; `walrus` is the DM
recipient for the "compose new DM" smoke test (the canonical "hey come to
Signal" use case).

- [ ] **F.1** Create `servers/test-reddit/` mirroring `servers/test-discord/`
      structure. Cargo deps: `axum`, `tokio`, `tower-http` (for static
      file serving), `tracing`, `serde`, `poly-test-common`.
- [ ] **F.2** Capture real fixtures from `old.reddit.com` (logged in as
      throwaway test account) for ten anchor pages:
      - `/subreddits/mine/`
      - `/r/rust/hot/`, `/r/rust/new/`, `/r/rust/top/`
      - `/r/rust/comments/<id>/` (one with deep nested comments)
      - `/message/inbox/` (with 3 DM threads)
      - `/message/messages/<id>/` (DM thread with 5 back-and-forth)
      - `/user/sheep/about.json`-equivalent HTML
      - `/login` (logged-out redirect target)
      - `/api/login/<u>` 200 response (cookie-set headers + JSON body)
      Sanitise all PII (real usernames → `sheep`, `walrus`, `penguin`,
      `koala`, `otter` per `docs/dev/test-backends.md` animal convention).
      Commit as `servers/test-reddit/fixtures/*.html` and `*.json`.
- [ ] **F.3** Implement axum routes that serve each fixture verbatim. Mock
      auth: any POST to `/api/login/sheep` with form `passwd=<anything>`
      returns the canned login response + sets `reddit_session=sheep_session`
      cookie. Other usernames → "wrong_password" JSON error.
- [ ] **F.4** Mock state mutations: `/api/comment`, `/api/submit`,
      `/api/vote`, `/api/compose` all accept the POST and return canned
      success JSON. Maintain in-memory state so subsequent GETs reflect the
      mutation (e.g. POSTing to `/api/compose` adds a fixture row to
      `/message/inbox/` on the next GET).
- [ ] **F.5** Avatar serving: `/avatars/<animal>` returns the corresponding
      `clients/demo/assets/<animal>.png` via the shared
      `servers/test-common::avatars::serve_animal` helper. Update
      `CLAUDE.md` "Test-server Avatar URL Conventions" table to include
      Reddit at port 9108. Both `sheep` (🐑) and `walrus` (🐋) avatars
      already exist in the assets dir; no new artwork needed.
- [ ] **F.6** Wire `poly-test-runner` to start `test-reddit` on port 9108.
      Update `servers/test-runner/src/main.rs` and
      `docs/dev/test-backends.md`.
- [ ] **F.7** Configure `poly-reddit` in test mode: when env
      `REDDIT_BASE_URL=http://127.0.0.1:9108` is set, override
      `base_url` field in `RedditClient`. Use this for
      `clients/reddit/tests/integration_*.rs` so the integration tests run
      against the local mock with real HTTP.

- [ ] **F.8** Test-account entries — the "Add test account" buttons. Add
      `pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry]`
      to `clients/reddit/src/signup.rs` (mirror `clients/lemmy/src/signup.rs:51`).
      Two entries:
      - `sheep` — `server_label: "Reddit — localhost:9108"`,
        `base_url: "http://localhost:9108"`, `password: "testpass123"`,
        avatar 🐑
      - `walrus` — same server, `password: "testpass123"`, avatar 🐋
      Both auto-appear in the `/signup/test` quick-add panel
      (`crates/core/src/ui/signup.rs:570 SignupTest`) once the entries are
      registered through `register_plugin` (see `clients/lemmy/src/lib.rs`
      registration call site).
- [ ] **F.9** Wire the entries into the plugin registration so the panel's
      `test_account_entries` collection picks them up under the
      `dev-plugins` feature gate. Verify the buttons render in the
      `/signup/test` page when the test runner is up.

### Phase G — UI surface (forum-style, mirrors Lemmy/HackerNews)

- [ ] **G.1** Add Reddit signup option in `crates/core/src/ui/onboarding/`
      under `#[cfg(feature = "reddit")]`. Two paths: username+password
      (works without 2FA), or paste-cookie (works with 2FA, surfaces 2FA
      requirement explicitly).
- [ ] **G.2** Channel-list rendering: subreddit servers show a single "posts"
      channel with the sort dropdown (hot / new / top / rising /
      controversial) inline at the top of the channel header. Mirror the
      pattern from `crates/core/src/ui/server/lemmy_channel_header.rs` (or
      wherever Lemmy renders its sort dropdown — `grep -rn 'ChannelSort' crates/core/src/ui/`).
- [ ] **G.3** Forum-post rendering: top-level posts render with title +
      preview + score + comment count. Threaded comment view drops into the
      existing forum-post-detail UI used by Lemmy / Forgejo / GitHub
      issues.
- [ ] **G.4** DM UI: DMs render in the existing DM channel view — same
      `DmChannelView` component that Matrix / Discord DMs use. The
      "compose new DM to username X" action is the headline workflow;
      surface it prominently (top-of-sidebar button or `/dm <username>`
      slash command).
- [ ] **G.5** Vote → reaction mapping: render the upvote/downvote arrows as
      Poly's standard `MessageReactionBar` with emoji 👍/👎, click handlers
      wired to `send_vote(...)`.
- [ ] **G.6** FTL keys for all new UI strings: add to
      `crates/core/i18n/en.ftl` under `reddit-*` prefix. At minimum:
      `reddit-signup-cookie-instructions`, `reddit-signup-2fa-required`,
      `reddit-channel-sort-{hot,new,top,rising,controversial}`,
      `reddit-dm-compose-to`.

### Phase H — End-to-end testing + acceptance + DONE bar

- [ ] **H.1** All Phase A-G boxes ticked.
- [ ] **H.2** Unit-test suite (`clients/reddit/tests/parser_*.rs`) passes
      against the committed HTML fixtures from F.2; every parser has at
      least one fixture-driven test plus a `LoggedOut` negative case using
      `login_redirect.html`.
- [ ] **H.3** Integration test suite (`clients/reddit/tests/integration_*.rs`)
      passes against `servers/test-reddit/` covering the full flow: login
      as `sheep` → list subscribed subreddits → open r/rust hot → drill
      into a post + read comments → open inbox → compose DM to `walrus`
      ("hey come to Signal") → reply on the resulting thread → upvote a
      post → log out → restore session via persisted cookie.
- [ ] **H.4** End-to-end harness (`TEST_HARNESS.md`-style): add a step that
      starts `poly-test-runner`, launches `apps/web` via `mcp__poly-web__*`,
      navigates to `/signup/test`, clicks the 🐑 sheep test-account button,
      asserts the resulting account loads r/rust posts + the inbox view
      shows 3 fixture DMs. Run via the haiku test agent per CLAUDE.md
      orchestration rules.
- [ ] **H.5** UI smoke flow recorded: sheep account → click "Compose DM" →
      type `walrus` as recipient → send → verify it appears in the sheep
      outbox AND in the walrus inbox (the test backend persists in-memory
      mutations per F.4).
- [ ] **H.6** Manual smoke test against real `old.reddit.com` with a
      throwaway account: sign up via cookie path, browse a subreddit, read
      DMs. (Record results in commit message; do not commit the cookie.)
- [ ] **H.7** `cargo check --features dev-plugins` passes; `cargo check`
      (without dev-plugins) does not pull in any Reddit code; the
      `/signup/test` page does NOT show reddit test-account buttons in
      release builds.
- [ ] **H.8** Documentation: `docs/dev/test-backends.md` updated with
      Reddit section (port 9108, sheep+walrus animal map, curl recipes for
      the mock endpoints, reset endpoint); `CLAUDE.md` avatar-conventions
      table updated.
- [ ] **H.9** Status header flipped to `## Status: ✅ DONE — all phases
      shipped (commits …)`.

---

## What this plan explicitly DOES include (changed from skeleton)

- Real HTTP scraping of `old.reddit.com`
- Cookie-based auth (with username/password and bring-your-own-cookie paths)
- DM read + compose + reply (the primary use case)
- Subreddit browsing with sort dropdown
- Vote + comment + post creation
- Heavyweight test backend with real captured HTML fixtures
- Full UI integration (signup, channel list, forum view, DM view)

## What this plan explicitly does NOT include

- New reddit (`www.reddit.com`) support — old reddit only
- OAuth-app flow — irrelevant for user accounts post-2023
- Modmail — out of scope
- Awards — dead UI surface
- Reddit Chat (the WebSocket-based chat product) — separate protocol, not
  HTML-scrapable; defer to a future plan if anyone asks
- Multireddits as first-class navigation — captured as `Category`s but no
  dedicated UI in MVP
