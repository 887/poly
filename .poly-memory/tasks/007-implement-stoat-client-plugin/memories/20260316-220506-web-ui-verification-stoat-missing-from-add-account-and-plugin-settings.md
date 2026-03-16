# Memory: Web UI verification: Stoat missing from Add Account and plugin settings

*Stored: 2026-03-16T22:05:06.662378491+00:00*

---

Verified live in poly-web MCP on 2026-03-16 at `http://127.0.0.1:3000/settings`:

- The generic Plugins page shows **Stoat (Revolt)** only as `not in this build` in the native backend list.
- The Plugin Settings nav contains only:
  - `Demo Settings`
  - `Poly Server`
- The Accounts section currently has only demo accounts and the Add Account flow is therefore not exposing Stoat.
- Code inspection matches the UI:
  - `crates/core/src/ui/mod.rs` currently registers native signup entries only for `poly`
  - native plugin settings only for `demo` and `poly`
  - there is no Stoat signup entry or Stoat plugin-settings registration path yet.

Implication: before continuing backend work, Poly needs a real Stoat activation path in the host UI (likely via WASM plugin loading or native feature-gated registration) plus signup entry registration so Stoat can be toggled on/off and selected from Add Account.
