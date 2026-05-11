# Client Settings — CLI Recipe Book

> Canonical reference for inspecting and adjusting per-backend client settings
> from the command line.  All examples use `poly-cli call <tool> --key value …`
> — the dynamic MCP-to-CLI bridge shipped in `tools/poly-cli/`.
>
> Source of truth for the `client_settings_*` MCP tool family and the
> `client.config.<backend_id>.*` KV namespace.

## Prerequisites

```bash
# Start the poly-chat-mcp server (or rely on your app's fullstack server)
# then confirm the CLI can reach it:
poly-cli health
```

Default server URL is `http://localhost:3010/mcp`.  Override with `--url`:

```bash
poly-cli --url http://localhost:3000/mcp health
```

---

## Overview

**Client settings** let you adjust what version string Poly advertises to a
backend service — without rebuilding the plugin or shipping a new release.
Backend services (Discord, Matrix, Teams, …) often gate certain features or
reject connections based on the `User-Agent` or a protocol-level version field.
When a backend tightens its accepted set, you can update the override yourself
and resume immediately.

**Mechanisms** are named behavioural toggles inside a backend plugin — for
example Discord's `captcha-sandbox` mode, which routes login challenges through
a sandboxed browser.  Mechanisms have a backend-default state; the client
settings surface lets you override that default per-backend without touching
plugin source code.

