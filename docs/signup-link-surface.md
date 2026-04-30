# Signup-Link Surface

> Reference for the "Register here" affordance that appears on every login
> screen across all Poly messenger backends.
>
> Implementation plan: `docs/plans/plan-client-signup-link-surface.md`
> (Phases A-D shipped; Phase E in progress; Phase F = this file).

---

## 1. Overview

Every login screen in Poly needs a **"Register" affordance** — a link or
navigation item that guides a first-time user toward creating an account on
the backend they are trying to connect to.

The affordance works differently per backend:

- **External URL** (most backends): a link that opens the backend's own
  registration page in the system browser or a new tab. The URL may be
  hardcoded (Discord, Teams, HackerNews) or derived from the user's
  configured server URL (Matrix, Stoat, Lemmy, Forgejo, GitHub Enterprise).
- **In-app flow** (poly-server): a Dioxus `Link` that navigates to a
  registration screen inside Poly itself. Poly-server uses an
  Ed25519-key-first, recovery-phrase flow that lives entirely in-app.
- **Not supported** (demo): the affordance is hidden entirely. The demo
  backend has no real auth — no registration is needed or possible.

The `RegisterLink` Dioxus component (
`crates/core/src/ui/signup/register_link.rs`) reads the backend's declared
`SignupMethod` and renders the appropriate element automatically. UI code
that mounts the component does not need to know which variant is active.

---

## 2. Per-backend table

Each backend implements `ClientBackend::get_signup_method(server_url:
Option<&str>) -> SignupMethod`. The table below shows the default return
value and whether a custom-server URL is honoured.

| Backend      | Default                                                     | Custom-server param? | Notes |
|--------------|-------------------------------------------------------------|----------------------|-------|
| discord      | external — `https://discord.com/register`                   | no | Real Discord OAuth covers re-login |
| matrix       | external — `{server-url}/_matrix/client/v3/register`        | yes | Honours configured homeserver; falls back to `https://app.element.io/#/register` when no server URL is set |
| teams        | external — `https://signup.live.com/signup?lic=1`           | no | Microsoft Teams Personal MSA signup. Last verified 2026-04-30 |
| stoat        | external — `{server-url}` (default `https://app.stoat.chat`)| yes | Self-hosted instance root used when server URL differs from `api.stoat.chat` |
| lemmy        | external — `{server-url}/signup` (default `https://lemmy.ml`)| yes | Lemmy signup is at `/signup` on every instance |
| forgejo      | external — `{server-url}/user/sign_up` (default `https://codeberg.org`) | yes | Forgejo/Gitea register at `/user/sign_up` |
| github       | external — `https://github.com/signup` (Enterprise: `{server-url}`) | yes (Enterprise) | Enterprise admins typically disable self-serve; instance root surfaces the SSO landing |
| hackernews   | external — `https://news.ycombinator.com/login`             | no | HN combines login + create-account on one page — no separate `/signup` URL |
| poly-server  | in-app — `/signup/poly`                                     | n/a | Ed25519-key-first, recovery-phrase flow |
| demo         | not-supported                                               | n/a | Demo backend; no signup needed |

Backends with `Custom-server param? = yes` pass the user's currently-typed
server URL through `get_signup_method(Some(server_url))`. When the user
hasn't typed a URL yet (picker page), `None` is passed and the per-backend
default is used.

---

## 3. Adding signup support to a new backend

### Step 1 — Implement `get_signup_method`

In `clients/<backend>/src/lib.rs`, override the trait method:

```rust
fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
    // Example: external URL, parameterised on server URL.
    let base = server_url.unwrap_or("https://example.com");
    SignupMethod::External(format!("{base}/register"))
}
```

Return one of:

| Variant | When to use |
|---------|-------------|
| `SignupMethod::External(url)` | Registration lives on an external website |
| `SignupMethod::InApp(route)` | Registration is an in-app flow registered via `SignupEntry` |
| `SignupMethod::NotSupported` | Backend has no self-serve signup |

The default impl in `clients/client/src/lib.rs` returns `NotSupported`, so
old plugin `.wasm` files continue to load without a version bump.

### Step 2 — No UI work needed

The `RegisterLink` component is mounted automatically by `AddAccountNav`
(the account-picker sidebar). Individual per-backend signup forms mount it
below the submit button via the `signup_render_fn` hook. You do not need to
add `<RegisterLink>` anywhere yourself — it is injected by the framework
once `signup_entries` is populated with the correct `signup_method` field.

### Step 3 — Add a Playwright scenario (Phase E)

