## Status: 🚧 PLANNED — not started

# Test-account avatars + Lemmy preview-image + Forum-composer UX overhaul

> Three loosely-coupled improvements that share a common theme — making the
> non-Discord forum/chat backends feel as polished as test-discord +
> test-stoat already do. They live in one plan because (a) Phase A's
> per-backend audit informs Phase B's Lemmy seed-data change, and (b)
> Phase C's unified Forum composer needs the new backend `create_post`
> contract that Phase B/A also touch. Splitting into three sibling plans
> would force orchestration overhead with no benefit.

## Why this is its own plan vs sibling additions

The avatar work is a cross-backend uniformity sweep (no single existing
plan owns it); the Lemmy preview-image work is a small new feature on the
Lemmy client + a new client-setting; the Forum composer is an
oversize-component refactor that incidentally needs `create_post` to be
real. Each is a short, isolated piece — bundling them as one plan keeps
the audit table in Phase A as a single source of truth that Phases B-D
can reference.

## Pre-flight audit (read-only, completed during planning)

### Per-backend avatar wire-up status

Pattern reference: test-discord ships avatar bytes via
`servers/test-discord/src/routes.rs:serve_avatar` (commit `71fd1d48`+;
the route shape is `GET /avatars/{user_id}/{file}.png`, mapping the
hash segment to embedded PNG bytes from `clients/demo/assets/`).
Surfaces inspected: chat messages, DM list rows, forum thread rows,
forum comments, member/friend lists, @mention previews.

| Backend         | Seed user(s)                       | Surface(s)                             | Avatar in seed?            | Server route exists?                              | Bytes available?                  | Action needed                                                                                        |
|-----------------|-------------------------------------|----------------------------------------|----------------------------|---------------------------------------------------|-----------------------------------|------------------------------------------------------------------------------------------------------|
| test-discord    | koala / kangaroo / wallaby (platypus stand-in) | chat, DM, forum, members          | YES (`avatar: Some("…")`) | YES (`/avatars/{user_id}/{file}.png`)             | YES (koala/kangaroo/platypus PNG) | DONE — leave as the reference pattern                                                                |
| test-stoat      | stoat / raccoon / lemming           | chat, DM, members                      | YES (`avatar_url: Some("…")`) | YES (`/avatars/{id}` mapped via `av_{user.id}`)   | YES (stoat/raccoon/lemming PNG)   | DONE — but see B.4 fallback note                                                                     |
| test-matrix     | owl / axolotl                       | chat, DM, members                      | YES (mxc URLs in seed)     | YES (`/_matrix/media/v3/thumbnail/{srv}/{id}`)    | YES (owl PNG, axolotl SVG, room SVGs) | EXTEND — only 2 users wired; add 2-3 more (e.g. `cat_avatar` for the `cat` test user if seeded)      |
| test-forgejo    | otter / flamingo / testuser         | issue authors, repo members, PR reviewers | YES (`avatar_url` ext URLs) | YES (`/avatars/{name}`)                          | YES (otter SVG, flamingo SVG)     | EXTEND — add an SVG for `testuser` (currently falls back to otter)                                   |
| test-github     | penguin / chameleon                 | issue authors, repo members, PR reviewers | YES (`avatar_url` set to `https://github.com/…`) | NO — URLs point to real github.com which 404s    | NO                                | ADD — `/avatars/{login}.png` route returning embedded PNG; rewrite seed `avatar_url` to local URL    |
| test-teams      | Sheep (U001) / Walrus (U002)        | chat, member roster, channel headers  | NO (`avatar_url: None`)    | NO                                                | NO                                | ADD — Graph-style `/users/{id}/photo/$value` route + seed users with `avatar_url`                    |
| test-lemmy      | testuser / beaver / hedgehog        | post authors, comment authors, community members | NO (`avatar: None`)        | NO                                                | NO                                | ADD — `/pictrs/image/{name}.png` route (Lemmy convention) + seed `avatar` URLs                       |
| test-hackernews | usernames only (`pg`, `dang`, …)    | story authors, comment authors        | N/A — HN has no avatars    | N/A                                               | N/A                               | NONE — HN itself has no user avatars; UI already falls back to coloured initial. Document & skip.    |

