# Plugin Testing Guide — Client UI Surface

> Full decision record: [`docs/plans/plan-client-ui-surface.md`](plans/plan-client-ui-surface.md) §6A  
> Test matrix at a glance: [`docs/4-testing/client-ui-test-matrix.md`](4-testing/client-ui-test-matrix.md)

---

## Overview

Every new client UI surface primitive ships with tests at five layers (D31):

1. **Unit tests** inline in source files
2. **Per-backend capability tests** in `clients/<name>/tests/capabilities.rs`
3. **Per-backend integration tests** in `clients/<name>/tests/integration*.rs`
4. **E2E via WASM host** in `crates/plugin-host-tests/tests/client_e2e/`
5. **Cross-backend parity** in `clients/client/tests/client_ui_surface_parity.rs`

Plus:

- **Lint-gate scanner tests** in `crates/lint-gate/src/lib.rs`
- **Trybuild compile-fail fixtures** in `crates/ui-macros/tests/compile-fail-client-ui/`
- **UI snapshots** via Playwright through the poly-web MCP

---

## Layer 1 — Unit Tests (inline)

Unit tests live in `#[cfg(test)] mod tests` blocks at the bottom of each source file.
All test files must carry the lint allowlist:

```rust
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    // ...
}
```

Run them with:

```bash
# All unit tests in the poly-client crate (ui_surface types)
cargo test -p poly-client --lib

# All unit tests in a specific backend
cargo test -p poly-discord --lib
cargo test -p poly-lemmy --lib

# All unit tests workspace-wide
cargo test --workspace --lib
```

Key coverage areas in `clients/client/src/ui_surface.rs`:

- `Cursor` round-trip serialize/deserialize for all four `CursorKind` variants (`Offset`,
  `Timestamp`, `Id`, `Opaque`) — tests present as `cursor_offset_roundtrip` etc.
- SVG sanitizer allowlist/denylist — see `crates/core/src/ui/client_ui/custom_block.rs`
  for `svg_path_allowed`, `svg_script_stripped`, `foreign_object_stripped`.
- `CustomBlock` HTML sanitization corpus — in the same file, covering `<script>`,
  `javascript:` URLs, `onclick` attributes, `<iframe>`, `<foreignObject>`.

---

## Layer 2 — Per-Backend Capability Tests

Every backend has a `capabilities.rs` test file asserting its declared UI surface items
are consistent with its stated `BackendCapabilities`.

Run them with:

```bash
cargo test -p poly-discord --test capabilities
cargo test -p poly-lemmy --test capabilities
cargo test -p poly-matrix --test capabilities
cargo test -p poly-hackernews --test capabilities
# and so on for each backend
```

What these tests cover (per D31):

- Declared menu items for each `MenuTargetKind` are well-formed (ids are kebab-case, FTL
  keys follow the `plugin-<id>-menu-<key>-label` pattern, slots are valid).
- Declared settings sections match the backend's feature set (Discord has `per-server`
  profile/notification/privacy sections; HackerNews has none).
- Sidebar layout matches the canonical mapping (Discord → `ChannelList`, Matrix →
  `SpacesRooms`, Lemmy → `Communities`).
- Composer buttons are consistent with backend capabilities (Discord declares stickers;
  Lemmy returns an explicit empty list).
- Every backend that claims `dms: true` in `BackendCapabilities` declares non-empty
  `Dm` target menu items or an explicit empty list with a comment.

---

## Layer 3 — Per-Backend Integration Tests

Integration tests exercise the full round-trip: declare an action, invoke it, assert the
outcome type. These live in `clients/<name>/tests/integration*.rs`.

```bash
# Common pattern — naming varies per backend
cargo test -p poly-discord --test integration_test
cargo test -p poly-lemmy --test integration_test
cargo test -p poly-matrix --test integration_test
```

Key assertions:

- `declares_all_required_surfaces()` — calls each new method and asserts it returns a
  well-formed result (possibly empty, not an error, not a panic).
- For each declared menu action ID: `invoke_context_action(id, target, target_id)` returns
  an `Ok(ActionOutcome::*)` variant, not `ClientError::NotFound`.
- For a fabricated unknown action ID: `invoke_context_action("made-up-id", ...)` returns
  `Err(ClientError::NotFound(...))`.
- Settings round-trip: `get_settings_sections()` returns sections; each section has at
  least one field; `get_setting_value(scope, "", key)` returns the declared default.

---

## Layer 4 — E2E via WASM Host

These tests run the actual `.wasm` plugin binary through the `poly-plugin-host` crate's
`PluginBackend` type. They are gated by per-backend feature flags:

```bash
cargo test -p poly-plugin-loader-tests --features test-demo
cargo test -p poly-plugin-loader-tests --features test-discord
cargo test -p poly-plugin-loader-tests --features test-matrix
cargo test -p poly-plugin-loader-tests --features test-stoat
cargo test -p poly-plugin-loader-tests --features test-teams
cargo test -p poly-plugin-loader-tests --features test-server
```

