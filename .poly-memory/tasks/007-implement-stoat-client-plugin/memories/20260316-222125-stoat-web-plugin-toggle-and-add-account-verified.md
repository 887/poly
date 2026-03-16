# Memory: Stoat web plugin toggle and Add Account verified

*Stored: 2026-03-16T22:21:25.215793075+00:00*

---

Verified live in the web app on 2026-03-16 that Stoat is fully host-registered in `poly-web`:

- Settings -> Plugins shows `Stoat (Revolt)` as a compiled native plugin (no longer `not in this build`).
- Disabling the Stoat plugin in the live Plugins page removes Stoat from the Add Account picker at `/signup`.
- Re-enabling the Stoat plugin restores Stoat to the Add Account picker.
- Selecting Stoat in Add Account navigates to `/signup/stoat` and renders the real Stoat signin form with `Server URL`, `Email Address`, `Password`, and `Sign In`.

This confirms the new host/UI integration path works in the actual browser UI, not just in code.

Live screenshot evidence:
- `devtools-screenshots/web-stoat-plugin-visible-2026-03-16.png`

Relevant host/UI files involved:
- `apps/web/Cargo.toml`
- `clients/stoat/src/signup.rs`
- `clients/stoat/locales/*/plugin.ftl`
- `crates/core/src/ui/mod.rs`
- `crates/core/src/ui/settings/plugins.rs`
- `crates/core/src/ui/settings/plugin_settings.rs`
- `crates/core/src/ui/signup/mod.rs`
- `crates/core/src/client_manager.rs`
