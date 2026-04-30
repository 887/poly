# Client Signup-Link Surface — WIT Extension + Per-Backend Defaults + Playwright

## Status: PLANNED — not started

> Why this is its own plan: every backend's account-add wizard currently lacks a
> "Don't have an account? Register here" affordance. Most users register
> externally (Discord, Teams, Lemmy, GitHub, …); a few backends own the flow
> in-app (poly-server's recovery-phrase signup). We need a single WIT-level
> surface that lets each plugin declare its preferred signup mode (external
> URL, in-app route, or unsupported) and a host helper that opens the URL in
> the right way for each shell (web tab vs Wry shell vs Electron
> `shell.openExternal`). Custom-server backends (Matrix, Stoat, Lemmy,
> Forgejo) MUST get the configured server URL parameterised into the signup
> link.

> Created: 2026-04-30
> Owner: TBD
> Scope: WIT surface + ClientBackend trait + 10 backends + per-shell browser
> helper + UI surface + Playwright spec per backend.

---

## Preconditions / Dependencies

- `plan-persona-e2e-multi-agent.md` Phase A+B (e2e harness scenario runner) —
  shipped, used in Phase E.
- WIT plugin host is loaded via `wit_bindgen` for sideloaded backends
  (Discord, Teams). Native compiled-in backends still use the Rust trait
  directly. Both must implement the new method.
- Existing in-app signup flows (poly-server `SignupEntry` + `signup_render_fn`)
  remain untouched — this plan is **additive**.

---

## Design summary

### WIT method signature (concrete)

Append to `interface messenger-client` in `wit/messenger-plugin.wit`:

```wit
/// Where the user should go to register a new account on this backend.
///
/// `server-url` is the configured homeserver / instance URL (e.g.
/// `https://matrix.org`, `https://lemmy.world`, a custom Forgejo host).
/// Backends that ignore the parameter (Discord, Teams, GitHub.com)
/// return a hardcoded URL. Backends with no signup surface return
/// `not-supported`.
///
/// Returning `external(url)` is the default — the host opens it in the
/// system browser (or a new tab on web). Returning `in-app(route)`
/// tells the host to navigate to a plugin-owned route inside Poly
/// (e.g. `/signup/poly`).
get-signup-method: func(server-url: option<string>) -> result<signup-method, client-error>;
```

And add to `interface types` (next to the existing auth records):

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

Mirror in `clients/client/src/lib.rs` on `trait ClientBackend`:

```rust
pub enum SignupMethod {
    External(String),
    InApp(String),
    NotSupported,
}

async fn get_signup_method(
    &self,
    server_url: Option<&str>,
) -> ClientResult<SignupMethod> {
    Ok(SignupMethod::NotSupported) // safe default — old plugins compile
}
```

The default impl returning `NotSupported` keeps the WIT change additive: a
plugin .wasm built against the previous WIT will not export
`get-signup-method`, and the host's bindings layer falls back to
`NotSupported` rather than failing the load. **No version bump required.**

### Per-backend default table

| Backend       | Default signup-method                                              |
|---------------|--------------------------------------------------------------------|
| discord       | `external("https://discord.com/register")`                         |
| matrix        | `external("{server-url}/_matrix/static/" or homeserver-specific)`, fallback `https://app.element.io/#/register` |
| teams         | `external("https://signup.live.com/signup?lic=1")` (MSA signup)    |
| stoat         | `external("{server-url}" or "https://app.revolt.chat")`            |
| lemmy         | `external("{server-url}/signup")`                                  |
| forgejo       | `external("{server-url}/user/sign_up")`                            |
| github        | `external("https://github.com/signup")` (Enterprise: `{server-url}` — admins disable self-serve, so link goes to instance root) |
| hackernews    | `external("https://news.ycombinator.com/login")` (signup is on the same page) |
| poly-server   | `in-app("/signup/poly")` — the existing Ed25519 key-first flow    |
| demo          | `not-supported` — demo backend has no real auth                    |

### Host browser-opening helper

Per-shell, behind one shared trait `OpenExternal` in a small new crate
`crates/host-open-external/`:

| Shell                   | Implementation                                                    |
|-------------------------|-------------------------------------------------------------------|
| `apps/web` (WASM)       | `web_sys::window().open_with_url_and_target(url, "_blank")` — same pattern already used in `attachment_context_menu.rs:133` |
| `apps/desktop` (Wry)    | `open::that(url)` (the `open` crate, native), gated behind `#[cfg(not(target_arch = "wasm32"))]` |
| `apps/desktop-electron` | New `/host/open-external` route on the fullstack server side that posts a message to the Electron main process, which calls `shell.openExternal(url)` (the call already exists at `electron/main.js:116`) |

The trait is exported via `use_context::<Arc<dyn OpenExternal>>()` and
provided once by each shell's `main.rs`.

### UI surface

Every backend's signup form (`/signup/:client`) gains a "Don't have an
account? Register here →" footer link. The link's behaviour is determined
at click time by calling `backend.get_signup_method(current_server_url)`:

- `External(url)` → call `OpenExternal::open(url)`
- `InApp(route)` → `navigator.push(route)` — but for poly-server the user
  is already on `/signup/poly`, so this is effectively a no-op; the link
  is hidden when `slug == current-route slug`.
- `NotSupported` → hide the link entirely (demo).

The link also appears on the **backend-picker page** (`/signup`) under
each card as a small secondary action — useful for users who land there
exploring before they pick.

### Playwright strategy

One spec per backend at `tests/e2e/signup/<backend>-signup.spec.ts`:

- Default mode (no env): drive the WASM app, navigate to
  `/signup/<backend>`, click the "Register here" link, assert one of:
  - For `External`: `page.context().on('page', …)` fires with a URL
    matching the expected pattern (parameterised for custom-server
    backends).
  - For `InApp`: the in-app signup screen mounts (assert by selector).
  - For `NotSupported`: the link is absent.
- Real-network mode (env-gated, e.g. `POLY_SIGNUP_E2E_REAL=1`): actually
  follow the URL and assert a meaningful page loads (title contains
  "Sign", form has email/username field). Skipped on CI by default.
- Driven through the existing `--scenario signup-link-<backend>` knob in
  `tests/e2e/persona-multi-agent.sh` (mirrors the pattern shipped in
  `plan-persona-e2e-multi-agent.md` Phase A+B).

---

## Phase A — WIT surface + ClientBackend trait extension

**Effort:** half day.

- [ ] **A.1** Add `signup-method` variant to `interface types` in
      `wit/messenger-plugin.wit`. Add `get-signup-method` to
      `interface messenger-client`.
- [ ] **A.2** Add `SignupMethod` enum + default `get_signup_method` impl
      to `trait ClientBackend` in `clients/client/src/lib.rs`. Default
      returns `NotSupported`.
- [ ] **A.3** Wire the new method through the WASM host bindings layer
      (whatever crate hosts `wit_bindgen`-generated impls — locate during
      implementation; update its host-side dispatch table).
- [ ] **A.4** `cargo build --workspace` clean. Confirm every backend
      crate compiles with the default impl in place (no per-backend
      changes yet).
- [ ] **A.5** Confirm a stale plugin .wasm built against the previous
      WIT still loads — host bindings must treat absent
      `get-signup-method` export as `NotSupported`. Add a regression
      test if the host has a plugin-load test harness.

**Acceptance:** workspace builds; old plugin .wasm files load and
`get_signup_method` returns `NotSupported` for every backend; no
behaviour change visible to users.

---

## Phase B — Per-backend default URLs + custom-server param wiring

**Effort:** 1 day (10 backends × ~30 min each, including unit tests).

- [ ] **B.1** Implement `get_signup_method` on every native backend
      crate per the table in the design summary. Custom-server
      backends (matrix, stoat, lemmy, forgejo, github-enterprise) honour
      the `server_url` argument; hardcoded backends ignore it.
- [ ] **B.2** Implement the same for the WASM-only backends (Discord,
      Teams) by adding `get-signup-method` exports in their guest
      crates.
- [ ] **B.3** Per-backend unit test in each crate's `tests/` (or inline
      `#[cfg(test)]`) verifying the URL is well-formed and the
      custom-server case produces the expected URL given a sample
      `server_url`.
