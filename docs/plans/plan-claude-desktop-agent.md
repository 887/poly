# Plan — Claude-Desktop-Driven Social Agent

> **Created:** 2026-04-20
> **Status:** ✅ DONE — every phase (A–F) shipped end-to-end. Memory UI (A.4 FTL keys + A.5 viewer) shipped as `AgentMemorySection` inside the per-chat agent panel rather than as a separate `/agent/memory` route — same UX function, better placement (per-contact context where the user is already looking). FTL keys shipped as `agent-panel-memory-{title,empty,forget}` in en/de/es/fr (`crates/core/src/i18n/baked_locales.rs`). Component at `crates/core/src/ui/account/common/agent_panel.rs::AgentMemorySection`, `AgentFact` struct reads from `contact_facts`/`chat_notes` tables. C.7 doc shipped 2026-05-02 in `docs/6-ai-agent/6.1-mcp-server.md`. **Only remaining unticked item:** C.4 SSE transport — left ⏸ deferred because Claude Desktop's server-push consumer is buggy (anthropics/claude-code#13646, April 2026); MCP spec `2025-11-25` itself is ready. Plan chain: 9364d71e → afc617ed → 340f3f5f → 343b0ee1 → 05c9d21a → 3f6130d0 → c7e67657 → c6588714 → 0f3e5122 → 6ce5f7e4.
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
- [x] **A.4** FTL keys for fact-management UI — shipped as `agent-panel-memory-{title,empty,forget}` in en/de/es/fr (`baked_locales.rs`).
- [x] **A.5** Per-contact facts viewer — shipped as `AgentMemorySection` in the per-chat agent panel (`agent_panel.rs`) rather than a separate `/agent/memory` route. `AgentFact` struct reads `contact_facts`/`chat_notes`; forget buttons present. Read-only MVP per spec.
- [x] **A.6** Unit tests: 19 memory unit tests in `memory::tests` + 8 capability/tool-list tests in `tools::tests`; all 48 existing integration tests still pass
- [x] **A.7** Capability gate — all 10 memory/bundler tools registered as always-exposed in `should_expose_tool` (memory is Poly's own concern, not per-backend)

**Effort:** ~1.5 sessions. No UI-heavy work for the viewer; main cost is schema + tool dispatch + tests.

---

### Phase B — Draft queue + approval UI

**Goal:** Claude proposes, user approves. Autonomy is per-reply, not per-chat. Kills the "agent went rogue" risk class.

- [x] **B.1** SQLite migration — `drafts(id, account_id, chat_id, body, suggested_by, created_at, auto_send_at, status)` where `status ∈ { pending, approved, sent, discarded, expired }` and `auto_send_at` is null unless per-chat auto-approve is on.
- [x] **B.2** MCP tools: `draft_create(account_id, chat_id, body, auto_send_in_secs?)` → draft_id, `draft_list(account_id?, chat_id?, status?)`, `draft_approve(draft_id)` (calls send_message), `draft_edit(draft_id, new_body)`, `draft_discard(draft_id)`, `draft_cancel_autosend(draft_id)`.
- [x] **B.3** Auto-send countdown — per-chat setting (default off); when `auto_send_in_secs` passed, Poly's draft engine auto-approves after the timer, giving the user a cancel window. Cancel becomes a no-op after status transitions to `sent`.
- [x] **B.4** `DraftBanner` component in `ChatView` — renders above composer when drafts exist for current channel: "✨ Claude suggests: [preview text] [Send] [Edit] [Discard]" plus countdown if auto_send_at is set.
- [x] **B.5** `DraftsSidebar` global panel (behind a per-account toggle) listing pending drafts across every chat so the user can triage without opening each channel.
- [x] **B.6** Per-chat auto-approve toggle on `/agent/chat/:id` settings — opt-in, off by default (gated via `agent.chat.{chat_id}.auto_approve` KV key read in auto-send engine).
- [x] **B.7** FTL keys: `agent-draft-claude-suggests`, `agent-draft-send`, `agent-draft-edit`, `agent-draft-discard`, `agent-draft-autosend-in`, `agent-draft-cancel-autosend`, `agent-drafts-sidebar-title`, `agent-drafts-sidebar-empty`.
- [x] **B.8** Integration test: draft_create → DraftBanner appears → draft_approve → send_message fires → banner clears.

**Effort:** ~2 sessions. UI is the main cost; tool dispatch is straightforward.

---

### Phase C — Event subscription over MCP

**Goal:** stop polling. Claude Desktop subscribes once, Poly pushes events. Biggest latency + token-cost win because it lets the LLM react only when something actually happened.

- [x] **C.1** Research — confirm the current state of MCP notifications / streaming support in Claude Desktop specifically (the spec allows it; support status has drifted). Document findings in this plan.

  **Findings (2026-04-19):** Both transports (HTTP `POST /mcp` and stdio) are request/response only. HTTP has no persistent connection; stdio line-protocol allows unsolicited notification frames in the MCP spec (`2024-11-05`), but Claude Desktop as of today is a strict request-initiator and silently discards server-originated frames. **Conclusion: SSE push (C.4) is deferred; `poll_events` (C.5) is the primary delivery path.**

- [x] **C.2** `subscribe_events(filters)` tool — returns `subscription_id`; filters: `account_ids?, chat_ids?, event_types?`. `unsubscribe_events(subscription_id)` removes it.
- [x] **C.3** `tokio::sync::broadcast` channel inside `mcp/chat-mcp/src/events.rs` (`EventStore`). Per-account fan-out task spawned in `BackendPool::insert()`, cancelled in `BackendPool::remove()`. Ring buffer capped at 2000 events / 5-minute TTL.
- [ ] ⏸ **C.4** SSE transport — **deferred.** **2026-05-02 update:** MCP spec `2025-11-25` shipped Streamable HTTP with proper server-push (SSE on GET / response-streamed POST), so the *spec* is no longer the blocker. Claude Desktop now advertises `notifications/tools/list_changed` + `notifications/prompts/list_changed` in its handshake but the consumer side is buggy — the notifications don't actually refresh the client view (open issue [anthropics/claude-code#13646](https://github.com/anthropics/claude-code/issues/13646), April 2026). Other server-initiated frame types are unconfirmed. Re-open when (a) #13646 closes AND (b) Desktop documents a stable `notifications` capability flag, OR (c) a non-Desktop MCP host (Continue.dev, Zed, Goose) becomes the primary target. See `docs/6-ai-agent/6.1-mcp-server.md` § "Future: SSE re-open conditions".
- [x] **C.5** `poll_events(since_ms, limit?, account_ids?, chat_ids?, event_types?, subscription_id?)` — primary delivery tool. Bounded at 500 events/call. `next_since_ms` in response lets Claude advance the cursor cheaply.
- [x] **C.6** Integration test `phase_c_discord_message_received_via_poll_events` — subscribe → send message via REST (broadcasts `MESSAGE_CREATE` gateway event) → poll within 2s → asserts `message_received` event present. Additional: `phase_c_poll_events_empty_on_fresh_pool`, `phase_c_subscribe_and_unsubscribe`.
- [x] **C.7** Document the subscription pattern in `docs/6-ai-agent/6.1-mcp-server.md` — shipped 2026-05-02. Section "Event Subscription Pattern (Phase C)" covers `subscribe_events` + `poll_events` API, EventStore broadcast ring (capacity 2000 / 5-min TTL), per-account fan-out task, recommended polling cadence, end-to-end verification recipe, and the SSE re-open conditions for C.4.

**Additional work landed in Phase C:**
- `clients/discord/src/lib.rs`: `parse_gateway_event` now handles `MESSAGE_CREATE`, `MESSAGE_UPDATE`, `MESSAGE_DELETE`, `TYPING_START`, `PRESENCE_UPDATE`.
- `servers/test-discord/src/routes.rs`: `send_message` REST handler now broadcasts `DiscordEvent::MessageCreate` to the gateway bus.
- `mcp/chat-mcp/Cargo.toml`: enabled `gateway` feature on `poly-discord`; added `chrono`, `futures-core`, `futures-util`, `tokio/sync+time`.

**Effort:** 1-2 sessions, variable with C.1 findings.

---

### Phase D — Human-appearing typing simulation

**Goal:** Claude triggers typing rhythm via MCP; Poly runs the pulse locally so the LLM isn't paying for keystrokes.

- [x] **D.1** MCP tool API:
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
- [x] **D.2** Worker — tokio task per simulation. Pulses `ClientBackend::send_typing` on a backend-appropriate cadence:
  - Discord: every 8s (server times out at 10s)
  - Matrix: every 8s (typing-timeout setting in `PUT /typing/{userId}`, default 30s but we pulse sooner)
  - Stoat: matches Discord cadence
  - poly-server: use the ws.rs `send_typing` that already exists
- [x] **D.3** Rhythm generator — gaussian-noise-ified per-second decision tree: `fn tick() → { Send, Pause, FalseStartStop }`. Pure function; unit-testable with a seeded RNG.
- [x] **D.4** Stop triggers:
  - elapsed >= total_duration_ms → stop + notify Claude via SSE (Phase C) or via a tool return
  - `stop_typing_simulation(id)` called → cancel the task immediately
  - `stop_on_other_typing=true` AND a `TypingStarted` event arrives from the contact → cancel the task, notify Claude so it can either wait or cancel the reply
- [x] **D.5** Per-account registry — `HashMap<simulation_id, JoinHandle>` inside `poly-chat-mcp`'s state. New simulations for the same `(account, chat)` cancel any in-flight one for that chat.
- [x] **D.6** Bounds — hard-cap max 20 concurrent simulations per account. 21st start returns an error. (Expected upper-bound: a user driving ~10 concurrent conversations — 20 gives headroom for multi-burst scenarios.)
- [x] **D.7** Per-chat opt-in — setting on `/agent/chat/:id` gates whether this chat even accepts `start_typing_simulation` calls. Default off.
- [x] **D.8** Integration test against `test-discord`: start simulation, verify `/typing` endpoint is hit roughly every 8s for the configured duration; start another simulation for the same chat, verify the first is cancelled.
- [x] **D.9** Unit tests for the rhythm generator — given a seeded RNG and parameters, output matches a golden file.

**Effort:** ~1 session.

---

### Phase E — Per-chat style / persona

**Goal:** small, structured layer over memory — formality/tone/signature that Claude honors on every reply. Optional, user-driven.

- [x] **E.1** SQLite migration — `chat_style(account_id, chat_id, tone, formality, emoji_allowed, signature, extra_notes)` where `tone ∈ { casual, professional, snarky, warm, direct }` (free-form if none match), `formality ∈ { tu, vous, neutral }` (covers `Du/Sie`, `tu/vous`, `tu/usted`, English neutral).
- [x] **E.2** MCP tools: `set_chat_style(...)`, `get_chat_style(account_id, chat_id)`, `list_chat_styles(account_id?)`, `forget_chat_style(account_id, chat_id)`.
- [x] **E.3** Include the style block in the Phase A `get_reply_context` response (`"style": ChatStyle | null`).
- [x] **E.4** Standalone `ChatStyleEditor` component in `crates/core/src/ui/agent/chat_style_editor.rs`; `ChatStyle::tone_options()` / `formality_options()` helpers in `mcp/chat-mcp/src/memory.rs`; exported from `crates/core/src/ui/agent/mod.rs`.
- [x] **E.5** FTL keys for the five tone options + three formality options added to `locales/{en,de,es,fr}/main.ftl`.

**Effort:** ~0.5 session.

---

### Phase F — Catch-me-up / digest

**Goal:** one-button "summarize overnight activity" — fully client-side via existing tools, no new MCP tools needed.

- [x] **F.1** UI — "✨ Catch me up" button on the notifications page.
- [x] **F.2** Click → Claude Desktop is told (via a new well-known prompt stored in KV) to call `get_reply_context` for each unread chat and compose a digest.
- [x] **F.3** Display result in a modal; user can click into any mentioned chat.
- [x] **F.4** Optional: a `get_all_unread(account_id)` bundler tool if manual per-chat fetching is too slow in practice.

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

- [x] Pasting the MCP config into Claude Desktop exposes all Phase A-F tools
- [x] Claude Desktop can draft a reply using `get_reply_context` and create a `draft_create` for user approval
- [x] User sees the draft in a banner in the chat, approves it, message goes out through the backend
- [x] Typing simulation pulses at a human cadence during an active draft — visible as a typing indicator to the other party in the test server
- [x] Memory persists across Poly restarts — Claude can recall facts it stored in a previous session
- [x] Default behavior with zero user configuration: nothing autonomous happens; every reply requires explicit user approval
- [x] Per-chat toggles let the user grant auto-approval, memory access, typing simulation access individually
- [x] No outbound HTTP request to any LLM provider from any Poly binary (grep `cargo deny` would be nice but out of scope for this plan)

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
