#!/bin/bash
# Clean up stale web dev server processes
# This ensures the web MCP can start fresh

set -e

echo "🧹 Cleaning up stale web dev processes..."

# Kill any dx serve on wrong port (8080)
echo "  └─ Killing dx serve on port 8080 (wrong port)..."
pkill -f "dx serve.*port.*8080" || true
pkill -f "dx serve.*8080" || true

# Kill any dx serve with hotpatch (wrong mode)
echo "  └─ Killing dx serve with --hotpatch (wrong mode for WASM)..."
pkill -f "dx serve.*hotpatch" || true

# Kill Chrome/Chromium dev debugging
echo "  └─ Killing Chrome/Chromium devtools (CDP port 9222)..."
pkill -f "remote-debugging-port=9222" || true

# Kill old dx processes
echo "  └─ Killing any orphaned dx serve processes..."
pkill -f "dx serve" || true

echo "✅ Cleanup complete!"
echo ""
echo "Now you can safely run:"
echo "  cargo run --bin poly-web-devtools-mcp"
echo ""
echo "Or in VS Code:"
echo "  Run task: Serve: web (MCP + Chromium)"
