# Memory: Scope for wasm crash handler + MCP timeouts

*Stored: 2026-03-10T17:13:49.664287424+00:00*

---

New task scope: add a real WASM/Dioxus crash handler for the app side so route-triggered freezes surface as an in-app crash overlay/loggable failure instead of hanging silently, and add timeout protection across devtools MCP methods on desktop/web/electron so wait/eval/screenshot/connect/navigation/input cannot hang forever.
