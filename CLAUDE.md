# CLAUDE.md — Poly Project Context

> Last updated: 2026-03-28

---

## !! MANDATORY — READ FIRST, ALWAYS !!

> Source: https://github.com/drona23/claude-token-efficient

- Think before acting. Read existing files before writing code.
- Be concise in output but thorough in reasoning.
- Prefer editing over rewriting whole files.
- Do not re-read files you have already read unless the file may have changed.
- No sycophantic openers or closing fluff.
- Keep solutions simple and direct. No over-engineering.
- If unsure: say so. Never guess or invent file paths.
- Read before writing. Understand the problem before coding.
- No redundant file reads. Read each file once.
- One focused coding pass. Avoid write-delete-rewrite cycles.
- Test once, fix if needed, verify once. No unnecessary iterations.
- Budget: 50 tool calls maximum. Work efficiently.

---

## Design Principles — SOLID pre-merge gate (8-item checklist)

Every new crate, every substrate addition, every work package that lands more
than ~200 LOC must pass this checklist before merging. The agent's report MUST
state pass / partial / fail per item with one-sentence evidence.

**Pre-merge gate:** PARTIAL must name the specific item and reason. FAIL must
either be fixed or carved into a follow-up cleanup task (with a filed issue or
plan entry) before the change lands on main. "We'll SOLID it later" is not a
passing gate.

Refactors ARE allowed and encouraged when SOLID gates fail — but each refactor
itself passes the same 8-item gate before merging.

1. **SRP — Single Responsibility.** Each module / type has exactly one reason
   to change. UI composes; services orchestrate; backends aggregate and fetch;
   stores persist — roles are not fused for convenience. A 684-line `rsx!`
   block is not one responsibility. If describing what a thing does needs
   "and", it is two things.

2. **OCP — Open/Closed.** The substrate is extensible without modifying
   existing impls. Canonical shape: trait + default impl + adapter-plugin
   slots. Used in `IsBackend` (the primary messenger backend substrate —
   adding a new messenger means adding a new `impl IsBackend`, not editing
   a match arm), `KvStore`, `AudioBackend`, `VideoBackend`, `HostRoute`.
   Adding a new backend or route must not require surgery on existing impls.

3. **LSP — Liskov Substitution.** Every impl of a trait honours the trait's
   full contract. If `IsBackend::send_message` says "may fail, won't panic",
   no impl may panic. No impl strengthens preconditions or weakens
   postconditions relative to the documented trait contract. Swapping one
   impl for another must not break callers.

4. **ISP — Interface Segregation.** Traits are client-focused, not
   god-interfaces. A trait with 15+ methods is a smell — split by client
   need. `BackendCapabilities` flags exist precisely so the UI can gate on
   what a backend actually supports; a backend that returns `NotSupported` on
   most methods is a sign the trait needs splitting. `Read + Write` over one
   `ReadWrite`.

5. **DIP — Dependency Inversion.** High-level modules depend on abstractions,
   not concretes. Pass `impl Trait` / `&dyn Trait` / generics rather than
   concrete types at call sites. A component that reads from
   `Signal<RoomList>` must not know how the list was loaded. Plugin-host
   wires concrete impls into abstract surfaces; nothing above the data layer
   holds a concrete reference to anything below it.

6. **No god-objects / no god-modules.** The largest type or module by LOC
   should be proportional to its job. The old `ClientBackend` god-trait was
   replaced by `IsBackend` in Phase H.4 for exactly this reason. If one file
   is 5x the size of its peers in the same crate, suspect god-creep. The
   three in-flight lint plans (`plan-component-lints.md`,
   `plan-connected-routes-static-check.md`,
   `plan-context-menu-quality-control.md`) exist to enforce this on the
   oversize UI components (`FavoriteServerIcon`, `ChatView`,
   `ServerContextMenu`). Split before merging, not after.

7. **Test seams at every IO boundary.** Every external boundary — SQLite
   (`KvStore` + `PluginStorageBackend`), HTTP outbound (`host-bridge /host/http`),
   WebSocket (`host-bridge /host/exec`), audio (`AudioBackend`), video
   (`VideoBackend`), notification sink (`NotificationSink`), OAuth token
   store (`ClientStateStore`), browser sandbox (`HostSandbox`) — has a
   trait + in-memory or stub impl + concrete impl. The in-memory/stub impl
   lets tests run without external dependencies. A boundary without a seam
   is an untestable surface and a pre-merge blocker.

8. **Pure plugins — no direct IO.** Plugin code (WASM components implementing
   `poly:messenger@0.1.0` or any future WIT interface) must never perform
   direct IO. All HTTP, storage, exec, clock, and logging calls must flow
   through the host-bridge capability surface (`/host/http`, `/host/kv/*`,
   `/host/exec`, `/host/status`). A plugin that opens a socket, reads a file,
   or calls a system clock directly violates the capability-isolation contract
   and must be fixed or rejected before merge. Native backends that implement
   `IsBackend` directly (demo, stoat, matrix, discord, teams, poly-server) are
   exempt — this item applies only to WASM guest components.

