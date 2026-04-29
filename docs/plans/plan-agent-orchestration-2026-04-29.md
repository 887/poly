# Orchestration Plan — 2026-04-29

> Five parallel streams. Orchestrator (this agent) tracks each, dispatches
> sub-agents at appropriate model tiers, ticks checkboxes as commits land.
> Skipped: Blitz component snapshot tests (deferred per user).

## Stream A — Catch-me-up button (✅ DONE — `5041a73e`)

✨ button on chat header opens a CatchUp utility-rail panel with the last
20 messages and a "Copy summary prompt" button. No LLM call from the host.

- [x] Tasteful spot in chat header (next to threads / pinned)
- [x] FTL keys (`chat-banner-catch-me-up`, `catch-up-empty`,
      `catch-up-recent-messages`, `catch-up-copy-prompt`)
- [x] `CatchUpPanel` component renders into ChatUtilityRail
- [x] "Copy summary prompt" copies a Claude-Desktop-ready prompt to clipboard
- [x] Empty state for no messages
- [x] Commit + push (`5041a73e`)

## Stream B — Typing-simulation UI button (✅ DONE — `5041a73e`)

⌨️ toggle button in composer toolbar. While ON fires `send_typing` every
5s up to 60s. Mirrors `chat-mcp/start_typing_simulation` as a one-click
manual trigger.

- [x] Composer toolbar placement (next to emoji 😀)
- [x] FTL keys (`composer-simulate-typing`, `composer-simulate-typing-stop`)
- [x] `TypingSimulationButton` component
- [x] WASM-safe sleep via `gloo_timers::TimeoutFuture`, native via tokio
- [x] Auto-stops after 12 ticks (60s)
- [x] Commit + push (`5041a73e`)

## Stream C — Discord e2e plan + tests (✅ DONE — `8dd9210c`)

Full feature matrix + mock-Discord HTTP server + Playwright specs.

- [x] Sub-agent spawned (worktree, sonnet)
- [x] `docs/plans/plan-discord-e2e.md` — 34-feature matrix
- [x] `servers/test-discord/` — added 6 missing routes for new context-menu ops
- [x] `tests/e2e/discord/` — 4 spec files (auth, message, context-menus, group-DM)
- [x] `tests/e2e/discord/README.md` + `playwright.config.ts` `discord-api` project
- [x] Real-OAuth specs are `test.skip` unless `DISCORD_TEST_WITH_REAL_OAUTH=1`
- [x] Commit + push (`8dd9210c`)

**Punted:** UI-level Playwright tests against the running WASM app
(orchestrator follow-up). Reaction-endpoint spec scaffolding deferred
until `DiscordHttpClient` extends.

## Stream D — Phase-5 backend smoke tests (✅ DONE — `1dc378d8`)

Code-level audit across all 11 backends, 10 visual-*.md files +
visual-INDEX.md.

- [x] Sub-agent spawned (worktree, sonnet)
- [x] visual-{demo, discord, matrix, stoat, teams, forgejo, github,
      lemmy, hackernews}.md — Phase-5 audit sections
- [x] visual-INDEX.md — executive summary, 11-backend feature matrix,
      14-new-ops support table, moderation matrix
- [x] Commit + push (`1dc378d8`)

**Three biggest gaps surfaced:**
1. **Teams** — CRITICAL: WASM hard freeze on every account activation;
   blocks all Teams testing. Must fix `Signal::write()` chain in Teams
   init path before anything Teams-related can be smoke-tested.
2. **Lemmy** — HIGH: subscribed communities absent from sidebar second
   nav; DMs unsupported with no icon; per-account settings show
   wrong (Discord-style) options.
3. **Forgejo + GitHub** — HIGH: issue detail fails to load on click in
   both backends. Primary code-forge interaction broken.

## Stream E — Meta-personalities design plan (✅ DONE — `5041a73e`*)

> *file landed in 5041a73e via jj-snapshot timing; will appear in `git log`
> as part of that commit. Functionally on main.

`docs/plans/plan-meta-personalities.md` (1079 lines, 8 phases A–H).

- [x] Sub-agent spawned (worktree, opus)
- [x] Sections: Concept · Persona schema (6 SQLite tables w/ DDL) ·
      Context aggregation (`PersonaContextBuilder` + 32KB cap) ·
      MCP surface (14 tools w/ JSON schemas for top 3) ·
      UI surface (9 named components w/ file paths) ·
      Heartbeat mode · Privacy/risk · Phases A-H
- [x] Plus dependency graph, effort table, acceptance criteria,
      file-path index, risk register, glossary

**Hottest take from the design:** Putting the heartbeat scheduler in
poly-host departs from `plan-claude-desktop-agent.md`'s "no autonomous
cron in Poly" non-goal. Path: templating-summariser MVP, LLM-via-
Claude-Desktop-callback as v2. To be defended in code review.

---

## Followups now visible after all streams landed

1. **Teams WASM freeze** — fix Signal::write() chain in Teams init path.
   Until this lands, Teams is a black box. (HIGHEST priority.)
2. **Lemmy navigation** — second-nav community list, DM icon, settings
   panel.
3. **Forgejo + GitHub issue detail** — fix click → load.
4. **Discord UI Playwright tests** — currently HTTP-only, no browser
   interaction. Follow-up under `tests/e2e/discord/`.
5. **Meta-personalities Phase A** — schema + tables, no UI yet.
6. **Catch-up: AI-driven summary** — the copy-prompt button is MVP;
   Phase 2 calls the LLM directly via a host-side bridge.
7. **Typing-sim: vary intervals** — current implementation is a flat
   5s tick; Claude Desktop's `start_typing_simulation` varies.

## Status

- 2026-04-29 17:30 — Plan committed (`4fbb7c34`).
- 2026-04-29 17:32 — Streams A + B landed (`5041a73e`).
- 2026-04-29 17:32 — Stream C landed (`8dd9210c`).
- 2026-04-29 17:33 — Stream D landed (`1dc378d8`).
- 2026-04-29 17:33 — Stream E plan-file present on main (in-tree).
- 2026-04-29 17:34 — All five streams done; consolidating into one PR.
