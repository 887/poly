# Research Findings — Task: Fix UI: Add-Server full-page route + channel list fix + E2E verify

*Auto-updated by poly-memory-mcp. Add findings via CLI or MCP tool.*

---


## Finding 2026-03-10T14:46:21Z

ROOT CAUSE of #general not showing: ServerChannelView renders channels ONLY from server.categories. Channels created without a category_id (e.g. via raw API) are uncategorized and never shown. get_server() in backend.rs only adds channel_ids to categories if ch.category_id matches — uncategorized channels fall through. FIX: ServerChannelView must also show channels not in any category, in a default "Text Channels" section.

---


## Finding 2026-03-10T14:46:26Z

Add Server = inline form in CreateServerButton in account_server_bar.rs lines ~455-540. Currently renders form_open/server_name inline in the bar. FIX: Change onclick to navigate to Route::CreateServer { account_id } instead. Need new route + component. The full page should show: FavoritesBar (bar1) + AccountServerBar (bar2) + Create Server form on the right. Route would be: /create-server/:account_id

---