**When SOLID kicks in:** the in-flight lint plans will force refactors on
oversize components. Apply this checklist during those refactors — especially
SRP and item 6 when deciding how to split an `rsx!` block or a large module.

---

## Agent Orchestration

This project uses a three-tier agent model:

| Role | Model tier | When to use |
|------|-----------|-------------|
| **Orchestrator** | Current session (default, most capable) | Planning, architectural decisions, talking to the user |
| **Coding agent** | `model: "sonnet"` | Implementation tasks, file edits, refactors |
| **Testing agent** | `model: "haiku"` | Running TEST_HARNESS.md, smoke tests, deterministic checks |

### Rules
- The orchestrator directs, delegates, and integrates — it does NOT do all the work itself.
- Spawn coding agents (sonnet-tier) for isolated implementation tasks that can run in
  parallel, using `isolation: "worktree"` so they work in separate copies.
- **Always run tests via a haiku-tier subagent** — pass `TEST_HARNESS.md` as the task.
  Haiku is fast and cheap; use it freely for verification loops.
- The user may type instructions to the main agent while subagents are running. This is
  intentional — process new instructions in parallel with ongoing delegated work.
- Tier names (`"haiku"`, `"sonnet"`, `"opus"`) are version-agnostic aliases in the
  Agent tool and will continue to refer to the appropriate tier as models evolve.

### Test harness
Run `TEST_HARNESS.md` via a haiku subagent after any non-trivial code change:

```
Agent tool → subagent_type: "general-purpose", model: "haiku"
prompt: "Read /home/laragana/workspcacemsg/TEST_HARNESS.md and execute every step.
         Report results as the table described at the bottom of the file."
```

For UI-only changes (CSS / RSX), skip step 4 (unit tests) but always run step 3 (WASM build).
For changes touching `mcp/chat-mcp/src/persona/` or `crates/core/src/ui/agent/persona/`, always run step 6 (persona e2e mock smoke) in addition to step 4.

---

## Plan files — checkbox + status discipline

Every plan file in `docs/plans/` MUST follow these rules. No
exceptions, no "I'll add checkboxes later".

1. **Numbered phases** using typeable letters: `Phase A`, `Phase B`, …
   No `§` characters.
2. **Sub-step checkboxes** in each phase: `- [ ] **A.1** …`,
   `- [ ] **A.2** …`. A phase with no sub-step checkboxes is forbidden
   — if you can't articulate sub-steps, you don't have a plan yet.
3. **Tick `- [x]` AS WORK LANDS** with a "shipped in change `<jj-change-id>`"
   note on the phase header. Do not batch.
   - **Use jj change IDs (the alphabetic prefix like `opknvmpk`),
     NEVER git commit hashes.** Get them via
     `jj log -r <revset> -T 'change_id.short()'` or read the first
     column of `jj log` output. Change IDs are stable across rebases;
     commit hashes shift on every history rewrite and break plan-doc
     references immediately.
4. **Mark plan DONE** at the top: `## Status: ✅ DONE — all phases
   shipped (changes a, b, c)`. Obsolete plans get
   `## Status: OBSOLETE — superseded by …`.
5. **Repo plans live in the repo.** A plan describing work in this
   repo MUST be at `docs/plans/`. Anything in `~/.claude/plans/` is
   personal scratch only — move it into the repo before sub-agents
   touch it.

**Why this is non-negotiable:** sub-agents in worktrees see stale
source, context windows compress, agents crash mid-task. A plan
without ticked checkboxes is unreadable to any non-orchestrator agent
and degrades into "vibes-based status" within two iterations.

**Sub-agent dispatch must include**: "Tick the checkboxes for sub-steps
you complete, AND add the commit ID to the phase header inline."

---

## Priority 2 — Use Jujutsu (jj) Instead of Git

- **Always use `jj` commands** for version control, never raw `git`
- `jj status`, `jj diff`, `jj log`, `jj show` for inspection
- `jj new`, `jj describe`, `jj commit` for creating changes
- `jj git push` to push to remote
- **"Commit and push" means: `jj describe` → `jj bookmark set main -r @` → `jj git push --bookmark main`.** That's it. Do NOT run `jj new` after. `jj git push` auto-advances `@` to a fresh empty commit (the pushed commit becomes immutable so jj automatically creates a new empty working copy on top). A redundant `jj new` creates a second empty commit that shows up as a rejected empty-ancestor on the next push.
- Only fall back to `git` if `jj` cannot accomplish the task

---

## Project Overview

**Poly** is an AI-powered social layer that unifies all your messaging platforms
(Discord, Matrix, Stoat, Teams, self-hosted) into one app — then adds an AI agent
that remembers your conversations, responds in your voice, manages your social
relationships, and acts as your external social memory.

