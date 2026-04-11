# Poly — Your AI-Powered Social Layer

A cross-platform messenger that unifies all your chat accounts — Discord, Matrix, Stoat, Teams, self-hosted servers — into a single app with an AI agent that remembers, responds, and manages your social life. Built in **Rust** with **Dioxus 0.7.3**, powered by **WASM Component Model** plugins and **SurrealDB 3.0.x**.

**Status (2026-04-03):** WASM Component Model plugins integrated. All 6 messenger backends compile to WebAssembly artifacts. Plugin host extracted to dynamically-linked crate (D22). 77 E2E tests passing. Chat UI fast — infinite scroll, scroll position memory, view-anchor restore. Matrix native client implemented (custom HTTP, no matrix-sdk). Phase 5 (Social Agent) planned.

---

## 🎯 Vision

Poly is not just a multi-client chat app. It's a **personal AI-powered social agent** that sits on top of all your messaging platforms.

**The problem:** Your conversations are scattered across 5+ apps. You miss messages, forget what people told you, lose links, and can't keep up with every chat. Switching between Discord, Teams, Matrix, and Stoat wastes hours.

**Poly solves this in two layers:**

### Layer 1 — Unified Chat UI
All your accounts from every platform in one app. One sidebar, one message view, one notification stream. Switch between your Discord server and your Matrix Space without switching windows.

### Layer 2 — Social Agent
An AI agent that connects to all your chat backends via MCP and acts as your external social memory:

- **Catches you up** — "What did I miss in #general?" gets a 3-sentence summary, not 200 messages to scroll
- **Remembers everything** — Who said what, when, what links were shared, what you promised, what they asked for
- **Responds as you** — With your tone, your humor, per-person personality, learned from how you actually write
- **Keeps relationships alive** — "Message Alice about something interesting every week" runs in the background
- **Finds anything** — "What was that article Bob sent about WebRTC 2 months ago?" — instant answer across all backends
- **Prioritizes** — VIP contacts surface immediately, noise batches into a daily digest

The agent shares your live connections — no duplicate logins. You can switch between responding yourself and letting the agent handle it, per-chat, at any time. Bring your own AI provider (Claude, GPT, Gemini, Ollama).

Nothing like this exists as a product today.

---

## 🏗️ Architecture

### WASM Plugin Architecture (DECISION D21)

As of **2026-03-06**, all messenger backend implementations are compiled to **WebAssembly Component Model** plugins (not embedded directly in the app).

**Why WASM?**
- **App store compliance**: No hardcoded API keys, no library licensing conflicts → app stores approve distribution
- **Modular updates**: Update individual backends without rebuilding the entire app
- **Offline-first**: Clients load from disk cache → instant startup
- **Native + Web**: Same WASM binaries run on desktop (via Wasmtime), Android, iOS, and web browsers
- **Security boundary**: Guest code (backend) isolated from host (UI) via component model

### Build Status

All 6 client plugins successfully compile to WASM + native:

| Backend | Size (Debug) | Coverage | Build Status |
|---------|------------|----------|--------------|
| **poly_demo.wasm** | 37 MB | Account/channel/message/mention UI | ✅ Fully functional |
| **poly_stoat.wasm** | 4.3 MB | Revolt API client ↔ Poly types | 🔄 Phase 3.1 (stub) |
| **poly_matrix.wasm** | 4.3 MB | Matrix SDK integration | 🔄 Phase 3.2 (stub) |
| **poly_discord.wasm** | 4.3 MB | Discord API client ↔ Poly types | 🔄 Phase 3.3 (stub) |
| **poly_teams.wasm** | 4.3 MB | Microsoft Teams API client | 🔄 Phase 3.4 (stub) |
| **poly_server_client.wasm** | 4.2 MB | Poly server protocol client | ⚠️ Stub (host-api only) |

**Total artifact size:** 58.7 MB (debug, unoptimized)

---

## 🔌 Plugin System

### Component Model Integration

Each backend plugin exports standard WIT (WebAssembly Interface Types):

```wit
// From crates/core/src/wit/messenger-client.wit
package poly:messenger

interface messenger-client {
  enum backend-type {
    demo,
    stoat,
    matrix,
    discord,
    teams,
    poly,
  }

  get-backend-type() -> backend-type
  get-backend-name() -> string
  create-session(...) -> session
  // ... full API defined in WIT
}
```

### Loading & Verification

At startup, **poly-core** loads all available .wasm files from disk:

1. **Registry** (`crates/plugin-host/src/registry.rs`):
   - Scans for `*.wasm` files in plugin directory
   - For each file, instantiates wasmtime component
   - **Calls WIT exports** `get_backend_type()` and `get_backend_name()` to verify identity
   - Caches results for fast lookup

2. **Dynamic Linking (DECISION D22)**:
   - Plugin host lives in `poly-plugin-host` (`crate-type = ["dylib"]`)
   - wasmtime isolated behind dynamic linking boundary — poly-core changes never recompile it
   - `poly-core` re-exports via `pub use poly_plugin_host as plugin_host`

3. **Integration + E2E Tests** (verified 2026-03-06):
   ```bash
   # Integration test — loads all 6 plugins, verifies types + names:
   cargo test -p poly-plugin-loader-tests --test integration -- --nocapture

   # Full E2E client interface tests (77 tests across all 6 clients):
   cargo test -p poly-plugin-loader-tests --all-features -- --nocapture
   ```
   **Result:** ✅ 77 tests passing — 26 demo E2E + 50 stub verification + 1 integration

4. **Runtime Lifecycle**:
   - User selects backend (Discord, Stoat, etc.) → registry loads matching .wasm
   - Guest instantiates session object with user credentials
   - UI calls guest functions via WIT bindings
   - Async tasks call backend-specific API clients
   - Messages flow through `PolyMessage` type → unified UI rendering

---

## 📂 Project Structure

```
poly/
├── clients/              # ⭐ 6 backend implementations (all WASM)
│   ├── client/           # poly-client: WIT type definitions (source of truth)
│   ├── demo/             # poly-demo: Fully functional example backend
│   ├── stoat/            # poly-stoat: Revolt API client (Phase 3.1)
│   ├── matrix/           # poly-matrix: Matrix SDK (Phase 3.2)
│   ├── discord/          # poly-discord: Discord API (Phase 3.3)
│   ├── teams/            # poly-teams: Teams API (Phase 3.4)
│   └── server-client/    # poly-server-client: Poly server protocol (stub)
│
├── apps/                 # Platform entry points
│   ├── desktop/          # Wry desktop (WebView)
│   ├── desktop-blitz/    # WGPU native GPU rendering (experimental)
│   ├── desktop-electron/ # Electron + WASM
│   ├── web/              # Browser (Axum + Dioxus fullstack)
│   ├── android/          # Mobile (Dioxus native)
│   ├── ios/              # Mobile (Dioxus native)
│   └── desktop-devtools/ # UI debugging tool
│
├── servers/
│   ├── server/           # Poly sync/backup server (Axum)
│   ├── backup-server/    # Encrypted backup service
│   └── test-*/           # Mock test servers per backend (Phase 4)
│
├── mcp/                  # Model Context Protocol servers
│   ├── desktop-devtools-mcp/  # Desktop DevTools
│   ├── electron-devtools-mcp/ # Electron DevTools
│   ├── web-devtools-mcp/      # Web DevTools
│   └── social-agent-mcp/     # Social Agent MCP (Phase 5)
│
└── crates/
    ├── core/             # poly-core: UI, routing, plugin host runtime
    ├── social-agent/     # AI agent: personality, memory, pipeline (Phase 5)
    └── [other support]
```

---

## 🚀 Getting Started

### Build All WASM Plugins

```bash
# Desktop app (uses loaded WASM)
cd apps/desktop
dx serve --hotpatch

# From another terminal, build all backend plugins:
cd crates/core
cargo component build -p poly-demo --target wasm32-wasip2
cargo component build -p poly-stoat --target wasm32-wasip2
# ... etc

# Output files appear in target/wasm32-wasip1/debug/
```

### Run Tests

```bash
# Integration test — load all 6 plugins:
cargo test -p poly-plugin-loader-tests --test integration -- --nocapture

# Demo E2E tests (26 tests — full ClientBackend exercised):
cargo test -p poly-plugin-loader-tests --features test-demo --test client_e2e -- --nocapture

# ALL E2E tests (77 tests across all 6 clients):
cargo test -p poly-plugin-loader-tests --all-features -- --nocapture
```

### Build for Distribution

```bash
# Release builds (optimized)
cargo component build -p poly-demo --target wasm32-wasip2 --release
# Output: target/wasm32-wasip1/release/poly_demo.wasm (smaller)

# Desktop app release
cd apps/desktop
dx build --platform desktop --release

# Web app — production build (strips dev-only backends, see below)
cd apps/web
dx build --platform web --release --no-default-features --features production
```

