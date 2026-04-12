# Phase 2.20 — Test Plan (WP-11)

> Companion to `phase-2.20-plugin-capabilities-plan.md`.
> This is the test strategy for the plugin-capabilities work. Every WP in
> the main plan has a testing stake here. Run this plan as its own
> work package, interleaved with the implementation WPs.

## Why a dedicated test WP

Phase 2.20 touches the trait layer, every plugin's capability declaration, the host's navigation, routing, composer, notifications, terminology, and the MCP tool surface. A bug in the trait default cascades silently — every plugin that forgets to override `backend_capabilities()` regresses. Manual QA can't catch that. We need:

1. **Unit coverage** for each plugin's declared capabilities (catches regressions where someone upgrades a plugin and forgets to update its declaration).
2. **Integration coverage** for the MCP capability-aware dispatch (catches `list_friends` on HN silently returning `[]`).
3. **Playwright coverage** for the UI — per-backend tab visibility, route redirects, composer read-only state, notification filter shape.
4. **Visual regression** via poly-web MCP screenshots — capture one canonical screenshot per active-account kind so we can diff.

All three testing tiers run in `TEST_HARNESS.md` as `cargo check`, `clippy`, `cargo test`, plus a new "playwright capability suite" step.

---

## Test inventory by WP

### WP-1 — `BackendCapabilities` redesign

**Unit tests (one per plugin crate):**
```
clients/hackernews/tests/capabilities.rs
clients/lemmy/tests/capabilities.rs
clients/github/tests/capabilities.rs
clients/discord/tests/capabilities.rs
clients/teams/tests/capabilities.rs
clients/matrix/tests/capabilities.rs
clients/stoat/tests/capabilities.rs
clients/demo/tests/capabilities.rs
```

Each file asserts the exact shape declared by that plugin — example for HN:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use poly_client::{BackendCapabilities, ClientBackend, MessagingModel, DmSupport, FriendModel, VoiceSupport};
use poly_hackernews::HackerNewsClient;

#[test]
fn hackernews_declares_read_only_feed() {
    let client = HackerNewsClient::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::ReadOnly);
    assert_eq!(caps.dms, DmSupport::None);
    assert_eq!(caps.friends, FriendModel::None);
    assert!(!caps.presence);
    assert!(!caps.typing_indicators);
    assert!(matches!(caps.voice, VoiceSupport::None));
    assert!(caps.advertised_mcp_tools.contains(&"list_servers"));
    assert!(!caps.advertised_mcp_tools.contains(&"list_friends"));
}
```

**Why:** Catches "plugin defaulted to `READ_ONLY_FEED` and the author forgot to opt into messaging".

### WP-2 — `is_forum()` deletion

**Existing unread-badge tests** in `crates/core` must keep passing. Add a regression test asserting the capability-derived check returns the same result for HN/Lemmy/GitHub as the old slug-based function did:

```
crates/core/tests/forum_capability_parity.rs
```

### WP-3 — Nav-button capability gating

**Playwright test:** `tests/capabilities/nav_buttons.spec.ts`
```
- Launch poly-web via MCP (launch_app → connect_cdp)
- Switch to HN account → assert only "Chat" button visible in Bar 2;
  assert DMs/Friends/Notifications/Create-Server buttons are NOT in DOM.
- Switch to Discord account → assert all 4+ buttons visible.
- Switch to Lemmy account → DMs YES, Friends NO, Notifications YES, Create-Server NO.
```

**Screenshot capture:** one `per-backend-nav.png` per account, stored under
`test-results/capabilities/` — used as visual-regression baseline.

### WP-4 — Route-level redirect guards

**Playwright test:** `tests/capabilities/route_guards.spec.ts`
```
- Navigate directly to /hackernews/.../friends → expect redirect to /hackernews/.../
- Navigate to /lemmy/.../voice → expect redirect + toast text "Lemmy doesn't support voice".
- Navigate to /discord/.../friends → expect the friends page to actually render.
```

**MCP eval assertion (fast path):** use `mcp__poly-web__evaluate_script` to
read `window.location.pathname` after the navigation instead of waiting on
screenshots — faster and deterministic.

### WP-5 — Notification category registry

**Unit tests:**
```
crates/core/tests/notification_categories.rs
```
- Given only HN active → `merged_categories()` is empty (no filter dropdown).
- Given HN + Discord → dropdown equals Discord's categories.
- Given GitHub + Discord → dropdown is the union (Discord's 5 + GitHub's ~15), deduped by id.
- Removing the Discord account → the "Voice invites" filter disappears from the dropdown.

**Playwright test:** `tests/capabilities/notification_filter.spec.ts`
- Drives the UI through Bar 1 selections and asserts the filter chip list re-renders.

### WP-6 — Per-plugin terminology

**Unit tests:** `crates/core/tests/terminology.rs`
- `terminology_for("hackernews").container_label()` == "Story list"
- `terminology_for("lemmy").container_label()` == "Community"
- `terminology_for("matrix").container_label()` == "Space"
- Fallback: unknown backend → "Server".

**Playwright test:** `tests/capabilities/terminology.spec.ts`
- Switch to Lemmy → hover the "+" button → tooltip text equals "Create community".
- Switch to Matrix → tooltip equals "Create space".

### WP-7 — Composer read-only gating

**Unit test:** `crates/core/tests/composer_readonly.rs` — pure rendering test
that feeds a mock `ClientManager` with a read-only capability and asserts the
composer component returns the read-only notice element instead of the textarea.

**Playwright test:** `tests/capabilities/composer_readonly.spec.ts`
- HN story view → assert `textarea[data-testid="composer-input"]` is NOT in the DOM.
- Discord channel view → assert it IS.

### WP-8 — MCP `list_plugin_tools` + honest NotSupported

**Integration tests in `mcp/chat-mcp/tests/mcp_integration.rs`:**

```rust
#[tokio::test]
async fn list_plugin_tools_hackernews_omits_list_friends() {
    let mut pool = BackendPool::new();
    // signin as HN anonymous
    let result = call(&mut pool, "list_plugin_tools", json!({
        "backend": "hackernews", "account_id": "hn-anonymous"
    })).await;
    assert_ok(&result);
    let tools: Vec<String> = parse_tool_names(&result);
    assert!(tools.contains(&"list_servers".to_string()));
    assert!(!tools.contains(&"list_friends".to_string()));
    assert!(!tools.contains(&"send_message".to_string()));
}

