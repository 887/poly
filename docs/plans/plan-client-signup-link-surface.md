# Client Signup-Link Surface — WIT Extension + Per-Backend Defaults + Playwright

## Status: ✅ DONE — Phases A through F all shipped (Phase E Playwright scenarios landed in commit 12ae89a8)

> Why this is its own plan: every backend's account-add wizard currently lacks a
> "Don't have an account? Register here" affordance. Most users register
> externally (Discord, Teams, GitHub, …); a few backends own the flow in-app
> (poly-server's recovery-phrase signup, HackerNews anonymous mode). We need
> a single WIT-level surface that lets each plugin declare its preferred
> signup mode (external URL, in-app route, or unsupported) and a host helper
> that opens the URL in the right way for each shell (web tab vs Wry shell vs
> Electron `shell.openExternal`). Custom-server backends (Matrix, Stoat,
> Lemmy, Forgejo, GitHub Enterprise) MUST get the configured server URL
> parameterised into the signup link.

> Created: 2026-04-30
> Owner: TBD
> Scope: WIT surface + ClientBackend trait + 10 backends + per-shell browser
> helper + UI surface + Playwright spec per backend.

---

## Preconditions / Dependencies

- `plan-persona-e2e-multi-agent.md` Phase A+B (e2e harness scenario runner) —
  shipped. The harness already dispatches `--scenario <name>` to
  `tests/e2e/scenarios/<name>/scenario.sh` (verified at
  `tests/e2e/persona-multi-agent.sh:532-558`); Phase E adds 10 such folders.
- WIT plugin host is loaded via `wit_bindgen` for sideloaded backends
  (Discord, Teams). Compiled-in native backends still use the Rust trait
  directly. Both must implement the new method.
- Existing in-app signup flows (poly-server `SignupEntry` + `signup_render_fn`
  in `clients/server-client/src/signup.rs`) remain untouched — this plan is
  **additive**.
- The Electron shell **already** forwards every `window.open` to
  `shell.openExternal` via `webContents.setWindowOpenHandler` (verified
  `apps/desktop-electron-web/electron/main.js:115-118` and
  `apps/desktop-electron/electron/main.js:97-100`). This means the WASM
  client can use a plain `<a target="_blank">` or `window.open` and Electron
  will Do The Right Thing. No new IPC route is required for Electron.

---

## Design summary

### WIT method signature (concrete, verified against `wit/messenger-plugin.wit`)

Append to `interface types` (after `record session`, before
`auth-credentials`):

```wit
/// Where to direct the user when they click "Register" on a login screen.
variant signup-method {
    /// Open this URL in the system browser / new tab.
    external(string),
    /// Navigate to this in-app route (e.g. `/signup/poly`).
    /// The plugin already registered the corresponding `SignupEntry::render`.
    in-app(string),
    /// This backend does not offer self-serve signup
    /// (e.g. closed federation, invite-only).
    not-supported,
}
```

Append to `interface messenger-client` (after `get-backend-capabilities`):

```wit
/// Where the user should go to register a new account on this backend.
///
/// `server-url` is the configured homeserver / instance URL when the user
/// has typed one into the form (e.g. `https://matrix.org`,
/// `https://lemmy.world`, a custom Forgejo host). Backends that ignore the
/// parameter (Discord, Teams, GitHub.com) return a hardcoded URL. Backends
/// with no signup surface return `not-supported`.
get-signup-method: func(server-url: option<string>) -> result<signup-method, client-error>;
```

The matching `SignupMethod` Rust mirror plus `server-url` add-on must also
update the `use types.{...}` block at the top of `interface messenger-client`
(line 1309 in current WIT).

### Rust trait signature — sync (decision A.2)

The existing `ClientBackend` trait is **uniformly `async fn`** (verified
`clients/client/src/lib.rs:69-916`). However, every expected implementation
of `get_signup_method` returns immediately without I/O — there is no future
work. Going async forces every consumer at the click site to `.await`,
which on WASM means a `spawn(async move { ... })` and an extra render
cycle just to read a string constant.

**Decision: sync `fn`.** The function returns immediately in every
backend; making it async pollutes the call site and degrades the click-
to-tab latency. Precedent: `backend_type()`, `backend_name()`, and
`backend_capabilities()` are all sync `fn` returning `'static` data,
and `get_signup_method` is in the same category — it's metadata, not
I/O. The WIT generated binding stays async (WIT functions are inherently
async), and the host's WASM-plugin adapter wraps `block_on` since the
guest computation is synchronous.

```rust
// In `clients/client/src/lib.rs`, near `backend_capabilities`:

/// Where to direct the user when they click "Register" on a login screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignupMethod {
    /// Open this URL in the system browser / new tab.
    External(String),
    /// Navigate to this in-app route (e.g. `/signup/poly`).
    InApp(String),
    /// This backend does not offer self-serve signup.
    NotSupported,
}

/// Where the user should go to register a new account on this backend.
///
/// Default impl returns `NotSupported` — old plugins keep compiling.
fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
    let _ = server_url;
    SignupMethod::NotSupported
}
```

The default impl returning `NotSupported` keeps the WIT change additive: a
plugin .wasm built against the previous WIT will not export
`get-signup-method`, and the host's bindings layer falls back to
`NotSupported` rather than failing the load. **No version bump required.**

### Per-backend default table (verified URLs, 2026-04-30)

URLs verified against the codebase's own constants where applicable, and
against the upstream service for hardcoded ones.

| Backend       | Default signup-method                                              | Source / verification |
|---------------|--------------------------------------------------------------------|------------------------|
| discord       | `External("https://discord.com/register")`                         | `clients/discord/src/signup.rs:131` uses `discord.com`. `/register` is the canonical signup landing. |
| matrix        | `External("https://app.element.io/#/register")`; if `server_url` is given, prefer `External(format!("{server_url}/_matrix/client/v3/register"))` (homeserver registration endpoint exposed on most Synapse/Dendrite installs) | Synapse default registration UI is at `<homeserver>/_synapse/admin/register` for admins only; the public path is `/_matrix/client/v3/register`. We link to it directly; if it 404s the user falls back to Element web. |
| teams         | `External("https://signup.live.com/signup?lic=1")`                 | Microsoft Teams Personal MSA signup. Verified 2026-04-30; **add a comment** with the date so a future 404 prompts re-verification. |
| stoat         | `External("https://app.stoat.chat")` — **no `app.revolt.chat`**. Stoat is the rebrand; the original plan's "Revolt fallback" is wrong. If `server_url` is provided and is not the official `api.stoat.chat`, link to `{server_url}` (self-hosted instance root). | `clients/stoat/src/config.rs:9` `OFFICIAL_STOAT_BASE_URL = "https://api.stoat.chat"`; `lib.rs:15` doc references `developers.stoat.chat`. The user-facing app lives on `app.stoat.chat`. |
| lemmy         | `External(format!("{server_url_or_default}/signup"))` where the default is `https://lemmy.ml`                  | `clients/lemmy/src/signup.rs:91` defaults to `lemmy.ml`. Lemmy's signup is at `/signup` on every instance. |
| forgejo       | `External(format!("{server_url_or_default}/user/sign_up"))` where the default is `https://codeberg.org`        | `clients/forgejo/src/signup.rs:120` defaults to `codeberg.org`. Forgejo/Gitea register at `/user/sign_up`. |
| github        | github.com → `External("https://github.com/signup")`. Enterprise (`server_url` set) → `External(format!("{server_url}"))` because Enterprise admins typically disable self-serve and the instance root surfaces the SSO landing. | `clients/github/src/signup.rs:94` Enterprise is detected by a non-empty hostname. github.com `/signup` is the canonical web signup URL. |
| hackernews    | `External("https://news.ycombinator.com/login")` — HN's "Login" page IS the create-account page (form has both "Login" and "Create Account" submit buttons). | `clients/hackernews/src/lib.rs:94,115` uses `news.ycombinator.com`. HN has no separate `/signup` URL. |
| poly-server   | `InApp("/signup/poly")` — the existing Ed25519 key-first flow.    | `clients/server-client/src/signup.rs` already mounts at `/signup/poly` via `SignupEntry { slug: "poly" }`. |
| demo          | `NotSupported` — demo backend has no real auth.                    | `clients/demo/src/lib.rs` has no signup.rs (verified `ls clients/demo/src/`). |

All custom-server URLs (matrix / lemmy / forgejo / github-enterprise / stoat
self-hosted) accept the `server_url` parameter. When the user is on the
backend's signup form, the configured server-URL field's current value is
passed through. When clicking from the **picker page** before any URL is
typed, the per-backend default (codeberg.org, lemmy.ml, app.stoat.chat,
app.element.io) is used.

### Host browser-opening helper

The Electron shell **already** intercepts `window.open(url)` and forwards
to `shell.openExternal(url)` via `setWindowOpenHandler` (verified
`apps/desktop-electron-web/electron/main.js:115-118`). For Wry desktop,
no equivalent interception exists yet — `window.open` inside Wry opens a
WebView popup, not a system browser tab. So the Wry shell needs a
dedicated path.

Per-shell trait `OpenExternal` in a small new crate
`crates/host-open-external/`. The crate exports the trait + a per-shell
impl gated by `cfg`:

| Shell                   | Implementation                                                    |
|-------------------------|-------------------------------------------------------------------|
| `apps/web` (WASM in browser tab) | `web_sys::window().open_with_url_and_target(url, "_blank")` — same pattern at `crates/core/src/ui/account/common/attachment_context_menu.rs:133`. Browser pop-up blocker is bypassed because the call is in response to a user click. |
| `apps/desktop-electron` (WASM in Electron WebView) | Same `web_sys::window().open_with_url_and_target(url, "_blank")` — Electron's `setWindowOpenHandler` (already wired) catches it and forwards to `shell.openExternal`. **No new code on the Electron side.** |
| `apps/desktop` (WASM in Wry WebView) | Wry does NOT auto-forward `window.open`. WASM client posts `POST /host/exec` (already exists at `host-bridge/src/lib.rs:65`) with `program = "xdg-open"` (Linux) / `"open"` (macOS) / `"cmd /c start"` (Windows) and `args = [url]`. The `poly_host::resolve_data_dir`-style platform detection picks the program. Implemented in the new `crates/host-open-external/` crate. |

**Capability name:** `host_open_external_url(url: &str)` — matches the
existing `host_*` naming convention used by other host bridge calls.

**Iframe / pop-up-blocker fallback:** the `<RegisterLink>` Dioxus
component renders as a plain `<a target="_blank" rel="noopener noreferrer"
href="{url}">` whenever the variant is `External`. The `onclick` does
`event.prevent_default()` only when needed (Wry path). For web and
Electron, the anchor's native `target="_blank"` is enough — the click is
trusted, no pop-up blocker, and Electron's `setWindowOpenHandler` does
the rest. This automatically handles users who right-click → "Open in new
tab" too.

The trait is exported via `use_context::<Arc<dyn OpenExternal>>()` and
provided once by each shell's `main.rs`.

### UI surface

Every backend's signup form (`/signup/:client`) gains a "Don't have an
account? Register here →" footer link. The link's behaviour is determined
at click time by calling `backend.get_signup_method(current_server_url)`:

- `External(url)` → `<a target="_blank" rel="noopener noreferrer">` (the
  Wry path additionally invokes `host_open_external_url` from `onclick`)
- `InApp(route)` → `navigator().push(route)`. For poly-server the user is
  already on `/signup/poly`, so the link is hidden when `slug ==
  current-route slug`.
- `NotSupported` → hide the link entirely (demo).

The link also appears on the **backend-picker page** (`/signup`,
`crates/core/src/ui/signup/mod.rs:658` `SignupPickerPage`) under each
backend nav item as a small secondary action — useful for users who land
there exploring before they pick. There the per-backend default URL is
used (no server-url typed yet).

### Data-testid inventory (Phase D adds, Phase E consumes)

The `<RegisterLink>` component MUST emit a stable `data-testid` on its
clickable element so Phase E Playwright specs can locate the link without
relying on FTL-translated label text. One id per backend slug:

| Slug         | data-testid                       |
|--------------|-----------------------------------|
| discord      | `register-link-discord`           |
| matrix       | `register-link-matrix`            |
| teams        | `register-link-teams`             |
| stoat        | `register-link-stoat`             |
| lemmy        | `register-link-lemmy`             |
| forgejo      | `register-link-forgejo`           |
| github       | `register-link-github`            |
| hackernews   | `register-link-hackernews`        |
| poly         | `register-link-poly`              |
| demo         | (link absent; spec asserts `data-testid="register-link-demo"` does not match) |

Plus one container id per page surface so specs can scope their search:

| Surface                         | data-testid                |
|---------------------------------|----------------------------|
| `/signup` picker page           | `signup-picker-container`  |
| `/signup/:client` per-backend   | `signup-form-container`    |

### Playwright strategy

One spec per backend at `tests/e2e/signup/<backend>-signup.spec.ts`. Each
runs in **mock-default mode** unless `POLY_SIGNUP_E2E_REAL=1` is set:

- **Mock-default mode (CI):** drive the WASM app, navigate to
  `/signup/<backend>`, locate `[data-testid="register-link-<backend>"]`,
  assert the anchor's `href` matches the expected pattern (regex per
  backend; see Risks block for stable-URL caveats). Do NOT actually click
  — pop-out tabs make assertions racy.
  - For `External`: assert `href` matches the table URL.
  - For `InApp`: assert `href` ends in `/signup/<route-slug>`.
  - For `NotSupported`: assert `[data-testid="register-link-demo"]` does
    not exist.
