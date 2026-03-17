# Memory: Shared DM/group contract parity

*Stored: 2026-03-17T13:11:38.371849715+00:00*

---

Expanded the shared `ClientBackend`/WIT/plugin-host contract with `add_group_member(group_id, user_id)`, `open_direct_message_channel(user_id)`, and `open_saved_messages_channel()`. Implemented demo native/plugin support as the contract canary; native Stoat now adopts the shared DM-open methods plus native `add_group_member` via `PUT /channels/{group}/recipients/{member}` and `remove_group_member` via `DELETE /channels/{group}/recipients/{member}`. Stoat WASM guest parity now covers DM open/save and add/remove group-member mutations through mocked host HTTP. Validation passed: `cargo test -p poly-stoat --features native`, `cargo test -p poly-plugin-loader-tests --features test-demo,test-stoat --test client_e2e`, `cargo check --workspace`, and `cargo cranky --workspace`. Remaining blocker: full component rebuilds still fail for `poly-server-client` on `wasm32-wasip2` because `openssl-sys` cannot discover a cross-target OpenSSL install.