### Dev-only backends — Discord & Microsoft Teams

The Discord and Microsoft Teams crates live in this repository (`clients/discord`,
`clients/teams`) but are **never shipped in release builds**. They are gated behind
the `dev-plugins` cargo feature in `apps/web`, which is on by default for local
development and must be explicitly disabled for production:

```bash
# Dev (default — discord + teams compiled in, surfaced under "WASM plugins" in settings)
cd apps/web && dx serve --platform web

# Production (no discord, no teams — anything app-store review touches)
cd apps/web && dx build --platform web --release \
    --no-default-features --features production
```

**Why?**

- **App store review**: Both Discord's and Microsoft's terms of service prohibit
  third-party clients in ways that would block iOS App Store / Google Play approval.
  Shipping either backend in a binary submitted for review is a non-starter.
- **TOS surface**: Even though the source is MIT-licensed and lives publicly in this
  repo (open source code is open source — undistributed source is fine), we don't
  want a release artifact that contains anything Discord/Teams-shaped to point at.
- **Visibility in dev**: When `dev-plugins` is on, both backends appear in
  `Settings → Plugins` under the **WASM plugins** section (not "Built-in") with a
  `(dev)` suffix, so it stays visually obvious they are not first-party Poly
  backends. They are loaded as if they were external WASM plugins a user dropped in.

If you fork Poly and want to ship Discord/Teams support, that's your call — flip
`default = ["dev-plugins"]` back on for the release build and accept the review
risk yourself.

---

## 📋 Checklist — WASM Implementation (DECISION D21, Last Updated: 2026-03-06)

- [x] **Step 1-9**: Research, architecture design, toolchain setup
- [x] **Step 10**: All 6 client plugins compile to WASM
- [x] **Step 11**: Integration test written and passing
- [x] **Step 12**: Backend type/name self-reporting via WIT exports
- [x] **Step 13**: Updated all agents.md files with WASM architecture
- [x] **Step 14**: Updated all README files with build instructions
- [x] **Step 15**: Plugin host extracted to `poly-plugin-host` dylib (D22)
- [x] **Step 16**: E2E client interface tests — 77 tests across all 6 clients (2.14.16)
- [ ] **Step 17**: Implement server-client real backend (host-api bindings)
- [ ] **Step 18**: Phase 3 implementation sprint (Stoat, Matrix, Discord, Teams backends)
- [ ] **Step 19**: Optimize WASM output size (release builds, LTO)
- [ ] **Step 20**: Phase 4 — Mock test servers for all backends (manual UI + E2E testing)
- [ ] **Step 21**: Phase 5 — Social Agent MCP server (AI responses, memory, personality, scheduling)

---

## 🔑 Key Files & Documentation

### Core Plugin System

| File | Purpose |
|------|---------|
| `wit/messenger-plugin.wit` | WIT interface definitions (types, functions) |
| `crates/plugin-host/` | WASM plugin host runtime (wasmtime, dynamic linking) |
| `crates/plugin-host/src/registry.rs` | Plugin loading, instantiation, caching |
| `crates/plugin-host/src/bridge.rs` | WIT ↔ Rust type conversions |
| `crates/plugin-host-tests/` | 77 integration + E2E tests (feature-flagged per client) |

### Backend Implementations

Each client crate follows the same structure:

| File | Purpose |
|------|---------|
| `clients/X/agents.md` | Crate-specific architecture, build commands, phase notes |
| `clients/X/README.md` | Quick start, build instructions, current status |
| `clients/X/Cargo.toml` | Dual crate-type: `["cdylib", "rlib"]` for WASM + native |
| `clients/X/src/guest.rs` | WIT interface implementation (WASM only) |
| `clients/X/src/lib.rs` | Native client API (Discord/Matrix SDK, etc.) |

### Project-Level Planning

| Doc | Purpose |
|-----|---------|
| `docs/overall-plan.md` | Comprehensive plan + decision registry |
| `docs/phase-2.14-plan.md` | WASM plugin system (D21) + dylib extraction (D22) + E2E tests |
| `docs/phase-3-plan.md` | Backend implementation roadmap (Phases 3.1–3.4) |
| `docs/phase-3.2-matrix-plan.md` | Matrix client — custom HTTP, no matrix-sdk |
| `docs/phase-4-test-servers-plan.md` | Mock test servers for all backends (manual + E2E testing) |
| `docs/phase-5-social-agent-plan.md` | Social Agent — MCP server, AI responses, memory, personality, scheduling |

---

## 💬 Chat UI