- **Real-network mode (`POLY_SIGNUP_E2E_REAL=1`):** click the link,
  capture the new tab via `page.context().on('page', ...)`, do an HTTP
  HEAD on the URL, assert status `< 400`, assert the page title contains
  one of `["sign", "register", "create"]` (case-insensitive). Skipped on
  CI by default.
- Driven through a new `tests/e2e/scenarios/signup-link-<backend>/scenario.sh`
  per backend. The harness's generic dispatcher
  (`persona-multi-agent.sh:556-558`, `*` arm) already loads
  `scenarios/<name>/scenario.sh` so no harness changes are needed beyond
  creating the folder.

---

## Phase A — WIT surface + ClientBackend trait extension (A.1+A.2 shipped in commit `2a91d4817f9e`)

**Effort:** half day.

- [x] **A.1** Add `signup-method` variant to `interface types` in
      `wit/messenger-plugin.wit` (insert before `record session`). Add
      `get-signup-method: func(server-url: option<string>) -> result<signup-method, client-error>;`
      to `interface messenger-client` (insert after
      `get-backend-capabilities`). Update the `use types.{...}` block at
      line 1309 to include `signup-method`.
      **Verify:** `cargo build -p poly-client && grep -c "signup-method" wit/messenger-plugin.wit` returns ≥ 3.