Built with Rust, Dioxus 0.7.3, and WASM Component Model plugins. Two layers:

1. **Unified Chat UI** — 6 messenger backends via plugin architecture (demo, stoat,
   matrix, discord, teams, poly-server). One sidebar, one message view.
2. **Social Agent** (Phase 5) — MCP server exposing all chat backends to AI. Per-chat
   personality, conversation memory, typing simulation, outreach scheduling, digest
   briefings. Bring your own AI provider (Claude, GPT, Gemini, Ollama).

## Platform Targets

| App | Shell | Dev Server Port | Debug Port | MCP |
|-----|-------|----------------|------------|-----|
| `apps/web` | Chrome/Chromium | 3000 | 9222 (CDP) | `poly-web` |
| `apps/desktop` | `apps/desktop-web` (Wry) | 3002 | 9223 (HTTP eval) | `poly-desktop` |
| `apps/desktop-electron` | `apps/desktop-electron-web` (Electron) | 3001 | 9224 (CDP) | `poly-electron` |

## Host-bridge (`/host/*` — per-shell fullstack port)

Every shell mounts the same `/host/*` route set on the **same port as
its WASM bundle** — one process, one port. The three UI crates
(`apps/web`, `apps/desktop`, `apps/desktop-electron`) are Dioxus
**fullstack** apps: `dx serve --fullstack` builds the WASM client and a
native axum server from the same `src/main.rs`. The server half merges
`poly_host::router(state)` into the Dioxus router before binding.

| Shell | Fullstack port | Storage backend |
|-------|----------------|-----------------|
| `apps/web` (Chromium) | 3000 | `storage.sqlite3` in the OS data dir |
| `apps/desktop-electron` (Electron) | 3001 | Same file |
| `apps/desktop` (Wry, web-shell mode) | 3002 | Same file |
| `apps/poly-host` (standalone daemon, optional) | 9333 | Same file |

Data dir resolution (via `poly_host::resolve_data_dir()`):

| Platform | Path |
|----------|------|
| Linux    | `$XDG_DATA_HOME/poly/storage.sqlite3` → `~/.local/share/poly/storage.sqlite3` |
| macOS    | `~/Library/Application Support/poly/storage.sqlite3` |
| Windows  | `%APPDATA%\poly\storage.sqlite3` |

Override with `POLY_DATA_DIR=/some/path`. All shells open the same
file, so accounts added in one shell show up in the others.

Routes: `GET /host/status`, `POST /host/kv/{get,set,delete,clear}`,
`POST /host/exec`, `POST /host/http`, `POST /host` (legacy tagged-union
dispatch, kept one release cycle).

### Running `apps/web` with persistent storage

```bash
cd apps/web
dx serve --platform web --fullstack \
  @client --no-default-features --features "dev-plugins,web" \
  @server --platform server --no-default-features --features "dev-plugins,server"
```

The `@server --platform server` flag is REQUIRED — without it dx tries
to build the server half for `wasm32-unknown-unknown` and fails. See
`docs/plans/phase-2.21-host-bridge-unification-plan.md`.

## WASM Hot-Reload Architecture

All three platforms use the same pattern:
1. `dx serve --platform web --port <PORT>` compiles the app as WASM
2. A thin native shell (Chrome / Wry / Electron) loads from the dev server
3. On code changes, only the WASM reloads — the native window stays alive
4. The MCP reconnects via CDP or eval-bridge after each rebuild

### Key Files

| Shell | Source |
|-------|--------|
| Desktop Wry shell | `apps/desktop-web/src/main.rs` |
| Electron thin shell | `apps/desktop-electron-web/electron/main.js` |
| Desktop MCP | `mcp/desktop-devtools-mcp/src/main.rs` |
| Electron MCP | `mcp/electron-devtools-mcp/src/main.rs` |
| Web MCP | `mcp/web-devtools-mcp/src/main.rs` |
| Shared protocol | `mcp/devtools-protocol/src/` |

## Critical Implementation Notes

### Client-config KV namespace
Per-backend client settings (version overrides, mechanism toggles) live under
`client.config.<backend_id>.*` in `poly_kv`.  CLI recipes and the rollback
story: `docs/client-settings.md`.  Code: `crates/host-bridge/src/client_config.rs`.

### ELECTRON_RUN_AS_NODE
VS Code and Claude Code terminals set `ELECTRON_RUN_AS_NODE=1`. This causes Electron
to run as plain Node.js where `require('electron')` fails. The MCPs strip this env var
when spawning Electron processes.

### Wry build_gtk
On Linux, `wry::WebViewBuilder::build_gtk()` must receive `window.default_vbox()`,
NOT `window.gtk_window()`. Using `gtk_window()` results in a 0x0 viewport.