The chat view uses a **column-reverse CSS layout** (newest messages at bottom, `scrollTop=0`). All scroll logic accounts for negative scrollTop values.

### Features

| Feature | Description |
|---------|-------------|
| **Infinite scroll (older)** | Scroll to the top edge loads older messages in 50-message pages, up to 200 messages in memory |
| **Infinite scroll (newer)** | Scroll to the bottom edge chain-loads newer pages (up to 20 pages per burst) |
| **Scroll position memory** | Returning to a channel restores the exact scroll position via auto-save on every scroll event |
| **View anchor restore** | If scrolled up when leaving a channel, re-entry loads messages *around* the last-viewed message (`MessageQuery::around`) and restores the pixel-exact viewport position |
| **Jump to Present** | One-click button chain-loads all newer pages and scrolls to the live tail. Shows a subtitle "You're Viewing Older Messages" when unloaded newer messages exist |
| **No stale button** | Jump to Present is cleared when switching channels |
| **Unread divider** | Red "NEW" line persists until channel switch (Discord-style), pushed up by new messages |

See [`specs/chat-scroll-and-history.md`](specs/chat-scroll-and-history.md) for the full implementation spec.

---

## 🎨 UI Themes

Poly includes **3 built-in themes** + custom CSS editor:

- **Neutral Dark** (default): Cool, minimal aesthetic
- **Purple** (Discord): Discord brand colors
- **Red** (Stoat): Stoat/Revolt brand colors

Customize every color via CSS custom properties, import/export theme files.

---

## 🔒 Security & Privacy

- **Local database**: Offline-first SurrealKV (no cloud sync by default)
- **Backup encryption**: All data encrypted before leaving device (AES-256-GCM)
- **Identity**: Ed25519 keypair + X25519 derived keys + BIP39 recovery phrase
- **Tokens**: Backend credentials stored locally (can be encrypted as option)
- **Proof-of-Work**: Backup server uses PoW challenge for auth

---

## 🌐 Localization (i18n)

User-facing strings via **Project Fluent** (`.ftl` files):

- **English** (default)
- **German**
- **French**
- **Spanish**

Located in `locales/` directory.

---

## 📱 Platform Targets

| Platform | Renderer | Status | Notes |
|----------|----------|--------|-------|
| **Desktop (Wry)** | WebKit2GTK system browser | ✅ Primary | Stable, hot-reload |
| **Desktop (Blitz)** | WGPU native GPU | 🧪 Experimental | High-performance rendering |
| **Desktop (Electron)** | Electron + WASM | 🔄 In development | App store distribution |
| **Web Browser** | Dioxus fullstack + Axum | 🔄 In development | Self-hosted instance |
| **Android** | Dioxus mobile (native) | 🔄 Phase 2.8+ | Using native UI |
| **iOS** | Dioxus mobile (native) | 🔄 Phase 2.9+ | Using native UI |

---

## 🛠️ Development

### Workspace Setup

```bash
# Update Rust
rustup update stable

# Check workspace status
cargo check --workspace

# Run all lints (zero-warning policy)
cargo cranky --workspace

# Format code
cargo fmt --all

# Run tests
cargo test --workspace
```

### Hot-Reload Development

```bash
# Desktop (Wry)
cd apps/desktop
dx serve --hotpatch

# Changes to poly-core instantly reflect in the running app
# CRITICAL: After structural changes, rebuild:
cargo check -p poly-web --target wasm32-unknown-unknown
```

### DevTools Verification

After changes to `poly-core`, verify UI renders correctly:

1. Launch desktop app via `dx serve --hotpatch`
2. Take screenshot via DevTools MCP
3. Enable demo account (click 🧪 toggle)
4. Navigate affected views
5. Confirm UI renders with no missing elements

---

## 📜 License

Dual licensed under **MIT** or **Apache-2.0**.

---

## 🤝 Contributing

See `docs/overall-plan.md` for architecture decisions and contribution guidelines. Read the relevant `agents.md` file in each crate/app before making changes.

**BEFORE COMMITTING:**
- [ ] `cargo check --workspace` passes
- [ ] `cargo cranky --workspace` has zero warnings
- [ ] `cargo fmt --all` applied
- [ ] New public items have doc comments
- [ ] Changes tested manually (especially poly-core via DevTools)

---

**Last Updated:** 2026-04-03
**WASM Status:** All 6 backend plugins built and integrated (D21). Matrix native client implemented.
**Roadmap:** Phase 3 (backends) → Phase 4 (test servers) → Phase 5 (social agent)