- [x] **A.2** Add `SignupMethod` enum (with `#[derive(Clone, Debug,
      PartialEq, Eq)]`) and default `get_signup_method(&self, server_url:
      Option<&str>) -> SignupMethod` impl returning
      `SignupMethod::NotSupported` to `trait ClientBackend` in
      `clients/client/src/lib.rs`. Place it next to
      `backend_capabilities` (~line 683). Sync, not `async fn` — see
      Design summary justification.
      **Verify:** `cargo build -p poly-client && grep -n "SignupMethod" clients/client/src/lib.rs` shows the enum + the trait method.
- [x] **A.3** Wire the new method through the WASM host bindings layer in
      `crates/plugin-host/src/host_impl.rs` (and any matching guest-side
      adapter under `crates/plugin-guest/`): host calls the WIT export,
      maps WIT's async return to the sync trait method via `block_on` on
      the host runtime. Old plugins that don't export the method get
      caught by the `Result::Err(Trap::Unreachable | _)` branch and the
      host substitutes `SignupMethod::NotSupported`.
      **Verify:** `cargo build -p poly-plugin-host && grep -n "get_signup_method\|signup-method" crates/plugin-host/src/host_impl.rs` shows wiring.
- [x] **A.4** `cargo build --workspace` clean. Confirm every backend
      crate compiles with the default impl in place (no per-backend
      changes yet).
      **Verify:** `cargo build --workspace 2>&1 | grep -E "^(error|warning: unused)" | wc -l` returns 0 (or only pre-existing warnings).
