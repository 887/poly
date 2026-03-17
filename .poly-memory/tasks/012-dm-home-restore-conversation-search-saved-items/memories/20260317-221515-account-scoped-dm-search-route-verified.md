# Memory: Account-scoped DM search route verified

*Stored: 2026-03-17T22:15:15.123917412+00:00*

---

Added a dedicated account-scoped conversation-search route and page on 2026-03-17:

- New route: `/:backend/:instance_id/:account_id/dms/search`
- New page: `crates/core/src/ui/account/common/conversation_search_view.rs`
- The DM sidebar `Search Conversations` button now routes there instead of the global `/search` page.
- The page renders inside `DmsLayout`, so the left DM shell remains visible (favorites rail, account rail, DM sidebar, account footer).
- Results are scoped to the currently active account only and only include DMs + Groups; Servers are not shown on this page.
- Sorting matches the DM home ordering logic: latest incoming message first, then overall recency fallback.

Poly-web verification:
- Opening `Search Conversations` from Cat account kept the Direct Messages shell visible.
- Header showed `Search Conversations` with description `Search DMs and group chats for Cat (demo)`.
- Results only showed Cat account conversations (6 DMs + 4 groups), not Dog account entries.
- Clicking Diana from the new search page navigated into Diana's DM successfully.
