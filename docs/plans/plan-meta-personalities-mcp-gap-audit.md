# MCP Gap Audit — Meta-Personalities Phase D UI Surface

## Status: ✅ AUDIT COMPLETE — 24 Phase D UI actions audited (20 OK / 1 recipe / 3 GAPs assigned to Phase F + H owners). Mandated by Phase J rescope of `plan-meta-personalities.md` (✅ DONE). Not a standalone plan; the 3 GAPs are tracked in their owner phases.

## Scope decision (Phase J, 2026-04-30)

This audit was mandated by the Phase J rescope in
`docs/plans/plan-meta-personalities.md`. The original Phase J intended to ship
8 new `meta_persona_*` tools and 3 shortcut wrappers. The rescope dropped those
additions because:

- `tools/poly-cli` already exposes every existing MCP tool as
  `poly-cli call <tool> --key val …` — no per-tool CLI boilerplate is needed.
- The 14 `meta_persona_*` tools shipped in Phase B already cover the full
  lifecycle.
- Shortcut tools (`set_enabled`, `pin_fact`, etc.) are read-modify-write
  operations a CLI script can do in two `poly-cli call` lines — maintaining a
  separate tool surface adds cost without benefit.
- Phase F and Phase E own heartbeat/invocation-history tools; adding them here
  would cause ownership confusion.

**Consequence for this audit:** a UI action can legitimately map to a
composed-call recipe rather than a dedicated tool. Every such case links to
`docs/personas-cli.md` for the canonical recipe. Only actions with no
MCP-reachable path at all are marked `GAP — needs follow-up tool`.

---

## Phase D UI surface — complete action inventory

Phase D shipped 7 files under `crates/core/src/ui/agent/persona/`:
`list_panel.rs`, `edit_modal.rs`, `sources_editor.rs`,
`tool_whitelist_editor.rs`, `route.rs`, `mcp.rs`, `types.rs`.

### PersonaListPanel / PersonaListRow (`list_panel.rs`)

| UI action | File:line | MCP tool | Status |
|---|---|---|---|
| Load persona list on mount | `list_panel.rs:88` | `meta_persona_list` | OK |
| "+" Create button — opens edit modal for `__new__` | `list_panel.rs:112` | `meta_persona_create` (fires on save in edit modal) | OK |
| "Talk to" button — logs info, no MCP call yet | `list_panel.rs:57-59` | `meta_persona_invoke` | RECIPE — see `docs/personas-cli.md` invoke recipe; UI stub fires on click but the full overlay is deferred to Phase E |
| Gear icon — opens PersonaEditModal | `list_panel.rs:65` | `meta_persona_get` (fires on modal load) | OK |
| Reload list after save | `list_panel.rs:149` | `meta_persona_list` | OK |

### PersonaEditModal (`edit_modal.rs`)

| UI action | File:line | MCP tool | Status |
|---|---|---|---|
| Load existing persona into form fields | `edit_modal.rs:317` | `meta_persona_get` | OK |
| Save — create mode (slug `__new__`) | `edit_modal.rs:502-509` | `meta_persona_create` | OK |
| Save — edit mode (existing slug) | `edit_modal.rs:511-519` | `meta_persona_update` | OK |
| Cancel / close modal | `edit_modal.rs:476` | no MCP (pure UI) | OK — no mutation, no tool needed |
| Toggle enabled/disabled (Identity section) | `edit_modal.rs:91-99` | `meta_persona_update` (fires on save with `enabled` field) | OK |
| Set name / avatar / system_prompt / style_notes | `edit_modal.rs:502-519` | `meta_persona_update` / `meta_persona_create` | OK |
| Read-only behaviour display (Phase F stub) | `edit_modal.rs:131-178` | `meta_persona_set_heartbeat` (future Phase F) | GAP — owner: Phase F |
| Read-only outbound display (Phase F stub) | `edit_modal.rs:439-443` | outbound tools (future Phase F) | GAP — owner: Phase F |
| Memory section — read-only fact list | `edit_modal.rs:186-209` | `meta_persona_get_memory` (loaded as part of `meta_persona_get` detail) | OK |
| Memory section — delete/pin fact buttons | `edit_modal.rs:186-209` | `meta_persona_forget_memory` / `meta_persona_set_memory` — buttons visible Phase H | GAP — owner: Phase H |
| Audit section — display recent audit rows | `edit_modal.rs:217-239` | `meta_persona_recent_actions` (loaded as part of `meta_persona_get` detail) | OK |

### PersonaSourcesEditor (`sources_editor.rs`)

| UI action | File:line | MCP tool | Status |
|---|---|---|---|
| Display existing source tree from loaded detail | `sources_editor.rs:118` | sources come from `meta_persona_get` detail | OK |
| Toggle source node Allow/Inherit/Deny (3-state pill) | `sources_editor.rs:174-179` | local signal mutation — committed on save | OK |
| "Save sources" button | `sources_editor.rs:213` | `meta_persona_set_sources` | OK |

### PersonaToolWhitelistEditor (`tool_whitelist_editor.rs`)

| UI action | File:line | MCP tool | Status |
|---|---|---|---|
| Display existing tool whitelist | `tool_whitelist_editor.rs:132-139` | whitelist from `meta_persona_get` detail | OK |
| Toggle individual tool checkbox | `tool_whitelist_editor.rs:175-184` | local signal mutation — committed on save | OK |
| "Save tools" button | `tool_whitelist_editor.rs:205` | `meta_persona_set_tool_whitelist` | OK |

### PersonaManagementRoute (`route.rs`)

| UI action | File:line | MCP tool | Status |
|---|---|---|---|
| Navigate to `/agent/personas` | `route.rs:17` | loads `PersonaListPanel` which calls `meta_persona_list` | OK |

---

## Summary

| Status | Count |
|---|---|
| OK | 20 |
| RECIPE | 1 |
| GAP — owner: Phase F | 2 |
| GAP — owner: Phase H | 1 |

**Total UI actions audited:** 24

Every Phase D gear-menu action either maps to a named tool, has an explicit
composed-call recipe in `docs/personas-cli.md`, or is logged as a real GAP
with a named owner phase.

### GAP details

- **Behaviour section (heartbeat/proactivity/rate-limit)** — displayed read-only
  in Phase D. MCP tool `meta_persona_set_heartbeat` exists for heartbeat, but the
  UI inputs are disabled. Full edit support deferred to Phase F (scheduled
  heartbeat + outbound pipeline). CLI recipe: use
  `poly-cli call meta_persona_set_heartbeat --slug=foo --interval_secs=3600`
  directly today; the UI will wire to it in Phase F.

- **Outbound section** — stub in Phase D. Full outbound allow-list management
  deferred to Phase F. CLI today:
  `poly-cli call meta_persona_set_outbound_allow --slug=foo --account_ids='["..."]'`.

- **Memory delete/pin buttons** — the memory section shows facts read-only in
  Phase D. Delete/pin buttons are deferred to Phase H. CLI today:
  `poly-cli call meta_persona_forget_memory --slug=foo --fact_id=42` and
  `poly-cli call meta_persona_set_memory --slug=foo --fact_text="..." --pinned=true`.