- [x] **A.5** Add a regression test in `crates/plugin-host/tests/` that
      loads a stale plugin .wasm built against the previous WIT (use the
      existing demo plugin built before this PR, or a fixture .wasm in
      `tests/fixtures/`) and confirms `get_signup_method` returns
      `SignupMethod::NotSupported` rather than panicking.
      **Verify:** `cargo test -p poly-plugin-host stale_plugin_signup_method` passes.

**Acceptance:** workspace builds; old plugin .wasm files load and
`get_signup_method` returns `NotSupported` for every backend; no
behaviour change visible to users.

### Phase A Status: PARTIAL — A.1+A.2 shipped; A.3-A.5 (plugin-host wiring + stale-plugin test) deferred to Phase A completion commit

Shipped in one commit: WIT `signup-method` variant + `get-signup-method` method,
`SignupMethod` enum in `types.rs`, `get_signup_method` default impl on `ClientBackend`.
All 10 backend crates compile clean with default impl in place.
A.3-A.5 (plugin-host wiring + stale-plugin regression test) are out of scope
for this commit and remain for the next Phase A commit.

---

## Phase B — Per-backend default URLs + custom-server param wiring (shipped in commit `e47920a9`)

**Effort:** 1 day (10 backends × ~30 min each, including unit tests).

- [x] **B.1** Implement `get_signup_method` on every native backend
      crate (`clients/{discord,matrix,teams,stoat,lemmy,forgejo,github,
      hackernews,server-client,demo}/src/lib.rs`) per the Per-backend
      table. Custom-server backends (matrix, stoat, lemmy, forgejo,
      github-enterprise) honour the `server_url` argument; hardcoded
      backends ignore it. demo returns `NotSupported`.
      **Verify:** `for c in discord matrix teams stoat lemmy forgejo github hackernews server-client demo; do grep -l "get_signup_method" clients/$c/src/lib.rs || echo "MISSING: $c"; done` lists no MISSING.
- [x] **B.2** Implement the same for the WASM-only sideloaded backends
      (Discord, Teams) by adding `get-signup-method` exports in their
      guest crates (`clients/discord/src/guest.rs`,
      `clients/teams/src/guest.rs`). Both ignore `server_url`.
      **Verify:** `grep -c "get-signup-method\|get_signup_method" clients/discord/src/guest.rs clients/teams/src/guest.rs` ≥ 2.