### Electron Frameless Windows
Use `frame: false` only. Do NOT combine with `titleBarStyle: 'hidden'` or
`titleBarOverlay: false` — these conflict on Linux and cause pixel offsets.

### CSS Layout
`.main-layout` uses `height: 100%` (not `100vh`) so it respects the flex parent's
allocated size when the Electron custom titlebar (34px) is present.

### Screenshot Safety
All MCPs guard against 0x0 viewport screenshots in `devtools-protocol/src/mcp.rs`.
A 0x0 or sub-100-byte image returns a text error instead of sending a corrupt PNG
to the API.

### Orphan Process Cleanup
The Electron MCP kills stale processes by matching `poly-desktop-electron-web` in
the command line (catches main, GPU, network, renderer). The desktop MCP uses
`poly-desktop-web` pattern. Both also kill by dx serve port pattern.

### Desktop WASM Compatibility
`apps/desktop/Cargo.toml` uses cfg-gated dependencies:
- Native: `dioxus = ["desktop"]`, `tokio`, `tracing-subscriber`
- WASM: `dioxus = ["web"]`, `getrandom04-wasm`

## Build Commands

```bash
# Build all MCPs
cargo build -p poly-desktop-devtools-mcp -p poly-electron-devtools-mcp -p poly-web-devtools-mcp

# Build desktop Wry shell
cargo build -p poly-desktop-web

# Test desktop WASM compilation
cd apps/desktop && dx build --platform web
```

### Cargo profiles (lean by default)

Three profiles are declared in `Cargo.toml` for agentic development:

- **`dev` (DEFAULT)** — `debug = "line-tables-only"`, `incremental = false`.
  Stack traces work (panics / backtraces still show file:line), but no full
  DWARF. Cuts ~60-80% off `target/` disk per worktree. Use for every
  `cargo build` / `check` / `test` / agent run that isn't a debugger
  step-through.
  - **`incremental = false` is deliberate.** The incremental-compile cache
    (`target/debug/incremental/`) was the #1 disk consumer — it ballooned to
    88G / 7k crate-rebuild dirs across this repo's build history, dwarfing
    `deps/` (45G). Incremental only pays off for a human iterating one line
    at a time; our workflow is agentic whole-crate/whole-workspace builds
    fanned out across parallel `jj workspace add` directories. **Fan-out is
    the bigger speedup, so per-edit incremental rebuild time doesn't matter**,
    and the plugin (WASM component) architecture means most iteration is
    rebuilding a single guest crate anyway. Turning it off stops the 88G
    re-accumulation; each parallel workspace's own `target/` then fits.
  - **Purge a stale incremental cache any time:** `rm -rf
    target/debug/incremental`. Safe, and NOT a cold rebuild — `deps/` stays
    warm; cargo just recomputes incremental state on the next touch.
  - **Want the cache back for local human iteration:** set
    `CARGO_INCREMENTAL=1` in the env for that session (keeps the committed
    profile lean for agents/CI while letting a human opt in).
- **`dev-symbols`** — opt-in full `debug = "full"`, `strip = "none"`.
  Use only when you're actually about to attach gdb/lldb. Invoke with
  `cargo build --profile dev-symbols`.
- **`release`** — production. Optimized, no debug, stripped.

### Build artifacts off `/home`

The repo lives at `/media/games/code/workspacemsg/` (SSD, plenty
of headroom). `/home/laragana/workspcacemsg` is a **convenience
symlink** to that real location. `/home` is the user's encrypted
volume that fills up under agent-driven parallel-worktree
patterns; routing the repo to `/media/games` eliminates the
disk-pressure footgun.

`target/` is a real directory inside the repo. `cargo clean` is
safe — it just empties `target/` in place, nothing to break.

New worktrees nest inside `.claude/worktrees/` (gitignored):

```bash
cd /home/laragana/workspcacemsg   # or /media/games/code/workspacemsg
jj workspace add .claude/worktrees/agent-<id> --name agent-<id>
```

(Poly has no `justfile`; contrast foundlings, which ships
`just worktree-new <name>` for this.)

## Test-server Avatar URL Conventions

Each mock backend serves avatar images via its own URL convention. These are the
stable patterns — use them when writing agent scripts, integration tests, or curl
one-liners that need to verify avatar bytes. All routes delegate to
`servers/test-common::avatars::serve_animal(name)`, the shared helper that maps
bare animal names to bundled PNG/SVG bytes from `clients/demo/assets/`. See
`docs/plans/plan-test-avatars-and-lemmy-forum-ux.md` for the full per-backend
animal mapping rationale (Phase A).

