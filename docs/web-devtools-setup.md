# Web Devtools Troubleshooting & Setup

> **TL;DR**: Use the VS Code task `Serve: web (MCP + Chromium)` or `cargo run --bin poly-web-devtools-mcp`. Never use `--hotpatch` for web.

---

## Common Issues & Fixes

### Issue: Browser stuck on "Your app is being rebuilt"

**Cause**: A stale `dx serve` with `--hotpatch` is running (breaks Dioxus 0.7.3 WASM).

**Fix**:
```bash
# Option 1: Run the cleanup script
./scripts/web-cleanup.sh

# Option 2: Manual cleanup
pkill -f "dx serve"
pkill -f "remote-debugging-port=9222"

# Then restart via:
cargo run --bin poly-web-devtools-mcp
```

---

### Issue: "Failed to connect to Chrome CDP at http://127.0.0.1:9222"

**Cause**: Either:
1. Chrome crashes and MCP watchdog hasn't restarted it yet
2. Port 9222 is in use by another process
3. `poly-web-devtools-mcp` isn't running

**Fix**:
```bash
# Kill any stale Chrome/Chromium
pkill -f "remote-debugging-port=9222"

# Wait 2 seconds
sleep 2

# Restart the MCP (it will relaunch Chromium)
cargo run --bin poly-web-devtools-mcp
```

---

### Issue: Port 3000 already in use

**Cause**: A previous `dx serve` is still running on 3000.

**Fix**:
```bash
# Find what's on port 3000
lsof -i :3000  # or
ss -tlnp | grep 3000

# Kill it
pkill -f "dx serve.*3000"
pkill -f "port 3000"

# Or kill all dx serve
pkill -f "dx serve"
```

---

### Issue: Port 8080 has stale dx serve

**Cause**: Old manual `dx serve` (no `--platform web --port 3000`) is still running from desktop or another target.

**Fix**:
```bash
# The MCP now auto-kills this on startup, but you can also:
pkill -f "dx serve.*8080"
pkill -f "dx serve"  # Kill all to be safe
```

---

## Correct Usage Patterns

### ✅ Recommended: Use the Web MCP

**VS Code Task**:
1. Open command palette: `Ctrl+Shift+P` (or `Cmd+Shift+P` on Mac)
2. Type: `Tasks: Run Task`
3. Select: `Serve: web (MCP + Chromium)`
4. Wait ~5 seconds for the build and Chrome to launch

**Or command line**:
```bash
cd /home/laragana/workspcacemsg
cargo run --bin poly-web-devtools-mcp
```

**Expected output**:
```
Started `dx serve` on port 3000 (building...)
Hot reload is active — file changes trigger automatic WASM recompile.
Launched Chrome (visible window) with CDP on port 9222
Wait ~3 seconds then call connect_cdp.
```

### ⚠️ Manual Development (If Needed)

Only use if you're not using the devtools/Copilot integration:

```bash
# Kill any existing dx serve first
pkill -f "dx serve"

# Start on port 3000 (NOT 8080, NOT with --hotpatch)
cd apps/web
dx serve --platform web --port 3000

# Open browser manually to http://localhost:3000
```

---

## Port Reference

| Port | Use | Status |
|---|---|---|
| **3000** | Web app (via MCP or manual) | ✅ Correct |
| 8080 | Desktop Wry hotpatch | ⚠️ Conflicts with web |
| 9222 | Chrome DevTools (CDP) | ✅ Correct |

---

## Prevention: Update VS Code Launchers

Make sure `.vscode/launch.json` and `.vscode/tasks.json` have:

**For Web Launch**:
```json
"args": ["serve", "--platform", "web", "--port", "3000"]
```

**For Web Task**:
```json
"args": ["run", "--bin", "poly-web-devtools-mcp"]
```

Never use `--hotpatch` for web.

---

## If All Else Fails

Run the full cleanup and restart:

```bash
# Full nuclear option
./scripts/web-cleanup.sh

# Verify ports are free
lsof -i :3000 || echo "Port 3000 free ✓"
lsof -i :8080 || echo "Port 8080 free ✓"
lsof -i :9222 || echo "Port 9222 free ✓"

# Restart the MCP
cargo run --bin poly-web-devtools-mcp
```

Then in VS Code:
1. **Reload Window** (`Developer: Reload Window`)
2. Run task: `Serve: web (MCP + Chromium)`
3. Wait for "Port 3000" and "Port 9222" messages

---

## Reference: Web MCP Architecture

```
VS Code / Copilot
    ↓ JSON-RPC stdio
poly-web-devtools-mcp  (cargo run --bin poly-web-devtools-mcp)
    ├─ Starts: dx serve --platform web --port 3000
    │   └─ Builds: poly-web (WASM) + serves on http://127.0.0.1:3000
    └─ Starts: Chromium --remote-debugging-port=9222
        └─ Loads: http://127.0.0.1:3000
            └─ Poly UI runs in browser
```

---

## Key Project Decisions

| Decision | Reason |
|---|---|
| **Port 3000 (not 8080)** | 8080 is claimed by desktop hotpatch; web is separate |
| **No `--hotpatch` for web** | Dioxus 0.7.3 WASM doesn't support experimental hotpatch yet |
| **Auto-clean on startup** | Prevents user confusion from stale processes |
| **Chromium auto-restart** | Users can close the browser window and it'll come back |

---

**Last Updated**: 2026-03-07  
**Web MCP Crate**: `mcp/web-devtools-mcp/`  
**Web App Crate**: `apps/web/`