The test files live in `crates/plugin-host-tests/tests/client_e2e/`. The shared harness
is split into surface-specific modules:

| File | Surface |
|---|---|
| `harness.rs` | Core: authenticate, backend_type, backend_name |
| `harness_menus.rs` | `client-menus` helpers |
| `harness_settings.rs` | `client-settings` helpers |
| `harness_sidebar.rs` | `client-sidebar` helpers |
| `harness_views.rs` | `client-views` helpers |
| `harness_composer.rs` | `client-composer` helpers |
| `harness_custom_block.rs` | `custom-block` sanitization helpers |
| `harness_build_route.rs` | `host-api.build-route` helpers |

Each backend driver (`demo.rs`, `discord.rs`, `matrix.rs`, etc.) calls every harness
helper applicable to its declared capabilities.

### Available harness helpers

**Menus:**

```rust
harness_menus::menu_items_well_formed(backend, target, target_id).await;
harness_menus::menu_items_have_valid_ftl(backend, ...).await;
harness_menus::menu_items_use_kebab_action_ids(backend, ...).await;
harness_menus::invoke_action_unknown_returns_notfound(backend, ...).await;
harness_menus::invoke_action_roundtrip(backend, known_id, ...).await;
harness_menus::menu_pending_action_polls(backend, ...).await;
```

**Settings:**

```rust
harness_settings::settings_sections_well_formed(backend).await;
harness_settings::setting_roundtrip(backend, scope, key, value).await;
harness_settings::setting_persists_across_reload(backend, ...).await;
```

**Sidebar:**

```rust
harness_sidebar::sidebar_declaration_well_formed(backend).await;
harness_sidebar::sidebar_layout_matches_capabilities(backend).await;
harness_sidebar::sidebar_invalidated_event_refetches(&mut backend).await;
harness_sidebar::invoke_sidebar_action_roundtrip(backend, ...).await;
```

**Views:**

```rust
harness_views::channel_view_descriptor_well_formed(backend, ch_id).await;
harness_views::view_rows_paginate(backend, ch_id).await;
harness_views::view_cursor_is_structured(backend, ch_id).await;
harness_views::view_detail_returns_custom_block(backend, ...).await;
```

**Composer:**

```rust
harness_composer::composer_buttons_well_formed(backend, ch_id).await;
harness_composer::message_actions_well_formed(backend, ch_id, msg_id).await;
harness_composer::invoke_composer_action_roundtrip(backend, ...).await;
```

**Custom-block security:**

```rust
harness_custom_block::custom_block_survives_sanitization(backend).await;
harness_custom_block::custom_block_scripts_stripped(html).await;    // unit-style
harness_custom_block::custom_block_javascript_url_stripped(html).await;
```

**Route builder:**

```rust
harness_build_route::plugin_builds_routes_via_host_api(backend).await;
harness_build_route::invalid_route_kind_returns_error(backend).await;
harness_build_route::navigate_outcome_routes_are_valid(backend).await;
```

### Adding a new harness helper

1. Identify which surface module it belongs to (`harness_menus.rs`,
   `harness_settings.rs`, etc.).
2. Add a `pub async fn` with a `&PluginBackend` (or `&mut PluginBackend` if the helper
   mutates state) and any extra parameters.
3. The function must assert, not just call — a helper that returns without asserting
   provides no coverage.
4. Call the helper from every backend driver that has the applicable surface
   declarations. Backends that declare empty lists still call the helper — which asserts
   that an explicit empty list is returned (not a panic, not `NotFound`).

```rust
// Pattern — new helper in harness_menus.rs
pub async fn menu_items_use_kebab_action_ids(
    backend: &PluginBackend,
    target: MenuTargetKind,
    target_id: &str,
) {
    let items = backend
        .get_context_menu_items(target, target_id)
        .await
        .expect("get_context_menu_items must not error");
    for item in &items {
        assert!(
            item.id.chars().all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
            "action id {:?} is not kebab-case",
            item.id
        );
    }
}
```

---

## Layer 5 — Cross-Backend Parity

```bash
cargo test -p poly-client --test client_ui_surface_parity
```

File: `clients/client/tests/client_ui_surface_parity.rs`

These tests instantiate every backend (compile-time enforced by D9) and assert
runtime shape consistency:

- Every backend implements all five surfaces (D9 — the workspace doesn't compile if
  they don't, but these tests also pin the runtime shape).
- Every backend that claims `dms: true` in `BackendCapabilities` returns non-empty
  `Dm` target items **or** an explicit empty list (the explicit empty is the evidence
  that the author made a deliberate choice).
- `server_menu_never_empty_when_groups_supported` — asserts that a backend supporting
  server creation returns at least one server menu item.
- `settings_sections_respect_scope` — a backend declaring `PerChannel` scope must have
  at least one channel context where the section applies.

---

## Lint-Gate Scanner Tests

```bash
cargo test -p poly-lint-gate --lib
```

File: `crates/lint-gate/src/lib.rs`

Tests for two scanners that the build runs against every plugin:

**`ftl_label_key_coverage`** (D21):

```
ftl_label_key_coverage_ok         — all declared keys present in FTL bundle
ftl_label_key_coverage_fails_on_missing — missing key causes cargo::error
```

**`action_id_naming`** (D25):

```
action_id_naming_ok                — kebab-case ids pass
action_id_naming_rejects_snake_case — "invite_people" fails
action_id_naming_rejects_camel_case — "invitePeople" fails
```

**`custom_block_usage_counter`**:

```
custom_block_usage_zero            — baseline: plugin with no custom blocks
custom_block_usage_counted         — plugin with two blocks reports count 2
```

**`forbid_backend_slug_match_in_ui`** (WP 7):

```
slug_match_in_ui_caught            — match arm on "discord" => under src/ui/ caught
slug_match_outside_ui_allowed      — same pattern outside src/ui/ is not flagged
```

---

## Trybuild Compile-Fail Fixtures

```bash
cargo test -p poly-ui-macros --test compile_fail_client_ui
```

Fixtures in `crates/ui-macros/tests/compile-fail-client-ui/`. Each fixture is a small
`.rs` file plus a `.stderr` snapshot.

Current fixtures:

| Fixture | What it tests |
|---|---|
| `ui_action_wrong_ftl_key.rs` | FTL key in wrong namespace fails lint |
| `action_id_not_kebab.rs` | Non-kebab-case action id fails lint |

### Adding a new fixture

1. Create `<fixture-name>.rs` with the minimal code that should fail to compile.
2. Run `cargo test -p poly-ui-macros --test compile_fail_client_ui` — it will fail and
   print the actual stderr.
3. Copy the stderr output into `<fixture-name>.stderr`.
4. Run again — it should pass now.

```rust
// Example fixture: action_id_not_kebab.rs
// This should fail with the action_id_naming lint.
fn main() {
    let _ = MenuItem {
        id: "invitePeople".to_string(),  // camelCase — should be "invite-people"
        // ...
    };
}
```

---

## UI Snapshot Tests (Playwright)

Snapshot golden files live in `tests/snapshots/<backend>/<surface>.html`. The CI job
diffs against these goldens and fails if they diverge without an intentional update.

Surfaces captured per backend: right-click menu (server/channel/user/message/dm),
settings panel (per account scope, per server scope), sidebar, forum/feed/issue view.

### Running snapshots via poly-web MCP

The poly-web MCP (`mcp/web-devtools-mcp`) drives Playwright through the running app.

```
launch_app → poll get_last_build_status → connect_cdp → take_snapshot
```

Right-click workflow:
1. `launch_app`
2. Poll `get_last_build_status` every 5–10 s until `state != "Running"`.
3. `connect_cdp`
4. Navigate to a backend account view.
5. Right-click the server list item via `click_at` with the right button.
6. `take_snapshot` of the context menu DOM.
7. Save to `tests/snapshots/<backend>/server-context-menu.html`.

### Refreshing golden files

When a surface intentionally changes (e.g., WP 2 adds Lemmy menu items), update the
golden by deleting the existing `.html` file and re-running the snapshot job. The new
output becomes the new golden. Commit the updated file alongside the code change.

To regenerate all goldens at once:

```bash
# From the repo root — runs the MCP-driven snapshot capture for each backend
cargo run -p poly-snapshot-refresh -- --all-backends
```

(The `poly-snapshot-refresh` binary is created in WP 0.)

### Interpreting `baseline.json`

The lint gate's `baseline.json` (in `crates/lint-gate/`) records the count of
`custom-block` usages per plugin at the time the last snapshot was taken. CI fails if
the count increases without a corresponding update to `baseline.json`. To update it:

```bash
cargo run -p poly-lint-gate -- --update-baseline
```

Commit the updated `baseline.json` with the PR that adds the new `custom-block` usage,
with a comment explaining why a declarative primitive was insufficient.

---

## Quick Reference — Which Command for What

| Goal | Command |
|---|---|
| Run all UI surface unit tests | `cargo test --workspace --lib` |
| Run Discord capability tests | `cargo test -p poly-discord --test capabilities` |
| Run Lemmy integration tests | `cargo test -p poly-lemmy --test integration_test` |
| Run E2E for Discord via WASM host | `cargo test -p poly-plugin-loader-tests --features test-discord` |
| Run cross-backend parity | `cargo test -p poly-client --test client_ui_surface_parity` |
| Run lint-gate scanner tests | `cargo test -p poly-lint-gate --lib` |
| Run trybuild compile-fail | `cargo test -p poly-ui-macros --test compile_fail_client_ui` |
| Full workspace test | `cargo test --workspace --all-features` |