- [ ] **B.4** Update plugin FTL bundles: add `plugin-<backend>-signup-link-label`
      = "Don't have an account? Register at &lt;backend&gt; →" for each
      compiled-in backend. English first; other locales follow as
      stubs.
- [ ] **B.5** `cargo test --workspace` green.

**Acceptance:** each backend returns a sensible URL or in-app route;
custom-server backends produce parameterised URLs; FTL keys present in
each plugin's bundle; all unit tests pass.

---

## Phase C — Host browser-opening helpers (per-shell)

**Effort:** 1 day.

- [ ] **C.1** Create `crates/host-open-external/` crate with the
      `OpenExternal` trait (`fn open(&self, url: &str)`).
- [ ] **C.2** Implement `WebOpenExternal` (WASM) using
      `window.open(url, "_blank", "noopener,noreferrer")` — already the
      pattern at `attachment_context_menu.rs:133`.
- [ ] **C.3** Implement `DesktopOpenExternal` (Wry / native) using
      the `open` crate (`open::that(url)`); add `open = "5"` to the
      desktop shell's `Cargo.toml`.
- [ ] **C.4** Add `POST /host/open-external` route to the fullstack
      host bridge (`crates/host-bridge/`?), which the Electron shell
      proxies to the main process for `shell.openExternal(url)`.
      Implement `ElectronOpenExternal` that fires this route.