| Backend         | Port | Avatar URL pattern                                    | Example                                              |
|-----------------|------|-------------------------------------------------------|------------------------------------------------------|
| test-matrix     | 9100 | `/_matrix/media/v3/thumbnail/{server}/{media_id}`     | `/_matrix/media/v3/thumbnail/localhost/owl_avatar`   |
| test-stoat      | 9101 | `/avatars/{av_id}` (id is `av_{USER_ID}`)             | `/avatars/av_STOAT01`                                |
| test-discord    | 9102 | `/avatars/{user_id}/{file}.png`                       | `/avatars/1/koala.png`                               |
| test-teams      | 9103 | `/v1.0/users/{user_id}/photo/$value`                  | `/v1.0/users/U001/photo/$value`                      |
| test-lemmy      | 9104 | `/pictrs/image/{filename}` (extension included)       | `/pictrs/image/beaver.svg`                           |
| test-hackernews | 9105 | N/A — HN has no user avatars; UI falls back to initial| —                                                    |
| test-forgejo    | 9106 | `/avatars/{name}` (bare animal name, no extension)    | `/avatars/otter`                                     |
| test-github     | 9107 | `/avatars/{login}.png`                                | `/avatars/penguin.png`                               |

All backends are started by `poly-test-runner` (see `servers/test-runner/`). For
detailed per-backend curl recipes, seed users, and reset endpoints, see
`docs/dev/test-backends.md`.

## MCP Workflow

```
launch_app → poll get_last_build_status → connect_cdp → take_screenshot / navigate
```

All `launch_app` and `rebuild_app` calls are **non-blocking** — poll `get_last_build_status`
every 5-10s until `state != "Running"`.

### NEVER `hard_kill` for routine smoke-tests / checkpoints

⚠️ **Stop and re-read this before reaching for `hard_kill`.**

When the user asks for a "checkpoint smoke-test", "verify the app still works",
or "make sure my change didn't break anything" mid-session, the **default
path is hot-reload, not kill-and-restart**:

- dx serve is already running and watches the source tree. Save your file
  edits → dx auto-rebuilds wasm → Chrome reloads automatically with the new
  bundle while keeping the user's session, route, scroll position, and
  agent-panel state intact.
- Use `mcp__poly-web__list_console_messages` and `take_screenshot` against
  the **already-running** Chrome to verify. No restart needed.
- If you need an explicit recompile signal (e.g. lint-gate baseline regen
  outside the watched tree), call **`mcp__poly-web__rebuild_app`** —
  triggers a recompile WITHOUT killing chromium.