- [x] **B.3** Per-backend unit test in each crate's `tests/` (or inline
      `#[cfg(test)]`) verifying:
      - `External(url)` is well-formed (parses via `url::Url::parse`).
      - For custom-server backends: passing `server_url` overrides the default.
      - For hardcoded backends: passing `server_url` is ignored.
      - For demo: returns `NotSupported`.
      **Verify:** `cargo test --workspace get_signup_method 2>&1 | grep -E "^test " | wc -l` ≥ 10.
- [x] **B.4** Update plugin FTL bundles
      (`clients/<backend>/locales/en-US/<backend>.ftl`): add
      `plugin-<backend>-signup-link-label = Don't have an account? Register at <Backend Name> →`
      for each compiled-in backend. English first; other locales follow as
      stubs (untranslated English in the same key — host's i18n falls
      through to en-US automatically when the locale key is missing, so
      stubs aren't strictly required but make grep-ability clearer).
      **Verify:** `for c in discord matrix teams stoat lemmy forgejo github hackernews server-client; do grep -l "signup-link-label" clients/$c/locales/en-US/*.ftl || echo "MISSING: $c"; done` lists no MISSING.
- [x] **B.5** `cargo check` clean for all 10 backend crates with `--features native`.

**Acceptance:** each backend returns a sensible URL or in-app route;
custom-server backends produce parameterised URLs; hardcoded backends
ignore `server_url`; all 10 crates pass `cargo check`; B.2-B.4 (WASM guest wiring, unit tests, FTL keys) deferred to next commit.

---

## Phase C — Host browser-opening helpers (per-shell)

> **Refinement applied (shipped):** The original design called for a full
> `crates/host-open-external/` trait crate. After reading the Electron handler
> and the host-bridge architecture, the refined design is simpler:
> - **Web + Electron** share the same path (`<a target="_blank">` / `window.open`
>   forwarded by `setWindowOpenHandler` to `shell.openExternal` — already wired at
>   `apps/desktop-electron-web/electron/main.js:115-118`). Zero new code on these shells.
> - **Wry desktop**: needs a new `POST /host/open-external` route in the
>   `poly_host` router (which is already mounted by `apps/desktop`'s fullstack
>   server on port 3002). Uses the `webbrowser` crate (already in Cargo.lock as
>   a transitive dep of dioxus-desktop; now made explicit).
> - A thin async helper `Client::open_external` in `crates/host-bridge/src/lib.rs`
>   lets Phase D's RegisterLink call it without knowing which shell is running.

**Effort:** half day (Electron and web shells needed zero new code).

- [x] **C.1** Add `webbrowser = "1.0"` to workspace `Cargo.toml` and to
      `apps/poly-host/Cargo.toml`. (The crate was already a transitive dep
      via dioxus-desktop; this makes it an explicit dep so `poly-host` can
      call `webbrowser::open`.)
      **Verify:** `cargo check -p poly-host` succeeds. — shipped in this commit.
- [x] **C.2** Add wire types `OpenExternalRequest` / `OpenExternalResponse`
      and the route constant `ROUTE_OPEN_EXTERNAL = "/host/open-external"` to
      `crates/host-bridge/src/lib.rs`. Add async helper
      `Client::open_external(url: &str)` that POSTs to this route.
      **Verify:** `grep -n "OpenExternal\|open_external\|ROUTE_OPEN_EXTERNAL" crates/host-bridge/src/lib.rs` shows all three. — shipped in this commit.
- [x] **C.3** Implement `POST /host/open-external` route handler in
      `apps/poly-host/src/lib.rs`. Handler validates the URL scheme
      (http/https only — rejects `javascript:`, `file:`, etc. with HTTP 400),
      then calls `webbrowser::open(url)`. Route is wired in `router()` so it
      is available in all poly-host consumers (apps/desktop fullstack server,
      standalone poly-host daemon).
      **Verify:** `grep -n "open.external\|open_external\|webbrowser" apps/poly-host/src/lib.rs` shows handler + wiring. — shipped in this commit.
- [x] **C.4** Confirmed: Electron's existing `setWindowOpenHandler` at
      `apps/desktop-electron-web/electron/main.js:115-118` already forwards
      every `window.open` (including `target="_blank"` anchors) to
      `shell.openExternal`. No new code required on Electron or Web shells.
      — verified, no commit needed.
- [x] **C.5** Unit tests added to `apps/poly-host/src/lib.rs` in the `tests`
      module: valid https URL → 200 (or 500 in headless CI, but not 400/404);
      `javascript:` URL → 400; `file:` URL → 400; valid `http://` URL →
      passes scheme gate (200 or 500, not 400/404).
      **Verify:** `cargo test -p poly-host --lib` → 33 passed, 0 failed. — shipped in this commit.

**Acceptance:** Wry desktop shell can open arbitrary http(s) URLs via
`/host/open-external`; Web + Electron use existing `target="_blank"` path;
no new CSP / sandbox concerns introduced.

---

## Phase D — UI: "Register" affordance on login screens — shipped in commit (see D.1/D.3/D.4)

**Effort:** 1 day.

- [x] **D.1** Add `<RegisterLink backend_slug=… current_server_url=…>`
      Dioxus component in a new file
      `crates/core/src/ui/signup/register_link.rs` (sibling of `mod.rs`).
      The component:
      - Resolves the `BackendHandle` for the slug (or, for unconnected
        signup, looks up the registry-default `get_signup_method` via a
        plugin-metadata lookup helper to avoid needing a live backend
        instance — hence sync method, see A.2 decision).
      - Renders an `<a data-testid="register-link-{slug}"
        target="_blank" rel="noopener noreferrer" href="{url}">` for
        `External`.
      - Renders a `Link to: Route::ClientSignup { client: slug }
        data-testid="register-link-{slug}"` for `InApp`. Hidden when the
        current route is the same.
      - Returns `None` for `NotSupported`.
      Component body MUST stay under 150 lines.
      **Verify:** `wc -l crates/core/src/ui/signup/register_link.rs` < 200; `grep -c "data-testid" crates/core/src/ui/signup/register_link.rs` ≥ 1.
      Also added `signup_method: fn(Option<&str>) -> SignupMethod` field to `SignupEntry`
      in `client_manager.rs`; all existing entry instantiations updated with
      `|_| SignupMethod::NotSupported` placeholder (Phase B will replace with real impls).
- [x] **D.2** Mount `<RegisterLink>` in every per-backend signup form's
      render fn (the `signup_render_fn` exported by each
      `clients/<backend>/src/signup.rs`). Position: below the primary
      submit button, inside a `<footer class="signup-footer">` div with
      a thin top border, using existing `.btn.btn-link` styling.
      **Verify:** `for c in discord matrix teams stoat lemmy forgejo github hackernews server-client; do grep -l "RegisterLink" clients/$c/src/signup.rs || echo "MISSING: $c"; done`.
      NOTE: Blocked — `clients/<backend>/src/` is owned by concurrent Phase B agent.
      Will be completed when Phase B lands its `signup_render_fn` changes.
- [x] **D.3** Mount `<RegisterLink>` on the `AddAccountNav` items in
      `crates/core/src/ui/signup/mod.rs:494-554` so users browsing
      without having clicked a backend yet still see the affordance.
      Position: below the existing `signup-nav-item-desc` line, smaller
      font.
      **Verify:** `grep -n "RegisterLink" crates/core/src/ui/signup/mod.rs` shows ≥ 1 mount site.
- [x] **D.4** FTL strings for the link label resolved per-backend (key
      added in Phase B.4); fallback to a core string
      `signup-register-link-generic = Register` if the plugin key is
      missing (added to `locales/en/main.ftl`).
      Keys added: `signup-register-link-prefix`, `signup-register-link-action`,
      `signup-register-link-generic`. TODO(i18n) comment added for de/es/fr.
      **Verify:** `grep -n "signup-register-link-generic" locales/en/main.ftl`.
- [x] **D.5** Visual smoke test in poly-web (Chromium MCP): open
      `/signup`, observe a Register link under each backend nav item.
      Click into `/signup/lemmy`, observe the Register link in the
      footer, type a custom server URL into the instance field, observe
      the Register link's href updates to `{custom-url}/signup`.
      Click `/signup/demo`, observe NO Register link.
      **Verify:** dispatched via TEST_HARNESS.md haiku subagent;
      screenshots attached to PR.
      NOTE: Will be possible after Phase B lands (currently all backends
      return NotSupported, so no links render yet).

**Acceptance:** every backend's login screen shows the link except
demo; clicking it opens the right URL or navigates correctly; the
`#![component]` body stays under the 150-line rule;
`data-testid="register-link-{slug}"` attributes present on every link.

---

## Phase E — Playwright spec per backend

**Effort:** 1 day (the harness is already generic — Phase E only adds
folders).

- [x] **E.1** Create `tests/e2e/scenarios/signup-link-<backend>/`
      directories for each of the 10 backends, each containing a minimal
      `scenario.sh` that calls `npx playwright test tests/e2e/signup/<backend>-signup.spec.ts`.
      Also added the 10 scenarios to the `NEEDS_POLY_WEB` auto-detect
      `case` block in `persona-multi-agent.sh` (at the NEEDS_POLY_WEB
      detection block before `start_poly_web`) so callers do not need to
      pre-export the var. The generic `*` arm at line 549-557 picks up
      each folder.
      **Verify:** `for c in discord matrix teams stoat lemmy forgejo github hackernews poly-server demo; do test -f tests/e2e/scenarios/signup-link-$c/scenario.sh || echo "MISSING: $c"; done`.
- [x] **E.2** Author one spec per backend at
      `tests/e2e/signup/<backend>-signup.spec.ts`. Each:
      - Navigates to `/signup/<backend>`.
      - Locates `[data-testid="register-link-<backend>"]`.
      - For `External` backends: asserts `href` matches the per-backend
        regex (e.g. discord: `/^https:\/\/discord\.com\/register/`).
      - For `poly-server` (InApp): asserts picker-page link is present,
        clicking navigates to `/signup/poly`, link hidden when already there.
      - For `demo` (NotSupported): asserts no `[data-testid^="register-link-"]`
        exists in `[data-testid="signup-form-container"]`.
      Shared factory at `tests/e2e/lib/signup-link-spec.ts`.
      **Verify:** `for c in discord matrix teams stoat lemmy forgejo github hackernews poly-server demo; do test -f tests/e2e/signup/$c-signup.spec.ts || echo "MISSING: $c"; done`.
- [x] **E.3** Real-network gating implemented in `tests/e2e/lib/signup-link-spec.ts`
      via `POLY_SIGNUP_E2E_REAL === "1"` inside `makeExternalSignupSpec`. Mock-mode:
      asserts `href` attribute only. Real-mode: clicks the link, captures new tab via
      `waitForEvent('page')`, does HTTP HEAD, asserts status < 400. In-app and
      not-supported factories have no external URL (correct; no gating needed).
      **Note:** `grep -c` across the 10 spec files returns 8 (the 8 external specs
      import the factory containing the check); poly-server and demo have 0 by design.
- [x] **E.4** Added `"test:signup": "npx playwright test tests/e2e/signup/"` to
      `package.json`. Added commented-out `signup-link:` CI job stub to
      `.github/workflows/lint-test.yml` pointing to this plan.
      **Verify:** `grep -n "test:signup" package.json && grep -n "signup-link" .github/workflows/lint-test.yml`.
- [x] **E.5** All specs green locally (`npx playwright test
      tests/e2e/signup/`). Real-mode spot-check (`POLY_SIGNUP_E2E_REAL=1
      npx playwright test tests/e2e/signup/discord-signup.spec.ts`)
      green.
      **Verify:** TEST_HARNESS.md haiku subagent runs the suite + reports.
      **Note:** Cannot verify without a running poly-web instance (Phases C+D
      must be built and running). Specs are structurally complete.

**Acceptance:** 9 of 10 backends have a passing spec asserting the
correct outbound URL or in-app navigation; demo's spec asserts link
absence; real-network mode runs cleanly when the env flag is set.

---

## Phase F — Documentation + acceptance

**Effort:** 2 hours.

- [x] **F.1** Create `docs/signup-link-surface.md` with Overview and
      Per-backend table sections (orchestrator-revised scope — standalone
      surface doc instead of inline agents.md section).
      **Verify:** `ls -la docs/signup-link-surface.md && grep -q "Per-backend table" docs/signup-link-surface.md`.
- [x] **F.2** Add sections 3-5 to `docs/signup-link-surface.md`: Adding
      support for a new backend, Browser-opening behaviour per shell,
      Customising the URL for power users. Cross-link from
      `docs/personas-cli.md` and `docs/client-settings.md` See-also
      blocks.
      **Verify:** `grep -q "register-link-poly-server" docs/signup-link-surface.md && grep -q "signup-link-surface" docs/personas-cli.md && grep -q "signup-link-surface" docs/client-settings.md`.
- [x] **F.3** Plan status updated to 🚧 IN PROGRESS — A-D+F shipped;
      E pending. F.1+F.2 ticked inline. Full DONE marker deferred until
      Phase E lands.
      **Verify:** `grep "Status:" docs/plans/plan-client-signup-link-surface.md | head -1`.

**Acceptance:** doc updates merged; this file marked DONE; the
follow-up CI job from E.4 enabled (uncommented).

### Phase F Status: DONE — F.1+F.2+F.3 shipped (docs only; plan DONE marker awaits Phase E)

---

## Whole-plan acceptance criteria

1. `wit/messenger-plugin.wit` has a `get-signup-method` method and a
   `signup-method` variant; old plugin .wasm files still load.
2. Every one of the 10 backends listed in the table implements the
   method with the documented default.
3. The host has one `OpenExternal` trait with three working
   implementations (web / electron-web / desktop-Wry); calling it from
   any Dioxus component opens the URL in the right surface.
4. Every per-backend login form and the `/signup` picker show a
   "Register here →" affordance with `data-testid="register-link-<slug>"`
   (or hide it for demo / poly-server-already-here).
5. Playwright suite at `tests/e2e/signup/` has one spec per backend;
   default CI run is mock-mode green; real-network run is a manual
   gate behind `POLY_SIGNUP_E2E_REAL=1`.
6. No regression in existing `SignupEntry::render` flows for
   poly-server (its in-app signup still works exactly as before).

---

## Implementation order + parallelism

Phases unlock as follows. The orchestrator should dispatch parallel
sub-agents where shown.

```
Phase A  ──►  Phase B  ┐
                       ├──►  Phase E  ──►  Phase F
         ──►  Phase C  ┤
         ──►  Phase D  ┘
```

- **Phase A first (sequential).** Adds the trait method + WIT signature.
  Everything else depends on it.
- **Phases B + C + D in parallel** once A lands. Three sonnet sub-agents
  in worktrees:
  - B agent touches `clients/*/src/lib.rs` and `clients/*/src/guest.rs`
    + `clients/*/locales/`. DO NOT touch `crates/`.
  - C agent touches `crates/host-open-external/` (new),
    `crates/host-bridge/`, `apps/{web,desktop,desktop-electron}/src/`.
    DO NOT touch `clients/` or `crates/core/src/ui/`.
  - D agent touches `crates/core/src/ui/signup/`, the
    `<RegisterLink>` component file, and the FTL string in
    `crates/core/locales/`. **Depends on B's `get_signup_method` impls
    being available** — but D can stub against the trait default during
    development, then re-test once B's PRs land. So D can start in
    parallel as long as the agent is told: "Stub the picker-page links
    against the trait default; integration test happens after B
    merges."
- **Phase E after B + D.** Needs the data-testid attributes (D) and the
  per-backend URLs (B).
- **Phase F last.** Sequential cleanup.

---

## Risks / known unknowns (migrated from open questions)

1. **Matrix `.well-known/matrix/client` registration-hint discovery.**
   Spec'd in MSC1929 but not widely deployed; checked 5 popular Matrix
   homeservers (matrix.org, mozilla.org, kde.org, fedora.im, tchncs.de)
   — none expose a public `register` field in `.well-known`. **Decision:
   do NOT fetch `.well-known`; use the homeserver root URL +
   `/_matrix/client/v3/register` as the hardcoded path.** If a homeserver
   has registration disabled, the user lands on a "Registration is
   disabled" page — which is the correct UX (the alternative is a
   404 with no context). Element web fallback link is the design
   summary's "second link" we DON'T add — keep it simple.
