# Plan — Claude-Desktop-Driven Social Agent

> **Created:** 2026-04-20
> **Status:** Phase A.1/A.2/A.3/A.6/A.7 complete — A.4/A.5 (UI) deferred to UI agent
> **Depends on:** `poly-chat-mcp` (shipped), `/agent` page KV persistence (shipped 7920bdb7), `send_typing` trait + MCP tool (shipped 6a587e66)
> **Supersedes:** the LLM-provider-in-Poly approach drafted in `docs/6-ai-agent/6.0-social-agent-vision.md` — **not** taking that path.

---

## Vision (revised)

Poly does NOT own the LLM. Claude Desktop (or any other MCP-host — OpenClaude, Goose, Continue, VS Code Copilot, etc.) is the brain. The user pays their Claude subscription **once** and uses it to drive Poly via the existing `poly-chat-mcp`; no per-token API burn inside Poly, no extra-budget charges, no storing AI-provider API keys.

### Division of responsibility

| Concern | Owner | Why |
|---|---|---|
| Thinking (LLM inference) | Claude Desktop | Already configured, user already paying |
| Chat data (messages, contacts, servers) | Poly | It's Poly's reason for existing |
| **Memory of facts tied to Poly chats** | **Poly** | Context like "Alice and I agreed to meet Thursday" is about *this Poly account in this chat*; it must persist in Poly's data dir, not Claude Desktop's session memory |
| Actions (send, react, typing, draft) | Poly | Poly owns the backend connections |
| Approval / autonomy decisions | User via Poly UI | Safety; Poly is the place the user sees what's happening |
| Scheduling / event reactions | Claude Desktop | **Not** Poly — a cron in Poly asking "should I ping Claude now?" burns tokens blindly |

### Non-goals

- No LLM HTTP client (Claude/OpenAI/Gemini/Ollama) in any Poly crate
- No stored AI-provider API keys in any Poly storage
- No autonomous cron / scheduler / outreach loop inside Poly
- No cloud sync for memory — stays in `~/.local/share/poly/storage.sqlite3`
- No `crates/social-agent/` library — the previous plan called for this; it's dead

---

## Anti-bot detection — why typing simulation matters

LLMs type faster than any human. Messenger networks (Discord, Matrix, Slack) increasingly flag "10 messages in 2 seconds with no typing indicator" as bot behavior and either rate-limit, shadow-ban, or terminate the account. The whole value of driving Poly through an agentic MCP is the user operating 10 concurrent conversations through their real accounts — and getting those real accounts killed kills the entire setup.

Typing simulation is **not** for gaslighting a contact about whether they're talking to a human. It's for staying under the radar of network-side bot detection. Claude Desktop drives when/how long to type; Poly runs the pulse locally (it has the open WebSocket connections to the backends) so the LLM isn't round-tripping through MCP for every keystroke.

No global timer — multi-account usage means each chat needs its own independent simulation. ~20 concurrent simulations per account is an adequate bound.

---

## Phases

### Phase A — Memory + context bundler

**Goal:** one fat MCP call gives Claude Desktop everything needed to draft a reply. Memory tools let Claude persist facts discovered mid-conversation.

- [x] **A.1** SQLite migration — new tables (`mcp/chat-mcp/src/memory.rs`, `MemoryDb::run_migrations`):
  - `contact_facts(id, account_id, contact_id, category, fact_text, created_at, updated_at)` — free-form per-contact facts; index on `(account_id, contact_id)`
  - `chat_notes(id, account_id, chat_id, note_text, created_at, updated_at)` — per-thread running context; index on `(account_id, chat_id)`
  - `chat_summaries(account_id, chat_id, summary_text, window_start_msg_id, window_end_msg_id, updated_at)` — rolling summaries; PK `(account_id, chat_id)`; UPSERT on conflict
- [x] **A.2** MCP tools in `mcp/chat-mcp/src/tools.rs`:
  - `remember_fact(account_id, contact_id, category, fact)` → `{ fact_id }`
  - `recall_facts(account_id, contact_id, category?)` → `[Fact]`
  - `forget_fact(fact_id)` → "fact deleted"
  - `search_facts(query, account_id?)` → `[Fact]` (SQL LIKE; FTS later if needed)
  - `store_chat_note(account_id, chat_id, note)` → `{ note_id }`
  - `get_chat_notes(account_id, chat_id)` → `[Note]`
  - `forget_chat_note(note_id)` → "note deleted"
  - `store_chat_summary(account_id, chat_id, summary, window_start_msg_id, window_end_msg_id)` → "summary stored"
  - `get_chat_summary(account_id, chat_id)` → `{ summary, window_start, window_end, updated_at }` | `null`
