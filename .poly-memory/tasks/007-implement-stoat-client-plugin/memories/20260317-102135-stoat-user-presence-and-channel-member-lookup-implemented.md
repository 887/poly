# Memory: Stoat user presence and channel member lookup implemented

*Stored: 2026-03-17T10:21:35.783595697+00:00*

---

Implemented two additional native Stoat slices on 2026-03-17:

1. User/profile presence lookup
- `get_user(id)` now uses `GET /users/{id}`.
- `get_presence(user_id)` now uses the Stoat user status payload instead of returning a stubbed offline value.
- Stoat user avatar URLs now resolve through Autumn when the instance config exposes `features.autumn.url`.
- Message-author user mapping now reuses the avatar-aware user conversion.

2. Channel member lookup
- `get_channel_members(channel_id)` now works for Stoat server channels.
- Flow:
  - `GET /channels/{id}` to resolve the backing server id
  - `GET /servers/{server}/members` to fetch the server roster
- Member nickname/avatar overrides are applied on top of user records.

Validation:
- `cargo test -p poly-stoat --features native` ✅ (15 integration tests, 27 unit tests)
- full workspace validation still pending after the final docs patch in this turn; run next before declaring complete.

Relevant files:
- `clients/stoat/src/api.rs`
- `clients/stoat/src/http.rs`
- `clients/stoat/src/lib.rs`
- `clients/stoat/tests/integration.rs`
- `clients/stoat/agents.md`
- `clients/stoat/SPEC.md`
- `docs/phase-3-plan.md`