**All settings are per-backend and persist across restarts** in
`~/.local/share/poly/storage.sqlite3` (Linux) / `~/Library/Application
Support/poly/storage.sqlite3` (macOS) / `%APPDATA%\poly\storage.sqlite3`
(Windows).  Override the data directory with `POLY_DATA_DIR`.  The underlying
KV namespace is `client.config.<backend_id>.*`; see the
[KV namespace reference](#kv-namespace-reference) below.

---

## CLI recipes

### 1. List all backend settings

Returns a snapshot of every known backend (10 slugs) with its current
version override and mechanism states.

```bash
poly-cli call client_settings_list
```

Narrow to a single backend:

```bash
poly-cli call client_settings_list --backend_id=discord
```

### 2. Get the effective version for a backend

Returns the override string if one is set, otherwise the backend's built-in
default.

```bash
poly-cli call client_settings_get_version --backend_id=discord
poly-cli call client_settings_get_version --backend_id=matrix
poly-cli call client_settings_get_version --backend_id=teams
```

### 3. Set a version override

```bash
poly-cli call client_settings_set_version_override \
  --backend_id=discord \
  --override="poly-discord/1.0.9000"
```

The override string is used verbatim as the `User-Agent` (and
`X-Super-Properties` for Discord).  No validation is performed — the backend
decides whether to accept it.

### 4. Clear a version override (revert to default)

Pass `null` to delete the override key.  The backend reverts to
`DEFAULT_CLIENT_VERSION` on the next request.

```bash
poly-cli call client_settings_set_version_override \
  --backend_id=discord \
  --override=null
```

### 5. List mechanisms for a backend

Returns only mechanisms that have been explicitly set.  Unset mechanisms use
the backend default.

```bash
poly-cli call client_settings_list_mechanisms --backend_id=discord
```

### 6. Enable or disable a mechanism

```bash
# Enable Discord captcha-sandbox mode
poly-cli call client_settings_set_mechanism \
  --backend_id=discord \
  --mechanism_id=captcha-sandbox \
  --enabled=true

# Disable it again
poly-cli call client_settings_set_mechanism \
  --backend_id=discord \
  --mechanism_id=captcha-sandbox \
  --enabled=false
```

### 7. Audit recent changes (SQLite direct query)

There is no `list_client_settings_audit` MCP tool yet.  Query the audit
rows directly:

```bash
sqlite3 ~/.local/share/poly/storage.sqlite3 \
  "SELECT created_at, backend_id, action, status FROM client_settings_audit
   ORDER BY created_at DESC LIMIT 20;"
```

A `list_client_settings_audit` MCP tool is tracked as a follow-up item.

---

## Recovery — rolling back a bad override

**Symptom:** you set a version string the backend rejects and can no longer
log in or receive messages.

### Recovery via CLI (preferred)

```bash
# Clear the override — takes effect on the next outbound request:
poly-cli call client_settings_set_version_override \
  --backend_id=discord \
  --override=null

# Sign out and back in, or restart the affected app shell.
```

### Recovery via direct SQLite (when the app cannot boot)

If the app is in a state where `poly-cli` cannot reach the MCP server:

```bash
sqlite3 ~/.local/share/poly/storage.sqlite3 \
  "DELETE FROM poly_kv WHERE key = 'client.config.discord.version_override';"

# Then relaunch — the backend will use DEFAULT_CLIENT_VERSION.
```

Replace `discord` with the affected backend slug.  The KV key shape is:

```
client.config.<backend_id>.version_override
```

For example:
- `client.config.discord.version_override`
- `client.config.matrix.version_override`
- `client.config.teams.version_override`

To also reset all mechanism overrides for a backend:

```bash
sqlite3 ~/.local/share/poly/storage.sqlite3 "
  DELETE FROM poly_kv
  WHERE key LIKE 'client.config.discord.%';
"
```

---

## Per-backend caveats

### Discord

The override string is used in two places: the HTTP `User-Agent` header **and**
the `X-Super-Properties` JSON blob that Discord reads from the base64-encoded
header of the same name.  Setting an incompatible string can trigger Discord's
bot-detection heuristics.  Start with a minor version bump before experimenting
with entirely different UA shapes.

```bash
# Example: bump minor version only
poly-cli call client_settings_set_version_override \
  --backend_id=discord --override="poly-discord/1.0.9001"
```

### GitHub

The GitHub backend delegates outbound HTTP to the `gh` CLI binary.  The
override is recorded in `poly_kv` and returned by `client_settings_get_version`,
but `gh` injects its own `User-Agent` at the transport layer — so the override
does not propagate to the wire in v1.  This is tracked as a Phase B fix-up
item.

```bash
poly-cli call client_settings_get_version --backend_id=github
# → "gh/x.y.z (poly-github/0.0.0)" — gh UA wins
```

### Matrix

The override affects the `User-Agent` header on **all** `/_matrix/*` requests
including the long-poll sync endpoint.  Homeservers rarely filter on UA, but
some Synapse deployments log it for rate-limit attribution.

```bash
poly-cli call client_settings_set_version_override \
  --backend_id=matrix --override="poly-matrix/0.9.0"
```

### Teams

The override affects the `User-Agent` and the `client-version` header on Teams
Graph API requests.  Microsoft validates the client version loosely; a
recognisable prefix (`poly-teams/x.y.z`) is sufficient.

```bash
poly-cli call client_settings_set_version_override \
  --backend_id=teams --override="poly-teams/1.2.0"
```

### All other backends (Stoat, Forgejo, Lemmy, Hacker News, Poly-server)

Standard `User-Agent` override only.  No additional protocol-level version
fields are affected.

---

## Mechanisms reference

Only mechanisms that have been defined by the backend plugin appear here.
Calling `client_settings_set_mechanism` with an unknown ID records the state
in KV but has no effect until the backend plugin reads it.

### Discord

| Mechanism ID | Default | Description |
|---|---|---|
| `super-properties` | enabled | Include `X-Super-Properties` header on every request. Disable only for debugging; Discord login breaks without it. |
| `captcha-sandbox` | disabled | Route CAPTCHA and hCaptcha login challenges through a sandboxed host-managed browser window. Requires `HostCap::SandboxBrowser`. **Live** — supported on all three shells (Wry, Electron, Web). Toggle renders as DISABLED-with-tooltip on shells that don't advertise the cap. |

### Teams

| Mechanism ID | Default | Description |
|---|---|---|
| `oauth-sandbox` | disabled | Route the Microsoft Entra ID (AAD) OAuth interactive popup through a sandboxed host-managed browser window. Not needed for device-code flow. Requires `HostCap::SandboxBrowser`. |

### All other backends (v1)

No mechanisms are declared in v1.  The mechanism list will be empty.

---

## Sandbox browser per-shell behaviour matrix

The `sandbox-browser` host capability (`HostCap::SandboxBrowser`) is now live
on all three shells. Advertised via `GET /host/caps`. The plugin-settings UI
shows a per-plugin "Sandbox available / unavailable" status row for Discord
and Teams (Phase D.3 of `docs/plans/plan-host-sandbox-impl.md`).

| Shell | Implementation | `/host/caps` response | Notes |
|---|---|---|---|
| **Wry desktop** (`apps/desktop`) | `WrySandbox` — isolated Wry WebView, incognito mode | `["SandboxBrowser"]` | Requires a display (GTK). Headless CI disables the display test via `POLY_SANDBOX_RUN_DISPLAY_TEST`. |
| **Electron** (`apps/desktop-electron`) | `ElectronSandbox` — `BrowserWindow` with `partition: "sandbox-<id>"` + IPC | `["SandboxBrowser"]` | Electron's own `/host/caps` handler takes precedence in the merged router. |
| **Web** (`apps/web`) | `WebSandbox` — `window.open` popup + `postMessage` capture | `["SandboxBrowser"]` | Requires the OAuth provider to redirect to `<origin>/sandbox/<id>` (the shim served by the fullstack server). Providers that hardcode their own callback URL won't work on web. |

### Test recipe (manual)

1. Open the Discord or Teams plugin-settings page in the running app.
2. Verify the "Sandbox available" row appears (or "Sandbox unavailable" if the
   shell feature is off).
3. Click "Test sandbox" — should show "Testing…" briefly, then "Sandbox test passed".
4. Via CLI:

```bash
# Enable captcha-sandbox on Discord
poly-cli client-settings set-mechanism \
  --backend_id=discord \
  --mechanism_id=captcha-sandbox \
  --enabled=true

# Disable it again
poly-cli client-settings set-mechanism \
  --backend_id=discord \
  --mechanism_id=captcha-sandbox \
  --enabled=false
```

---

## KV namespace reference

The full key set written by the client-settings surface:

| Key | Type | Meaning |
|---|---|---|
| `client.config.<backend_id>.version_override` | `String` | Overridden User-Agent / version string |
| `client.config.<backend_id>.mechanisms` | `JSON array of strings` | Registry of mechanism IDs that have been explicitly set |
| `client.config.<backend_id>.mechanism.<mech_id>` | `bool` | Whether the named mechanism is enabled |

The `mechanisms` registry key exists so `client_settings_list_mechanisms` can
enumerate all set mechanism IDs without a prefix-scan (the KV store has no such
capability).

---

## Future work

A `list_client_settings_audit` MCP tool exposing the audit rows via `poly-cli`
(instead of direct SQLite) is a planned follow-up.

---

## See also

- `docs/signup-link-surface.md` — how each backend declares its "Register"
  affordance (`get_signup_method`), per-backend URL table, and browser-opening
  behaviour across Web / Electron / Wry shells.
- `docs/personas-cli.md` — CLI recipe book for meta-persona tools.
