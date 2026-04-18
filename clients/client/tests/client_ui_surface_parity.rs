#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Cross-backend parity tests for the client-UI surface.
//!
//! These tests enforce that every ClientBackend implementation satisfies
//! the shape contracts declared in the plan at docs/plans/plan-client-ui-surface.md.
//!
//! Filled incrementally by WP 1 onwards; stubs today.

/// WP 1: Every backend WASM component exports all five UI-surface interfaces
/// (`client-menus`, `client-settings`, `client-sidebar`, `client-views`,
/// `client-composer`) with correctly-typed signatures. Verified by loading each
/// plugin in the WASM host and calling each surface method.
#[test]
#[ignore = "WP 1: WIT surface not yet defined"]
fn every_backend_declares_all_five_ui_surfaces() {
    todo!("WP 1: implement per plan")
}

/// WP 2: For every backend that declares `groups: true` in its
/// `BackendCapabilities`, `get-context-menu-items` for a server target must
/// return at least one item (the plugin has something to say about groups).
#[test]
#[ignore = "WP 2: context-menu WIT surface not yet defined"]
fn server_menu_never_empty_when_groups_supported() {
    todo!("WP 2: implement per plan")
}

/// WP 2: For every backend that declares `dms: true` in its
/// `BackendCapabilities`, `get-context-menu-items` for a user target must
/// contain an item whose action ID is `block-user` (D25 kebab-case).
#[test]
#[ignore = "WP 2: context-menu WIT surface not yet defined"]
fn user_menu_has_block_action_if_blocking_supported() {
    todo!("WP 2: implement per plan")
}

/// WP 3: Every settings section returned by `get-settings-sections` has a
/// `scope` field that matches the call site: account-global sections appear
/// only when called with `scope: account-global`, server-scoped sections only
/// when called with a server target. No cross-scope leakage.
#[test]
#[ignore = "WP 3: settings WIT surface not yet defined"]
fn settings_sections_respect_scope() {
    todo!("WP 3: implement per plan")
}

/// WP 4: The sidebar declaration returned by `get-sidebar-declaration` for
/// each backend maps to one of the five stock layout kinds
/// (`channel-list`, `spaces-rooms`, `communities`, `feed`, `repo-tree`) or
/// declares a custom section — never an empty declaration when the backend
/// has a known `BackendCapabilities` non-null landing page.
#[test]
#[ignore = "WP 4: sidebar WIT surface not yet defined"]
fn sidebar_layout_matches_capabilities_mapping() {
    todo!("WP 4: implement per plan")
}

/// WP 4: Every `route-kind` value referenced inside a sidebar declaration
/// has a corresponding host handler registered in `poly_host::router()`.
/// Prevents dead-link sidebars where a plugin declares a route the host
/// doesn't know how to render.
#[test]
#[ignore = "WP 4: sidebar WIT surface not yet defined"]
fn sidebar_route_kinds_have_host_handlers() {
    todo!("WP 4: implement per plan")
}

/// WP 5: Every `view-descriptor` returned by `get-view-descriptor` has a
/// `cursor.kind` that matches the declared cursor-kind for that backend's
/// pagination model (D23: `offset | timestamp | id | opaque`). Prevents a
/// backend returning `cursor-kind::timestamp` cursors being registered as
/// `offset` in its descriptor, which would break the host's pagination logic.
#[test]
#[ignore = "WP 5: view-descriptor WIT surface not yet defined"]
fn view_descriptor_cursor_kind_matches_declaration() {
    todo!("WP 5: implement per plan")
}

/// WP 6: Every composer-toolbar button declared via `get-composer-buttons`
/// for a backend is consistent with that backend's `BackendCapabilities`
/// (e.g. no attachment button when `attachments: false`, no reaction picker
/// when `reactions: false`).
#[test]
#[ignore = "WP 6: composer WIT surface not yet defined"]
fn composer_buttons_match_backend_features() {
    todo!("WP 6: implement per plan")
}

/// WP 7: After the cleanup pass, none of the per-backend context-menu Rust
/// source files (`context_menu.rs`) exist under `crates/core/src/ui/account/`
/// and the `backend_server_context_menu_extras` dispatcher is gone. Enforced
/// by asserting that `std::fs::metadata` returns `Err` for each expected path.
#[test]
#[ignore = "WP 7: cleanup pass not yet run"]
fn no_dead_per_backend_files() {
    todo!("WP 7: implement per plan")
}