Animal assignments (chosen so the avatar matches the username when
possible; reuses files already in `clients/demo/assets/`):

| Backend       | User           | Animal asset                                    |
|---------------|----------------|-------------------------------------------------|
| test-matrix   | additional `cat` (if seeded) | `cat.png`                                      |
| test-matrix   | additional `dog` (if seeded) | `dog.png`                                      |
| test-forgejo  | testuser       | `axolotl.svg`                                   |
| test-github   | penguin        | currently no `penguin.png` — use `koala.png` placeholder OR commission a new `penguin.png` (decide in A.4) |
| test-github   | chameleon      | `parrot.png` (closest brightly-coloured stand-in) OR new `chameleon.png` |
| test-teams    | Sheep (U001)   | `sheep.png`                                     |
| test-teams    | Walrus (U002)  | `walrus.png`                                    |
| test-lemmy    | beaver         | `beaver.svg` → render to PNG OR ship `beaver.png` |
| test-lemmy    | hedgehog       | `hedgehog.svg` → render to PNG OR ship `hedgehog.png` |
| test-lemmy    | testuser       | `axolotl.svg` (matches forgejo testuser for cross-backend recognition) |

### Lemmy preview-image gap

- `clients/lemmy/src/api.rs:144-153` — `LemmyPost` struct does **NOT**
  carry `thumbnail_url`. Real Lemmy API returns `thumbnail_url:
  Option<String>` on the `post::Post` row (see lemmy-api-common
  `LemmyPost`/`Post` schema in `lemmy_db_schema::source::post::Post`,
  populated by pict-rs when a URL is shared and metadata fetch
  succeeds). Verify on first impl by hitting a public Lemmy instance:
  `curl https://lemmy.world/api/v3/post/list?limit=1 | jq
  '.posts[0].post | keys'` should contain `"thumbnail_url"`.
- `clients/client/src/types.rs:1130-1153` — in-app `Message` has no
  preview-image field. Existing `Attachment` type is used for files
  (and Lemmy's mapper at `clients/lemmy/src/api.rs:444-452` already
  shoves the post URL into one), but no field for "preview thumbnail
  to show next to a forum row".
- `crates/host-bridge/src/client_config.rs` — `ClientConfigStore`
  exists (Phase C of the client-version plan); `Settings::lemmy.*`
  does not yet have a `render_previews` field.

### Forum composer pain points

- `crates/core/src/ui/create_forum_post.rs:124` — Submit button is a
  literal stub: `// TODO: call backend create_post when API is
  available`. The backend WIT method `create-forum-post` (line 1582 of
  `wit/messenger-plugin.wit`) DOES exist already and most backends
  implement it — the UI just doesn't wire to it.
- The current form has no markdown preview tab, no attachment upload,
  no draft autosave, no link-URL preview-fetch, no length counter, no
  keyboard shortcut to submit (Ctrl/Cmd+Enter).
- `crates/core/src/ui/account/common/forum_view.rs:399-490` —
  `ForumComment` renders the recursive comment tree but has **NO
  Reply/Comment button**. There's no inline reply composer at all;
  the UI is read-only for comments.
- `discord_forum_view.rs` (809 lines) is its own gallery/list shell;
  Lemmy/Forgejo/GitHub/HN go through `forum_view.rs` (492 lines).
  Both lack a unified "compose" abstraction.

## Phase A — per-backend avatar wire-up

Effort: ~6-8h. Owner: 1 sonnet agent (worktree, can parallelise A.1-A.5
since each touches a disjoint backend).
Depends on: nothing.
Acceptance: every test backend (except hackernews) returns 200 + valid
image bytes for at least 2 of its seeded users when the URL exposed in
its API response is fetched directly. Verified by adding a Rust
integration test per backend.

