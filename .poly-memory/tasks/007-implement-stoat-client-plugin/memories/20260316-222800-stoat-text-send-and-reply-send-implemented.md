# Memory: Stoat text send and reply send implemented

*Stored: 2026-03-16T22:28:00.878149531+00:00*

---

Implemented the next native Stoat backend slice on 2026-03-16:

- `StoatClient::send_message()` now sends plain text messages through `POST /channels/{target}/messages`.
- `StoatClient::send_reply_message()` now sends text replies through the same route using `replies: [{ id, mention: false, fail_if_not_exists: true }]`.
- Requests now include a generated nonce/idempotency hint.
- Reply sends hydrate Poly's `MessageReplyPreview` by fetching the referenced Stoat message via `GET /channels/{target}/messages/{message_id}`.
- Attachment upload is intentionally still pending: `MessageContent::WithAttachments` currently returns `ClientError::NotSupported("Stoat attachment upload is not implemented yet")` until the Stoat file upload lifecycle is implemented.

Validation completed:
- `cargo test -p poly-stoat --features native` ✅
- `cargo fmt --all` ✅
- `cargo check --workspace` ✅
- `cargo check -p poly-web --target wasm32-unknown-unknown` ✅
- `cargo cranky --workspace` ✅

Relevant files:
- `clients/stoat/src/api.rs`
- `clients/stoat/src/http.rs`
- `clients/stoat/src/lib.rs`
- `clients/stoat/agents.md`
- `docs/phase-3-plan.md`