- [ ] **C.5** Each shell's `main.rs` (`apps/web`, `apps/desktop`,
      `apps/desktop-electron`) provides the appropriate
      `Arc<dyn OpenExternal>` via `use_context_provider`.
- [ ] **C.6** Sanity-check on each shell that calling
      `cx.open("https://example.com")` opens a new tab / system browser
      window with no security-policy errors (CSP on web, sandbox on
      Electron).

**Acceptance:** all three shells can open an arbitrary URL via the
shared trait; no new CSP / sandbox warnings in console.

---

## Phase D — UI: "Register" affordance on login screens

**Effort:** 1 day.

- [ ] **D.1** Add a `<RegisterLink backend_slug=… current_server_url=…>`
      Dioxus component in `crates/core/src/ui/signup/`. On click, calls
      `backend.get_signup_method(current_server_url)` and dispatches to
      `OpenExternal::open` or `Navigator::push` based on the variant.
      Hides itself when the result is `NotSupported` or when
      `InApp(route)` matches the current route.
- [ ] **D.2** Mount `<RegisterLink>` in every per-backend signup form
      component (`clients/<backend>/src/signup.rs`'s
      `signup_render_fn`). Position: below the primary submit button,
      separated by a thin divider, secondary-button styling.
- [ ] **D.3** Mount `<RegisterLink>` on each card in the backend-picker
      page (`/signup`) so users browsing without having clicked a
      backend yet still see the affordance.
- [ ] **D.4** FTL strings for the link label resolved per-backend (key
      added in Phase B.4); fallback to a core string
      `signup-register-link-generic` = "Register" if the plugin key is
      missing.
- [ ] **D.5** Visual smoke test in poly-web (Chromium): open `/signup`,
      pick each backend, confirm the link appears, click it, observe
      tab opens / nothing happens (NotSupported).

**Acceptance:** every backend's login screen shows the link except
demo; clicking it opens the right URL or navigates correctly; the
`#![component]` body stays under the 150-line rule.

---

## Phase E — Playwright spec per backend