#[tokio::test]
async fn list_friends_on_hackernews_returns_not_supported_error() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "list_friends", json!({
        "backend": "hackernews"
    })).await;
    assert_err(&result);
    let text = text_of(&result);
    assert!(text.contains("not supported") || text.contains("no friends concept"));
}

#[tokio::test]
async fn send_message_on_github_returns_not_supported_error() { /* ... */ }
```

### WP-9 — Feature-unsupported placeholders

**Playwright test:** manual capability override (dev-only feature flag)
swaps the composer into force-read-only mode and asserts the placeholder
text is rendered. Keeps WP-9 testable even when all real read-only
backends are covered by WP-4 redirects.

### WP-10 — Capability matrix regression test

`crates/core/tests/capabilities_matrix.rs`:
- Iterates every native plugin, reads `backend_capabilities()`, and asserts
  each one matches the expected shape stored in
  `crates/core/tests/fixtures/capabilities_matrix.json`.
- When a plugin legitimately gains a capability, the fixture is updated in
  the same PR so the diff is obvious in review.

---

## Test harness integration

`TEST_HARNESS.md` already runs `cargo check`, `clippy`, WASM build, and `cargo test` in steps 1–4. After this phase, the list of `-p` flags in steps 2 and 4 covers every plugin crate plus `poly-chat-mcp` so a single harness run catches:

- Any plugin that stops compiling (step 1)
- Any plugin whose declared capabilities produce a clippy warning (step 2)
- Any plugin whose capability shape regresses against its unit tests (step 4)
- Any MCP tool that silently returns `Ok([])` where it should return `NotSupported` (step 4)
- Any UI capability regression (step 5, Playwright)

Step 5 (Playwright) gets a new subfolder `tests/capabilities/` containing the per-WP specs listed above. The harness runs them automatically as part of `--project=chromium` — no separate command needed.

### New step 6 — poly-web MCP capability smoke test

Append to `TEST_HARNESS.md`:

```
## 6. Capability smoke test (poly-web MCP)

> Skip if no ClientBackend / capability / plugin_settings file changed.

For each of [hackernews, lemmy, github, discord]:
  - launch_app → connect_cdp
  - evaluate_script: click Bar 1 icon for the account
  - take_snapshot: verify Bar 2 button count matches expected_capabilities.json
  - navigate_page: /<backend>/.../friends
  - evaluate_script: read window.location.pathname → assert redirect if
    capability is missing, same pathname if present
  - take_screenshot: save to test-results/capabilities/<backend>-nav.png

Expected: all four backends render the right nav shape and redirect
correctly on unsupported routes.
```

This smoke test is intentionally shallow — the Playwright specs are the real
gate, this is just the "is the app obviously broken" fast check.

---

## Test execution schedule

Each implementation WP is shipped with its tests in the same PR. Nothing
merges without its tests passing.

| Wave | WPs | Test commands |
|------|-----|---------------|
| 1    | WP-1, WP-10 | `cargo test -p <each plugin crate>` + `cargo test -p poly-core capabilities_matrix` |
| 2    | WP-2, WP-3, WP-6, WP-8 | same + Playwright `nav_buttons.spec.ts` + `terminology.spec.ts` + mcp_integration |
| 3    | WP-4, WP-5, WP-7, WP-9 | same + `route_guards.spec.ts` + `notification_filter.spec.ts` + `composer_readonly.spec.ts` |
| Final | WP-11 (this) | full `TEST_HARNESS.md` run including step 6 |

## Reporting format

When a haiku subagent runs this test plan, it should report as:

| WP | cargo check | clippy | unit tests | Playwright | Notes |
|----|-------------|--------|------------|------------|-------|
| WP-1 | PASS/FAIL   | PASS/FAIL | N passed | — | |
| WP-3 | — | — | — | PASS/FAIL | nav_buttons.spec.ts |
| … | | | | | |

Any FAIL stops the wave; bugs get filed and re-tested on the next iteration.

## Out of scope

- Load testing the capability registry (it's a static lookup, not a hot path).
- Cross-browser Playwright runs — chromium only for this phase.
- Accessibility audits of the new "feature unsupported" placeholders — phase 2.21.
