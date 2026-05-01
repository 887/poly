# Direct Calls & Temporary Calls

## Status: 📋 DESIGN/REFERENCE DOC — current-state reference for the 1:1 DM call surface in `poly-core`. Not an execution plan; describes the model already shipped.

_Last updated: 2026-03-20_

## Goal

Support 1:1 DM calls and video calls that are **not tied to server voice channels**, while reusing Poly's existing global voice/call controls.

This document covers the current shared-core implementation used by `poly-core`.

## Current Model

Poly already had a global active voice session via:

- `ChatData.voice_connection: Option<VoiceConnection>`
- `ChatData.voice_channel_participants: HashMap<String, Vec<VoiceParticipant>>`
- UI surfaces:
  - `VoiceBanner`
  - `VoiceBar`
  - `VoiceChannelView`

That model assumed **server voice/video channels**.

## Direct / Temporary Call Extension

Direct calls now extend the same model instead of creating a second incompatible call subsystem.

### Connection Kinds

`poly-client::VoiceConnection` now has:

- `kind: VoiceConnectionKind`
  - `ServerChannel`
  - `TemporaryCall`
- `dm_id: Option<String>`
- `participant_user_ids: Vec<String>`

### Temporary Call Behavior

A temporary call is:

- anchored to an account,
- optionally anchored to a DM (`dm_id`) when one exists,
- stored in the same global active-call slot as normal voice connections,
- rendered through the same voice banner / bar controls.

### Held Calls

`ChatData` now also stores:

- `held_voice_connections: Vec<VoiceConnection>`

Rules:

1. Only **one** call is active at a time.
2. Starting a different call parks the current active call into `held_voice_connections`.
3. Disconnecting the active call automatically resumes the most recent held call.
4. The banner and compact voice bar expose a **swap** control for held calls.

This is intentionally Teams-/Discord-inspired rather than a strict single-call-only model.

## Pseudo-Backend Scope

Temporary direct calls are currently implemented as a **UI-local pseudo-backend** in shared core.

That means:

- the call session is synthesized in `poly-core`,
- participants are derived from:
  - the local session user,
  - the DM target user,
  - additional users explicitly added while a temporary call is active,
- no backend/network transport is performed yet,
- media permissions / camera preview reuse the same JS helpers already used by voice-channel UI.

This provides real product/UI behavior without pretending that WebRTC signaling is complete.

## Entry Points

### User Profile Modal

The global profile modal actions now do the following:

- **Message**
  - closes the profile modal,
  - opens or creates the DM,
  - routes to that DM.
- **Call**
  - closes the profile modal,
  - starts a temporary direct call.
- **Video**
  - closes the profile modal,
  - starts a temporary direct call with video as the initial local state.

When a temporary call is already active, profile-call actions may add the selected user to the current temporary call instead of replacing it.

## Pending Call Route

Outgoing direct calls now enter a lightweight route-backed pending phase before the
temporary call is marked connected.

Routes:

- `/:backend/:instance_id/:account_id/dms/:dm_id/call`
- `/:backend/:instance_id/:account_id/dms/:dm_id/video-call`
- `/:backend/:instance_id/:account_id/dms/:dm_id/call/add`
- `/:backend/:instance_id/:account_id/dms/:dm_id/video-call/add`

Behavior:

1. DM header / profile modal call buttons push one of these routes.
2. The route renders the underlying DM plus a cancelable "calling…" overlay.
3. Browser back / swipe-back / × cancel all dismiss the route cleanly before connection.
4. After a short pseudo-backend delay, the route replaces itself back to the DM and only then
  starts the temporary call, allowing the blue voice bar to appear only once the call is
  considered connected.

## DM Header Controls

For 1:1 DMs:

- desktop header now shows **Call** + **Video** buttons in the top-right action area,
- mobile header shows **Call** + **Video** buttons immediately left of the contact/right-wing toggle.

These buttons use the same temporary-call helper as the profile modal, but are interpreted as **start/switch this DM's call** rather than add-to-current-call.

## Banner / Voice Bar Behavior

### Voice Banner

The banner now supports:

- server voice channels,
- temporary direct/group calls.

For temporary calls, the banner's center button routes back to the anchored DM when `dm_id` is available.

### Voice Bar

The compact sidebar voice bar now:

- works for temporary calls too,
- shows the same global active call,
- includes a held-call swap control,
- resumes held calls when disconnecting the current one.

## Add-People Support

Temporary calls can be expanded into ad-hoc group calls.

Current support mechanism:

- if a temporary call is already active,
- and another user's profile is opened,
- pressing **Call** or **Video** can add that user to the active temporary call.

This is intentionally a lightweight first step toward real meeting-style ad-hoc calling.

## Known Limits

This is not full backend/WebRTC meeting support yet.

Missing future pieces include:

- real backend call signaling,
- call ringing / invite acceptance,
- per-call device/session persistence,
- explicit held-call list UI with names and ordering,
- dedicated add-people sheet / picker,
- temporary call history / notifications,
- real temporary-call routes/view instead of banner/bar-first UX.

## Future Backend Contract Direction

When promoted from pseudo-backend to real backend support, likely client-trait additions will include concepts such as:

- create temporary call,
- invite user to active call,
- answer / reject direct call invite,
- hold / resume / transfer,
- fetch active temporary call participants.

For now, shared UI and state intentionally move first so the product model is validated before backend contracts ossify.
