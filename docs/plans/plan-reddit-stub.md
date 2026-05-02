## Status: PLANNED — not started

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

### Phase A — Crate scaffold + dev-plugins gating

- [ ] **A.1** Create `clients/reddit/Cargo.toml` mirroring
      `clients/discord/Cargo.toml`. Deps: `scraper` (HTML), `reqwest`
      (HTTP, with `cookie_store` feature), `regex`, `chrono`, `serde`,
      `tracing`, `async-trait`, `poly-client-types` (workspace), `thiserror`.
- [ ] **A.2** Create `clients/reddit/src/lib.rs` with `RedditClient` struct
      holding `client: reqwest::Client` (with cookie jar), `base_url:
      &'static str = "https://old.reddit.com"`, `modhash: Mutex<Option<String>>`.
- [ ] **A.3** Add WIT bindings module (`wit_bindings.rs`) and guest module
      (`guest.rs`), both gated `cfg(target_os = "wasi")`, mirroring
      `clients/discord/src/{wit_bindings,guest}.rs`.
- [ ] **A.4** Add `Reddit` variant to `BackendType` enum in
      `clients/client/src/types.rs`. Add arms for `display_name() -> "Reddit"`,
      `slug() -> "reddit"`, `from_slug("reddit") => Some(Reddit)`,
      `BackendType::all()` inclusion.
- [ ] **A.5** Register in `crates/core/src/bundled_plugins.rs` under
      `#[cfg(feature = "reddit")]` block, identical pattern to Discord (line
      174) and Teams (line 185).
- [ ] **A.6** Add `reddit` feature to root `Cargo.toml` `[features]` section,
      grouped under `dev-plugins = ["discord", "teams", "reddit"]`.
- [ ] **A.7** Add `poly-reddit` to workspace `Cargo.toml` members. Verify
      `cargo check -p poly-reddit --features reddit` passes.

### Phase B — HTML parser layer

- [ ] **B.1** Create `clients/reddit/src/parser/` module tree:
      `mod.rs`, `subreddit.rs` (subreddit listing → Vec<Post>),
      `post.rs` (single post page → Post + Vec<Comment>), `inbox.rs` (DM
      inbox), `user.rs` (user profile).
- [ ] **B.2** Implement `parser::subreddit::parse_listing(html: &str) -> Result<Vec<RawPost>, ParseError>`:
      `scraper::Selector` against `div.thing[data-fullname^="t3_"]`, extract
      `data-fullname`, `data-author`, `data-subreddit`, `data-score`,
      `data-timestamp`, post title from `.title a.title`, body from
      `.usertext-body .md`.
- [ ] **B.3** Implement `parser::post::parse_post_page(html: &str) -> Result<(RawPost, Vec<RawComment>), ParseError>`:
      same `t3_` extraction for the OP, then walk `.commentarea > .sitetable >
      .thing[data-fullname^="t1_"]` recursively (comments nest as
      `.child > .listing > .thing`). Track parent IDs from the `data-parent-fullname` attr.
- [ ] **B.4** Implement `parser::inbox::parse_inbox(html: &str) -> Result<Vec<RawDm>, ParseError>`:
      `div.message[data-fullname^="t4_"]`, extract `data-author`,
      `.subject`, `.md` body, `time.live-timestamp` for ISO timestamp.
- [ ] **B.5** Implement `parser::user::parse_user_overview(html: &str) -> Result<UserProfile, ParseError>`:
      `.titlebox` for karma + cake-day, `.profile-img` for avatar.
- [ ] **B.6** Define `ParseError` enum with `MissingSelector(&'static str)`,
      `ParseInt(...)`, `MalformedTimestamp(...)`, `LoggedOut` variants. Every
      parser returns `LoggedOut` if the page contains
      `.login-form` (Reddit redirects unauth'd to login on protected pages).
- [ ] **B.7** Per-parser unit tests using fixture HTML files committed to
      `clients/reddit/tests/fixtures/`. At minimum: `r_rust_hot.html`,
      `comments_t3_xyz.html`, `inbox_empty.html`, `inbox_with_dms.html`,
      `user_overview.html`, `login_redirect.html`.

### Phase C — Cookie auth + modhash

- [ ] **C.1** Implement `RedditClient::login(username, password) -> Result<()>`:
      POST to `https://www.reddit.com/api/login/<username>` form-encoded
      `user, passwd, api_type=json`. Parse JSON response, extract `modhash`
      and the `reddit_session` cookie (auto-stored by `reqwest::cookie_store`).
- [ ] **C.2** Implement `RedditClient::login_with_cookie(reddit_session_cookie: String) -> Result<()>`:
      bypass username/password, set the cookie directly, then GET
      `https://old.reddit.com/api/me.json` to retrieve modhash + verify the
      cookie is live. (This is the "bring your own cookie" path for users
      with 2FA enabled — they grab the cookie from a logged-in browser
      session.)
- [ ] **C.3** Modhash refresh: every response that contains
      `<input name="uh" value="...">` updates the cached modhash. Required
      for any state-mutating POST.
- [ ] **C.4** Persist `reddit_session` cookie in `poly_kv` under
      `client.reddit.<account_id>.session_cookie`. Restore on
      `RedditClient::resume(account_id)`.
- [ ] **C.5** Detect and surface 2FA: login response with
      `"reason": "wrong_password"` AND username has 2FA enabled (no way to
      detect from outside) → return `ClientError::TwoFactorRequired`.
      UI surfaces this with "use cookie auth instead" instructions.
- [ ] **C.6** Rate limit handling: respect `X-Ratelimit-Remaining`,
      `X-Ratelimit-Reset` response headers. Sleep + retry once on `429`.

### Phase D — Read flows (subreddit browsing, DMs)

- [ ] **D.1** `get_servers()` → return user's subscribed subreddits scraped
      from `https://old.reddit.com/subreddits/mine/.compact` (the compact
      HTML view is smaller). Each becomes a `Server { id: "r_<sub>", … }`.
- [ ] **D.2** `get_channels(server_id)` → return single `Channel { id:
      "c_posts", name: "posts" }`. (Sort is UI-side, not channel-side.)
- [ ] **D.3** `get_messages(server_id, channel_id, sort: ChannelSort)` →
      GET `https://old.reddit.com/r/<sub>/<sort>/.compact` where `sort ∈
      {hot, new, top, rising, controversial}`, parse via
      `parser::subreddit::parse_listing`, convert to forum-style
      `Vec<Message>`.
- [ ] **D.4** `get_message_thread(server_id, channel_id, post_id)` → GET
      `https://old.reddit.com/r/<sub>/comments/<post_id>/.compact`, parse via
      `parser::post::parse_post_page`, return OP as parent + comments as
      threaded replies.
- [ ] **D.5** `get_dm_channels()` → GET
      `https://old.reddit.com/message/inbox/.compact`, parse via
      `parser::inbox::parse_inbox`, group messages by counterparty into
      `DmChannel`s.
- [ ] **D.6** `get_dm_messages(dm_id)` → GET
      `https://old.reddit.com/message/messages/<dm_id>/.compact`, parse the
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
      - `/subreddits/mine/.compact`
      - `/r/rust/hot/.compact`, `/r/rust/new/.compact`, `/r/rust/top/.compact`
      - `/r/rust/comments/<id>/.compact` (one with deep nested comments)
      - `/message/inbox/.compact` (with 3 DM threads)
      - `/message/messages/<id>/.compact` (DM thread with 5 back-and-forth)
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
      `/message/inbox/.compact` on the next GET).
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
