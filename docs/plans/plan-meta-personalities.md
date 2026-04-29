# Plan — Meta-Personalities (Personas Above Accounts)

> **Created:** 2026-04-29
> **Status:** DRAFT — design only, no code
> **Depends on:** `plan-claude-desktop-agent.md` (shipped Phases A-F: memory, drafts, events, typing, style, catch-me-up)
> **Layered above:** per-chat `chat_style` (Phase E) and per-contact `contact_facts` (Phase A) — meta-personas are a third layer that *spans* accounts and chats rather than living inside one.
> **Owner crates:** `mcp/chat-mcp/`, `crates/core/src/ui/account/common/agent_panel.rs`, `crates/core/src/ui/agent/`

---

## 1. Concept

### What is a meta-personality?

A **meta-personality** (or "persona") is a user-defined, durable AI agent that
sits **above** Poly's chat accounts rather than inside any one of them. Where
the existing `chat_style` (Phase E of `plan-claude-desktop-agent.md`) is a
small per-chat tone profile that Claude Desktop honours when drafting one
specific reply, a persona is a **named, addressable, multi-account observer**
with its own system prompt, knowledge-source bindings, memory partition, and
optional heartbeat schedule.

Three layers, ordered from narrow → broad:

| Layer | Scope | Identity | Owns |
|---|---|---|---|
| `contact_facts` | one contact in one account | the contact | facts about a person |
| `chat_style` | one chat thread | the thread | tone/formality/signature |
| **persona** (this plan) | **N chats across N accounts** | **the user's brain in a domain** | **system prompt, scoped memory, action audit, heartbeat** |

### How is it different from an account?

- An **account** is a credential bound to a backend (Discord token, Matrix
  homeserver session, Stoat OAuth). It's *what* you connect with.
- A **persona** is a behavioural lens layered over an arbitrary subset of
  accounts. It has no credentials of its own; it borrows the user's existing
  account connections through allowlists.
- An account survives logout only as a stored credential. A persona is pure
  Poly-side state — it persists across restarts because it lives in
  `~/.local/share/poly/storage.sqlite3`.

### How is it different from "the agent" (Phase A-F)?

- The Phase A-F agent is **anonymous** — Claude Desktop sees a single global
  view of every chat the user has granted MCP access to. Memory is keyed by
  `(account_id, contact_id)` or `(account_id, chat_id)` only.
- A persona is a **named, scoped lens**. Same Claude Desktop, but the prompt
  context, tool whitelist, and memory partition are filtered by the persona's
  configuration. Two personas talking about the same Discord server may see
  completely different context bundles.

### When does the user invoke a persona vs. talk to a friend directly?

The user **never** uses a persona to *replace* talking to a friend on Discord
directly — that's what the chat composer is for, and Phase B drafts already
cover "Claude helps me write a reply." Personas are for the cases where the
*user wants advice or analysis spanning many chats* and doesn't want to spell
out the relevant context every time.