**Effort:** 1.5 days (10 backends × ~45 min for spec + harness wiring).

- [ ] **E.1** Add `--scenario signup-link-<backend>` flag to
      `tests/e2e/persona-multi-agent.sh` (or the existing scenario
      runner shipped by `plan-persona-e2e-multi-agent.md` Phase B).
      The scenario boots poly-web, navigates to
      `/signup/<backend>`, clicks the Register link, captures the
      newly-opened tab via `page.context().on('page', …)`.
- [ ] **E.2** Author one spec per backend at
      `tests/e2e/signup/<backend>-signup.spec.ts`. Each asserts the
      URL pattern matched in the table above. For poly-server, asserts
      the in-app route mounted (no new tab). For demo, asserts the
      link is absent.
- [ ] **E.3** Add real-network gating: each spec checks
      `process.env.POLY_SIGNUP_E2E_REAL === "1"` and only then follows
      the URL and asserts page-content (title / form fields). Default
      CI run skips the follow-through and asserts only the URL.
- [ ] **E.4** Add an `npm run test:signup` aggregate script + a CI job
      stub (commented-out in the workflow file with a note pointing to
      this plan) that runs the suite in mock-mode.
- [ ] **E.5** All specs green locally (`npx playwright test
      tests/e2e/signup/`).

**Acceptance:** 9 of 10 backends have a passing spec asserting the
correct outbound URL or in-app navigation; demo's spec asserts link
absence; real-network mode runs cleanly when the env flag is set.

---

## Phase F — Documentation + acceptance

**Effort:** 2 hours.

- [ ] **F.1** Update `clients/client/agents.md` (or the equivalent
      backend-author HOW-TO) with a "Implementing get_signup_method"
      section, including the per-backend table as the canonical
      reference.
- [ ] **F.2** Add a one-paragraph note to the project README (or the
      Phase 5 / messaging-architecture doc — locate during
      implementation) calling out the new "Register" link as a user-
      visible feature.
- [ ] **F.3** Tick every `- [x]` in this plan, mark `## Status: DONE`,
      and reference the merge commits in each phase header per the
      checkbox rule.

**Acceptance:** doc updates merged; this file marked DONE; the
follow-up CI job from E.4 enabled (uncommented).

---

## Whole-plan acceptance criteria

1. `wit/messenger-plugin.wit` has a `get-signup-method` method and a
   `signup-method` variant; old plugin .wasm files still load.
2. Every one of the 10 backends listed in the table implements the
   method with the documented default.
3. The host has one `OpenExternal` trait with three working
   implementations (web / desktop / electron); calling it from any
   Dioxus component opens the URL in the right surface.
4. Every per-backend login form and the `/signup` picker show a
   "Register here →" affordance (or hide it for demo / poly-server-
   already-here).
5. Playwright suite at `tests/e2e/signup/` has one spec per backend;
   default CI run is mock-mode green; real-network run is a manual
   gate behind `POLY_SIGNUP_E2E_REAL=1`.
6. No regression in existing `SignupEntry::render` flows for
   poly-server (its in-app signup still works exactly as before).

---

## Open questions (to resolve during Phase A)

- **Matrix homeserver-specific signup URL discovery.** Matrix has no
  standard registration URL; some homeservers expose `/_synapse/admin`,
  some delegate to Element. Default in the table is the homeserver
  root `{server-url}` with a fallback to Element web. Confirm with the
  matrix client author whether `.well-known/matrix/client` carries a
  registration hint we should respect.
- **Teams MSA signup URL stability.** Microsoft has changed the live.com
  signup URL in the past. May want to point to a Microsoft-owned
  redirector instead.
- **Should `get_signup_method` be sync?** The default impl is
  `async fn` for consistency with the rest of the trait, but every
  expected impl returns immediately. If the WIT side-effect of being
  async is awkward, drop the `async` and return `Result<SignupMethod>`
  directly. Decide in A.2.