- [ ] **A.1 — test-matrix avatar extension**
  - Audit `servers/test-matrix/src/state.rs` seed for any users beyond
    owl/axolotl. If a `cat` or `dog` user is seeded, add `mxc://localhost/cat_avatar`
    + `dog_avatar` to their `avatar_url` field.
  - Extend `routes.rs:media_thumbnail` match arm with `cat_avatar` →
    `cat.png`, `dog_avatar` → `dog.png` (include_bytes! from
    `clients/demo/assets/`).
  - Add `tests/avatar_serving.rs` integration test that asserts
    `GET /_matrix/media/v3/thumbnail/localhost/cat_avatar` returns 200
    + `content-type: image/png` + nonzero body.
- [ ] **A.2 — test-forgejo testuser SVG**
  - Add `axolotl.svg` to `serve_avatar` match in
    `servers/test-forgejo/src/routes.rs:441` so `testuser` resolves
    to a distinct asset instead of falling back to otter.
  - Update assertion in any existing test-forgejo Rust test (search
    `servers/test-forgejo/tests/`).
- [ ] **A.3 — test-teams Graph-photo route**
  - Seed `U001` with `avatar_url: Some("sheep")`, `U002` with
    `Some("walrus")`.
  - Add `serve_user_photo` handler at
    `GET /v1.0/users/{user_id}/photo/$value` returning embedded
    sheep.png/walrus.png bytes (this is the path the Microsoft Graph
    SDK calls for profile photos).
  - Mount on the Teams router in `lib.rs`.
  - Map server response in `clients/teams/src/api.rs` to populate
    `User.avatar_url` from this URL.
  - Smoke test: hit the endpoint with a curl one-liner from the
    test-teams integration suite.
- [ ] **A.4 — test-github avatar route + asset decision**
  - Decide whether to ship new `penguin.png` + `chameleon.png` assets
    (preferred — adds two animals to the demo set) or to alias to
    `koala.png` + `parrot.png`. If shipping new: drop into
    `clients/demo/assets/`; if aliasing: document the substitution in
    a comment.
  - Add `GET /avatars/{login}.png` route to test-github and rewrite
    the seed `avatar_url` from `https://github.com/penguin.png` to
    the local URL `http://localhost:<port>/avatars/penguin.png`
    (resolve port from runtime state; pattern matches forgejo).
  - Integration test asserting the URL the API returns is reachable
    from the same process (loopback `reqwest::get` in test).