**`hard_kill` is for stuck processes only.** Specifically:
- `connect_cdp` / `evaluate_script` / `list_console_messages` time out
  because the WASM main thread is wedged (CLAUDE.md hang classes #1-#8),
  AND
- you've already tried `rebuild_app` and the page is still unresponsive.

`hard_kill` SIGKILLs both the dx static-file server AND Chromium. The user
loses every browser tab/state and pays a 60+ second cold rebuild. Doing it
reflexively as a "fresh start" pattern for routine verification is **a real
incident** — has happened twice in this session and both times the user
called it out as a mistake. **Do not repeat.**

### MCP Identity — DO NOT CONFUSE

The **poly-electron**, **poly-web**, and **poly-desktop** MCP servers are custom Rust
binaries in this repo (`mcp/*/src/main.rs`). They are **NOT** `chrome-devtools-mcp`,
`chrome-devtools-headless`, or `firefox-devtools-mcp`. Never substitute a generic
browser MCP for a poly MCP — they have different tools (`launch_app`, `rebuild_app`,
`get_last_build_status`, `connect_cdp`, etc.) and manage the full app lifecycle.
If the poly MCPs are not loaded in the current session, say so — do not fall back
to chrome-devtools as a replacement.

## Parallel Agent Work — `.claude/worktrees/` pattern

When you spawn an Agent with `isolation: "worktree"`, the runtime creates a
workspace directory under `.claude/worktrees/agent-<id>/` that the subagent
edits. The `PreToolUse` hook in `.claude/settings.json` symlinks `target/`
inside each worktree to `/media/games/workspacemsg-worktree-targets/agent-<id>/`
so build artifacts live on a separate disk and don't fill `/`. The `Stop` hook
cleans worktrees older than a day.

### jj workspace isolation — why per-agent workspaces are safe

`jj workspace add` (jj 0.41.0) is jj's **native** isolation primitive — the
proper equivalent of `git worktree add`. Each workspace directory backed by the
same `.jj/repo` gets its **own working-copy commit and its own `@` resolution**.
Two concurrent agents, each in their own `jj workspace add` directory, will NOT
fight over `@` or clobber each other's uncommitted edits. This is safe by design.

**The default-workspace edit race** (documented separately in memory) was caused
by two agents sharing the *same* default workspace — NOT by jj workspaces being
unsafe. The fix is the pattern already used here: one `jj workspace add
.claude/worktrees/agent-<id>` directory per concurrent agent. With each agent
owning its own workspace, the race is structurally impossible.

After a workspace directory is cleaned up, run `jj workspace forget agent-<id>`
so jj stops tracking its stale working-copy commit. Use `jj workspace list` to
audit live workspaces.

### MANDATORY before the subagent exits — `jj describe` AND verify the commit landed

Worktree directories get cleaned up. The git/jj branch (`worktree-agent-<id>`)
persists, so committed work survives. **Uncommitted edits do not.** And
`jj describe` by itself is **not** sufficient proof — concurrent worktree
operations or a background squash can rewrite the working copy out from under
the subagent, leaving no commit on the branch even though `jj describe`
returned zero exit code.

#### Agent-side prompt requirement — the subagent MUST prove the commit landed

Every parallel-work subagent prompt **must** include this verification block
verbatim (adapt the commit message):

> Before reporting done:
> 1. Run `jj describe -m "<one-line summary>"`.
> 2. Then run `jj log -r 'worktree-agent-<your-id> & description(<summary>)'
>    --no-graph -T 'commit_id.short()'` and paste the output in your final
>    message.
> 3. If that `jj log` returns an empty result, DO NOT report done — your
>    commit did not land. Retry `jj describe` (check for a `jj squash` or
>    `jj abandon` that ran concurrently) and re-verify until the commit
>    appears on the branch.

The commit-id echo in the subagent's final message is the load-bearing signal —
it proves the commit is real, not just that `describe` exited 0.

#### Orchestrator-side verification — don't trust the "done" message alone

After the subagent returns, before moving on:

1. **Verify the commit exists on the worktree branch:**
   ```
   jj log -r 'worktree-agent-<id>' --no-graph -T 'commit_id.short() ++ " | " ++ description.first_line()'
   ```
   If this is empty or shows the pre-agent parent commit, the commit did not
   land. Go to rescue step.

2. **Diff the worktree directory against main as a sanity check:**
   ```
   diff -rq --exclude=target --exclude=.jj --exclude=.git \
     .claude/worktrees/agent-<id>/ /home/laragana/workspcacemsg/
   ```
   If this lists changed files but step 1 showed no commit, the work exists
   only as uncommitted edits in the worktree directory and is about to be
   cleaned up. **Rescue immediately.**

3. **Rescue path when the subagent lied:**
   ```
   # copy uncommitted files out of the worktree back into main
   for f in <list-from-diff-rq>; do
     cp -f ".claude/worktrees/agent-<id>/$f" "$f"
   done
   # then commit from main normally
   jj describe -m "<summary>" && jj bookmark set main -r @ && jj git push --bookmark main
   ```

4. **Normal path when the commit is real:**
   ```
   jj rebase -s <commit-id> -d main
   jj bookmark set main -r @
   jj git push --bookmark main
   ```

If the rescue path fires, note in the orchestrator commit message: "Recovered
from worktree <id> after subagent reported done without a landed commit" so
the pattern is searchable in history.

**Two real incidents before this rule existed:**
- Phase 6 of `plan-discord-forums-threads.md`: subagent reported done,
  worktree path was cleaned, no jj commit, all edits lost except one stray
  file the LSP had auto-saved.
- `send_typing` MCP tool sonnet agent: reported "done" with a plausible
  summary, but `jj log -r 'worktree-agent-<id>'` showed the unchanged
  parent commit. Recovered by rsync-diff against the worktree dir before
  the Stop hook cleaned it up.

### Disjoint files = parallel-safe

Run multiple worktree-isolated agents in parallel only when their target files
don't overlap. Each subagent prompt should explicitly list "DO NOT touch X"
for any file another parallel agent might be editing.

## Debugging hard WASM hangs in poly-web

When poly-web freezes hard — Chrome tab unresponsive, devtools console
itself stops draining — `mcp__poly-web__list_console_messages` and
`click_at` time out. The poly-web MCP relies on CDP, which relies on the
page's main thread being able to service messages. A WASM tight loop
or infinite recursion eats the main thread → CDP becomes useless on
that page.

The Playwright MCP (`mcp__plugin_playwright_playwright__*`) drives a
**separate** Chrome instance. Even after poly-web's CDP-driven Chrome
is wedged, Playwright's browser may still respond — until the same
freeze hits it too. Even when Playwright also hangs, the messages it
queued before the wedge are recoverable on the next session.

### Bisect recipe

1. **Make the freeze observable.** Add `tracing::warn!(target: "BISECT", "step N: <what>")` lines around every Signal read/write, every `spawn`, every `await`, every `nav.push`. The closure that hangs the page is reached *before* the actual hang line; the highest-numbered warn that appears in the console pinpoints the bug.

2. **Drive the click with Playwright not poly-web's CDP** when the page is borderline. Sequence:
   - `mcp__plugin_playwright_playwright__browser_navigate` to the route.
   - `browser_snapshot` (or `browser_evaluate` with `getBoundingClientRect`) to find the target ref/coords.
   - `browser_click` (by ref) or call `.click()` via `browser_evaluate`.
   - `browser_console_messages level="warn"` immediately after.
   - If `browser_console_messages` times out → the WASM tight loop has now starved Playwright's main thread too. The bisect warn that fired LAST before the timeout is the answer.

3. **Gut the suspected handler to a single warn first** — confirms whether the freeze is in the handler body at all (some "freezes" are upstream in Dioxus event dispatch or render-loop, not in your closure). If the gutted handler still freezes the page, the cause is BEFORE the closure runs.

4. **Restore the body in N numbered tracing steps** — wrap each statement in a `tracing::warn!("step N")` envelope, ideally with one statement per step. Whichever N is the LAST log before the freeze is the offending line.

5. **Beware: the freeze persists across page reloads** because it's a tight CPU loop the Chrome scheduler can't preempt. Hard-kill Chrome (`mcp__poly-web__hard_kill`) and `launch_app` between attempts; reload-from-overlay rarely clears the wedge.

6. **The boot-hang watchdog** (`crates/core/src/wasm_crash_handler.rs`,
   `BOOT_HANG_TIMEOUT_MS`) shows an "App not responding" overlay if the
   startup overlay doesn't dismiss in time. False positives are common
   when boot involves many restored accounts/servers; if the friends
   grid renders BEHIND the overlay the page is healthy, just slow.
   Bump the timeout instead of treating it as a real hang.

### Common WASM-hang causes (ranked by frequency in this codebase)

Each hang class is paired with its active countermeasure — the pattern/API +
CI lint that makes it hard to reintroduce. A fresh recurrence of class #N
means a missed migration site, an escape-hatch `#[allow]`, or a genuinely new
class worth documenting. (Commit hashes / phase history live in git + the
referenced plan docs; only the actionable pattern is kept here.)

1. **`Signal::write()` chains in a click handler / loader.** Each `.write()`
   guard-drop schedules a Dioxus re-render; 5–7 in a row starve the WASM
   single-thread scheduler.
   **Countermeasure:** `BatchedSignal<T>` newtype — use `.batch(|v| …)` /
   `.pending_update()` instead of `.write()`. Lint
   `tools/scripts/forbid-signal-write.sh` bans raw `Signal::write()` in
   `crates/core/src/ui/`; allowlist `signal-write-allowlist.txt`. Plan:
   `docs/plans/plan-batched-signal.md`.

2. **Live `Signal::read()` guard across a `.write()` of the same signal.**
   WASM panics with no unwinding → tight loop. Scope reads in `{ … }` so the
   guard drops before any write.
   **Countermeasure:** `BatchedSignal::batch(|v| …)` (mutation through a
   closure) + `BatchedSignal::with(|v| …)` for multi-statement reads. Lint
   `forbid-long-read-guard.sh`; inline allow: `// poly-lint: allow
   long-read-guard — <reason>`. Patterns: `docs/dev/reactive-state.md`.

3. **`.write()` inside a `use_effect` that also subscribes to that signal**
   (directly or via a spawned task) → infinite re-render loop.
   **Countermeasure:** `use_spawn_once<K>(key, async_fn)` hook bakes in the
   `spawned_for` guard. Lint `forbid-use-effect-spawn-cycle.sh`; allowlist
   `use-effect-spawn-cycle-allowlist.txt`. Plan: `plan-use-spawn-once.md`.

4. **`tokio::sync::RwLock::read().await` on a backend with a perpetual
   writer.** WASM scheduler starves readers. **WARNING:** the naive
   `tokio::time::timeout(_, backend.read())` wrap **panics on WASM**
   (`Instant::now()` unimplemented on `wasm32-unknown-unknown`).
   **Countermeasure:** `BackendHandleExt::read_with_timeout(dur)` — cfg-gated
   (tokio timeout native / `gloo_timers` raced via `futures::select` on WASM).
   Lint `forbid-raw-backend-read.sh` bans raw `backend.read().await` in
   `crates/core/src/ui/`; inline allow: `// poly-lint: allow raw
   backend.read().await — <reason>`. Plan: `plan-backend-read-timeout.md`.

5. **A spawned async task that writes Signals while the spawning closure
   still holds a guard.** Indirect form of #2 — drop the guard before
   `spawn(async move { … })`.
   **Countermeasure:** closed by #1's type contract — `BatchedSignal::batch`
   drops the guard at closure exit; `PendingUpdate::apply()` acquires-and-drops
   atomically. Unmigrated plain `Signal<T>` locals are single-component-scoped
   so the cross-task mode doesn't apply.

6. **`use_effect(move || …)` captures a non-Signal value (prop, local) that
   drifts across re-renders.** Effect runs once with the initial value and
   never re-fires. Symptom: "second navigation has no effect" / stale UI.
   **Countermeasure:** `use_reactive_effect<Deps>(deps, body)` mirrors `deps`
   into a Signal each render so the body re-fires via PartialEq dedup
   (`use_spawn_once` uses the same pattern). Lint (HARD-FAIL)
   `forbid-stale-effect-capture.sh`; inline allow: `// poly-lint: allow
   stale-effect-capture — <reason>`. Plan: `plan-use-reactive-effect.md`.

7. **Render-time `signal.read()` for a value used only as a hook key /
   one-shot snapshot.** The render-body `.read()` subscribes the WHOLE
   component; any later write to that signal re-renders → re-reads → loop.
   **Countermeasure:** use `.peek()` whenever the value isn't needed
   reactively (hook keys, one-shot snapshots, props the child re-subscribes
   to). Lint `forbid-render-time-read.sh` (continue-on-error; ~988 legit
   rsx!/cond-render sites allowlisted); inline allow: `// poly-lint: allow
   render-time-read — <reason>`. (No type-system fix: `peek`/`read` return
   identical guards.) Plan: `plan-peek-vs-read.md`.

8. **`use_effect` that subscribes to `S` and writes `S` with no equality
   check.** `batch` always notifies subscribers, so a self-writing effect
   re-fires forever unless its early-return guard covers the steady state
   (classic hole: `messages_loaded` stays false for an empty channel).
   Distinct from #2 (borrow panic) and #6 (effect NOT re-firing).
   **Countermeasure:** `BatchedSignal::set_if_changed(next)` /
   `batch_if_changed(|cur| -> next)` skip the write when unchanged (needs
   `T: PartialEq`). Lint `forbid-effect-self-write.sh`; inline allow:
   `// poly-lint: allow effect-self-write — <reason>`.

**Last-resort diagnostic path — the out-of-band trace sink.** When a hang
starves CDP (`evaluate_script` and `list_console_messages` time out), raw
`tracing::warn!` goes nowhere in WASM (no subscriber wired) and
`console.warn` overrides don't intercept `web_sys::console::warn_1`. The
pattern that worked for the Teams Sheep bisect:
`fn bisect_log(msg) { window.fetch('/host/kv/set', { body: {key: 'bisect:<counter>', value: msg} }); doc.set_title(msg); }`.
Persists to SQLite via the host bridge even when the main thread wedges
immediately after (fetch dispatches to the network thread before JS
continues). Query via `sqlite3 ~/.local/share/poly/storage.sqlite3
"SELECT payload, COUNT(*) FROM poly_kv WHERE key LIKE 'bisect:%' GROUP BY
payload ORDER BY COUNT(*) DESC"` — top-count rows pinpoint the cascading
call site.

When the bug doesn't match classes #1–#5, the freeze is likely in
generated code (Dioxus interpreter, `wit_bindgen` bridge) and you'll
need a real DevTools session — `chrome-devtools-mcp` if it's loaded,
or ask the user to paste a stack trace from the Sources panel.

## Persona-subsystem footguns

Persona subsystem (`mcp/chat-mcp/src/persona/`, `mcp/chat-mcp/src/tools.rs`):
**privacy and contract bugs** (not concurrency). Same countermeasure shape —
allowlisted regex lints, all hard-fail CI (`continue-on-error: false`).
Plan: `docs/plans/plan-persona-quality-gates.md`.

- **P1 — Cross-persona memory leak.** DML (`SELECT`/`DELETE`/`UPDATE`) against
  a persona-scoped table (`persona_facts`, `persona_audit`, `persona_sources`,
  `persona_tool_whitelist`, `persona_outbound_allowlist`) WITHOUT a
  `WHERE persona_slug = ?` binding hits the wrong persona's rows — silent, no
  error. Lint `forbid-cross-persona-memory.sh` (requires a `persona_slug`
  binding within 10 lines); allowlist `cross-persona-memory-allowlist.txt`;
  inline: `// poly-lint: allow cross-persona-memory — <reason>`.

- **P2 — Unaudited persona handler.** A `fn handle_meta_persona_*` that mutates
  persona state but doesn't call `audit()` / `record_persona_audit()` on the
  success path → the `persona_audit` forensic trail silently loses an event.
  Lint `forbid-unaudited-persona-tool.sh`; read-only handlers (`_list`,
  `_recent_actions`) in `unaudited-persona-tool-allowlist.txt`; inline:
  `// poly-lint: allow unaudited-persona-tool — <reason>`.

- **P4 — Raw backend read in persona builder.** `BackendPoolProvider`
  (`persona/context.rs`) calling a backend method without
  `tokio::time::timeout` blocks the native async runtime thread indefinitely
  (no WASM scheduler here, but still hangs the MCP call). Use
  `tokio::time::timeout(BACKEND_TIMEOUT, …)` (5s). Lint
  `forbid-raw-backend-read.sh` scans `persona/` too; allowlist
  `raw-backend-read-allowlist.txt`; inline: `// poly-lint: allow raw
  backend.read().await — <reason>`.