Concrete examples (the user's own framing):

- **"Broker Bob"** — system prompt: *"You are my finance broker. Watch my
  finance Discord servers and the family-finance Matrix room. Surface deals,
  flag risks. Speak plainly, no MBA jargon."* Knowledge sources: 3 Discord
  servers + 1 Matrix room. Heartbeat: every 4 hours, produce a draft digest.
- **"Greens Greg"** — system prompt: *"You are my golf-buddy social manager.
  Track DMs and the #golf channel across my golf-club Discord. Remind me
  about tee times, surface invitations I haven't replied to, draft RSVPs in
  my voice."* Knowledge sources: one Discord guild + DM allowlist of 6
  contacts. Heartbeat: daily at 18:00, draft only.
- **"Frag Frank"** — system prompt: *"You are my gamer hype-man. Know my Stoat
  raid roster, ping me when raid invites land, draft loadout discussions."*
  Knowledge sources: 1 Stoat server, 2 channels. Heartbeat: off (manual
  invocation only — Frank is loud, the user wants to summon him on demand).

### Invocation modes

A persona can be invoked in three ways:

1. **From Claude Desktop** — the user types "yo, Frag Frank, tell me what's
   up" and Claude calls `meta_persona_invoke("frag-frank", "tell me what's
   up")`. Returns a freeform text response synthesised from the persona's
   scoped context bundle. This is the path the user explicitly named.
2. **From Poly UI** — a "Talk to" button in the agent panel opens an inline
   chat overlay that pipes through the same `meta_persona_invoke` MCP tool
   (Poly UI talks to Claude Desktop the same way Claude Desktop talks to Poly
   — over the local MCP).
3. **Heartbeat (autonomous)** — an internal scheduler triggers
   `meta_persona_invoke` on a fixed cadence with a stock prompt
   (`"Catch me up on what's happened since your last run."`). Output goes
   into the **draft queue** by default, never directly to a chat.

### Why this is worth doing

- **Single-shot context bundling.** Today the user has to either (a) open
  every relevant chat and paste contents into Claude Desktop, or (b) maintain
  ad-hoc Claude prompts that re-list account names. A persona stores both
  the prompt and the source bindings once.
- **Memory continuity.** A persona has its own memory partition — facts
  Claude learns about "the broker domain" don't pollute the per-contact
  memory of a specific friend, and vice-versa.
- **Cross-account analysis.** Today, `get_reply_context` scopes to one chat.
  Personas are the natural layer for "summarise three servers from three
  backends."
- **Heartbeat without rebuilding the world.** The user can opt a persona into
  autonomous runs without granting any other persona that capability.

---

## 2. Persona schema (data model)

All persona state lives in the same SQLite file as the rest of the agent
data: `~/.local/share/poly/storage.sqlite3`. Migration code goes in
`mcp/chat-mcp/src/memory.rs::MemoryDb::run_migrations` immediately after
the existing Phase E `chat_style` table (line ~103 today). DDL:

```sql
-- Top-level persona definition. One row per persona the user has created.
CREATE TABLE IF NOT EXISTS personas (
    slug TEXT PRIMARY KEY,                -- url-safe id, e.g. "broker-bob"
    name TEXT NOT NULL,                    -- display name, e.g. "Broker Bob"
    avatar_emoji TEXT NOT NULL DEFAULT '🤖',
    system_prompt TEXT NOT NULL,           -- the persona's "who am I" block
    style_notes TEXT,                       -- free-form additional voice notes
    -- behaviour
    heartbeat_interval_secs INTEGER,       -- NULL = manual-only
    proactivity TEXT NOT NULL DEFAULT 'drafts-only',
        -- 'drafts-only' | 'notify' | 'outbound-allowlisted'
    rate_limit_per_hour INTEGER NOT NULL DEFAULT 4,
    -- lifecycle
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_run_at TEXT,
    enabled INTEGER NOT NULL DEFAULT 1     -- 0 disables heartbeat AND invoke
);

-- Knowledge-source bindings: which (account, chat-set) tuples can this
-- persona read? Filter by tag is supported via 'tag' rows whose
-- selector_kind = 'tag' and selector_value names a chat tag.
CREATE TABLE IF NOT EXISTS persona_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    persona_slug TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
    account_id TEXT NOT NULL,              -- which Poly account
    selector_kind TEXT NOT NULL,            -- 'all' | 'server' | 'channel' | 'dm' | 'tag'
    selector_value TEXT,                    -- server_id, channel_id, contact_id, or tag name
    include INTEGER NOT NULL DEFAULT 1,    -- 1 = allow, 0 = explicit deny (deny wins)
    created_at TEXT NOT NULL,
    UNIQUE (persona_slug, account_id, selector_kind, selector_value, include)
);
CREATE INDEX IF NOT EXISTS idx_persona_sources_slug ON persona_sources(persona_slug);

-- Tool whitelist: which chat-mcp / memory-mcp tool names is the persona
-- allowed to invoke when Claude Desktop calls back into Poly on its
-- behalf? Empty whitelist = read-only (only get_reply_context, recall_facts,
-- list_servers, list_channels, get_messages allowed).
CREATE TABLE IF NOT EXISTS persona_tool_whitelist (
    persona_slug TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    PRIMARY KEY (persona_slug, tool_name)
);

-- Persona-scoped memory partition. Separate from contact_facts (which is
-- per (account, contact)). A persona can store/retrieve its own facts.
CREATE TABLE IF NOT EXISTS persona_facts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    persona_slug TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
    category TEXT,                          -- e.g. 'observation', 'reminder', 'preference'
    fact_text TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,     -- pinned facts always included in context
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_persona_facts_slug ON persona_facts(persona_slug);
CREATE INDEX IF NOT EXISTS idx_persona_facts_pinned ON persona_facts(persona_slug, pinned);

-- Outbound allowlist (only consulted when proactivity = 'outbound-allowlisted').
-- Each row authorises the persona to actually send messages into one specific
-- (account, chat) combination — never blanket. UI for editing this requires
-- a typed-confirm modal (per CLAUDE.md feedback_destructive_actions).
CREATE TABLE IF NOT EXISTS persona_outbound_allowlist (
    persona_slug TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
    account_id TEXT NOT NULL,
    chat_id TEXT NOT NULL,
    max_messages_per_day INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    PRIMARY KEY (persona_slug, account_id, chat_id)
);

-- Audit log. Every persona action — invocation, heartbeat run, tool call,
-- outbound send — gets a row. Visible in Phase G UI; auto-pruned to
-- 30 days of history per persona.
CREATE TABLE IF NOT EXISTS persona_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    persona_slug TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
    occurred_at TEXT NOT NULL,
    actor TEXT NOT NULL,                    -- 'user', 'claude-desktop', 'heartbeat'
    action TEXT NOT NULL,                   -- 'invoke', 'heartbeat_run', 'tool_call',
                                            -- 'draft_create', 'outbound_send',
                                            -- 'memory_write', 'memory_read'
    target_account TEXT,                    -- nullable
    target_chat TEXT,                        -- nullable
    payload_json TEXT,                       -- JSON blob; small (<4KB enforced)
    result TEXT NOT NULL,                    -- 'ok' | 'denied' | 'error'
    error_msg TEXT
);
CREATE INDEX IF NOT EXISTS idx_persona_audit_slug_time ON persona_audit(persona_slug, occurred_at DESC);
```

### Schema notes

- **`enabled` is global kill-switch.** Setting `enabled = 0` short-circuits
  heartbeat AND `meta_persona_invoke`. The UI shows it as the prominent
  "pause" toggle on the persona row.
- **`proactivity` has three levels.**
  - `drafts-only` (default for new personas): persona can only emit drafts
    via `draft_create`. No notifications, no outbound.
  - `notify`: persona can additionally push entries to a per-persona
    notification feed visible in the agent panel (no DM out).
  - `outbound-allowlisted`: persona can call `send_message` *only* into
    chats listed in `persona_outbound_allowlist`. Unauthorised sends are a
    `denied` audit row.
- **`rate_limit_per_hour`.** Hard cap on ANY action that produces user-visible
  output (drafts created, outbound sends, notifications). Implemented as a
  rolling-window check against `persona_audit`.
- **Why deny-wins for sources.** A user might allow `(account=discord-1,
  selector_kind=server, selector_value=guild-A)` and then explicitly deny one
  channel inside it via `(selector_kind=channel, selector_value=ch-7,
  include=0)`. Deny rows trump allow rows.
- **`persona_facts` is structurally separate from `contact_facts`.** Cross-
  contamination would be a privacy footgun (Broker Bob shouldn't see what
  Greens Greg knows). The `meta_persona_get_memory` tool ONLY reads from
  `persona_facts WHERE persona_slug = ?`.

---

## 3. Context aggregation

When a persona is invoked, Poly assembles a **context bundle** that combines
the persona's own state with a filtered view of the user's chats.

### `PersonaContextBuilder` — algorithm

A new module `mcp/chat-mcp/src/persona/context.rs` exposes:

```rust
pub struct PersonaContextRequest {
    pub slug: String,
    pub user_prompt: Option<String>,         // user's freeform invoke text
    pub max_messages_per_chat: usize,        // default 30
    pub max_chats: usize,                    // default 25
    pub include_summaries: bool,             // default true
}

pub struct PersonaContextBundle {
    pub persona: PersonaHeader,
    pub system_prompt: String,
    pub pinned_facts: Vec<PersonaFact>,
    pub chat_summaries: Vec<ChatSummaryEntry>,  // per-chat one-paragraph
    pub recent_messages_by_chat: BTreeMap<ChatRef, Vec<MessageBrief>>,
    pub recent_facts: Vec<PersonaFact>,        // last N non-pinned, time-ordered
    pub user_prompt: Option<String>,
}
```

Build steps:

1. **Load persona row + sources** from `personas` and `persona_sources`.
   Resolve sources → concrete `(account_id, chat_id)` set:
   - `selector_kind='all'` → enumerate all chats for that account via
     `client.list_servers + list_channels + list_dms`
   - `selector_kind='server'` → enumerate `list_channels(server_id)`
   - `selector_kind='channel'`/`'dm'` → use directly
   - `selector_kind='tag'` → join against KV-stored chat tags
     (`agent.chat.<account>.<chat>.tags` set)
   - subtract any `include=0` rows
2. **Cap to `max_chats`.** Sort candidate chats by recency-of-last-message
   (cheap KV lookup; `agent.chat.<account>.<chat>.last_msg_ts` is already
   tracked by Phase A). Drop the tail.
3. **For each surviving chat, fetch chat summary first.** `chat_summaries`
   table from Phase A is the cheap layer. If a summary exists and
   `include_summaries=true`, use it; only call `client.get_messages` for
   chats *without* a recent summary, or for the top-K most-recent chats.
   This is the "summarise per channel via a cheap pass first" hint.
4. **Pull recent messages.** Bounded `client.get_messages(chat,
   limit=max_messages_per_chat)`. Use `read_with_timeout(5s)` per backend
   handle (per CLAUDE.md class #4 guidance).
5. **Pull persona facts.** All pinned + last 50 non-pinned from
   `persona_facts WHERE persona_slug = ?`.
6. **Drafts are EXCLUDED from persona context.** A persona looking at its
   own pending drafts could feedback-loop on heartbeat; explicit non-goal.
   Only the user sees drafts.
7. **Filter PII per source-deny rules a second time** post-fetch (defence in
   depth; the chat-set step already filters but a backend may return DMs we
   didn't enumerate, e.g. group DMs).

### Output shape — JSON delivered to Claude Desktop

```json
{
  "persona": { "slug": "broker-bob", "name": "Broker Bob", "avatar_emoji": "💼" },
  "system_prompt": "You are my finance broker...",
  "pinned_facts": [ { "id": 12, "text": "Long on COIN through Q3", "category": "position" } ],
  "user_prompt": "what's the read on tonight's earnings?",
  "chats": [
    {
      "account": "discord-personal",
      "chat_id": "guild-1234/channel-5678",
      "chat_name": "#earnings-watch",
      "summary": "Heated debate about CRWD guidance...",
      "recent_messages": [ { "from": "alice", "ts": "...", "text": "..." }, ... ]
    },
    ...
  ],
  "recent_facts": [...]
}
```

### Cost discipline

- Bundle size capped at **~32KB** (config constant in `context.rs`). Past
  that cap, oldest messages drop first; if still over, drop chat-level
  detail (keep summaries only).
- The bundle is what gets returned to Claude Desktop from
  `meta_persona_invoke`. Claude does the actual completion; Poly is **not**
  the LLM here (consistent with `plan-claude-desktop-agent.md` non-goal
  "no LLM HTTP client in any Poly crate").

---

## 4. MCP surface

New tools added to `mcp/chat-mcp/src/tools.rs` (alongside the existing
~50 tools). Capability gate: all `meta_persona_*` tools are always-exposed
(per Phase A.7 of `plan-claude-desktop-agent.md` — memory/agent tools are
backend-agnostic).

### Tool list

| Tool | Purpose |
|---|---|
| `meta_persona_list` | enumerate personas with summary fields |
| `meta_persona_get` | full row for one persona by slug |
| `meta_persona_create` | create a new persona |
| `meta_persona_update` | update name/system_prompt/etc. |
| `meta_persona_delete` | remove a persona + cascade tables |
| `meta_persona_set_sources` | replace the sources allowlist (atomic) |
| `meta_persona_set_tool_whitelist` | replace the allowed-tool set |
| `meta_persona_invoke` | run the persona once; returns context bundle + persona prompt |
| `meta_persona_set_heartbeat` | configure or clear interval |
| `meta_persona_get_memory` | read persona_facts |
| `meta_persona_set_memory` | upsert a persona_facts row |
| `meta_persona_forget_memory` | delete a fact |
| `meta_persona_recent_actions` | tail of `persona_audit` |
| `meta_persona_set_outbound_allow` | edit outbound allowlist |

### How does Claude Desktop call these?

Same MCP transport already in use (`mcp/chat-mcp/src/main.rs`). Claude Desktop
sees the persona tools in its tool list once the chat-mcp config is loaded.
Typical session flow:

1. User says: *"yo, Frag Frank, tell me what's up"*
2. Claude calls `meta_persona_invoke({ slug: "frag-frank", user_prompt:
   "tell me what's up" })`.
3. Poly returns the `PersonaContextBundle` JSON described in section 3.
4. Claude composes a reply using its OWN inference, anchored on the bundle's
   `system_prompt + pinned_facts + chats`.
5. (Optional) Claude calls `meta_persona_set_memory` to record any new
   observation (e.g. "Frank noticed Alice is logged in for the raid").
6. (Optional) Claude calls `draft_create` to queue a drafted reply for the
   user — this still goes through Phase B's draft system so the user keeps
   final approval.

### JSON schemas — three most important tools

#### `meta_persona_invoke`

```json
{
  "name": "meta_persona_invoke",
  "description": "Invoke a meta-personality. Returns a context bundle (system prompt + scoped chat data + persona memory). Claude composes the reply.",
  "inputSchema": {
    "type": "object",
    "required": ["slug"],
    "properties": {
      "slug":         { "type": "string", "description": "persona slug, e.g. 'broker-bob'" },
      "user_prompt":  { "type": "string", "description": "freeform user instruction; optional" },
      "max_messages_per_chat": { "type": "integer", "default": 30, "minimum": 1, "maximum": 200 },
      "max_chats":             { "type": "integer", "default": 25, "minimum": 1, "maximum": 100 },
      "include_summaries":     { "type": "boolean", "default": true }
    }
  }
}
```

#### `meta_persona_set_heartbeat`

```json
{
  "name": "meta_persona_set_heartbeat",
  "description": "Set or clear the heartbeat interval for a persona. NULL/0 disables. The heartbeat scheduler runs in poly-host.",
  "inputSchema": {
    "type": "object",
    "required": ["slug"],
    "properties": {
      "slug":             { "type": "string" },
      "interval_secs":    { "type": ["integer", "null"], "minimum": 60, "maximum": 86400,
                            "description": "60s minimum, 24h maximum; null disables" }
    }
  }
}
```

#### `meta_persona_set_memory`

```json
{
  "name": "meta_persona_set_memory",
  "description": "Store a fact in this persona's memory partition. Persona memory is separate from contact_facts.",
  "inputSchema": {
    "type": "object",
    "required": ["slug", "fact_text"],
    "properties": {
      "slug":       { "type": "string" },
      "category":   { "type": "string", "description": "free-form, e.g. 'observation', 'preference', 'reminder'" },
      "fact_text":  { "type": "string", "maxLength": 2000 },
      "pinned":     { "type": "boolean", "default": false }
    }
  }
}
```

---

## 5. UI in agent panel

The agent panel (`crates/core/src/ui/account/common/agent_panel.rs`) currently
has four sections: Access toggle, Memory, Drafts (Phase B), Style (Phase E).
The persona work adds a **fifth top-level section** ("Personas") AND a new
top-level route for the management UI.

### Where personas slot into the existing UI

- The 🤖 robot button in the chat header keeps opening the per-chat
  `AgentPanel` (unchanged).
- A new **"Personas" tab** at the top of the panel (sibling tab to "Memory"
  / "Drafts" / "Style") exposes a compact list. Compact list = one row per
  persona, name + status + "Talk to" button + gear-icon → edit modal.
- Full management lives at a new route `/agent/personas` (mounted in the
  same router as `/agent/memory` from Phase A.5). Component:
  `PersonaManagementRoute` in `crates/core/src/ui/agent/persona/route.rs`.

### Component breakdown

All under `crates/core/src/ui/agent/persona/` (new directory):

| Component | File | Role |
|---|---|---|
| `PersonaListPanel` | `list_panel.rs` | compact list inside `AgentPanel` |
| `PersonaListRow` | `list_panel.rs` | single row: avatar + name + status indicator + actions |
| `PersonaEditModal` | `edit_modal.rs` | full-screen modal for create/edit |
| `PersonaSourcesEditor` | `sources_editor.rs` | per-account, per-chat allow/deny tree |
| `PersonaToolWhitelistEditor` | `tool_whitelist_editor.rs` | checkbox grid grouped by category |
| `PersonaOutboundAllowlistEditor` | `outbound_editor.rs` | only visible when proactivity = outbound-allowlisted |
| `PersonaTalkToOverlay` | `talk_to_overlay.rs` | inline chat panel that pipes through `meta_persona_invoke` |
| `PersonaManagementRoute` | `route.rs` | full-page list + create button |
| `PersonaAuditPanel` | `audit_panel.rs` | last 50 audit rows for a persona (Phase G) |

Add `pub mod persona;` to `crates/core/src/ui/agent/mod.rs` (next to
`chat_style_editor`).

### Status indicator

Three states, derived from runtime + DB:

- 🟢 **idle** — `enabled=1`, no heartbeat in flight
- 🔄 **running** — heartbeat task currently executing OR
  `meta_persona_invoke` in flight
- ✉️ **awaiting reply** — last action produced a draft that is still
  `pending` in the `drafts` table
- ⏸️ **paused** — `enabled=0`

### Edit modal — fields

Grouped sections (collapsible):

1. **Identity** — name, slug (autogenerated from name on create, then locked),
   avatar emoji, system prompt (multiline), style notes.
2. **Sources** — `PersonaSourcesEditor`. Per-account toggle "this persona can
   read this account" → expand to show server/channel tree from
   `client.list_servers / list_channels` cached results.
3. **Tools** — checkbox grid grouped: read-only (default-on),
   memory (default-on), draft (default-on), outbound (default-off, blocked
   unless proactivity = outbound-allowlisted).
4. **Behaviour** — heartbeat interval picker (Off / 15m / 1h / 4h / daily /
   custom), proactivity dropdown, rate-limit slider.
5. **Outbound allowlist** — only visible when proactivity ==
   `outbound-allowlisted`. Per-chat row with daily-cap stepper.
6. **Memory** — link out to "Manage memory" → opens a sub-view listing the
   persona_facts rows with delete buttons. Per CLAUDE.md
   `feedback_destructive_actions`, the **bulk "Forget all"** button needs a
   typed-confirm.
7. **Audit** — last 20 rows (collapsed by default; "View all" → audit panel).

### "Talk to" button

Opens `PersonaTalkToOverlay`:

- Slides in over the right-side utility rail (same spot the agent panel
  uses).
- Single-line composer at bottom + scrolling transcript above.
- Each user line → `meta_persona_invoke` call → response displayed inline.
  The response is the *context bundle JSON* in dev mode; in normal mode it's
  what Claude Desktop produced as a follow-up tool call (e.g. the draft body
  Claude wrote).
- This is the friendliest entry point for users who don't want to bounce out
  to Claude Desktop UI.

### Permissions UX — the per-account toggle

The user's vision specifies "this persona can read this account" toggles.
`PersonaSourcesEditor` realises this as:

- Tab strip per Poly account (same icons/avatars used in
  `account_server_bar.rs`).
- Each tab pane: master "Allow this whole account" toggle + nested per-server
  toggles + nested per-channel toggles. Three-state: allow / inherit / deny.
- Deny-wins precedence is reflected visually (a denied channel under an
  allowed server shows a red strike-through).
- Bulk "Allow all servers", "Deny all DMs" shortcuts.

### Wiring points (file refs)

- Sidebar entry for `/agent/personas` route → add to the existing agent nav
  list defined alongside `/agent/memory` in Phase A.5.
- `AgentPanel` body in `agent_panel.rs:252-273` — add `PersonaListPanel { }`
  between `AgentDraftsSection` and `AgentStyleSection`.
- The 🤖 utility-rail tab system already exists (per `agent_panel.rs:246-249`
  — "the agent panel now lives inside the utility-rail tab system"). The
  persona list reuses this.

---

## 6. Heartbeat mode

### Where the scheduler runs

Heartbeat lives in **poly-host** (the per-shell native server, port 3000/3001/3002
depending on shell), NOT in the WASM client. Reasons:

- The WASM client may be closed or backgrounded (Electron tray, Wry hidden).
  Heartbeat needs to run while the user isn't looking at the app.
- The host process is the only place that has reliable access to the
  backend connections (chat-mcp lives there), the SQLite DB, and a real OS
  timer.

### Implementation outline

- New module `crates/poly-host/src/persona_heartbeat.rs`.
- On host startup, after `MemoryDb` opens, query
  `SELECT slug, heartbeat_interval_secs FROM personas WHERE enabled=1 AND heartbeat_interval_secs IS NOT NULL`.
- For each row, spawn a `tokio::time::interval` task. Use **wall-clock
  alignment** — interval ticks computed from `last_run_at` so that
  re-starting the host doesn't re-fire all heartbeats at once.
- On each tick: call the same `PersonaContextBuilder::build()` used by
  `meta_persona_invoke`, then post the bundle to Claude Desktop **via a new
  webhook channel** (see "How does Claude Desktop receive a heartbeat
  notification?" below).
- Persist `last_run_at` after each tick.
- A `PersonaHeartbeatRegistry` owns `HashMap<String, JoinHandle<()>>`;
  reacts to `meta_persona_set_heartbeat` calls (new schedule → cancel
  + respawn).

### How does Claude Desktop receive a heartbeat notification?

Claude Desktop is a strict request-initiator (per Phase C.1 finding of
`plan-claude-desktop-agent.md`) — it cannot consume server-initiated
notifications today. Three paths considered:

1. ❌ **MCP notifications** — not supported by Claude Desktop. Same blocker
   as Phase C.4.
2. ❌ **Phantom tool that pulls** — Claude Desktop won't poll on its own.
3. ✅ **Heartbeat output goes into the draft queue, user-facing notification
   surface, and `persona_audit`** — Claude Desktop is **not** notified. The
   user sees the result via Poly's UI (DraftsSidebar + persona notification
   feed). If the user wants Claude Desktop to react, they explicitly tell
   it ("Claude, run my Broker Bob persona").

So the heartbeat is **strictly an internal Poly run** — it builds the
context bundle, asks ZERO of Claude Desktop, and uses a small built-in
summariser to emit `(notification, draft, fact)` records. The summariser
is a thin templating layer:

- For each chat in the bundle that has at least N new messages since
  `last_run_at`: emit a notification "{persona.name} noticed activity in
  {chat_name}: {message_count} new messages from {sender_count} people."
- For each chat where the persona has an outbound allowlist entry AND the
  user has a pending unanswered question (heuristic: last message in chat
  is from a contact, > 24h ago, contains `?`): emit a draft *placeholder*
  ("…Claude Desktop, Broker Bob would like a reply here") that the user can
  later send to Claude with a single click.

This is intentionally NOT trying to be smart — Claude Desktop is the
inference engine. Heartbeat is just a prompt-builder cron.

### Heartbeat output options & default

Three output classes (mapped onto `proactivity`):

| Proactivity | Drafts | Notifications | Outbound |
|---|---|---|---|
| `drafts-only` (default) | yes | no | no |
| `notify` | yes | yes | no |
| `outbound-allowlisted` | yes | yes | yes (only into allowlist) |

**Default for new personas: `drafts-only`.** The user sees drafts in
`DraftsSidebar`; nothing happens automatically. This matches the spirit of
`plan-claude-desktop-agent.md`'s privacy model: "by default, all auto-approve
flags off; drafts require user click."

### Rate-limiting

- `rate_limit_per_hour` is checked on every output emission (draft create,
  notification, outbound). Implementation: `SELECT COUNT(*) FROM
  persona_audit WHERE persona_slug = ? AND occurred_at > now-1h AND action IN
  ('draft_create', 'notify', 'outbound_send')`.
- When over limit: emit a single "rate-limited" audit row, skip the rest of
  the heartbeat tick.

### Dry-run mode

A persona row has an implicit "dry-run" mode by setting `proactivity =
drafts-only` AND `rate_limit_per_hour = 0`. In that combination, heartbeats
still run but produce ONLY audit entries, never drafts or notifications. This
is the testing posture — recommended for the first 24h after enabling
heartbeat on a new persona.

---

## 7. Privacy / risk

### Cross-account data flow

- A persona reads from N accounts simultaneously. The audit log captures
  every cross-account read by writing a `memory_read` row with
  `target_account` = the source account. Auditable.
- The output of a heartbeat tick that consumed data from account A and
  account B is logged as `(action=heartbeat_run, payload_json={accounts:[A,B],
  chats_touched: N})`. Visible in `PersonaAuditPanel`.
- **Hard rule:** persona context bundles never get persisted to disk in
  full form. Only the AUDIT summary persists. The actual message text
  flows: SQLite → in-memory bundle → Claude Desktop / heartbeat summariser
  → discarded.

### Outbound spam vector

Heartbeat-mode personas + outbound enabled is the riskiest combination
(could spam contacts at 3am). Mitigations layered:

1. **Outbound is per-(account, chat) allowlisted, never blanket.** A
   persona with proactivity = `outbound-allowlisted` and an empty
   `persona_outbound_allowlist` is functionally equivalent to `notify`.
2. **Per-chat daily cap** in `persona_outbound_allowlist.max_messages_per_day`.
   Default 1.
3. **Global rate limit** via `rate_limit_per_hour` (default 4).
4. **Quiet-hours respect** — heartbeat won't emit outbound between 22:00
   and 08:00 in the user's local TZ. Configurable per-persona later; not in
   v1.
5. **Dry-run posture mandatory before enable** — UI nudges the user to keep
   `rate_limit_per_hour = 0` for the first day after enabling outbound mode,
   and shows a banner "{persona} is in dry-run; X audit-only events recorded."

### "Forget me" UX

Three escalation levels:

1. **Forget one fact** — delete row from `persona_facts`. Per-row delete
   button in the memory sub-view.
2. **Forget all persona memory** — `DELETE FROM persona_facts WHERE
   persona_slug = ?`. Typed-confirm modal.
3. **Delete persona entirely** — `DELETE FROM personas WHERE slug = ?` →
   ON DELETE CASCADE removes sources, tool whitelist, facts, outbound
   allowlist, audit. Typed-confirm modal with the persona's slug.
4. **Nuclear all-personas wipe** — extends the existing
   `/agent/access` "Clear all agent data" button (`plan-claude-desktop-
   agent.md` privacy section) to also include the four persona tables.

### Permission boundary surprise

Subtle risk: a persona with read access to *guild-A* has implicit access to
the user's display name and avatar **as visible in guild-A**, which may
differ from the user's primary identity. The audit log captures every read,
but the user may not realise. Mitigation: `PersonaListPanel` shows a
"data exposure summary" inline:

> Broker Bob can read **34 channels across 3 servers in 2 accounts**
> ([details](#sources)). Last full read: 2 hours ago.

Numbers update live. Click → expanded source view.

---

## 8. Sequenced phases

### Phase A — Schema + tables (no UI, no MCP)

- [x] **A.1** Add 6 new tables (`personas`, `persona_sources`,
  `persona_tool_whitelist`, `persona_facts`, `persona_outbound_allowlist`,
  `persona_audit`) to `mcp/chat-mcp/src/memory.rs::run_migrations` after the
  Phase E `chat_style` block.
- [x] **A.2** Struct serialisation: types are represented as `serde_json::Value`
  rows (matching the existing codebase pattern — no separate module needed).
  Helper functions `read_persona_row`, `collect_persona_facts`,
  `collect_persona_audit` added in `memory.rs`.
- [x] **A.3** CRUD helpers on `MemoryDb` in `mcp/chat-mcp/src/memory.rs`:
  `create_persona`, `get_persona`, `list_personas`, `update_persona`,
  `delete_persona`, plus `add_persona_source`, `list_persona_sources`,
  `remove_persona_source`, `add_persona_tool`, `remove_persona_tool`,
  `list_persona_tools`, `add_persona_fact`, `list_persona_facts`,
  `remove_persona_fact`, `forget_all_persona_facts`,
  `set_persona_outbound_allow`, `remove_persona_outbound_allow`,
  `list_persona_outbound_allows`.
- [x] **A.4** Audit-write helper `record_persona_audit(slug, actor, action,
  target_account, target_chat, payload_json, result, error_msg)`.
- [x] **A.5** Prune helper `prune_persona_audit_before(cutoff_iso8601)`
  returns deleted row count — ready for poly-host daily scheduler to call.
- [x] **A.6** Unit tests for migration + CRUD round-trip + cascade delete
  (51 tests total, all pass).
- [x] **A.7** Migration is idempotent — all `CREATE TABLE IF NOT EXISTS` /
  `CREATE INDEX IF NOT EXISTS`; `migration_is_idempotent` test verifies.

**Effort:** 1 session. Land-able in 1 PR.

---

### Phase B — chat-mcp tools wired to schema (no UI yet)

- [ ] **B.1** Add 14 `meta_persona_*` tools to `mcp/chat-mcp/src/tools.rs`
  dispatch and to `should_expose_tool` always-on list.
- [ ] **B.2** Implement read-side tools first: `meta_persona_list`,
  `meta_persona_get`, `meta_persona_get_memory`, `meta_persona_recent_actions`.
- [ ] **B.3** Implement write-side: `meta_persona_create`, `_update`,
  `_delete`, `_set_sources`, `_set_tool_whitelist`, `_set_memory`,
  `_forget_memory`, `_set_outbound_allow`, `_set_heartbeat`.
- [ ] **B.4** Implement `meta_persona_invoke` — calls
  `PersonaContextBuilder::build()` (Phase C below) and returns the bundle.
- [ ] **B.5** Audit-row writes on every successful tool call (or `denied` /
  `error` row otherwise).
- [ ] **B.6** JSON-schema declarations for each tool, hand-checked against
  the MCP spec for type fidelity.
- [ ] **B.7** Integration test: Claude-Desktop-style stdio session creates
  a persona, sets sources, invokes, retrieves audit.
- [ ] **B.8** Tool-list capability test extension (mirrors
  `mcp/chat-mcp/src/tools.rs::tests` Phase A.7 pattern).

**Effort:** 1.5 sessions.

---

### Phase C — Context builder

- [ ] **C.1** `mcp/chat-mcp/src/persona/context.rs` skeleton with
  `PersonaContextRequest`, `PersonaContextBundle`.
- [ ] **C.2** Source resolution: enumerate concrete `(account_id, chat_id)`
  list from `persona_sources` rows, including deny-wins precedence.
- [ ] **C.3** Per-account chat enumeration via `client.list_servers /
  list_channels / list_dms` with `read_with_timeout(5s)`.
- [ ] **C.4** Per-chat summary fetch (Phase A `chat_summaries` table from
  `plan-claude-desktop-agent.md`) — fall back to
  `client.get_messages(limit=30)` only if no summary.
- [ ] **C.5** 32KB bundle-size cap with progressive degradation (drop
  oldest messages, then drop to summary-only).
- [ ] **C.6** Audit row: `(action=memory_read, target_account=…,
  target_chat=…, payload_json={message_count})`.
- [ ] **C.7** Unit tests with mocked `ClientBackend`s for source resolution
  + size-cap behaviour.
- [ ] **C.8** Integration test: end-to-end `meta_persona_invoke` against
  `test-discord` returning a non-empty bundle.

**Effort:** 1.5 sessions.

---

### Phase D — `PersonaListPanel` + `PersonaEditModal` UI

- [ ] **D.1** New directory `crates/core/src/ui/agent/persona/`. Add
  `pub mod persona;` to `crates/core/src/ui/agent/mod.rs`.
- [ ] **D.2** `PersonaListPanel` — fetches via `meta_persona_list`, renders
  rows with status indicator, "Talk to", gear → edit modal.
- [ ] **D.3** Mount `PersonaListPanel` inside `AgentPanel`
  (`agent_panel.rs:252-273`) between Drafts and Style sections.
- [ ] **D.4** `PersonaEditModal` skeleton with collapsible sections
  (Identity / Sources / Tools / Behaviour / Outbound / Memory / Audit).
- [ ] **D.5** `PersonaSourcesEditor` — per-account tabs, server/channel tree,
  three-state allow/inherit/deny, deny-wins visual.
- [ ] **D.6** `PersonaToolWhitelistEditor` — checkbox grid grouped by
  category (read / memory / draft / outbound).
- [ ] **D.7** New route `/agent/personas` → `PersonaManagementRoute`.
- [ ] **D.8** FTL keys for all persona UI strings in
  `locales/{en,de,es,fr}/main.ftl`.
- [ ] **D.9** Component-lint compliance (each component < 150 lines per
  CLAUDE.md design principles + `plan-component-lints.md`).
- [ ] **D.10** Use `BatchedSignal<T>` and `use_reactive_effect` exclusively
  per CLAUDE.md hang-class countermeasures #1 / #6 / #7.

**Effort:** 2.5 sessions — UI-heavy.

---

### Phase E — `PersonaTalkToOverlay` + invoke from UI

- [ ] **E.1** `PersonaTalkToOverlay` component — slide-in over utility rail.
- [ ] **E.2** Composer + transcript scroller; transcript stored in a
  `Signal<Vec<TalkLine>>` keyed to `(persona_slug, session_id)`.
- [ ] **E.3** "Send" → call `meta_persona_invoke` via the local MCP, render
  the returned bundle (dev mode) or the Claude Desktop follow-up content
  (normal mode).
- [ ] **E.4** Wire "Talk to" button in `PersonaListPanel` to open the
  overlay.
- [ ] **E.5** Persist transcript history in KV (`persona.talk.<slug>.<session>`)
  for the last 5 sessions per persona — older sessions auto-pruned.
- [ ] **E.6** Loading state + error state with retry button.
- [ ] **E.7** Integration test against `test-discord`: open overlay, send
  prompt, see context bundle returned.

**Effort:** 1 session.

---

### Phase F — Heartbeat scheduler

- [ ] **F.1** `crates/poly-host/src/persona_heartbeat.rs` — registry struct
  + per-persona task spawn.
- [ ] **F.2** Wall-clock-aligned `tokio::time::interval` based on
  `last_run_at`.
- [ ] **F.3** Heartbeat tick: build context bundle via `PersonaContextBuilder`,
  pass through built-in summariser, emit `(notify, draft, fact)` records
  per `proactivity` level.
- [ ] **F.4** Built-in summariser — pure-function templating; emits one
  notification per chat with N+ new messages, optionally a draft placeholder
  for unanswered questions.
- [ ] **F.5** Rate-limit check vs `persona_audit` rolling 1h window.
- [ ] **F.6** Quiet-hours guard (22:00-08:00 local TZ) for outbound only.
- [ ] **F.7** React to `meta_persona_set_heartbeat` calls — cancel + respawn.
- [ ] **F.8** Integration test: start a persona with 60s heartbeat,
  populate `test-discord` with 5 messages, verify draft placeholder appears.

**Effort:** 1.5 sessions.

---

### Phase G — Outbound-mode allowlist + rate limiting

- [ ] **G.1** `PersonaOutboundAllowlistEditor` UI (only visible when
  proactivity = outbound-allowlisted).
- [ ] **G.2** Per-chat daily-cap stepper.
- [ ] **G.3** Send-path enforcement — when a persona invokes
  `send_message` indirectly (via `draft_approve` or auto-send), check
  `persona_outbound_allowlist` AND
  `persona_outbound_allowlist.max_messages_per_day` + audit count for today.
- [ ] **G.4** Dry-run posture banner — appears when `proactivity =
  outbound-allowlisted` AND `rate_limit_per_hour = 0`.
- [ ] **G.5** "Confirm outbound mode" typed-confirm modal on first enable
  per persona.
- [ ] **G.6** Quiet-hours UI control.
- [ ] **G.7** Integration test: enable outbound, exceed daily cap, verify
  send is denied + audit row written.

**Effort:** 1 session.

---

### Phase H — Telemetry + audit-log UI

- [ ] **H.1** `PersonaAuditPanel` component — paged list of `persona_audit`
  rows with filters (action / time range / target_account).
- [ ] **H.2** Inline JSON viewer for `payload_json` (collapsible).
- [ ] **H.3** Daily auto-prune cron in poly-host (`DELETE WHERE occurred_at
  < now-30d`).
- [ ] **H.4** "Export audit" → JSONL download for power users.
- [ ] **H.5** Persona "data exposure summary" widget on `PersonaListPanel`
  ("can read X channels across Y accounts").
- [ ] **H.6** "Forget all persona memory" + "Delete persona" typed-confirm
  flows.
- [ ] **H.7** Integration into `/agent/access` nuclear wipe to also clear
  the four persona tables.

**Effort:** 1 session.

---

### Phase I (stretch) — Cross-persona coordination

Out of scope for v1 but worth noting so we don't paint ourselves into a
corner with the schema:

- [ ] **I.1** Personas could share a fact pool when explicitly linked
  (e.g. Broker Bob and Frag Frank both know "user travels Tuesday").
  Realised as a `persona_fact_links` join table — additive, no schema
  rewrite needed.
- [ ] **I.2** "Council mode" — invoke multiple personas in parallel and
  aggregate the responses. Pure UI feature on top of multiple
  `meta_persona_invoke` calls.
- [ ] **I.3** Cross-persona conflict detection ("Broker Bob and Greens
  Greg gave contradictory predictions") — pure UI.

---

## 9. Dependencies & ordering

```
A (schema) ──► B (MCP tools) ──► C (context builder) ──┐
                                                       │
                                                       ├──► D (list + edit UI) ──► E (talk-to overlay)
                                                       │
                                                       └──► F (heartbeat) ──► G (outbound) ──► H (audit UI)
```

Critical path: A → B → C → D, after which E/F can ship in parallel.

**Recommended shipping order:** A → B → C → D → E → F → G → H. A through C
can land without UI; the user already gets value from invoking personas via
Claude Desktop. D unlocks self-service config. E is the "killer demo." F is
where the heartbeat bet pays off; G + H are the safety / observability layer
that should land before the user is encouraged to enable outbound mode.

---

## 10. Effort estimate

| Phase | Sessions | Notes |
|---|---|---|
| A | 1.0 | Schema + CRUD + audit helper + tests |
| B | 1.5 | 14 MCP tools + dispatch + tests |
| C | 1.5 | Context builder + size cap + integration test |
| D | 2.5 | UI-heaviest phase — 9 components + FTL |
| E | 1.0 | Talk-to overlay |
| F | 1.5 | Heartbeat in poly-host |
| G | 1.0 | Outbound + rate limit |
| H | 1.0 | Audit UI + nuclear wipes |
| **Total** | **~11 sessions** | Comparable to Phase A-F of `plan-claude-desktop-agent.md` |

---

## 11. Acceptance criteria

- [ ] User creates a persona "Broker Bob" via UI; row appears in `personas`.
- [ ] User binds 2 Discord servers + 1 Matrix room; rows appear in
  `persona_sources`.
- [ ] In Claude Desktop, calling `meta_persona_invoke({slug: "broker-bob",
  user_prompt: "what's up"})` returns a context bundle containing recent
  messages from those 3 sources only.
- [ ] User clicks "Talk to" in `PersonaListPanel`, types a question, sees
  the bundle / Claude follow-up rendered inline.
- [ ] Setting heartbeat to 15m and walking away for an hour produces 4
  audit rows + (with proactivity=`drafts-only`) zero outbound messages.
- [ ] Toggling proactivity to `outbound-allowlisted` with no allowlist rows
  produces zero outbound messages even on heartbeat tick.
- [ ] Rate-limit-exceeded path is observable as a single audit row, no
  side-effects.
- [ ] "Delete persona" cascades; verify all 6 tables are clean for that
  slug.
- [ ] No outbound HTTP from any Poly binary on heartbeat — heartbeat is
  100% local-summariser, NOT an LLM call (consistent with parent plan).

---

## 12. Privacy model summary

- **Default:** new persona has empty sources, empty tool whitelist (read-only
  defaults), `proactivity = drafts-only`, heartbeat disabled. Zero
  cross-account exposure until the user explicitly binds sources.
- **Visibility:** every persona action is in `persona_audit`; surfaced in
  `PersonaAuditPanel`.
- **Wipes:** per-fact, per-persona-memory, full-persona-delete, and
  agent-wide nuclear option, each with appropriate confirm step.
- **Out-of-band writes:** even with `outbound-allowlisted`, sends still flow
  through the existing `draft_create → draft_approve` path (Phase B of
  parent plan); auto-approve is gated by per-chat KV setting AND the
  persona's outbound allowlist.
- **No cloud sync, no LLM calls from Poly** — same posture as
  `plan-claude-desktop-agent.md`. Personas are purely a local lens / cron.

---

## 13. Explicit non-scope

- ❌ Storing AI-provider API keys in Poly. Personas borrow Claude Desktop's
  inference; they don't own a model.
- ❌ Cross-Poly-instance persona sync. SQLite-local only.
- ❌ Voice / audio personas (TTS). Future, not v1.
- ❌ Computer-vision persona-source bindings (e.g. "watch this folder of
  screenshots"). Out of scope; chats only.
- ❌ Auto-creation of personas from conversation patterns ("looks like you
  talk a lot about golf — want a Greens Greg?"). Possible Phase I.
- ❌ Persona-to-persona conversation. They aren't agents that chat with each
  other; they're lenses.
- ❌ Streaming persona responses in the talk-to overlay. Bundle returns,
  Claude Desktop completes async, response shows when ready. No mid-stream
  rendering.

---

## 14. Open questions / decisions captured

| Question | Decision | Why |
|---|---|---|
| Where does heartbeat run — WASM client or host? | **Host (`poly-host`)** | Client may be closed; host has the SQLite + backend connections |
| What does heartbeat output without an LLM? | **Built-in templating summariser** | Matches `plan-claude-desktop-agent.md` non-goal: no LLM in Poly |
| How does Claude Desktop receive heartbeat? | **It doesn't — heartbeat is internal** | Claude Desktop is request-initiator only (Phase C.1 finding) |
| Default proactivity for new personas | `drafts-only` | Privacy default; user opts into more |
| Memory partition shape | **Persona-scoped, separate from `contact_facts`** | Avoid cross-persona leaks |
| Source filter precedence | **Deny wins** | Predictable, matches firewall conventions |
| Outbound default | **Off; requires explicit per-chat allowlist** | Lowest spam risk |
| Bundle size cap | **32KB hard, progressive degradation** | Bounds Claude Desktop request size |
| Talk-to overlay placement | **Right-side utility rail** | Reuses existing tab system |
| Audit retention | **30 days** | Privacy + storage bound |
| Quiet hours | **22:00-08:00 local TZ for outbound only** | Don't spam at night |
| Cross-persona memory sharing | **Phase I — additive table later** | Not v1 |

---

## 15. File-path index (for future implementation agents)

| Concern | Path |
|---|---|
| Schema migrations | `mcp/chat-mcp/src/memory.rs` (after `chat_style` block ~L103) |
| New persona module | `mcp/chat-mcp/src/persona/{mod,store,context}.rs` (new) |
| MCP tool dispatch | `mcp/chat-mcp/src/tools.rs` |
| Capability gate (`should_expose_tool`) | `mcp/chat-mcp/src/tools.rs` |
| Heartbeat scheduler | `crates/poly-host/src/persona_heartbeat.rs` (new) |
| Per-chat agent panel mount | `crates/core/src/ui/account/common/agent_panel.rs` (L252-273) |
| New persona UI | `crates/core/src/ui/agent/persona/{mod,list_panel,edit_modal,sources_editor,tool_whitelist_editor,outbound_editor,talk_to_overlay,route,audit_panel}.rs` |
| Sibling reference (per-chat style) | `crates/core/src/ui/agent/chat_style_editor.rs` |
| Existing memory MCP (separate process) | `mcp/memory-mcp/src/{store,ops,types,mcp}.rs` |
| Storage SQLite path | `~/.local/share/poly/storage.sqlite3` (per CLAUDE.md) |
| Locale strings | `locales/{en,de,es,fr}/main.ftl` |
| Privacy nuclear-wipe | `/agent/access` route (extend existing) |

---

## 16. Risk register

| Risk | Severity | Mitigation |
|---|---|---|
| Heartbeat × outbound = spam vector | HIGH | Per-chat allowlist + daily cap + quiet hours + rate limit + dry-run banner |
| Persona-context leak between personas | MEDIUM | Separate `persona_facts` partition; query always scoped to slug |
| Bundle exceeds Claude Desktop input limit | MEDIUM | 32KB hard cap + progressive degradation |
| User toggles `enabled=0` mid-heartbeat | LOW | Heartbeat task checks `enabled` flag before each emission |
| Migration corrupts existing data | HIGH | All `IF NOT EXISTS` + idempotency test in Phase A.7 |
| Source enumeration races backend reconnect | MEDIUM | `read_with_timeout(5s)` per backend handle |
| Audit-log unbounded growth | LOW | 30-day prune + per-row payload size cap (4KB) |
| Cross-account read in audit visible only post-hoc | MEDIUM | "Data exposure summary" widget shows live current scope |
| WASM hang from `Signal::write` chains in persona UI | MEDIUM | Mandatory `BatchedSignal::batch` per CLAUDE.md class #1 |
| `use_effect` capture of persona slug going stale | MEDIUM | Use `use_reactive_effect` per CLAUDE.md class #6 |

---

## 17. Future hooks (post-v1)

- **Phase I.1** Cross-persona fact-pool linkage table.
- **Phase I.2** Council mode — parallel invoke, aggregated transcript.
- **Phase I.3** Auto-suggest personas from conversation patterns.
- **Phase I.4** Persona "templates" — pre-baked configs (Broker / Gamer /
  Coach / Producer) the user can clone.
- **Phase I.5** Per-persona LLM provider override — for users who DO want
  to pay direct API costs and break the "no LLM in Poly" rule per-persona.
  Explicitly out of v1; would require a whole separate subsystem.
- **Phase I.6** External persona invocation — webhook so other tools (not
  just Claude Desktop) can drive `meta_persona_invoke`.

---

## 18. Glossary

- **Persona / meta-personality** — a user-defined named lens above N accounts.
- **Source binding** — `(account_id, selector_kind, selector_value, include)`
  row that grants/denies persona access.
- **Tool whitelist** — set of MCP tools the persona is allowed to call.
- **Heartbeat** — internal scheduler tick; produces drafts/notifications,
  never directly an LLM call.
- **Proactivity** — the level of autonomous output: `drafts-only` |
  `notify` | `outbound-allowlisted`.
- **Dry-run** — `proactivity=drafts-only AND rate_limit_per_hour=0` —
  audit-only mode for safe testing.
- **Bundle** — the `PersonaContextBundle` JSON returned from
  `meta_persona_invoke`.

---

## Phase A Status

| Item | Date | Notes |
|---|---|---|
| Schema + CRUD + tests landed | 2026-04-27 | commit `bd80fbd7` on `worktree-agent-a6eed0ee4bde17ee2` |

All 7 Phase A checklist items complete. Implementation note: the plan
referenced a new `mcp/chat-mcp/src/persona/` module, but the existing
codebase keeps all DB logic in `memory.rs` as a single `MemoryDb` impl block
(no separate module for `contact_facts`, `drafts`, `chat_style` either).
Phase A follows that pattern — schema + CRUD + tests all in `memory.rs`.
A separate `persona/` module can be introduced in Phase B when the MCP tool
dispatch logic warrants it.

51 unit tests pass (`cargo test -p poly-chat-mcp --lib -- memory`).
`cargo check -p poly-chat-mcp` clean.
