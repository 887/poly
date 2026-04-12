# Poly Documentation Index

> Last updated: 2026-04-12
> For project overview: [0.1-vision-and-roadmap.md](0-project/0.1-vision-and-roadmap.md)

---

## 0 — Project

| # | Document | Status | Description |
|---|---|---|---|
| 0.0a | [Original Project Brief](0-project/0.0-original-project-brief.md) | DONE | Unedited brainstorm from 2026-02-28 |
| 0.0b | [Original Prompt](0-project/0.0-original-prompt.md) | DONE | Initial prompt that kicked off the project |
| 0.1 | [Vision and Roadmap](0-project/0.1-vision-and-roadmap.md) | DONE | Core goals, phased roadmap summary |
| 0.2 | [Technology Stack](0-project/0.2-technology-stack.md) | DONE | Dioxus, WASM, crypto, platform shell choices |
| 0.3 | [Work Plan](0-project/0.3-work-plan.md) | IN-PROGRESS | Active task list for Phase 3 clients + Phase 5 CLI |

---

## 1 — Architecture

| # | Document | Status | Description |
|---|---|---|---|
| 1.0 | [Overview](1-architecture/1.0-overview.md) | DONE | Monorepo structure, ClientBackend trait, UI dispatch |
| 1.1 | [WASM Plugin System](1-architecture/1.1-wasm-plugin-system.md) | DONE | WIT interface, wasmtime, plugin loading, sandboxing |
| 1.2 | [Host Bridge](1-architecture/1.2-host-bridge.md) | DONE | `/host/*` routing, fullstack architecture, SQLite storage |
| 1.3 | [Storage and Crypto](1-architecture/1.3-storage-and-crypto.md) | DONE | SQLite, IndexedDB, Ed25519 identity, encrypted backup |

---

## 2 — Clients

| # | Document | Status | Description |
|---|---|---|---|
| 2.0 | [Client Trait](2-clients/2.0-client-trait.md) | DONE | `ClientBackend` trait, shared types, WIT correspondence |
| 2.1 | [Demo](2-clients/2.1-demo.md) | DONE | Mock backend for UI development |
| 2.2 | [Stoat](2-clients/2.2-stoat.md) | IN-PROGRESS | Stoat/Revolt REST+WS client — core flow working |
| 2.3 | [Poly Server](2-clients/2.3-poly-server.md) | DONE | First-party poly-server client |
| 2.4 | [Matrix](2-clients/2.4-matrix.md) | IN-PROGRESS | matrix-sdk wrapper — stub, not yet functional |
| 2.5 | [Hacker News](2-clients/2.5-hackernews.md) | DONE | Read-only HN Firebase API, forum model |
| 2.6 | [Lemmy](2-clients/2.6-lemmy.md) | DONE | Lemmy REST API v3, federated forum |
| 2.7 | [GitHub](2-clients/2.7-github.md) | DONE | GitHub Issues/PRs/notifications |
| 2.8 | [Discord](2-clients/2.8-discord.md) | TBD | TOS risk — approach not yet decided |
| 2.9 | [Teams](2-clients/2.9-teams.md) | TBD | Microsoft Graph API — stub |
| 2.10 | [Client Backends Research](2-clients/2.10-client-backends-research.md) | DONE | Research notes from Phase 1 |

---

## 3 — Platforms

| # | Document | Status | Description |
|---|---|---|---|
| 3.0 | [Platform Overview](3-platforms/3.0-platform-overview.md) | DONE | Port table, shell architecture, fullstack pattern |
| 3.1 | [Web](3-platforms/3.1-web.md) | DONE | Chrome/Chromium, dx serve fullstack, CDP on 9222 |
| 3.2 | [Desktop Wry](3-platforms/3.2-desktop-wry.md) | DONE | Wry/WebKit2GTK, eval bridge on 9223 |
| 3.3 | [Desktop Electron](3-platforms/3.3-desktop-electron.md) | DONE | Electron thin shell, CDP on 9224 |
| 3.4 | [Mobile iOS](3-platforms/3.4-mobile-ios.md) | TBD | AOT WASM, Dioxus iOS target |
| 3.5 | [Mobile Android](3-platforms/3.5-mobile-android.md) | TBD | JIT WASM, Dioxus Android target |

---

## 4 — UI

| # | Document | Status | Description |
|---|---|---|---|
| 4.0 | [Component Architecture](4-ui/4.0-component-architecture.md) | DONE | Dioxus signals, RSX, theming, i18n, layout structure |
| 4.1 | [Chat Scroll and History](4-ui/4.1-chat-scroll-and-history.md) | DONE | Column-reverse layout, infinite scroll, position memory |
| 4.2 | [Forum Channels](4-ui/4.2-forum-channels.md) | IN-PROGRESS | Forum channel type for Lemmy, HN, Discord forums |
| 4.3 | [Mobile Layout](4-ui/4.3-mobile-layout.md) | IN-PROGRESS | Three-pane collapse to swipeable pages |

---

## 5 — Testing

| # | Document | Status | Description |
|---|---|---|---|
| 5.0 | [Test Architecture](5-testing/5.0-test-architecture.md) | DONE | Test layers, lifecycle endpoints, animal accounts |
| 5.1 | [Test Servers](5-testing/5.1-test-servers.md) | DONE | All 7 mock server stubs, EventBus, per-backend APIs |
| 5.2 | [MCP Devtools](5-testing/5.2-mcp-devtools.md) | DONE | poly-web/electron/desktop MCPs, tools, workflow |
| 5.3 | [Web Devtools Troubleshooting](5-testing/5.3-web-devtools-troubleshooting.md) | DONE | Common issues, port conflicts, cleanup procedures |

---

## 6 — AI Agent

| # | Document | Status | Description |
|---|---|---|---|
| 6.0 | [Social Agent Vision](6-ai-agent/6.0-social-agent-vision.md) | IN-PROGRESS | Per-chat AI persona, memory, approval flow, architecture |
| 6.1 | [MCP Chat Server](6-ai-agent/6.1-mcp-server.md) | IN-PROGRESS | `poly-chat-mcp` tools, verified E2E flows |
| 6.2 | [poly-cli](6-ai-agent/6.2-poly-cli.md) | IN-PROGRESS | Dynamic CLI client for the chat MCP |
| 6.3 | [Memory MCP](6-ai-agent/6.3-memory-mcp.md) | IN-PROGRESS | Project-meta memory and task tracking for AI agents |

---

## 7 — Infrastructure

| # | Document | Status | Description |
|---|---|---|---|
| 7.0 | [Poly Server Protocol](7-infrastructure/7.0-poly-server-protocol.md) | DONE | REST + WebSocket protocol spec |
| 7.1 | [Backup Server](7-infrastructure/7.1-backup-server.md) | DONE | Encrypted settings sync server — PoW auth, blob storage |

---

## Archive

Historical phase plans (reference only, not maintained):

- `archive/phases/` — all `phase-*.md` implementation plans from Phase 1 through 5

---

## Root-Level Docs (Not Moved)

| File | Purpose |
|---|---|
| `README.md` | Project setup and getting started |
| `CLAUDE.md` | AI agent instructions and project context |
| `TEST_HARNESS.md` | Standard test procedure (run via haiku subagent) |
| `agents.md` | Root agent rules |
