# Phase: Voice & Video — Plan

> **Created:** 2026-03-08  
> **Status:** In Progress  
> **Scope:** Full-width voice dock, real screenshare/webcam, audio settings, RNNoise

---

## Goal

Give Poly a fully testable voice/video experience in the demo client:

1. **VoiceBar becomes a full-width bottom dock** — spans the channel list column AND the  
   main content area, matching Discord's voice panel design. Participant tiles appear inline.

2. **Real screen sharing** — `getDisplayMedia()` via browser JS interop; live preview  
   shown inside the dock/voice view.

3. **Real webcam capture** — `getUserMedia({video: true})` via browser JS interop;  
   local camera feed shown in a preview panel.

4. **Audio settings** — mic device picker, speaker device picker, volume control, and  
   a noise-cancellation toggle (nnnoiseless / RNNoise Rust implementation).

5. **Demo client voice state** — the demo client returns realistic voice participants  
   so the voice dock/view is non-trivial to test.

---

## Architecture

### Voice Dock Layout

```
┌─ FavoritesBar ─┬─ AccountServerBar ─┬─────────────────────────────────┐
│                │                    │ account-view-shell (flex col)   │
│                │                    │ ┌─────────────────────────────┐ │
│                │                    │ │ account-view-main (flex row) │ │
│                │                    │ │ ┌──────────┬──────────────┐ │ │
│                │                    │ │ │channel   │  main        │ │ │
│                │                    │ │ │list      │  content     │ │ │
│                │                    │ │ │wrapper   │  (Outlet)    │ │ │
│                │                    │ │ │          │              │ │ │
│                │                    │ │ │ (no more │              │ │ │
│                │                    │ │ │ VoiceBar)│              │ │ │
│                │                    │ │ │ AccountBar              │ │ │
│                │                    │ │ └──────────┴──────────────┘ │ │
│                │                    │ │ VoiceDockBar (full width)   │ │
│                │                    │ │ [Status │ Participants │ Ctrls]│ │
│                │                    │ └─────────────────────────────┘ │
└────────────────┴────────────────────┴─────────────────────────────────┘
```

### VoiceDockBar Sections

```
[● Voice Connected       ] [👤 Dog (demo)  👤 Alice ...] [🎤 🔕 📹 🖥️  ⚙️  📵]
[Reading Night / Book Club]  (participant mini-tiles row)  (control buttons)
 LEFT: Status info             CENTER: Participants          RIGHT: Controls
```

### Media Capture Pipeline

```
getUserMedia() → MediaStream → window.__polyCameraStream → <video#poly-local-camera>
getDisplayMedia() → MediaStream → window.__polyScreenStream → <video#poly-local-screen>

Rust state (VoiceConnection.is_video_on, is_streaming) tracks toggle state.
JS (via document::eval) manages MediaStream lifecycle.
Cross-render safety: <video> elements always rendered (hidden via CSS), srcObject 
reattached on mount via onmounted JS eval.
```

### Audio/Noise Pipeline (phase 2 — WASM AudioWorklet)

```
Mic MediaStream
  → AudioContext.createMediaStreamSource()
  → AudioWorklet (RNNoise WASM module)
    → nnnoiseless::DenoiseState::new().process_frame()
  → AudioContext.createMediaStreamDestination()
  → Processed MediaStream → WebRTC send track (future)
```

For Phase 1, the toggle exists in UI. Native noise cancellation works on non-WASM  
targets. WASM AudioWorklet integration is Phase 2.

---

## Checklist

### Phase 1 — Layout & UI (this session)

- [x] Create `docs/phase-voice-plan.md` (this file)
- [ ] CSS: Add `.account-view-shell` (flex column container)
- [ ] CSS: Add `.account-view-main` (flex row: channel list + main content)
- [ ] CSS: Redesign `.voice-bar` to be full-width horizontal dock
- [ ] CSS: Add `.voice-dock-info` (left status section)
- [ ] CSS: Add `.voice-dock-participants` (center scrollable mini-tile row)
- [ ] CSS: Add `.voice-dock-tile` (individual participant mini-tile)
- [ ] CSS: Add `.voice-dock-controls` (right buttons section)
- [ ] CSS: Add `.voice-preview-panel` (floating video preview panel)
- [ ] CSS: Add `.voice-preview-video` (video element styling)
- [ ] CSS: Add `.voice-settings-popup` (audio settings overlay)
- [ ] `routes.rs`: Wrap DmsLayout in `account-view-shell` / `account-view-main`
- [ ] `routes.rs`: Wrap ServerLayout in `account-view-shell` / `account-view-main`
- [ ] `routes.rs`: Remove VoiceBar from inside `channel-list-wrapper`
- [ ] `routes.rs`: Remove `voice-view-right-spacer` hack
- [ ] `voice_bar.rs`: Redesign as horizontal full-width dock
  - [ ] Left: `VoiceDockStatus` (green dot, connected label, channel/server name)
  - [ ] Center: `VoiceDockParticipants` (scrollable row of mini-tiles)
  - [ ] Right: `VoiceDockControls` (mute, deafen, camera, screen, settings, disconnect)
  - [ ] Camera button: JS `getUserMedia` / stop stream
  - [ ] Screen button: JS `getDisplayMedia` / stop stream  
  - [ ] Video preview elements (always rendered, CSS hidden when inactive)
  - [ ] `onmounted` reattach JS for video elements
  - [ ] Settings gear button → opens audio settings popup
