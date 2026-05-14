# Video Codec Strategy

## Architecture

```
Browser (apps/web)               Native shells (apps/desktop*, poly-host)
┌─────────────────────┐          ┌──────────────────────────────────────────┐
│ WebCodecs API       │          │ NativeVideoBackend (crates/video-backend) │
│ (HW-accelerated)    │          │  - nokhwa: camera capture (V4L2/MSMF/AVF)│
│                     │          │  - scap: screen capture (PipeWire/etc.)   │
│ encode → ReadableStream        │  ↓                                        │
│ decode ← WritableStream        │ POST /host/video/encode_h264              │
└─────────────────────┘          │ POST /host/video/decode_h264              │
                                 │          ↓                                 │
                                 │ poly-host axum server (port 9333)         │
                                 │  → video.rs handlers                      │
                                 │  → openh264-rs (Cisco reference impl)     │
                                 └──────────────────────────────────────────┘
```

## Why host-bridge for H.264?

The user directive: *"if we have to ship H.264 encoding, make that part of the
host functions so we can do it for all plugins."*

Putting the codec in `poly_host` (the axum server side of the host bridge) means:

1. **Single codec instance** — one openh264 context shared across all plugins
   and the native video backend. No N separate statically-linked copies.
2. **WASM isolation** — openh264 is a heavy C library. Keeping it out of the
   WASM bundle keeps WASM load time low. Browser targets use the native
   WebCodecs API instead (HW-accelerated, no licensing concern).
3. **Easy swap** — the HTTP API surface is stable. Swapping openh264 for
   ffmpeg, GStreamer, or AV1 later only changes the handler implementation,
   not any callers.
4. **Plugin access** — any WASM plugin can POST to `/host/video/encode_h264`
   via the same host bridge HTTP call it already uses for KV and exec.

## Cisco / MPEG-LA Patent Licensing

H.264 is patent-encumbered. Cisco's reference encoder (`openh264`) ships in
two modes:

| Mode | Cargo feature | How codec is obtained | Cisco patent grant? |
|------|--------------|----------------------|---------------------|
| Source build | `openh264 = { features = ["source"] }` | Compiled from Cisco's C source at build time | **NO** — BSD-2-Clause only |
| Binary load | `openh264 = { features = ["libloading"] }` | Cisco's pre-built `.so`/`.dll`/`.dylib` loaded at runtime | **YES** — Cisco's MPEG-LA grant attaches |

**Current landing uses `source`** (`crates/host-bridge/Cargo.toml`). This is
correct for development and internal use. For consumer distribution:

- **Option A (recommended for native desktop)**: Switch to `features =
  ["libloading"]` and bundle/download the Cisco binary with the app. Cisco
  provides the binary for free; their grant protects end-users and distributors.
- **Option B (royalty-free, more work)**: Replace the entire encode/decode path
  with AV1. `rav1e` (encoder) and `dav1d` (decoder) are BSD-licensed with no
  patent concern. Requires updating callers to negotiate AV1 instead of H.264.

**Action required before shipping to end-users**: pick Option A or B and update
the `video` feature in `crates/host-bridge/Cargo.toml` accordingly. Add a note
to the release checklist.

## Frame Format Conventions

| Format | Where used | Notes |
|--------|-----------|-------|
| `bgra` | Input from screen-capture (scap) | 4 bytes/pixel, B first |
| `nv12` | Input from camera (nokhwa on some platforms) | Semi-planar YUV 4:2:0 |
| `yuv420p` | Native openh264 input; canonical decoded output | Planar YUV 4:2:0 |

The encode handler converts `bgra` and `nv12` to `yuv420p` before passing to
openh264. The decode handler always returns `yuv420p`. Callers convert to their
preferred display format (BGRA, RGBA, etc.) themselves — this avoids baking a
display-specific format into the wire protocol.

## JSON + Base64 Transport Overhead

Binary fields (`data`, NAL units) are base64-encoded in the JSON body.
Base64 adds ~33% size overhead over raw bytes.

Rough numbers for 1080p30 (uncompressed BGRA → encode request):
- Raw frame: 1920 × 1080 × 4 = ~8 MB
- Base64: ~10.7 MB
- After H.264 encode: typically 50–200 KB per frame at 2 Mbps

So the _request_ payload is large (10 MB), but the _response_ (encoded NAL
units) is small. For screen share this is acceptable on loopback IPC.

**When to optimize**: if encode latency becomes the bottleneck (e.g. 60fps
screen share), consider:

1. **Binary endpoint**: replace JSON+base64 with a multipart/octet-stream or
   a custom binary framing. Same route, different content-type — `VideoBridgeClient`
   would add a `encode_raw()` method that posts raw bytes.
2. **In-process FFI**: link openh264 directly into `NativeVideoBackend` and
   skip the HTTP hop entirely. This breaks the "shared for all plugins" property
   but is appropriate when the video backend is the only codec consumer.

## Session Lifecycle

Encoders and decoders are stateful (SPS/PPS parameter sets, reference frames).
Each video stream gets a `session_id` (any unique string; UUID is fine):

```
POST /host/video/encode_h264  { session_id: "stream-0", ... }  → NAL units
POST /host/video/encode_h264  { session_id: "stream-0", ... }  → NAL units
... (repeat for each frame)
POST /host/video/close_session { session_id: "stream-0" }      → cleanup
```

Sessions are not automatically evicted. If a caller crashes without closing its
session, the encoder/decoder map accumulates orphaned entries. A future
idle-timeout sweep can GC them without breaking the API.

## Dependencies

- `openh264 = "0.9"` — Rust bindings for Cisco's reference H.264 codec.
  Feature `source` builds the codec from C source (cmake + nasm required at
  build time). See `crates/host-bridge/Cargo.toml`.
- Feature flag: `poly-host-bridge/video` (default OFF). Opt in via
  `poly-host/video` feature. All shells can enable it independently.
