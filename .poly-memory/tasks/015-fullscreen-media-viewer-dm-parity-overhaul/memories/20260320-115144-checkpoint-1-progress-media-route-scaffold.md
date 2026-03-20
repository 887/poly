# Memory: Checkpoint 1 progress: media route scaffold

*Stored: 2026-03-20T11:51:44.413093341+00:00*

---

## 2026-03-20 — Checkpoint 1 progress (Item 1)

Implemented the first post-plan scaffold for route-backed fullscreen media:

- Added new account-scoped route variant in `crates/core/src/ui/routes.rs`:
  - `Route::MediaViewerRoute { backend, instance_id, account_id, channel_id, message_id, attachment_index }`
  - URL: `/:backend/:instance_id/:account_id/media/:channel_id/:message_id/:attachment_index`
- Added `route_account_id` support for this route.
- Added `sync_route_to_app_state` handling for `MediaViewerRoute`.
- Added placeholder route component (`MediaViewerRoute(...) -> ChatView {}`) as item-1 scaffold.
- Checks passed:
  - `cargo check -p poly-core`
  - `cargo check -p poly-web --target wasm32-unknown-unknown`
  - `cargo cranky -p poly-core`

Next incremental item:
- Wire image attachment click handlers to open this route (server channels + DMs + groups).

