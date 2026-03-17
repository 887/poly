# Memory: Account-scoped notifications + special-page footer layout

*Stored: 2026-03-17T22:53:04.769812033+00:00*

---

On 2026-03-17, notifications routing was changed from the app-level `/notifications` route to the account-scoped route `/:backend/:instance_id/:account_id/notifications` so the notifications page always preserves the active account context and reliably shows Bar 2 (`account-server-bar`). `AccountBarNotifsButton` now pushes the account-scoped route using the current backend/instance/account.

For special split-shell pages (People / Saved Messages / Notifications), the bottom `VoiceAccountFooter` was also constrained to the page sidebar (`width: 100%; margin-left: 0`) instead of trying to overlap Bar 2. This avoids the footer/account-bar vs `account-server-bar` visual clash on those pages while still keeping the account footer at the bottom of the left menu.

Validation:
- `cargo check -p poly-core` passed
- `cargo check -p poly-web --target wasm32-unknown-unknown` passed
- `cargo cranky --workspace` passed
- poly-web verified:
  - `/demo/demo/demo-cat/notifications` shows Bar 2 + notifications left menu + footer
  - People page footer no longer collides with Bar 2