- [ ] **A.5 — test-lemmy pict-rs-style route**
  - Add `GET /pictrs/image/{filename}` handler to test-lemmy
    (Lemmy's real upload service serves images at this path).
  - Match `beaver.png`, `hedgehog.png`, `axolotl.png` to embedded
    bytes (use existing PNG when present, otherwise convert SVG to
    PNG at build time via a build.rs OR ship a hand-converted PNG).
  - Update `state.rs:140,152,164` to set `avatar:
    Some("http://localhost:<port>/pictrs/image/beaver.png")` etc.
    Use a runtime-resolved base URL — the Stoat fixture pattern at
    `routes.rs:223` reads the host header; copy that.
  - Integration test that a `GET /api/v3/user?username=beaver`
    response carries an `avatar` field that is fetchable on the same
    server.
- [ ] **A.6 — Shared helper (optional refactor)**
  - If A.1-A.5 produce >3 near-identical `serve_avatar` functions,
    extract `servers/test-common/src/avatars.rs` with
    `pub fn serve_animal_png(name: &str) -> Result<&'static [u8], …>`
    that maps animal names to the bundled bytes. Each backend's route
    becomes a 5-line wrapper.
  - Document the helper in `servers/test-common/README.md`.

## Phase B — Lemmy preview-image data flow

Effort: ~4-6h. Owner: 1 sonnet agent.
Depends on: Phase A only insofar as it touches test-lemmy (B.2 needs
test-lemmy seed posts to set `thumbnail_url`).
Acceptance: a Lemmy post seeded with `thumbnail_url` renders a 64x64
thumbnail in the forum list when the per-account toggle is on; absent or
toggle-off, no thumbnail.

- [ ] **B.1 — Wire `thumbnail_url` through the Lemmy client**
  - Add `pub thumbnail_url: Option<String>` to `LemmyPost` in
    `clients/lemmy/src/api.rs:144-153`.
  - Verify against real Lemmy spec by fetching
    `https://lemmy.world/api/v3/post/list?limit=1` once during impl
    and confirming `thumbnail_url` appears in the response (cite the
    response in the commit message).
  - Add `pub preview_image_url: Option<String>` to the in-app `Message`
    struct in `clients/client/src/types.rs:1130-1153` (gated as a new
    optional field with `#[serde(default)]` to keep forward
    compatibility on persisted state).
  - Update `clients/lemmy/src/api.rs:map_post_to_message` to populate
    `preview_image_url: post.thumbnail_url.clone()`.
  - Unit test: feed a JSON fixture with `thumbnail_url: "https://x/y.png"`
    into `serde_json::from_str::<PostListResponse>` and assert the
    `preview_image_url` propagates onto the mapped `Message`.
- [ ] **B.2 — Surface `thumbnail_url` from test-lemmy seed**
  - Add `pub thumbnail_url: Option<String>` to test-lemmy's `Post`
    struct (`servers/test-lemmy/src/state.rs:52`).
  - Set 1-2 seed posts to a `thumbnail_url` pointing at a bundled
    image (reuse the pict-rs route from A.5 — e.g. point at a small
    `clients/demo/assets/koala.png` so the dev demo shows preview-rendering).
  - Update routes.rs JSON serialisation to emit `thumbnail_url` on
    the post payload.
- [ ] **B.3 — WIT extension (only if cross-WIT carry needed)**
  - The `Message` change above is host-side Rust only. If the
    forum-post WIT record needs the field too (i.e. for the
    `create-forum-post` return path), add `preview-image-url:
    option<string>` to `forum-post` in `wit/messenger-plugin.wit:219`
    AND to the corresponding host bindings.
  - Skip this sub-step if the forum-post UI consumes the `Message`
    type directly (verify at impl time by tracing the flow from
    `get_forum_posts` to render).
- [ ] **B.4 — Per-Lemmy-account "Render previews" setting**
  - Extend the lemmy section of `Settings` (location TBD —
    `crates/host-bridge/src/client_config.rs` Phase C plan or
    `clients/lemmy/src/lib.rs`) with `render_previews: bool` defaulting
    to `true`.
  - Wire through `ClientConfigStore::get/set` so the value persists
    across restarts.
  - Add a toggle to the Lemmy account-settings UI page (find via
    `grep -rn "lemmy.*settings\|account.*lemmy" crates/core/src/ui/`).
- [ ] **B.5 — UI render**
  - In the forum list rendering (`crates/core/src/ui/account/common/forum_view.rs`,
    likely the post-row body around the existing thumbnail-emoji
    placeholder), check `preview_image_url.is_some() && setting.render_previews`
    and render an `<img class="forum-post-preview" src="…" />` 64x64
    next to the title.
  - CSS: `.forum-post-preview { width: 64px; height: 64px;
    border-radius: 4px; object-fit: cover; flex-shrink: 0; }`.
    Add a `@media (max-width: 640px) { .forum-post-preview { display: none; } }`
    so mobile hides it (per the user's "Hide on mobile?" question —
    default YES, can be revisited).
  - Snapshot test in `crates/core` if forum-list snapshots exist
    (search `forum_view::tests`); else add a small one.

## Phase C — Forum composer overhaul (unified component)

Effort: ~12-16h. Owner: 1 sonnet agent (worktree); large component,
single owner to avoid merge churn.
Depends on: Phase B (only if `preview_image_url` is part of the
create-post payload — most Lemmy posts get their thumbnail
server-derived from the `url` field, so no hard dep).
Acceptance: the same `<ForumComposer />` component is used to create a
new Lemmy post AND to reply to an existing comment; submit fires the
backend's `create_forum_post` (or new `create_comment`) WIT method;
markdown preview tab toggles between source and rendered HTML; draft
autosave persists across page reloads.

- [ ] **C.1 — Define unified `ForumComposer` component contract**
  - New file: `crates/core/src/ui/account/common/forum_composer.rs`.
  - Props: `mode: ComposerMode` enum with variants `NewPost { channel_id }`,
    `ReplyToPost { post_id, channel_id }`, `ReplyToComment { parent_comment_id, post_id, channel_id }`.
  - Props: `backend: BackendKind` so the component knows whether to
    call `create_forum_post` (new top-level), `reply_to_message`
    (chat-style), or a backend-specific reply API.
  - Public sub-components: `<ComposerHeader />`, `<ComposerEditor />`
    (full-height textarea + markdown preview tab), `<ComposerActions />`
    (Cancel + Submit + draft-status indicator).
  - Document in module rustdoc the SOLID-SRP split: editor knows about
    text + preview, actions know about submit lifecycle, header knows
    about title/tags, the wrapper knows about backend dispatch.
- [ ] **C.2 — Wire the existing `CreateForumPostPage` to the new component**
  - Replace the body of `crates/core/src/ui/create_forum_post.rs:74-132`
    with `<ForumComposer mode={ComposerMode::NewPost { channel_id }} … />`.
  - Preserve all routes that point at the existing page; the page
    becomes a thin route wrapper.
- [ ] **C.3 — Add inline reply composer to `ForumComment`**
  - In `crates/core/src/ui/account/common/forum_view.rs:444-491`, add
    a "Reply" button per comment (next to `[+]/[-]` collapse).
  - Click expands a `<ForumComposer mode={ComposerMode::ReplyToComment …} />`
    inline below the comment body.
  - Submit → calls backend → optimistically inserts the new comment
    into `thread_comments` Signal so the UI updates without a full
    refetch.
- [ ] **C.4 — Markdown preview tab**
  - Add a tab toggle inside `<ComposerEditor />` between "Write" and
    "Preview".
  - Use existing markdown rendering helper (search
    `crates/core/src/ui/` for `pulldown_cmark` or `markdown::to_html`;
    reuse whatever the chat composer uses).
  - The textarea grows to fill the card (CSS `flex: 1; min-height: 200px`).
- [ ] **C.5 — Draft autosave**
  - On every input change, debounce-save the draft into
    `localStorage` keyed by `forum-draft:<backend>:<channel_id>:<mode>`.
  - On mount, restore the draft if present.
  - Discard on successful submit.
- [ ] **C.6 — Attachment drag-and-drop (stretch)**
  - Wire `ondragover` + `ondrop` on the editor to accept image files.
  - Upload via the backend's existing attachment-upload path (find
    via `clients/*/src/lib.rs` `upload_attachment` if present).
  - If the backend doesn't support uploads, fall back to "Drop URL
    here" link insertion.