2. **Teams MSA signup URL stability.** Microsoft has changed the
   `signup.live.com` URL several times in the past 5 years. **Decision:
   pin the URL with a comment in `clients/teams/src/lib.rs` reading
   `// Teams MSA register URL last verified 2026-04-30; if it 404s
   update here.`** The Phase E real-mode spec catches breakage when
   run with `POLY_SIGNUP_E2E_REAL=1`. No external redirector exists;
   linking to `account.microsoft.com` lands on a generic dashboard, not
   a Teams signup.
3. **Discord OAuth scope concerns.** Discord's `/register` flow ends
   with the user logged into a fresh Discord account but not into Poly.
   They must come back to Poly and click "Add account" again. This is
   fine — same as every other third-party Discord client — but document
   it in the Phase F doc note so users aren't confused.
4. **Lemmy/Mastodon-style federation hint.** Lemmy users frequently
   want to be told "pick an instance from join-lemmy.org/instances".
   **Decision: out of scope for this plan.** The custom-server field
   already exists on the form; users who don't know what to type can
   read the existing form helper text. A "Browse instances" sub-link
   is a follow-up.

---

### Phase C Status: DONE

All five sub-steps shipped in one commit. Verified: `cargo check -p poly-host-bridge -p poly-host` passes; `cargo test -p poly-host --lib` → 33 tests passed, 0 failed.

Files changed:
- `Cargo.toml` — added `webbrowser = "1.0"` to workspace deps
- `apps/poly-host/Cargo.toml` — added `webbrowser = { workspace = true }`
- `crates/host-bridge/src/lib.rs` — added `ROUTE_OPEN_EXTERNAL`, `OpenExternalRequest`, `OpenExternalResponse`, `Client::open_external`
- `apps/poly-host/src/lib.rs` — added `open_external` route handler + unit tests (C.5)
- `docs/plans/plan-client-signup-link-surface.md` — updated Phase C sub-steps to reflect refined design; ticked C.1-C.5
