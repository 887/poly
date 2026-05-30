# Plan: discord voice/voice_bridge cranky-zero (feature-gated surface)

## Status: TODO — 592 lints, deferred from the workspace-cranky sweep 2026-05-30

> Discovered when `cargo cranky --workspace` was run end-to-end (vs the
> per-crate `--lib` default-feature sweeps that drove the earlier
> "41/41 at 0" result). The voice code is behind the `voice` /
> `voice-bridge` / `gateway` / `gateway-bridge` feature flags, so
> default-feature lib builds never compiled it and never linted it. It is
> almost certainly PRE-EXISTING (not introduced by the dep upgrade).

## Scope
~592 lints, essentially all in `clients/discord/src/voice/` and
`clients/discord/src/voice_bridge/` (worst: `voice/video.rs`), plus a few in
`gateway_bridge.rs`. Enabled by discord features:
- `gateway` → tokio-tungstenite + tokio-stream
- `voice` = gateway + audiopus (Opus encode/decode — NATIVE ONLY, no WASM)
- `voice-bridge` = poly-audio-backend + poly-video-backend
- `gateway-bridge` = gloo-net + gloo-timers

## Lint breakdown (by type, from the cranky --workspace log)
| count | lint | nature |
|---|---|---|
| 95 | as_conversions (silent `as`) | codec/sample math |
| 73 | default_numeric_fallback | codec math literals |
| 73 | arithmetic_side_effects | sample-buffer index/offset math |
| 47 | cast_possible_truncation | uN→uN downcasts |
| 44 | indexing_slicing (index may panic) | frame/sample buffers |
| 27 | redundant_closure | style |
| 20 | future_not_send | async voice tasks (single-thread executor) |
| 15 | significant_drop_tightening | lock scoping |
| 13 | slicing may panic | buffers |
| 13 | cast u→u via From | style |
| 12 | cognitive_complexity | codec decode fns |
| 12 | non-binding let on must_use | style |
| 11 | wildcard_imports | style |
| ~ | misc (map_err_ignore, let_else, too_many_lines, const_fn, RefCell-across-await ×3, etc.) | |

## Strategy (NOT yet executed — design decision needed)
The as/cast/arithmetic/default_numeric storm (≈288 of 592) is **codec/DSP
math**, where per-site `as`→`TryFrom` rewrites are noisy and arguably wrong
(sample math genuinely wants wrapping/truncating `as`). The honest, low-risk
move for THAT class is a **scoped module-level `#![allow]`** on the voice
modules with a rationale, NOT 288 per-site edits — analogous to how
`codec_opus_server` was handled earlier (see git log
`fe356886 style(lint): const fn + cognitive_complexity allow in
codec_opus_server`).

Remaining non-math classes (indexing/slicing panics, RefCell-across-await ×3,
future_not_send, drop_tightening) deserve per-site review — `indexing_slicing`
in particular is a real panic-class lint, BUT in fixed-size codec frame buffers
it's often provably-in-bounds; review each.

### Phases (to be filled in when this is picked up)
- [ ] **A.1** Confirm the voice lints are pre-existing (lint a pre-dep-upgrade
      rev with the same feature set; rule out the dep bump as a cause)
- [ ] **A.2** Decide math-class policy: scoped `#![allow]` for
      as_conversions/cast_*/arithmetic_side_effects/default_numeric_fallback on
      `voice/` + `voice_bridge/` modules, with rationale comment
- [ ] **A.3** Per-site review the panic-class + correctness lints
      (indexing_slicing, slicing, RefCell-across-await, future_not_send)
- [ ] **A.4** Verify `cargo cranky --workspace` four-guard zero with discord
      voice features enabled
- [ ] **A.5** Mark DONE

## Verification command (the source of truth — NOT per-crate --lib)
```
cargo cranky --workspace 2>cranky.log
# four-guard: Finished>=1 AND could-not-compile=0 AND ^error=0 AND index.html#=0
```
NOTE: per-crate `cargo clippy -p X --lib` does NOT see feature-gated code or
bin targets — it was the blind spot that hid these 592. Always verify with
`cargo cranky --workspace`.

## Also remaining (tiny, outside discord-voice — fold in or fix ad hoc)
- `tools/stoat-voice-smoke/src/main.rs` — 4 (cognitive_complexity 111, too_many_lines, map_err_ignore, map_unwrap_or)
- `crates/plugin-host-tests/src/lib.rs:76` — 1 (significant_drop_tightening)
- `clients/server-client/src/http.rs:212` — 1