- [ ] **C.7 — Wire `create_forum_post` for Lemmy**
  - The WIT method exists; verify `clients/lemmy/src/lib.rs`
    implementation. If it returns `not-supported`, add a real
    implementation that POSTs to `/api/v3/post`.
  - End-to-end: from the new ForumComposer Submit, the post lands in
    test-lemmy and shows up on next list fetch.
- [ ] **C.8 — Add `create_comment` WIT method (if missing)**
  - Search `wit/messenger-plugin.wit` for any reply/comment creation
    method. If absent, add `create-comment(channel-id, parent-id, body)
    -> result<message, client-error>`.
  - Implement for Lemmy first (POST to `/api/v3/comment`); other
    backends return `NotSupported` initially.

## Phase D — Tests

Effort: ~3-4h. Owner: 1 haiku agent (per CLAUDE.md test-harness rule).
Depends on: A, B, C all merged.
Acceptance: `cargo test -p poly-test-{discord,matrix,forgejo,github,teams,lemmy,stoat}`
all green; one Playwright spec drives the new ForumComposer end-to-end
on test-lemmy.

- [ ] **D.1 — Per-backend avatar serving integration tests**
  - For each backend touched in Phase A, add a
    `tests/avatar_serving.rs` that boots the server with `seed()`
    called, fetches the seed user's avatar URL, asserts 200 + nonzero
    body + correct content-type.
