#!/usr/bin/env python3
"""
Test script for poly-electron-devtools-mcp.
Starts the MCP server, launches Electron, connects via CDP, and takes a screenshot.
"""

import subprocess
import json
import sys
import time
import base64
import os
import threading

WORKSPACE = "/home/laragana/workspcacemsg"
MCP_BIN = f"{WORKSPACE}/target/debug/poly-electron-devtools-mcp"
SCREENSHOT_PATH = f"{WORKSPACE}/devtools-screenshots/electron-test.png"

def send(proc, msg):
    line = json.dumps(msg) + "\n"
    proc.stdin.write(line.encode())
    proc.stdin.flush()

def recv(proc, timeout=5):
    """Read a JSON-RPC response line."""
    proc.stdout.settimeout = None  # not applicable for Popen stdout
    import select
    r, _, _ = select.select([proc.stdout], [], [], timeout)
    if not r:
        return None
    line = proc.stdout.readline()
    if not line:
        return None
    return json.loads(line.decode().strip())

def call_tool(proc, tool_id, name, arguments=None, timeout=300):
    """Send a tools/call request and wait for the response."""
    send(proc, {
        "jsonrpc": "2.0",
        "id": tool_id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments or {}}
    })
    start = time.time()
    while time.time() - start < timeout:
        resp = recv(proc, timeout=min(30, timeout - (time.time() - start) + 1))
        if resp is None:
            continue
        if resp.get("id") == tool_id:
            return resp
        # Otherwise it's a notification or different id - skip
    return None

def main():
    os.makedirs(os.path.dirname(SCREENSHOT_PATH), exist_ok=True)

    print(f"[*] Starting MCP server: {MCP_BIN}")
    proc = subprocess.Popen(
        [MCP_BIN],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=sys.stderr,
        cwd=WORKSPACE,
    )

    # ── Initialize handshake ──────────────────────────────────────────────────
    print("[*] Sending initialize...")
    send(proc, {
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "electron-test", "version": "0.1"}
        }
    })
    init_resp = recv(proc, timeout=10)
    if init_resp is None:
        print("[!] No initialize response received")
        proc.kill()
        sys.exit(1)
    print(f"[+] Server info: {init_resp.get('result', {}).get('serverInfo', {})}")

    # Send initialized notification
    send(proc, {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})

    # ── launch_app ────────────────────────────────────────────────────────────
    print(f"\n[*] Calling launch_app (workspace={WORKSPACE})...")
    print("    This will: kill stale Electron → dx build --platform web → npm install → launch Electron")
    print("    dx build may take 1-2 min on cold cache, ~30s on warm cache...")

    resp = call_tool(proc, 1, "launch_app", {"workspace": WORKSPACE}, timeout=360)
    if resp is None:
        print("[!] launch_app timed out")
        proc.kill()
        sys.exit(1)

    result = resp.get("result", {})
    is_err = result.get("isError", False)
    content = result.get("content", [])
    text = content[0].get("text", "") if content else str(result)
    if is_err:
        print(f"[!] launch_app error:\n{text}")
        proc.kill()
        sys.exit(1)
    print(f"[+] launch_app result:\n{text}")

    # Wait for Electron to fully initialize
    print("\n[*] Waiting 6 seconds for Electron to initialize...")
    time.sleep(6)

    # ── connect_cdp ───────────────────────────────────────────────────────────
    print("[*] Calling connect_cdp...")
    resp = call_tool(proc, 2, "connect_cdp", {}, timeout=30)
    if resp is None:
        print("[!] connect_cdp timed out")
        proc.kill()
        sys.exit(1)

    result = resp.get("result", {})
    is_err = result.get("isError", False)
    content = result.get("content", [])
    text = content[0].get("text", "") if content else str(result)
    if is_err:
        print(f"[!] connect_cdp error:\n{text}")
        proc.kill()
        sys.exit(1)
    print(f"[+] connect_cdp: {text}")

    # ── take_screenshot ───────────────────────────────────────────────────────
    print("\n[*] Calling take_screenshot...")
    resp = call_tool(proc, 3, "take_screenshot", {"format": "png"}, timeout=60)
    if resp is None:
        print("[!] take_screenshot timed out")
        proc.kill()
        sys.exit(1)

    result = resp.get("result", {})
    is_err = result.get("isError", False)
    content = result.get("content", [])

    if is_err:
        text = content[0].get("text", str(result)) if content else str(result)
        print(f"[!] take_screenshot error: {text}")
        proc.kill()
        sys.exit(1)

    # Find the image content item
    for item in content:
        if item.get("type") == "image":
            b64_data = item.get("data", "")
            image_bytes = base64.b64decode(b64_data)
            with open(SCREENSHOT_PATH, "wb") as f:
                f.write(image_bytes)
            print(f"[+] Screenshot saved! {len(image_bytes):,} bytes → {SCREENSHOT_PATH}")
            break
    else:
        # No image found — check for text (could be a file path)
        for item in content:
            if item.get("type") == "text":
                print(f"[+] Screenshot text result: {item.get('text', '')}")
        print("[!] No image content in screenshot response")

    print("\n[*] Test complete! Killing Electron and MCP server...")
    call_tool(proc, 4, "kill_app", {}, timeout=10)
    proc.kill()
    proc.wait()
    print("[+] Done.")


if __name__ == "__main__":
    main()
