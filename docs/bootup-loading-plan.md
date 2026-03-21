# Bootup Loading Plan

> Created: 2026-03-21
> Status: Proposed

## Goal

Hide startup flashes behind a short, debuggable boot overlay that stays visible until the shell, persisted state, and initial account bootstrap work are ready enough to reveal smoothly.

## Product Direction

- Show a centered floating boot window on desktop and a full-screen boot view on mobile.
- Put account icons across the top with per-account status badges like loading, connected, cached, error.
- Render a scrolling boot log underneath, similar to a Linux boot log, with structured events from storage restore and account connection work.
- Keep the main UI rendering behind the overlay so CSS, layout, avatars, and restored state can settle without visible flashes.
- Never block forever on account connections; the overlay should gate only the app-shell readiness plus a short minimum animation window.

## Non-Goals

- Do not wait for every account websocket to connect before showing the app.
- Do not make this a generic splash screen with no diagnostics.
- Do not break MCP / browser automation flows that depend on fast reload cycles.

## User Experience

### Desktop

- Dimmed app background with the real UI mounted behind it.
- Centered floating boot panel.
- Top row: account pills/icons with status dots/checkmarks/spinners.
- Bottom section: monospaced boot log with timestamped events.
- Smooth fade/slide from overlay out to the real UI in.

### Mobile

- Full-screen boot surface using the same data model.
- Account row stays near the top.
- Boot log fills the main body.

## Readiness Model

Split startup into two separate concepts:

1. Visual readiness
   - base CSS loaded
   - theme applied
   - root layout mounted
   - persisted shell/navigation/settings restored
   - first stable frame rendered behind the overlay

2. Account bootstrap progress
   - IndexedDB / SurrealKV reads
   - account session restore
   - plugin/backend init
   - websocket / worker connect attempts
   - initial sync snapshots

The overlay hides only on visual readiness plus a minimum display time (target around 500 ms), not on full account completion.

## Proposed Architecture

### 1. Startup Coordinator

Add a shared startup coordinator in `poly-core` that owns:

- global phase enum: `Booting`, `ReadyToReveal`, `Revealed`
- min-visible timer tracking
- structured boot events list
- per-account boot state map
- URL override state

Suggested state shape:

```rust
struct StartupBootState {
    phase: StartupPhase,
    overlay_enabled: bool,
    started_at_ms: f64,
    min_visible_ms: u32,
    visual_ready: bool,
    reveal_allowed: bool,
    events: Vec<BootEvent>,
    accounts: HashMap<String, AccountBootState>,
}
```

### 2. Boot Event Stream

Every meaningful startup step emits a structured event:

- `storage.open.begin`
- `storage.open.done`
- `settings.restore.begin`
- `settings.restore.done`
- `account.restore.begin`
- `account.restore.done`
- `account.connect.begin`
- `account.connect.ok`
- `account.connect.err`
- `ui.css.ready`
- `ui.layout.stable`
- `boot.reveal`

Each event should include:

- timestamp
- scope (`app` or account id)
- phase key
- human-readable message
- optional severity

This should be easy to surface in the overlay and easy to inspect in MCP/devtools.

### 3. Overlay Rendering Strategy

- Mount the normal app immediately behind the overlay.
- Add a top-level boot overlay component near the root `App` / `MainLayout` boundary.
- Keep overlay `position: fixed` and above all normal UI.
- Prevent pointer interaction with the hidden app until reveal.
- Reveal with opacity/transform transition once startup coordinator says ready.

Important: avoid `display: none` for the app shell during boot; render it behind the overlay so layout and assets can settle off-screen.

### 4. Stable Reveal Heuristic

Reveal when all are true:

- theme + shell state restored
- first route resolved
- one or two `requestAnimationFrame` ticks after root render
- minimum visible duration reached

Optionally extend the overlay briefly if heavy persisted state restore is still in-flight.

## URL / Debug Controls

Add query params similar to existing layout overrides:

- `?boot=off` or `?startup=off` -> disable overlay entirely
- `?boot=on` -> force overlay even in fast local dev
- `?bootlog=verbose` -> show extra structured events
- `?bootmin=0` -> no minimum duration for debugging

Recommended default for MCP/devtools:

- document that the boot overlay may appear for about 500 ms or until readiness completes
- use `?boot=off` for tests that need zero animation or ultra-fast reload loops

## MCP / Automation Compatibility

To avoid confusing reload/debug agents:

- expose a deterministic DOM marker like `data-poly-startup-phase="booting|ready|revealed"`
- expose `window.__polyStartupState` for devtools inspection
- expose a simple JS predicate such as `window.__polyStartupState?.phase === 'revealed'`
- teach MCP workflows to wait for reveal unless `?boot=off` is used

## Account Status UX

Per-account icon row should show:

- spinner while restoring/connecting
- checkmark when initial bootstrap reached a healthy state
- warning/error badge on failure
- optional cached marker when restored from local storage before live connect

This status should stay meaningful after the overlay disappears, so reuse the same underlying account boot state where possible.

## Storage / Backend Integration

Emit boot events from:

- SurrealDB / IndexedDB init
- settings restore
- account session restore
- plugin/backend construction
- worker launch (future)
- websocket connect + first sync receipt (future)

Do not invent a second parallel state system if existing connection/account state can be extended.

## Incremental Delivery Plan

### Phase A - Foundation

- Add startup state model and boot event registry.
- Add query param parsing for boot overlay on/off.
- Add DOM + `window` debug hooks.

### Phase B - Visual Overlay

- Build desktop/mobile overlay UI.
- Add minimum 500 ms display guard.
- Render app behind overlay and smooth reveal.

### Phase C - Real Event Wiring

- Wire storage restore events.
- Wire theme/settings/layout readiness.
- Wire current account init / reconnect steps.

### Phase D - Account Progress Polish

- Add per-account checkmarks/spinners/error badges.
- Add richer boot log formatting and severity colors.
- Support future worker/websocket milestones.

### Phase E - Automation + Docs

- Update `agents.md` / MCP docs with boot overlay behavior.
- Add visual verification checklist for startup flows.
- Add a recommended `?boot=off` path for rapid reload debugging.

## Verification Plan

- Web: normal launch with boot overlay visible and reveal smooth.
- Web: `?boot=off` disables overlay completely.
- Desktop layout: floating centered boot window.
- Mobile layout: full-screen boot screen.
- Slow simulated storage/backend path keeps overlay visible without flashes.
- MCP can reliably wait on startup phase marker.

## Open Questions

- Whether reveal should wait for avatar/icon image decode, or only shell/layout stability.
- Whether old boot events remain inspectable after reveal via diagnostics.
- Whether account boot logs should merge into the existing diagnostics page later.

## Recommendation

Start with Phase A plus Phase B first. The key win is eliminating flash/jank while creating a durable startup contract that future real backend boot work can plug into.
