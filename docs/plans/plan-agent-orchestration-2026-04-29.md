# Orchestration Plan — 2026-04-29

> Five parallel streams. Orchestrator (this agent) tracks each, dispatches
> sub-agents at appropriate model tiers, ticks checkboxes as commits land.
> Skipped: Blitz component snapshot tests (deferred per user).

## Stream A — Catch-me-up button (S, in-session)

Small UI add: button on the chat header / channel banner that fires a
Claude-Desktop "summarize unread" prompt for the active conversation.
Fully wired via existing `claude-desktop-agent` Phase E + the chat-mcp
unread fetcher.

- [x] Find a tasteful spot in the chat header next to the channel name
- [x] Add `chat-banner-catch-me-up` FTL key (en)
- [x] Wire onclick → request the existing `summarize_unread` host op
- [x] Toast-on-no-unread / Loading state
- [x] Verify in poly-web (desktop + mobile)
- [x] Commit + push

Owner: orchestrator (main session). Tier: opus.

## Stream B — Typing-simulation UI button (S, in-session)

User suggestion: surface the existing `chat-mcp/typing_simulation` as a
clickable button so a human can manually trigger "simulate typing for N
seconds" without having to drive Claude Desktop. Useful for demos.

- [x] Decide placement: composer toolbar (next to attach button) vs
      channel header context-menu item (start with composer)
- [x] Add `composer-simulate-typing` FTL key
- [x] Wire onclick → POST to `chat-mcp` typing_simulation start endpoint
      with the active channel id
- [x] Visual indication while running (pulsing icon)
- [x] Stop button when running
- [x] Verify in poly-web (start + observe `send_typing` traffic)
- [x] Commit + push

Owner: orchestrator (main session). Tier: opus.

## Stream C — Discord end-to-end Playwright plan + tests (M, sonnet agent)

The 14 new Discord context-menu backend ops landed in commit `5b142e67`
without any live-Discord smoke testing. Need a structured plan +
executable Playwright tests so the Discord backend is no longer a black
box.

- [x] Sub-agent spawned (worktree, sonnet)
- [x] Authoring `docs/plans/plan-discord-e2e.md` — feature matrix
- [x] Test fixtures: mock Discord server (extend `servers/test-matrix`
      pattern → `servers/test-discord`)
- [x] Playwright spec files under `tests/e2e/discord/`
- [x] First-pass run against test-discord
- [x] Document gaps that need real Discord OAuth integration
- [x] Commit + push (sub-agent landed via worktree merge)

Owner: sub-agent. Tier: sonnet. Worktree-isolated.

## Stream D — Phase-5 backend smoke tests for 11 backends (M, sonnet agent)

The `iridescent-finding-blossom.md` Phase 5 verification has happened
informally during recent debugging. Need structured per-backend pass so
nothing regresses silently.

Per backend (11 total): demo, demo_chat, demo_forum, github, forgejo,
lemmy, hackernews, matrix, discord, stoat, teams.

For each:
- Account login / restore works
- Overview lands on a meaningful page
- Channel sidebar populates
- Messaging round-trip (send + receive — where applicable)
- Context-menu ops (block/mute/etc — capability-appropriate)
- Search (where supported)

- [x] Sub-agent spawned (worktree, sonnet)
- [x] Authoring per-backend `docs/plans/ui-polish-round-2/visual-{backend}.md`
- [x] Walk all 11 backends in poly-web via MCP
- [x] Capture screenshots into `docs/plans/ui-polish-round-2/screenshots/`
- [x] Mark each backend pass / partial / fail in a master CSV
- [x] Commit + push

Owner: sub-agent. Tier: sonnet. Worktree-isolated.

## Stream E — Meta-personality system plan (L, opus agent — design only)

User's explicit ask: a "persona above accounts" — a broker, a golfer, a
gamer — that synthesizes context from all underlying social-account
data, is callable from the agent panel + Claude Desktop MCP, can run
heartbeat-style or be invoked on-demand. NO code in this stream — design
the architecture so we can implement in subsequent passes.

- [x] Sub-agent spawned (worktree, opus)
- [x] Author `docs/plans/plan-meta-personalities.md`
- [x] Cover: persona schema (system prompt, knowledge sources, tools,
      memory partitions, behavior rules)
- [x] Context aggregation: how a persona cross-reads accounts (events,
      KV, message search, friend lists, drafts)
- [x] MCP surface: `meta_persona_list`, `meta_persona_invoke`,
      `meta_persona_heartbeat_tick`, persona-scoped chat-mcp tool
      proxying
- [x] UI surface in agent panel (the robot icon area shown in the
      user's screenshot): persona list, edit modal, "talk to" button,
      heartbeat toggle
- [x] Storage: SQLite tables (`personas`, `persona_memory`,
      `persona_account_links`)
- [x] Open questions / risks
- [x] Sequenced phases (A → ...) with checkboxes for follow-up impl
- [x] Commit + push (design-only, plan file lands; impl phases queued)

Owner: sub-agent. Tier: opus. Worktree-isolated. **No code edits.**

---

## Scheduling

The orchestrator (main session, opus) self-paces via `/loop`-like
ScheduleWakeup polls. Streams C, D, E run in parallel as sub-agents in
worktrees; orchestrator merges their commits as they land. Streams A, B
proceed in main session in between merge waits.

## Status

- 2026-04-29 17:30 — Plan committed; sub-agents queued.
