# poly-memory-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-10 — per-task JSON file layout (was monolithic tasks.json)

---

## Purpose

`poly-memory-mcp` is the **persistent task list + memory + knowledge-base** server for
Poly agents. It gives AI agents a place to:

1. **Track tasks** — numbered, named, with status and checklist items.
2. **Store memories** — per-task markdown notes that survive session restarts.
3. **Store findings** — research findings appended to a per-task `findings.md` as discovered.
4. **Build a knowledge base** — general reusable knowledge not tied to any task.

---

## PREFERRED ACCESS: CLI, not MCP

**Always prefer the CLI over MCP protocol access.**

CLI is faster, directly verifiable in a terminal, and doesn't require an MCP client session.
Use MCP only when embedded in a GitHub Copilot session that can't run shell commands.

```bash
# Quick reference:
cargo run --bin poly-memory-mcp -- tasks list
cargo run --bin poly-memory-mcp -- tasks create "My new task"
cargo run --bin poly-memory-mcp -- tasks reminders 1
cargo run --bin poly-memory-mcp -- finding store 1 "Key finding content"
cargo run --bin poly-memory-mcp -- memory list 1
cargo run --bin poly-memory-mcp -- work --count 3
```

---

## Data Layout

```
~/.poly-memory/               (or POLY_MEMORY_DIR env var)
├── poly-memory.json          # global ID counter: { "next_id": N }
├── knowledge/                # general knowledge base (not task-specific)
│   └── <topic-slug>.md
└── tasks/
    ├── 001_my_task_title.json          # per-task metadata + checklist (≤50 chars)
    ├── 001-my-task-title/              # per-task findings + memories (dash-slug)
    │   ├── findings.md                 # append-only research findings
    │   └── memories/
    │       └── <timestamp>-<slug>.md   # individual memory notes
    ├── 002_another_task.json
    └── 002-another-task/
        └── ...
```

### File naming rules

| File/Dir | Pattern | Notes |
|---|---|---|
| `poly-memory.json` | fixed | stores `{ "next_id": N }` — NOT the old monolithic tasks array |
| task JSON file | `{id:03}_{title_underscored}.json` | max 50 chars total per filename (excl. `.json`) |
| task subdir | `{id:03}-{title_dashed}/` | dash-slug for backward compat with existing dirs |
| findings | `{task_subdir}/findings.md` | append-only |
| memory notes | `{task_subdir}/memories/{ts}-{slug}.md` | individual files |

### Migration from legacy format

If a monolithic `tasks.json` is found on startup it is **automatically migrated**:
1. Each task is written to its own `tasks/{id:03}_{slug}.json` file.
2. `poly-memory.json` is created with `next_id = max_existing_id + 1`.
3. The old file is renamed to `tasks.json.bak` (never deleted).

---

## Mandatory Agent Workflow

### Before starting ANY task:
1. Call `task_start_reminders <id>` — prints mandatory rules + existing context.
2. Call `load_memories <id>` — read prior session notes.
3. Call `load_findings <id>` — read research findings.

### During work:
4. Call `store_finding <id> "..."` **immediately** when you discover something.
   - Do NOT wait until the end of research — crash = lost findings.
   - Call this after EVERY significant discovery.
5. Call `store_memory <id> "title" "..."` for key decisions and progress.
6. Call `check_task_item <id> <item_id>` as you complete each checklist item.

### When done:
7. Call `set_task_status <id> completed`.

### Working on multiple tasks:
8. Call `work_plan --count N` for a structured plan.
9. Work on one task at a time, fully completing each before moving on.

---

## Rules File Enforcement

The memory MCP embeds reminders to follow `agents.md` rules. When the server
sends `task_start_reminders`, the response includes:
- Read `agents.md` and the relevant crate's `agents.md`.
- Run `cargo cranky --workspace` before declaring any coding task done.
- Run `cargo check -p poly-web --target wasm32-unknown-unknown` for UI changes.

---

## Architecture

```
poly-memory-mcp
├── src/main.rs      — entry: MCP mode or CLI mode
├── src/config.rs    — data dir: POLY_MEMORY_DIR or ~/.poly-memory
├── src/types.rs     — Task, TaskStatus, ChecklistItem
├── src/store.rs     — async file I/O (tokio::fs)
├── src/ops.rs       — business logic (create_task, store_finding, etc.)
├── src/mcp.rs       — JSON-RPC 2.0 server loop
└── src/cli.rs       — CLI argument parsing and dispatch
```

---

## Adding New Tools

1. Add a handler in `ops.rs`.
2. Add the MCP tool definition in `mcp::tool_list()`.
3. Add a match arm in `mcp::dispatch_inner()`.
4. Add a CLI command in `cli::dispatch_*()`.
5. Document in README.md.
6. Run `cargo cranky -p poly-memory-mcp` — zero warnings policy.
