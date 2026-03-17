# Memory: DM home split people-vs-compose verified

*Stored: 2026-03-17T21:03:36.720416230+00:00*

---

Implemented and poly-web verified the DM-home UX split requested on 2026-03-17.

What changed:
- Added dedicated `account_last_dm_routes` persistence and verified Conversations reopens the last selected DM instead of the empty DMs placeholder.
- DM sidebar `New Conversation` now routes to a dedicated composer (`/:backend/:instance_id/:account_id/dms/new`) instead of reusing the friends page.
- Added a dedicated Bar-2 people-management button (`👥`) between Conversations and Notifications, routing to the existing friends route now repurposed as a management surface.
- Friends page now acts as People management with tabs for Friends / Ignored / Blocked; Friends has explicit `Message` actions instead of being the compose flow.
- `Search Conversations` routes into the shared Search page with only `DMs` and `Groups` enabled (Servers disabled).
- Saved Messages now opens the aggregated pinned-items page and clicking a card jumps into the source DM/group message.
- DMs and groups are sorted in code by latest incoming message (other user/member first, then overall recency fallback).

Poly-web verification performed:
- `New Conversation` opened the dedicated composer with friend checkboxes and Start Conversation button.
- `👥` opened the People management page with Friends / Ignored / Blocked tabs.
- After opening Alice, leaving to People, and returning via Conversations, the app reopened Alice (last-DM restore works).
- `Search Conversations` opened Search with checkbox state: Servers=false, DMs=true, Groups=true.
- Saved Messages opened aggregated pinned cards and clicking `# Diana` navigated back into Diana's DM.

Known limitation kept explicit in the UI:
- multi-person new-conversation selection is visually prepared, but real shared group-DM creation still needs a backend contract/API.
