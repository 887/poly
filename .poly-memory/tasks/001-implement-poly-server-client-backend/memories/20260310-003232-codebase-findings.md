# Memory: Codebase Findings

*Stored: 2026-03-10T00:32:32.555086238+00:00*

---

server-client findings:
- http.rs: has all main endpoints but missing: update/delete server, update/delete channel, update/delete category, kick member, group DM create/member mgmt, upload attachment
- backend.rs: get_dm_channels returns User with empty id (needs participant lookup), send_reply_message uses default (needs reply_to), map_message ignores attachments and reactions, event stream misses reactions/voice/server-channel events, get_groups returns empty (server has group DMs), get_channel always returns NotFound
- ws.rs: holds only read half after connect (send_message is no-op), needs Arc<Mutex<SplitSink>> to send typing indicators
- models.rs: complete - all wire types match server responses
- tests/integration.rs: exists and exercises main flows but missing tests for: reply messages, group DMs, categories, upload, WS events
- Server endpoints: GET /channels/:id does NOT exist in server API - must get channel from a list; POST /channels/@groups (create group); reaction events in WS 
- Server WS events: supports reaction add/remove, voice state, server update, channel created/deleted, member joined/left