Create `tests/e2e/scenarios/signup-link-<backend_id>/scenario.sh` following
the pattern in `tests/e2e/scenarios/signup-link-discord/scenario.sh`. The
spec file lives at `tests/e2e/signup/<backend_id>-signup.spec.ts` and
should:

1. Navigate to `/signup/<backend_id>`.
2. Assert `[data-testid="register-link-<backend_id>"]` is present (or
   absent for `NotSupported` backends).
3. Assert the `href` attribute equals the expected URL.
4. (Optional, real-network mode only, gated on `POLY_SIGNUP_E2E_REAL=1`)
   Follow the link and assert the page title contains one of `["sign",
   "register", "create"]`.

---

## 4. Browser-opening behaviour

The `SignupMethod::External` variant produces an `<a>` element with
`target="_blank" rel="noopener noreferrer"`. The click handler behaviour
differs per shell:

### Web shell (`apps/web` — Chromium tab)

The anchor's native `target="_blank"` opens a new browser tab directly.
No extra JavaScript is required. Pop-up blockers do not fire because the
click is a trusted user gesture.

### Electron shell (`apps/desktop-electron`)

Electron's `setWindowOpenHandler` is already wired in
`apps/desktop-electron-web/electron/main.js:115-118` to intercept every
`window.open` call (including those triggered by `target="_blank"`) and
forward the URL to `shell.openExternal`. No new IPC wiring is needed — the
existing handler covers `RegisterLink` automatically.

### Wry desktop shell (`apps/desktop`)

Wry does not auto-forward `window.open` to the system browser. The
`RegisterLink` `onclick` handler therefore also calls
`host_bridge::Client::open_external(url)`, which POSTs to
`POST /host/open-external` on the fullstack server (shipped in Phase C,
`crates/host-open-external/`). The server handler executes:

| Platform | Command |
|----------|---------|
| Linux | `xdg-open <url>` |
| macOS | `open <url>` |
| Windows | `cmd /c start <url>` |

The `<a href>` attribute is still set correctly so right-click → "Open in
new tab" works in the Wry WebView as a fallback.

### Data-testid

Every rendered link carries `data-testid="register-link-{backend_slug}"`.
Examples: `data-testid="register-link-poly-server"` (in-app link),
`data-testid="register-link-discord"` (external link),
`data-testid="register-link-matrix"` (external, instance-parameterised).
The `NotSupported` variant renders nothing, so the testid is absent for
demo. Phase E Playwright specs locate links by this attribute rather than
by translated text, making them locale-agnostic.

---

## 5. Customising the URL for power users

### Backends that honour the configured server URL

These backends derive the registration URL from the homeserver / instance
URL the user has set in their account or is currently typing in the form.
Changing the server URL automatically changes the Register link:

| Backend | How the URL is derived |
|---------|----------------------|
| matrix | `{homeserver}/_matrix/client/v3/register` |
| stoat | `{instance-root}` (registration is at the instance root) |
| lemmy | `{instance}/signup` |
| forgejo | `{instance}/user/sign_up` |
| github (Enterprise) | `{instance}` (SSO landing at root) |

**To use a custom server:** update the server URL field in the account-add
form. The Register link updates in real time because `server_url` is passed
to `get_signup_method` on each render.

### Backends with hardcoded URLs

Discord, Teams, and HackerNews return a fixed URL regardless of
`server_url`. If the upstream service changes its registration URL:

1. Edit `clients/<backend>/src/lib.rs` — the `get_signup_method` impl.
2. Update the URL constant (or inline string).
3. Update the verification date comment next to the URL.
4. Run `cargo check -p poly-client-<backend>`.

No other files need changing — the UI reads the URL at runtime from the
plugin.

---

## Future work

A potential enhancement is **account-import-from-URL**: for Matrix, Lemmy,
and Forgejo, the `.well-known/matrix/server` (or equivalent) discovery
document can surface the registration URL automatically when a user pastes
an existing account URL. This would make it possible to derive the Register
link from an account URL without the user having to type the homeserver
separately. This is out of scope for the current plan and is tracked as a
future improvement.

---

## See also

- `docs/personas-cli.md` — CLI recipe book for meta-persona tools
- `docs/client-settings.md` — CLI recipes for per-backend version overrides
  and mechanism toggles; also covers the signup-link surface's KV storage
- `docs/plans/plan-client-signup-link-surface.md` — full implementation plan
  with phase status and acceptance criteria
- `crates/core/src/ui/signup/register_link.rs` — `RegisterLink` component
  source
- `clients/client/src/types.rs` — `SignupMethod` enum definition
