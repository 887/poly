# Memory: Native Stoat group-member removal

*Stored: 2026-03-17T12:52:36.057494487+00:00*

---

Implemented native `remove_group_member(group_id, user_id)` for Stoat via `DELETE /channels/{group}/recipients/{member}`. This aligns with the existing shared `ClientBackend` surface and existing core UI path in `crates/core/src/ui/account/common/dm_user_sidebar.rs`, so Stoat no longer returns `NotSupported` for removing a member from a group DM. Validation: `cargo test -p poly-stoat --features native`, `cargo check --workspace`, and `cargo cranky --workspace` all passed; live `poly-web` stayed healthy at `/demo/demo/demo-cat/dms` with `#main` present during the verification pass.