- [ ] `chat_data.rs`: Add `VoiceMediaSettings` struct
- [ ] `chat_data.rs`: Add `voice_media_settings: VoiceMediaSettings` to `ChatData`
- [ ] New: `voice_settings.rs` — Audio settings popup component
  - [ ] Mic device picker (JS `enumerateDevices`)
  - [ ] Speaker device picker
  - [ ] Noise cancellation toggle
  - [ ] Test microphone button
  - [ ] Close button
- [ ] `voice_bar.rs`: Add settings popup signal + render `VoiceSettingsPopup`

### Phase 1 — Demo Client

- [ ] Verify `get_voice_participants` returns participants for voice channels
- [ ] Make demo voice channel have 2-3 pre-populated participants on join

### Phase 1 — i18n + Crates

- [ ] FTL (`locales/en/main.ftl`): Add `voice-audio-settings`, `voice-noise-cancel`,  
  `voice-noise-cancel-desc`, `voice-mic-device`, `voice-speaker-device`, `voice-test-mic`,  
  `voice-screen-sharing`, `voice-camera-preview`, `voice-default-device`,  
  `voice-stop-share`, `voice-stop-camera`
- [ ] Sync new FTL keys to `de/`, `fr/`, `es/` locale files
- [ ] `Cargo.toml` (workspace): Add `nnnoiseless = "0.5"`
- [ ] `crates/core/Cargo.toml`: Add `nnnoiseless`

### Phase 1 — Validation

- [ ] `cargo check --workspace` — zero errors
- [ ] `cargo cranky --workspace` — zero warnings
- [ ] `cargo check -p poly-web --target wasm32-unknown-unknown` — WASM clean
- [ ] Web MCP visual test: join voice channel, verify full-width bar appears
- [ ] Web MCP visual test: click camera button, verify video preview shows
- [ ] Web MCP visual test: click screen share button, verify picker dialog  
- [ ] Web MCP visual test: click settings gear, verify audio settings popup

---

### Phase 2 — WebRTC (future)

- [ ] Add `webrtc` crate (native targets only; WASM uses browser WebRTC APIs)
- [ ] Implement WebRTC signaling protocol in `ClientBackend` trait
  - `open_voice_connection(channel_id)` → negotiates SDP offer/answer
  - `send_ice_candidate(channel_id, candidate)` → trickle ICE
  - `close_voice_connection(channel_id)`
- [ ] Stoat client: implement Revolt voice via Vortex protocol
- [ ] Voice event stream: `VoiceUserJoined`, `VoiceUserLeft`, `VoiceStateUpdated`
- [ ] Real participant tracking (live updates, speaking indicator via audio level)
- [ ] Bidirectional audio send/receive

### Phase 3 — RNNoise WASM (future)

- [ ] Create `crates/noise-processor/` with AudioWorklet glue JS
- [ ] Expose `process_noise(samples: &[f32]) -> Vec<f32>` with `wasm-bindgen`
- [ ] Build custom AudioWorklet that calls into the WASM module
- [ ] Bundle the worklet with `dx build` asset pipeline
- [ ] Wire mic capture → worklet → processed stream (replaces raw mic track)

---

## Design Decisions

| # | Decision | Rationale |
|---|---|---|
| V-1 | VoiceBar moved to full-width dock outside `channel-list-wrapper` | Matches Discord UX; shows participants inline without taking the whole main area |
| V-2 | JS `eval()` for `getUserMedia`/`getDisplayMedia` | Dioxus doesn't expose WebAPI bindings; eval is the pragmatic WASM bridge |
| V-3 | Always-rendered `<video>` elements with CSS hide/show | Avoids Dioxus DOM recreation clearing `srcObject`; `onmounted` callback reattaches streams |
| V-4 | `nnnoiseless` added now, WASM AudioWorklet integration is Phase 3 | Separates the easy Rust API from the complex browser AudioWorklet plumbing |
| V-5 | Demo client populates 2-3 fake participants on join | Makes the voice dock non-trivial; real backends supply participants via `get_voice_participants` |
| V-6 | Video preview floats above the VoiceDockBar | Doesn't interfere with channel/message layout; easy to dismiss |

---

## Session Log

### 2026-03-08 (initial implementation)

- Analyzed existing `VoiceBar` (vertical 240px-wide column inside `channel-list-wrapper`)
- Analyzed existing `VoiceChannelView` (full-screen participant grid in main area)
- Analyzed `VoiceBanner` (decorative top-of-screen banner, kept as-is)
- Planned new `account-view-shell` wrapper to contain channel list + main + voice dock
- Implemented layout, VoiceBar redesign, JS media interop, audio settings component
- Added nnnoiseless to Cargo.toml for future WASM noise pipeline
