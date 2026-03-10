# poly-memory-mcp

Persistent task list, per-task memory notes, research findings, and general knowledge base for AI agents working on the Poly project.

## Why It Exists

AI agents working across multiple sessions need persistent memory:
- Which tasks exist and their status
- What was already researched (findings)
- Key architectural decisions made (memories)
- Reusable facts about libraries/patterns (knowledge base)

Without persistent storage, every new session starts blind. `poly-memory-mcp`
solves this by keeping everything in human-readable markdown files on disk.

## Data Layout

```
~/.poly-memory/
├── tasks.json            ← master task list (all tasks, metadata, checklists)
├── knowledge/            ← general knowledge base
│   └── *.md
└── tasks/
    └── 001-task-slug/
        ├── findings.md   ← research findings (append-only per session)
        └── memories/
            └── YYYYMMDD-HHMMSS-title.md
```

## Usage — CLI (PREFERRED)

> **Always use CLI over MCP where possible** — it's faster and testable directly.

```bash
# Task management
poly-memory-mcp tasks list
poly-memory-mcp tasks create "Add CLI to MCP servers"
poly-memory-mcp tasks get 1
poly-memory-mcp tasks status 1 in-progress
poly-memory-mcp tasks redo 1              # reset + uncheck items, keep memories
poly-memory-mcp tasks next                # show next pending task
poly-memory-mcp tasks reminders 1        # MANDATORY before starting any task
poly-memory-mcp tasks plan --count 3     # work plan for next 3 tasks

# Checklist items
poly-memory-mcp tasks item add 1 "Research existing MCPs"
poly-memory-mcp tasks item check 1 1     # toggle item 1 done

# Memories (key decisions, progress notes)
poly-memory-mcp memory store 1 "Architecture Decision" "We chose X because..."
poly-memory-mcp memory list 1

# Findings (research results — call CONSTANTLY during research!)
poly-memory-mcp finding store 1 "The DevtoolsBackend trait has fn js_eval()"
poly-memory-mcp finding list 1

# General knowledge base
poly-memory-mcp knowledge store "dioxus-rsx-fmt" "Always add #[rustfmt::skip] before..."
poly-memory-mcp knowledge search "dioxus"
poly-memory-mcp knowledge list
poly-memory-mcp knowledge get "dioxus-rsx-fmt"

# Multi-task workflow
poly-memory-mcp work --count 3
```

## Usage — MCP (VS Code Copilot)

Configured in `.vscode/mcp.json`. Start with `mcp` mode (default, no args):

```bash
cargo run --bin poly-memory-mcp
```

Available tools (same functionality as CLI):
`list_tasks`, `create_task`, `get_task`, `set_task_status`, `redo_task`,
`add_task_item`, `check_task_item`, `store_memory`, `load_memories`,
`store_finding`, `load_findings`, `store_knowledge`, `search_knowledge`,
`list_knowledge`, `get_knowledge`, `next_task`, `task_start_reminders`, `work_plan`

## Mandatory Agent Workflow

```
EVERY task:
1. task_start_reminders <id>   ← ALWAYS first (loads rules + context)
2. load_memories <id>          ← read prior notes
3. load_findings <id>          ← read research findings
4. [do work]
5. store_finding <id> "..."    ← call IMMEDIATELY on each discovery
6. store_memory <id> title "..." ← save key decisions
7. check_task_item <id> N      ← as each item completes
8. set_task_status <id> completed
```

## Task Status Values

| Status | Emoji | Meaning |
|---|---|---|
| `todo` | ⬜ | Not started |
| `in-progress` | 🔵 | Currently being worked on |
| `completed` | ✅ | Done |
| `redo` | 🔄 | Reset for redo (memories preserved, checklist unchecked) |

## Configuration

| Env Var | Default | Description |
|---|---|---|
| `POLY_MEMORY_DIR` | `~/.poly-memory` | Root data directory |

## VS Code Integration

Both MCP and CLI are configured in `.vscode/`:
- `.vscode/mcp.json` — `poly-memory` MCP server (stdio)
- `.vscode/tasks.json` — CLI tasks for common operations

## License

MIT / Apache-2.0