- [x] **A.3** Context bundler tool `get_reply_context(account_id, chat_id, message_limit=20)` — gracefully returns `null` for missing sections:
  ```json
  {
    "account": { "id", "backend", "display_name" },
    "chat": { "id" },
    "recent_messages": [...],
    "contact": { "id", "display_name", "presence", "last_seen", "facts": [...] } | null,
    "chat_notes": [...],
    "chat_summary": { "summary", "window_start", "window_end", "updated_at" } | null
  }
  ```
- [ ] **A.4** FTL keys for fact-management UI: `agent-memory-title`, `agent-memory-empty`, `agent-memory-category-*`
- [ ] **A.5** `/agent/memory` route + page — per-contact facts viewer/editor (read-only MVP; editing is a later follow-up)
- [x] **A.6** Unit tests: 19 memory unit tests in `memory::tests` + 8 capability/tool-list tests in `tools::tests`; all 48 existing integration tests still pass
- [x] **A.7** Capability gate — all 10 memory/bundler tools registered as always-exposed in `should_expose_tool` (memory is Poly's own concern, not per-backend)

**Effort:** ~1.5 sessions. No UI-heavy work for the viewer; main cost is schema + tool dispatch + tests.

---

### Phase B — Draft queue + approval UI

**Goal:** Claude proposes, user approves. Autonomy is per-reply, not per-chat. Kills the "agent went rogue" risk class.

- [ ] **B.1** SQLite migration — `drafts(id, account_id, chat_id, body, suggested_by, created_at, auto_send_at, status)` where `status ∈ { pending, approved, sent, discarded, expired }` and `auto_send_at` is null unless per-chat auto-approve is on.
- [ ] **B.2** MCP tools: `draft_create(account_id, chat_id, body, auto_send_in_secs?)` → draft_id, `draft_list(account_id?, chat_id?, status?)`, `draft_approve(draft_id)` (calls send_message), `draft_edit(draft_id, new_body)`, `draft_discard(draft_id)`, `draft_cancel_autosend(draft_id)`.
- [ ] **B.3** Auto-send countdown — per-chat setting (default off); when `auto_send_in_secs` passed, Poly's draft engine auto-approves after the timer, giving the user a cancel window. Cancel becomes a no-op after status transitions to `sent`.
- [ ] **B.4** `DraftBanner` component in `ChatView` — renders above composer when drafts exist for current channel: "✨ Claude suggests: [preview text] [Send] [Edit] [Discard]" plus countdown if auto_send_at is set.
- [ ] **B.5** `DraftsSidebar` global panel (behind a per-account toggle) listing pending drafts across every chat so the user can triage without opening each channel.
- [ ] **B.6** Per-chat auto-approve toggle on `/agent/chat/:id` settings — opt-in, off by default.
- [ ] **B.7** FTL keys: `agent-draft-claude-suggests`, `agent-draft-send`, `agent-draft-edit`, `agent-draft-discard`, `agent-draft-autosend-in`, `agent-draft-cancel-autosend`, `agent-drafts-sidebar-title`, `agent-drafts-sidebar-empty`.
- [ ] **B.8** Integration test: draft_create → DraftBanner appears → draft_approve → send_message fires → banner clears.

**Effort:** ~2 sessions. UI is the main cost; tool dispatch is straightforward.

---

### Phase C — Event subscription over MCP

**Goal:** stop polling. Claude Desktop subscribes once, Poly pushes events. Biggest latency + token-cost win because it lets the LLM react only when something actually happened.

- [ ] **C.1** Research — confirm the current state of MCP notifications / streaming support in Claude Desktop specifically (the spec allows it; support status has drifted). Document findings in this plan.
- [ ] **C.2** `subscribe_events(filters)` tool in `poly-chat-mcp` — filters: `account_ids: Option<Vec<String>>, chat_ids: Option<Vec<String>>, event_types: Option<Vec<EventKind>>` where `EventKind ∈ { MessageReceived, MessageEdited, MessageDeleted, FriendRequest, TypingStarted, PresenceChanged, ReactionAdded }`.
- [ ] **C.3** `tokio::sync::broadcast` channel inside the MCP server that fans out every `ClientBackend::event_stream()` tick to all live subscribers.
- [ ] **C.4** SSE transport — MCP supports server-to-client notifications via the transport layer; implement them on the HTTP transport. Verify against the real Claude Desktop build in use today.
- [ ] **C.5** **Fallback** — if SSE isn't reliable in the target Claude build, ship `poll_events(since_timestamp) → [Event]` as a second-best MCP tool. Claude Desktop polls every N seconds; still cheaper than `get_messages` polling every channel.
- [ ] **C.6** Integration test with `poly-test-discord` — subscribe, push a message via testhook, observe on the event stream within 2s.
- [ ] **C.7** Document the subscription pattern in `docs/6-ai-agent/6.1-mcp-server.md` with a worked example Claude Desktop can copy.

**Effort:** 1-2 sessions, variable with C.1 findings.

---

### Phase D — Human-appearing typing simulation

**Goal:** Claude triggers typing rhythm via MCP; Poly runs the pulse locally so the LLM isn't paying for keystrokes.

- [ ] **D.1** MCP tool API:
  ```
  start_typing_simulation(
    account_id, chat_id,
    total_duration_ms: u32,           // hard-capped to 60_000 server-side
    avg_wpm: u16,                      // 40-70 realistic; server rejects <10 or >120
    false_start_probability: f32,      // 0.0-0.3; prob per second of briefly stopping then resuming
    pause_probability: f32,            // 0.0-0.5; prob per second of a mid-sentence pause (1-3s)
    stop_on_other_typing: bool,        // if true, stop when contact starts typing
  ) -> simulation_id
  ```
  and:
  ```
  stop_typing_simulation(simulation_id) -> ok
  ```
- [ ] **D.2** Worker — tokio task per simulation. Pulses `ClientBackend::send_typing` on a backend-appropriate cadence:
  - Discord: every 8s (server times out at 10s)
  - Matrix: every 8s (typing-timeout setting in `PUT /typing/{userId}`, default 30s but we pulse sooner)
  - Stoat: matches Discord cadence
  - poly-server: use the ws.rs `send_typing` that already exists
- [ ] **D.3** Rhythm generator — gaussian-noise-ified per-second decision tree: `fn tick() → { Send, Pause, FalseStartStop }`. Pure function; unit-testable with a seeded RNG.
- [ ] **D.4** Stop triggers:
  - elapsed >= total_duration_ms → stop + notify Claude via SSE (Phase C) or via a tool return
  - `stop_typing_simulation(id)` called → cancel the task immediately
  - `stop_on_other_typing=true` AND a `TypingStarted` event arrives from the contact → cancel the task, notify Claude so it can either wait or cancel the reply
- [ ] **D.5** Per-account registry — `HashMap<simulation_id, JoinHandle>` inside `poly-chat-mcp`'s state. New simulations for the same `(account, chat)` cancel any in-flight one for that chat.
- [ ] **D.6** Bounds — hard-cap max 20 concurrent simulations per account. 21st start returns an error. (Expected upper-bound: a user driving ~10 concurrent conversations — 20 gives headroom for multi-burst scenarios.)
- [ ] **D.7** Per-chat opt-in — setting on `/agent/chat/:id` gates whether this chat even accepts `start_typing_simulation` calls. Default off.
- [ ] **D.8** Integration test against `test-discord`: start simulation, verify `/typing` endpoint is hit roughly every 8s for the configured duration; start another simulation for the same chat, verify the first is cancelled.
- [ ] **D.9** Unit tests for the rhythm generator — given a seeded RNG and parameters, output matches a golden file.

**Effort:** ~1 session.

---

### Phase E — Per-chat style / persona

**Goal:** small, structured layer over memory — formality/tone/signature that Claude honors on every reply. Optional, user-driven.

- [ ] **E.1** SQLite migration — `chat_style(account_id, chat_id, tone, formality, emoji_allowed, signature, extra_notes)` where `tone ∈ { casual, professional, snarky, warm, direct }` (free-form if none match), `formality ∈ { tu, vous, neutral }` (covers `Du/Sie`, `tu/vous`, `tu/usted`, English neutral).
- [ ] **E.2** MCP tools: `set_chat_style(...)`, `get_chat_style(account_id, chat_id)`, `list_chat_styles(account_id?)`.
- [ ] **E.3** Include the style block in the Phase A `get_reply_context` response.
- [ ] **E.4** `/agent/chat/:id` UI — style editor (dropdowns + textarea). Tiny page.
- [ ] **E.5** FTL keys for the five tone options + three formality options.

**Effort:** ~0.5 session.

---

### Phase F — Catch-me-up / digest

**Goal:** one-button "summarize overnight activity" — fully client-side via existing tools, no new MCP tools needed.

- [ ] **F.1** UI — "✨ Catch me up" button on the notifications page.
- [ ] **F.2** Click → Claude Desktop is told (via a new well-known prompt stored in KV) to call `get_reply_context` for each unread chat and compose a digest.
- [ ] **F.3** Display result in a modal; user can click into any mentioned chat.
- [ ] **F.4** Optional: a `get_all_unread(account_id)` bundler tool if manual per-chat fetching is too slow in practice.

**Effort:** ~0.5 session.

---

## Phase ordering / critical path

```
A ────┐
      ├──► B (drafts, blocked on A because DraftBanner uses memory for context in its preview)
      │
      ├──► D (typing, independent of B; can ship in parallel)
      │
      └──► C (events, independent; biggest token-cost win, ship after A so subscribers have somewhere useful to send event-triggered reactions)

E ──► (after A — small, opportunistic)
F ──► (after A — small, opportunistic)
```

**Recommended shipping order:** A → B → D → C → E → F. A is prerequisite to everything; B is the safety net the user wants before any autonomy exists; D is the anti-detection layer needed before any real-world use; C is the latency/cost optimization; E and F are polish.

---

## Effort estimate

| Phase | Sessions | Notes |
|---|---|---|
| A | 1.5 | Schema + tools + bundler + tests |
| B | 2.0 | Most UI-heavy — DraftBanner, DraftsSidebar, auto-send countdown |
| C | 1-2 | Depends on MCP SSE support in Claude Desktop (C.1) |
| D | 1.0 | Rhythm generator + per-backend pulse cadences + per-account registry |
| E | 0.5 | Small surface |
| F | 0.5 | Mostly UI glue |
| **Total** | **~7 sessions** | Less than half the previous LLM-in-Poly plan (~10 sessions) and with zero ongoing API cost |

---

## Acceptance criteria

- [ ] Pasting the MCP config into Claude Desktop exposes all Phase A-F tools
- [ ] Claude Desktop can draft a reply using `get_reply_context` and create a `draft_create` for user approval
- [ ] User sees the draft in a banner in the chat, approves it, message goes out through the backend
- [ ] Typing simulation pulses at a human cadence during an active draft — visible as a typing indicator to the other party in the test server
- [ ] Memory persists across Poly restarts — Claude can recall facts it stored in a previous session
- [ ] Default behavior with zero user configuration: nothing autonomous happens; every reply requires explicit user approval
- [ ] Per-chat toggles let the user grant auto-approval, memory access, typing simulation access individually
- [ ] No outbound HTTP request to any LLM provider from any Poly binary (grep `cargo deny` would be nice but out of scope for this plan)

---

## Privacy model

- **Default:** all auto-approve flags off; drafts require user click; no chat is "watched" by Claude unless the user toggles `agent.chat.${account_id}.${chat_id}.mcp_access = true`. Zero facts stored until Claude explicitly calls `remember_fact`.
- **Visibility:** `/agent/access` page lists every chat Claude currently has MCP access to, with a revoke button per row.
- **Nuclear option:** `/agent/access` has a "Clear all agent data" button that truncates `contact_facts`, `chat_notes`, `chat_summaries`, `chat_style`, and `drafts` in one transaction.
- **Out-of-band writes:** Claude can never write TO the user's chats directly — the only send path is `draft_create → draft_approve`, where `draft_approve` can be gated by the UI. Auto-approve exists but is off by default and visually loud when on.

---

## Explicit non-scope (this plan)

- Actual autonomous agent loops inside Poly (Phase 5.2 of the old plan — dead)
- AI-provider UI / API-key input (not needed; Claude Desktop owns provider config)
- LLM cost accounting / quota (Claude Desktop's problem, not Poly's)
- Outreach scheduling (Claude Desktop's cron / reminder system)
- Digest briefings that Poly *generates* (F uses Claude's composition, not Poly's)
- Personality loaded from TOML config files (E.1's DB row is the replacement)
- Streaming-token mid-typing UI (Claude Desktop handles its own UI; Poly just stores the finished draft)