- [ ] **D.2 — Lemmy `thumbnail_url` mapping unit test**
  - In `clients/lemmy/src/api.rs::tests`, add a JSON-fixture test
    that proves `thumbnail_url` propagates to `Message.preview_image_url`.
- [ ] **D.3 — ForumComposer Playwright e2e**
  - Spec: drive the test-lemmy backend, click "New Post" in a
    forum channel, type title + body, click Submit, assert the post
    appears in the list with correct title and (if seeded with a
    URL) a preview thumbnail.
  - Spec lives at `apps/web/playwright/forum-composer.spec.ts` (or
    nearest existing forum spec dir).
- [ ] **D.4 — Inline reply Playwright e2e**
  - Spec: open an existing forum post, click "Reply" on the first
    comment, type a reply, submit, assert the reply nests under the
    parent.

## Phase E — Documentation

Effort: ~2h. Owner: 1 sonnet or haiku agent.
Depends on: D green.
Acceptance: docs land in the repo, no broken links, screenshots
captured on real test backends.

- [ ] **E.1 — Update CLAUDE.md "MCP Workflow" section**
  - Add a one-paragraph note about the new `/avatars/...` and
    `/pictrs/image/...` and `/v1.0/users/.../photo/$value` patterns
    so future agents know which backend uses which convention.
- [ ] **E.2 — Capture screenshots**
  - Drive test-lemmy via Playwright → take screenshots of: forum
    list with previews on, forum list with previews off (toggle),
    new-post composer with markdown preview tab active, inline reply
    composer expanded under a comment.
  - Save under `docs/screenshots/forum-composer/` and reference from
    this plan + from any user-facing release note.
- [ ] **E.3 — CLI recipe in `docs/dev/test-backends.md`**
  - If that file exists, add a per-backend `curl` one-liner for
    fetching an avatar so devs can sanity-check from the shell.
    Otherwise create the file with the recipe + a one-liner per
    backend port.

## Acceptance summary

- Per-backend table in this plan covers 8 / 8 backends with explicit
  status (DONE | EXTEND | ADD | NONE-by-design).
- Lemmy `thumbnail_url` field name verified against
  lemmy-api-common's `Post` schema (cite the JSON response captured
  during Phase B.1 impl in the commit message).
- New unified component path: `crates/core/src/ui/account/common/forum_composer.rs`.
- All five phases have ≥2 sub-step checkboxes.
- No section-sign characters anywhere in this file (per CLAUDE.md
  feedback_no_section_sign rule).

## Open questions

- Phase A.4 (test-github): commission new `penguin.png` /
  `chameleon.png` assets, or alias to existing animals? Recommend
  commissioning to grow the demo asset set, but it adds an
  illustration task outside the agent's scope.
- Phase B.4: does the `Settings::lemmy` per-account section already
  exist in `ClientConfigStore`? If not, need to define the schema
  alongside `render_previews`. Check during impl.
- Phase C.6 (drag-and-drop attachments): only Lemmy + Forgejo +
  GitHub really support inline images; Discord/HN/Matrix have other
  flows. Worth scoping down to "Lemmy only" for the first cut.
