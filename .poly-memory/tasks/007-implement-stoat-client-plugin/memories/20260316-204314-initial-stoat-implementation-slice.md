# Memory: Initial Stoat implementation slice

*Stored: 2026-03-16T20:43:14.438907955+00:00*

---

Started task 7 by reading root/client/stoat agents plus `docs/phase-3-plan.md`.

Decision for the first increment: implement **3.1.2.1 HTTP client setup with base URL configuration** entirely inside `clients/stoat`, keeping all Stoat-specific logic isolated from the main app and preserving the WIT/plugin boundary.

Reasoning:
- `clients/stoat` is still a pure stub (`src/lib.rs`, `src/guest.rs`).
- The smallest useful resumable step is to add internal Stoat config + HTTP client scaffolding, not UI integration.
- This respects the project's WASM-component isolation rule: no Stoat logic should leak into `poly-core` or app crates.
- After this slice, the next logical increment will be credential extraction/auth request plumbing.
