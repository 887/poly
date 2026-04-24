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

## Design Principles — SOLID (as it maps to Rust)

Design new code — and opportunistically refactor old code — against SOLID. These
are *design* principles, not a refactor mandate: don't rewrite working code just
to hit a checklist. Do apply them when you're touching a file anyway, and
especially when the component-size / connected-routes / context-menu lint plans
force a refactor of oversize components.

- **Single Responsibility.** One reason to change per type/module/function. If
  describing what a thing does needs "and", it's two things. A 684-line rsx!
  isn't one responsibility — it's a dozen.
- **Open/Closed.** Add new variants/impls, don't edit existing ones. In Rust:
  prefer adding a trait impl or a new enum variant to swapping out a match arm's
  semantics. New backends/routes should not require surgery on old code.
- **Liskov Substitution.** A trait impl must obey the trait's documented
  contract. If `ClientBackend::send_message` says "may fail, won't panic", no
  impl can panic. Don't strengthen preconditions or weaken postconditions in
  impls.
- **Interface Segregation.** Small traits over kitchen-sink traits. Consumers
  should depend only on methods they actually call. `Read + Write` over one
  `ReadWrite`; split capability traits when a backend only supports part of
  the surface (cf. `NotSupported` returns — a sign the trait needs splitting).
- **Dependency Inversion.** Depend on abstractions. Pass `impl Trait` /
  `&dyn Trait` / generics rather than concrete types at call sites. A
  component that reads from `Signal<RoomList>` should not know how the list
  was loaded.

**When this kicks in:** the three in-flight lint plans
(`plan-component-lints.md`, `plan-connected-routes-static-check.md`,
`plan-context-menu-quality-control.md`) will force refactors on the oversize
components (`FavoriteServerIcon`, `ChatView`, `ServerContextMenu`, …). Apply
SOLID during those refactors — especially Single Responsibility when deciding
*how* to split an rsx! block.

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

## MCP Workflow

```
launch_app → poll get_last_build_status → connect_cdp → take_screenshot / navigate
```

All `launch_app` and `rebuild_app` calls are **non-blocking** — poll `get_last_build_status`
every 5-10s until `state != "Running"`.

### MCP Identity — DO NOT CONFUSE

The **poly-electron**, **poly-web**, and **poly-desktop** MCP servers are custom Rust
binaries in this repo (`mcp/*/src/main.rs`). They are **NOT** `chrome-devtools-mcp`,
`chrome-devtools-headless`, or `firefox-devtools-mcp`. Never substitute a generic
browser MCP for a poly MCP — they have different tools (`launch_app`, `rebuild_app`,
`get_last_build_status`, `connect_cdp`, etc.) and manage the full app lifecycle.
If the poly MCPs are not loaded in the current session, say so — do not fall back
to chrome-devtools as a replacement.

## Parallel Agent Work — `.claude/worktrees/` pattern

When you spawn an Agent with `isolation: "worktree"`, the runtime creates a git
worktree under `.claude/worktrees/agent-<id>/` that the subagent edits. The
`PreToolUse` hook in `.claude/settings.json` symlinks `target/` inside each
worktree to `/media/games/workspacemsg-worktree-targets/agent-<id>/` so build
artifacts live on a separate disk and don't fill `/`. The `Stop` hook cleans
worktrees older than a day.

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

Each hang class is paired with the active countermeasure — the prescribed
pattern that makes it mechanically impossible (or very hard) to reintroduce.
When a new hang matches class #N, check the countermeasure status first:
if the plan ships as claimed, the bug should be uncatchable — so a fresh
recurrence means either a missed migration site, an escape-hatch
`#[allow]`, or a genuinely new hang class worth documenting below.

1. **`Signal::write()` chains in a click handler / loader.** Every `.write()`
   guard drop schedules a Dioxus reactive re-render. 5–7 consecutive writes
   → 5–7 cascades on the WASM single-thread → scheduler starves. Historical
   incidents: commit `a761fe01` (AccountIcon.onclick), the `chat_view.rs`
   `open_message_hit` batches, `restore_server_channel` PendingUpdate
   conversion, plus 3 more.
   **Countermeasure (in progress): `BatchedSignal<T>` newtype.** Shipped
   Phases 1-3 (commits `e091281c`, `33b18d4d`, `828f9584`) covering
   `Signal<ChatData>` and `Signal<AppState>` — 271 `.write()` sites
   collapsed to `.batch(|v| …)` / `.pending_update()`. The deprecated
   shadow `BatchedSignal::write()` fails `#[deny(deprecated)]` so the
   bug is now a compile error on the migrated signals. See
   `docs/plans/plan-batched-signal.md` for the remaining Phase 4–6
   work (other hot-path signals, clippy/dylint).

2. **Live `Signal::read()` guard across a `.write()` of the same signal.**
   WASM panics → no panic_hook unwinding → tight loop / unreachable. Wrap
   reads in tightly-scoped `{ … }` so the guard drops before any write.
   **Countermeasure (partial):** `BatchedSignal::batch(|v| …)` forces the
   mutation through a closure, so no outer read can live alongside the
   write guard on the same signal. Cross-signal read-across-write (read
   signal A, write signal B with a handler that also reads A) is still
   possible and not type-gated. No plan yet; add one if incidents recur.

3. **`.write()` inside a `use_effect` whose body is also a subscriber to
   that signal** (including indirectly via a spawned async task that
   writes the signal). Causes infinite re-render loop. Historical
   incidents: Teams Sheep wedge (fix `904920b9`, ServerHome missing
   `spawned_for` guard) — the SQLite-persisted BISECT trace captured
   ~1.2M iterations before sampling.
   **Countermeasure (in progress): `use_spawn_once<K>(key, async_fn)`
   hook.** Phase 1 in flight at plan-authoring time; the hook bakes the
   `spawned_for: Signal<Option<K>>` guard into the call-site API so it
   can't be forgotten. Phase 5 is a clippy/dylint ban on the raw
   `use_effect` + `spawn(async move { … signal.batch(…) })` triple.
   See `docs/plans/plan-use-spawn-once.md`.

4. **`tokio::sync::RwLock::read().await` on a backend that has a perpetual
   writer.** Single-threaded WASM scheduler can starve readers. Wrap with
   `tokio::time::timeout(Duration::from_secs(5), backend.read())` and bail
   with a warning on timeout.
   **Countermeasure: none yet.** Low incident rate (one confirmed case).
   If another hits, draft a plan for a `read_with_timeout` helper that's
   the only allowed `backend.read().await` surface.

5. **A spawned async task that writes Signals while the spawning closure
   still holds a guard.** Same root cause as #2 but indirect. Drop the
   guard before `spawn(async move { … })`.
   **Countermeasure: same as #2** — `BatchedSignal::batch` closure scope
   prevents the outer closure from holding a guard across the spawn. Fully
   gated on migrated signals; unmigrated plain `Signal<T>` locals still
   susceptible.

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
