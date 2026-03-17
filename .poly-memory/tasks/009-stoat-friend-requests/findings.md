# Research Findings — Task: Stoat friend requests

*Auto-updated by poly-memory-mcp. Add findings via CLI or MCP tool.*

---


## Finding 2026-03-17T12:21:49Z

Implemented native Stoat friend-request support on 2026-03-17: incoming relationship metadata now maps to `NotificationKind::FriendRequest`, native Stoat supports send/accept/reject friend-request endpoints, and Poly notifications UI now calls backend friend-request actions instead of only mutating local state.

---


## Finding 2026-03-17T12:21:49Z

Live `poly-web` verification found and fixed a separate demo-fixture bug: `clients/demo/src/data.rs` demo notifications still used stale `account_id = "demo"` instead of `DEMO_ACCOUNT_ID` / `demo-cat`, which broke backend lookup for notification actions. After fixing that and making the UI optimistic, accepting a demo friend request on `/notifications` removed the card and decremented the visible count from 9 to 8.

---
