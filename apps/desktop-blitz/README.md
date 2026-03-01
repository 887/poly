# apps/desktop-blitz

Experimental **Poly** desktop app entry point using [Dioxus](https://dioxuslabs.com/) with the **Blitz** native renderer (WGPU-based, no system webview).

## Purpose

An alternative desktop build that renders Poly's UI via Blitz — Dioxus's native GPU renderer — instead of a system webview. Because Blitz renders directly to the GPU via WGPU, it has no dependency on the OS webview and achieves consistent cross-platform rendering identical to the web target.

> **Status: Experimental.** Blitz is under active development in the Dioxus ecosystem. This entry point tracks upstream progress.

## How It Works

Same lifecycle as `apps/desktop` but launched with Blitz's runtime instead of Wry. The `poly-core` `App` component is unchanged.

## Running

```bash
# Development
dx serve --platform desktop

# Production build
dx build --release --platform desktop
```

## Key Differences from `apps/desktop`

| | `apps/desktop` (Wry) | `apps/desktop-blitz` (Blitz) |
|---|---|---|
| Renderer | OS system webview | WGPU (GPU native) |
| CSS support | Full browser CSS | Subset (Blitz-supported) |
| Binary size | Small | Larger (includes renderer) |
| Stability | Stable | Experimental |

## License

MIT / Apache-2.0
