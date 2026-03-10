//! # poly-desktop-devtools-mcp
//!
//! MCP server for the **desktop** devtools backend.
//!
//! Launches the desktop-devtools app via `dx serve --hotpatch` and communicates
//! with the app via its embedded HTTP eval-bridge at `http://127.0.0.1:9223`.
//!
//! ## Hot Reload
//!
//! The app runs under `dx serve --hotpatch` so the desktop window stays alive
//! across code changes (no window-jumping on every recompile).  The eval bridge
//! inside the app uses recreatable channels that survive hot-patch remounts.
//!
//! For changes that can't be hot-patched (rare structural changes), Dioxus falls
//! back to a full rebuild — the MCP waits for the bridge to come back.
//!
//! ## Usage
//! ```bash
//! cargo run --bin poly-desktop-devtools-mcp
//! ```
//! Or via `.vscode/mcp.json` for GitHub Copilot integration.

use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use poly_devtools_protocol::backend::{DevtoolsBackend, ScreenshotParams, ScreenshotResult};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::{Value, json};
use tokio::sync::Mutex;

const BASE: &str = "http://127.0.0.1:9223";

// ─── HTTP helpers ─────────────────────────────────────────────────────────────

async fn http_eval(client: &reqwest::Client, js: &str) -> anyhow::Result<String> {
    let resp = client
        .post(format!("{BASE}/eval"))
        .body(js.to_string())
        .send()
        .await?;
    let body = resp.text().await?;
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::String(body));
    if let Some(r) = v.get("result").and_then(|r| r.as_str()) {
        return Ok(r.to_string());
    }
    if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
        return Err(anyhow::anyhow!("{e}"));
    }
    Ok(v.to_string())
}

async fn http_get(client: &reqwest::Client, path: &str) -> anyhow::Result<Vec<u8>> {
    let resp = client.get(format!("{BASE}{path}")).send().await?;
    Ok(resp.bytes().await?.to_vec())
}

// ─── dx serve Process State ──────────────────────────────────────────────────

/// Handle to a managed `dx serve` process.
///
/// Tracks the process ID for hard-kill via SIGKILL.
struct DxServeProcess {
    /// OS process ID — used for hard-kill via SIGKILL.
    pid: u32,
}

// ─── Desktop HTTP Backend ─────────────────────────────────────────────────────

/// Desktop devtools backend — launches the app via `dx serve` and
/// talks to the embedded HTTP eval-bridge at [`BASE`].
struct DesktopHttpBackend {
    client: reqwest::Client,
    /// Handle to the managed `dx serve` process (if we started it).
    dx_serve: Arc<Mutex<Option<DxServeProcess>>>,
    /// Workspace path — set during `launch_app`, used by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
}

impl DesktopHttpBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            dx_serve: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if the eval bridge is currently responding.
    async fn is_bridge_alive(&self) -> bool {
        self.client
            .get(format!("{BASE}/status"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Poll the eval bridge until it responds or timeout.
    async fn wait_for_bridge(&self, max_seconds: u64) -> anyhow::Result<()> {
        let polls = max_seconds * 2; // poll every 500 ms
        for _ in 0..polls {
            if self.is_bridge_alive().await {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::bail!("Eval bridge at {BASE} did not become ready within {max_seconds}s")
    }

    /// Touch source files to trigger ``dx serve``'s file watcher, causing a
    /// full rebuild.
    ///
    /// Two files are touched:
    /// - `crates/core/src/lib.rs`  — triggers poly-core + poly-desktop-devtools
    ///   recompilation via the normal dependency chain.
    /// - `apps/desktop-devtools/src/main.rs` — causes the devtools `build.rs`
    ///   to rerun (it is listed in `cargo:rerun-if-changed`), which emits a
    ///   fresh `POLY_BUILD_TS` env-var so that `build_id()` in the app returns
    ///   a new value after each hotpatch, making rebuild detection reliable.
    async fn touch_source_file(workspace: &str) -> anyhow::Result<()> {
        // Touch the core lib.rs to trigger dx serve's file watcher and cause
        // poly-core + poly-desktop-devtools to recompile.
        let core_trigger = format!("{workspace}/crates/core/src/lib.rs");
        tokio::process::Command::new("touch")
            .arg(&core_trigger)
            .status()
            .await?;

        // Increment the rebuild counter that `build_id()` in the devtools app
        // reads at runtime.  Using a runtime counter file is the most reliable
        // approach: cargo's `rerun-if-changed` uses content checksums so a
        // bare `touch` never reruns build.rs, making compile-time embedding
        // fragile.  The running app reads this file on every /generation call.
        Self::increment_rebuild_counter().await?;
        Ok(())
    }

    /// Atomically increment `/tmp/poly-devtools-rebuild-counter`.
    async fn increment_rebuild_counter() -> anyhow::Result<()> {
        let path = std::path::Path::new("/tmp/poly-devtools-rebuild-counter");
        let current: u64 = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        std::fs::write(path, (current + 1).to_string())?;
        Ok(())
    }
}

#[async_trait]
impl DevtoolsBackend for DesktopHttpBackend {
    fn name(&self) -> &str {
        "desktop-http"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Remember workspace for rebuild_app / reset_app.
        *self.workspace.lock().await = Some(workspace.to_string());

        // ── Step 1: check if an existing instance is already healthy ──────
        if self.is_bridge_alive().await {
            return Ok(format!(
                "App already running on {BASE} — reusing existing instance.\n\
                 Hot-patch is active. Call connect_cdp to interact."
            ));
        }

        // ── Step 2: kill any stale processes ──────────────────────────────
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-devtools[^-]"])
            .status()
            .await;
        // Also kill any stale dx serve for this app.
        let _ = tokio::process::Command::new("bash")
            .args([
                "-c",
                "pkill -f 'dx.*serve.*desktop-devtools' 2>/dev/null || true",
            ])
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        // ── Step 3: start dx serve --hotpatch ─────────────────────────────
        //
        // --hotpatch keeps the desktop window alive across code changes by
        // patching the running binary in-place (subsecond hot-reload).
        // The eval bridge inside the app uses recreatable channels that
        // survive hotpatch remounts.
        let app_dir = format!("{workspace}/apps/desktop-devtools");
        let mut child = tokio::process::Command::new("dx")
            .args(["serve", "--hotpatch", "--platform", "desktop"])
            .current_dir(&app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;

        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("dx serve process has no PID"))?;

        *self.dx_serve.lock().await = Some(DxServeProcess { pid });

        // Background task: reap the child and clean up state on exit.
        let dx_ref = self.dx_serve.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *dx_ref.lock().await = None;
        });

        // ── Step 4: wait for eval bridge (first build can take a while) ───
        // Poll for up to 120 s — initial compilation can be slow.
        match self.wait_for_bridge(120).await {
            Ok(()) => Ok(format!(
                "dx serve --hotpatch started in {app_dir}\n\
                 Eval bridge ready at {BASE}\n\
                 Hot-patch is active — code changes update the running window in-place.\n\
                 Use rebuild_app for forced rebuild, kill_app to stop everything."
            )),
            Err(_) => Ok(format!(
                "dx serve --hotpatch started in {app_dir}\n\
                 Eval bridge not yet responding at {BASE} — first build may still be compiling.\n\
                 Call connect_cdp in a moment to check."
            )),
        }
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        // Drop the dx serve handle (closes stdin, helping it exit).
        *self.dx_serve.lock().await = None;

        // Kill the desktop app process (not the MCP server).
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-devtools[^-]"])
            .status()
            .await;
        // Kill dx serve for this app.
        let _ = tokio::process::Command::new("bash")
            .args([
                "-c",
                "pkill -f 'dx.*serve.*desktop-devtools' 2>/dev/null || true",
            ])
            .status()
            .await;

        Ok("Killed poly-desktop-devtools and dx serve. Call launch_app to restart.".to_string())
    }

    async fn connect(&self) -> anyhow::Result<String> {
        let resp = self
            .client
            .get(format!("{BASE}/status"))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Cannot reach eval-bridge at {BASE}/status: {e}\n\
                     Make sure poly-desktop-devtools is running (call launch_app)."
                )
            })?;
        let ok = resp.text().await?;
        Ok(format!("Eval-bridge connected ✓  ({BASE}/status → {ok})"))
    }

    async fn take_screenshot(
        &self,
        _params: &ScreenshotParams,
    ) -> anyhow::Result<ScreenshotResult> {
        // Desktop Wry only supports PNG — format/quality params are ignored.
        let image_bytes = http_get(&self.client, "/screenshot").await?;
        Ok(ScreenshotResult {
            image_bytes,
            mime_type: "image/png".to_string(),
        })
    }

    async fn js_eval(&self, expression: &str) -> anyhow::Result<String> {
        http_eval(&self.client, expression).await
    }

    async fn hard_kill(&self) -> anyhow::Result<String> {
        // SIGKILL the dx serve process by PID (precise — avoids killing the MCP).
        let pid = self.dx_serve.lock().await.as_ref().map(|s| s.pid);
        *self.dx_serve.lock().await = None;

        if let Some(pid) = pid {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }
        // Also SIGKILL the app and any orphaned dx child processes.
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", "poly-desktop-devtools[^-]"])
            .status()
            .await;
        let _ = tokio::process::Command::new("bash")
            .args([
                "-c",
                "pkill -9 -f 'dx.*serve.*desktop-devtools' 2>/dev/null || true",
            ])
            .status()
            .await;

        Ok(
            "Hard-killed dx serve and poly-desktop-devtools (SIGKILL). Call launch_app to restart."
                .to_string(),
        )
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Touch a source file to trigger dx serve's file watcher.
        // With --hotpatch, this triggers a hot-patch (window stays alive)
        // or a full rebuild (for non-patchable changes).
        Self::touch_source_file(workspace).await?;

        // Brief wait for the compilation + patch cycle.
        // Hot-patches are near-instant; full rebuilds take 10-30s.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        match self.wait_for_bridge(120).await {
            Ok(()) => Ok("Rebuild triggered (touched crates/core/src/lib.rs).\n\
                 dx serve --hotpatch is recompiling — hot-patchable changes are near-instant,\n\
                 full rebuilds take 10-30s with a warm cache.\n\
                 The window stays alive; eval bridge reconnects automatically."
                .to_string()),
            Err(e) => Err(anyhow::anyhow!(
                "Rebuild triggered but eval bridge didn't come back: {e}"
            )),
        }
    }

    async fn reset_app(&self) -> anyhow::Result<String> {
        // Remove poly's data directory.
        let data_dir = dirs_data_path();
        if let Some(dir) = data_dir
            && std::path::Path::new(&dir).exists()
        {
            std::fs::remove_dir_all(&dir)?;
        }

        // Trigger a rebuild so the app restarts fresh at the setup wizard.
        let ws = self.workspace.lock().await.clone();
        if let Some(ws) = ws {
            // Touch a source file to trigger rebuild.
            Self::touch_source_file(&ws).await?;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _ = self.wait_for_bridge(60).await;
            Ok(
                "Data directory removed and rebuild triggered. App should restart at setup wizard."
                    .to_string(),
            )
        } else {
            Ok(
                "Data directory removed. Call launch_app or rebuild_app to restart at setup wizard."
                    .to_string(),
            )
        }
    }

    fn extension_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "get_generation",
                "description": "Returns rebuild-detection counters for this MCP session.\n\n\
                    **IMPORTANT: Semantics differ by platform!**\n\n\
                    **Desktop MCP (this tool):**\n\
                    - **generation**: starts at 1 on launch, increments on each hot-patch (component remount). \
                      Resets to 1 only on full process restart (PID change).\n\
                    - **build_id**: increments on each rebuild_app call (reads /tmp/poly-devtools-rebuild-counter). \
                      0 = no rebuild this session.\n\
                    - **pid**: OS process ID — stable across hot-patches, changes only on full restart.\n\n\
                    **Web MCP (poly-web-devtools-mcp):**\n\
                    - **generation**: increments on EVERY connect_cdp call (not on each rebuild). \
                      This is because each WASM rebuild drops the CDP WebSocket, requiring explicit reconnection.\n\
                    - **build_id**: increments on each rebuild_app call (same as desktop, reads /tmp/poly-devtools-web-rebuild-counter).\n\
                    - **dx_serve_pid**: OS process ID of managed dx serve process.\n\n\
                    **Decision table (Desktop):**\n\
                    - generation changed, pid stable → hot-patch applied\n\
                    - pid changed (generation back to 1) → full rebuild / process restart\n\
                    - build_id changed → rebuild was triggered (independent of generation / pid)\n\n\
                    **Key difference:** Desktop generation may NOT change on every rebuild (hot-patches preserve state). \
                    Always check build_id to know if a rebuild happened. Call connect_cdp explicitly after \
                    rebuild_app to get updated generation (web) or check if hot-patch succeeded (desktop).",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }),
            json!({
                "name": "force_rebuild",
                "description": "Force a full desktop rebuild by running `dx build --platform desktop` directly.\n\n\
                    USE THIS when rebuild_app (hot-patch via touch lib.rs) fails to apply changes.\n\
                    This kills the running desktop app, rebuilds from scratch, and the app process\n\
                    must be relaunched via launch_app afterwards.\n\n\
                    NOTE: Unlike the web MCP force_rebuild, this does NOT auto-launch the app.\n\
                    After this tool returns:\n\
                    1. Call launch_app to start the freshly built binary\n\
                    2. Call connect_cdp\n\
                    3. Verify with get_generation that build_id and pid both changed",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }),
        ]
    }

    async fn handle_extension_tool(
        &self,
        name: &str,
        _args: &Value,
    ) -> Option<anyhow::Result<String>> {
        match name {
            "get_generation" => {
                let result = async {
                    let resp = self
                        .client
                        .get(format!("{BASE}/generation"))
                        .send()
                        .await
                        .map_err(|e| anyhow::anyhow!("HTTP error: {e}"))?;
                    resp.text()
                        .await
                        .map_err(|e| anyhow::anyhow!("Read error: {e}"))
                }
                .await;
                Some(result)
            }
            "force_rebuild" => {
                let workspace = self
                    .workspace
                    .lock()
                    .await
                    .clone()
                    .unwrap_or_else(|| "/home/laragana/workspcacemsg".to_string());
                let app_dir = format!("{workspace}/apps/desktop-devtools");
                let status = tokio::process::Command::new("dx")
                    .args(["build", "--platform", "desktop"])
                    .current_dir(&app_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await;
                let _ = Self::increment_rebuild_counter().await;
                match status {
                    Ok(s) if s.success() => Some(Ok(
                        "Force rebuild complete! Fresh desktop binary is ready.\n\
                         Call launch_app to start it, then connect_cdp."
                            .to_string(),
                    )),
                    Ok(s) => Some(Err(anyhow::anyhow!(
                        "dx build --platform desktop failed with exit code: {s}"
                    ))),
                    Err(e) => Some(Err(anyhow::anyhow!("Failed to spawn dx build: {e}"))),
                }
            }
            _ => None,
        }
    }

    // ── Input method overrides ────────────────────────────────────────────────
    //
    // The desktop eval bridge wraps JS in `async function(dioxus) { SCRIPT }`.
    // The `do_eval` helper further wraps any script containing top-level
    // semicolons in a second IIFE `return (function(){ SCRIPT; return null; })()`
    // which discards the inner function's return value.
    //
    // The fix: start every script with `return ` so that `do_eval` passes it
    // through unchanged (it only skips wrapping when the trimmed JS already
    // starts with `return `).  We wrap ourselves in `return (function(){...})()`
    // which correctly propagates the inner return value.
    //
    // `click_at` additionally works around the WebKit2GTK `elementFromPoint`
    // issue (returns null for physical-pixel coords on HiDPI displays) by:
    //   1. Trying `elementFromPoint(x, y)` at CSS pixel coords.
    //   2. Retrying at `(x/dpr, y/dpr)` in case the caller used physical pixels.
    //   3. Falling back to a `getBoundingClientRect()` scan over all elements.

    async fn click_at(&self, x: f64, y: f64, dbl_click: bool) -> anyhow::Result<String> {
        let count = if dbl_click { 2 } else { 1 };
        // Starts with `return ` → do_eval passes it through without double-wrapping.
        //
        // IMPORTANT: x,y must be CSS pixel coordinates (same space as
        // getBoundingClientRect()), NOT screenshot display pixels.
        // The displayed screenshot image is scaled by the viewer — do NOT use
        // image pixel offsets here.  Always use getBoundingClientRect() to find
        // exact element centres before calling click_at.
        let js = format!(
            r#"return (function(){{
                var rx={x},ry={y},count={count};
                var dpr=window.devicePixelRatio||1;

                // Interactive tags / roles — prefer these as the click target over
                // presentational children (span, div, svg, etc).
                var INTERACTIVE=['A','BUTTON','INPUT','SELECT','TEXTAREA','LABEL'];
                var INTERACTIVE_ROLES=['button','link','menuitem','option','tab','checkbox','radio','combobox','listbox'];

                function isInteractive(el){{
                    if(INTERACTIVE.indexOf(el.tagName)!==-1)return true;
                    var r=(el.getAttribute('role')||'').toLowerCase();
                    if(INTERACTIVE_ROLES.indexOf(r)!==-1)return true;
                    if(el.hasAttribute('onclick')||el.hasAttribute('data-dioxus-id'))return true;
                    return false;
                }}

                // Walk up from a hit element to the nearest interactive ancestor
                // (within 8 hops) — this prevents landing on a child span/svg
                // inside a button and missing the click handler.
                function liftToInteractive(el){{
                    var cur=el,hops=0;
                    while(cur&&hops<8){{
                        if(isInteractive(cur))return cur;
                        cur=cur.parentElement;hops++;
                    }}
                    return el; // give up — return original
                }}

                // Hit-test: try native first, then bounding-rect scan.
                function findAt(cx,cy){{
                    var el=document.elementFromPoint(cx,cy);
                    if(el&&el!==document.documentElement&&el!==document.body)return el;
                    // Manual scan: smallest element whose rect contains the point.
                    var all=Array.from(document.querySelectorAll('*')),best=null,bestSz=Infinity;
                    for(var i=0;i<all.length;i++){{
                        var r=all[i].getBoundingClientRect();
                        if(cx>=r.left&&cx<=r.right&&cy>=r.top&&cy<=r.bottom){{
                            var sz=r.width*r.height;
                            if(sz>0&&sz<bestSz){{bestSz=sz;best=all[i];}}
                        }}
                    }}
                    return best;
                }}

                // Try CSS pixel coords; if null and DPR!=1 try scaled fallback.
                var hit=findAt(rx,ry);
                var usedX=rx,usedY=ry;
                if(!hit&&dpr!==1){{
                    var sx=rx/dpr,sy=ry/dpr;
                    hit=findAt(sx,sy);
                    if(hit){{usedX=sx;usedY=sy;}}
                }}
                if(!hit)return 'No element at ('+rx+','+ry+') dpr='+dpr+'. Use evaluate_script+getBoundingClientRect() for exact coords.';

                // Lift to nearest interactive ancestor to avoid missing Dioxus handlers.
                var el=liftToInteractive(hit);

                el.scrollIntoView({{block:'nearest',behavior:'instant'}});
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{el.focus();}}

                var opts={{bubbles:true,cancelable:true,clientX:usedX,clientY:usedY,screenX:usedX,screenY:usedY,view:window}};
                for(var k=0;k<count;k++){{
                    el.dispatchEvent(new PointerEvent('pointerdown',Object.assign({{pointerId:1,isPrimary:true}},opts)));
                    el.dispatchEvent(new MouseEvent('mousedown',opts));
                    el.dispatchEvent(new PointerEvent('pointerup',Object.assign({{pointerId:1,isPrimary:true}},opts)));
                    el.dispatchEvent(new MouseEvent('mouseup',opts));
                    el.dispatchEvent(new MouseEvent('click',Object.assign({{detail:count}},opts)));
                }}

                var tag=el.tagName.toLowerCase();
                var id=el.id?'#'+el.id:'';
                var cls=(el.className&&typeof el.className==='string')&&el.className.trim()?'.'+el.className.trim().split(/\s+/)[0]:'';
                var txt=(el.textContent||el.value||'').trim().slice(0,60);
                var liftMsg=(el!==hit)?' (lifted from '+hit.tagName.toLowerCase()+')'  :'';
                return 'Clicked '+tag+(id||cls||'')+liftMsg+' at ('+usedX+','+usedY+')'+(txt?' "'+txt+'"':'');
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn click_element(&self, selector: &str) -> anyhow::Result<String> {
        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        // Starts with `return ` → bypasses do_eval double-wrapping.
        let js = format!(
            r#"return (function(){{
                var el=document.querySelector('{escaped}');
                if(!el)return 'Error: No element found for selector: {escaped}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{el.focus();}}
                el.dispatchEvent(new MouseEvent('click',{{bubbles:true,cancelable:true}}));
                var tag=el.tagName.toLowerCase();
                var id=el.id?'#'+el.id:'';
                var txt=(el.textContent||el.value||'').trim().slice(0,50);
                return 'Clicked '+tag+id+(txt?' "'+txt+'"':'');
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn hover_element(&self, selector: &str) -> anyhow::Result<String> {
        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"return (function(){{
                var el=document.querySelector('{escaped}');
                if(!el)return 'Error: No element found for selector: {escaped}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                var rect=el.getBoundingClientRect();
                var cx=rect.left+rect.width/2,cy=rect.top+rect.height/2;
                var opts={{bubbles:true,clientX:cx,clientY:cy,view:window}};
                el.dispatchEvent(new MouseEvent('mouseenter',opts));
                el.dispatchEvent(new MouseEvent('mouseover',opts));
                el.dispatchEvent(new MouseEvent('mousemove',opts));
                return 'Hovered over '+el.tagName.toLowerCase()+(el.id?'#'+el.id:'');
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn fill_element(&self, selector: &str, value: &str) -> anyhow::Result<String> {
        let sel = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let val = value.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"return (function(){{
                var el=document.querySelector('{sel}');
                if(!el)return 'Error: No element found for selector: {sel}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                el.focus();
                if(el.tagName==='SELECT'){{
                    for(var i=0;i<el.options.length;i++){{
                        if(el.options[i].value==='{val}'||el.options[i].text==='{val}'){{
                            el.selectedIndex=i;
                            el.dispatchEvent(new Event('change',{{bubbles:true}}));
                            return 'Selected "'+el.options[i].text+'"';
                        }}
                    }}
                    return 'Error: Option not found: {val}';
                }}
                var nativeSet=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')
                    ||Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value');
                if(nativeSet&&nativeSet.set)nativeSet.set.call(el,'{val}');else el.value='{val}';
                el.dispatchEvent(new Event('input',{{bubbles:true}}));
                el.dispatchEvent(new Event('change',{{bubbles:true}}));
                return 'Filled '+el.tagName.toLowerCase()+(el.id?'#'+el.id:'')+' with "'+'{val}'.slice(0,40)+'"';
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn type_text(&self, text: &str, submit_key: Option<&str>) -> anyhow::Result<String> {
        let escaped_text = text.replace('\\', "\\\\").replace('\'', "\\'");
        let key_js = match submit_key {
            Some(k) => {
                let ek = k.replace('\'', "\\'");
                format!(
                    "el.dispatchEvent(new KeyboardEvent('keydown',{{key:'{ek}',bubbles:true}}));\
                     el.dispatchEvent(new KeyboardEvent('keyup',{{key:'{ek}',bubbles:true}}));"
                )
            }
            None => String::new(),
        };
        let display = match submit_key {
            Some(k) => format!("Typed \"{escaped_text}\" + {k}"),
            None => format!("Typed \"{escaped_text}\""),
        };
        let js = format!(
            r#"return (function(){{
                var el=document.activeElement||document.body;
                var t='{escaped_text}';
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{
                    var nativeSet=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')
                        ||Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value');
                    if(nativeSet&&nativeSet.set)nativeSet.set.call(el,el.value+t);else el.value+=t;
                    el.dispatchEvent(new Event('input',{{bubbles:true}}));
                    el.dispatchEvent(new Event('change',{{bubbles:true}}));
                }}else{{
                    for(var i=0;i<t.length;i++){{
                        var c=t[i];
                        el.dispatchEvent(new KeyboardEvent('keydown',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keypress',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keyup',{{key:c,bubbles:true}}));
                    }}
                }}
                {key_js}
                return '{display}';
            }})()"#
        );
        self.js_eval(&js).await
    }
}

/// Best-effort path to Poly's data directory.
fn dirs_data_path() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(format!("{home}/.local/share/poly"))
}

// ─── CLI Mode ─────────────────────────────────────────────────────────────────
//
// PREFERRED: Use the CLI over MCP access wherever possible.
// CLI is faster, scriptable, and testable without a Copilot MCP session.
//
// Usage examples:
//   cargo run --bin poly-desktop-devtools-mcp -- status
//   cargo run --bin poly-desktop-devtools-mcp -- screenshot --save /tmp/shot.png
//   cargo run --bin poly-desktop-devtools-mcp -- snapshot
//   cargo run --bin poly-desktop-devtools-mcp -- eval "document.title"
//   cargo run --bin poly-desktop-devtools-mcp -- launch /path/to/workspace
//   cargo run --bin poly-desktop-devtools-mcp -- generation

/// Detect the workspace root at runtime (POLY_WORKSPACE env var or cwd).
fn cli_detect_workspace() -> String {
    if let Ok(ws) = std::env::var("POLY_WORKSPACE") {
        return ws;
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// Commands that trigger CLI mode instead of MCP server mode.
const CLI_COMMANDS: &[&str] = &[
    "status", "launch", "kill", "screenshot", "snapshot",
    "eval", "click", "fill", "navigate", "generation",
    "help", "--help", "-h",
];

/// Check if the first argument selects CLI mode.
fn is_cli_mode(args: &[String]) -> bool {
    args.get(1)
        .map(|a| CLI_COMMANDS.contains(&a.as_str()))
        .unwrap_or(false)
}

/// Write a line to stdout without using `println!`.
fn cli_write(text: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    writeln!(std::io::stdout().lock(), "{text}")?;
    Ok(())
}

/// Extract value of `--flag <value>` from args.
fn extract_cli_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).map(String::as_str)
}

/// CLI help text for the desktop MCP.
fn desktop_cli_help() -> &'static str {
    "poly-desktop-devtools-mcp — CLI mode (PREFERRED over MCP)

COMMANDS:
  status                    Check if app is running
  launch [workspace]        Start the devtools app
  kill                      Stop the devtools app
  screenshot [--save path]  Take a screenshot (saves PNG or prints base64)
  snapshot [--verbose]      Print DOM snapshot
  eval <script>             Evaluate JavaScript expression
  click <selector>          Click a CSS selector
  fill <selector> <value>   Fill an input element
  navigate <url>            Navigate to a URL
  generation                Get rebuild/hotpatch generation counters
  help                      Show this help

MCP mode (default, no subcommand):
  cargo run --bin poly-desktop-devtools-mcp
"
}

/// Handle the `screenshot` CLI command.
async fn cli_screenshot_cmd(
    backend: &DesktopHttpBackend,
    args: &[String],
) -> anyhow::Result<String> {
    use base64::Engine as _;
    let save_path = extract_cli_flag(args, "--save");
    let params = poly_devtools_protocol::backend::ScreenshotParams::default();
    let result = backend.take_screenshot(&params).await?;
    if let Some(path) = save_path {
        std::fs::write(path, &result.image_bytes)?;
        Ok(format!("Screenshot saved to {path}"))
    } else {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&result.image_bytes);
        Ok(format!("data:{};base64,{b64}", result.mime_type))
    }
}

/// Dispatch a single CLI command for the desktop backend.
async fn dispatch_desktop_cli(
    backend: &DesktopHttpBackend,
    cmd: &str,
    args: &[String],
) -> anyhow::Result<String> {
    use poly_devtools_protocol::backend::NavigateParams;
    match cmd {
        "status" | "connect" => backend.connect().await,
        "launch" => {
            let ws = args
                .first()
                .map(String::as_str)
                .map(str::to_string)
                .unwrap_or_else(cli_detect_workspace);
            backend.launch_app(&ws).await
        }
        "kill" => backend.kill_app().await,
        "snapshot" => {
            let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
            backend.take_snapshot(verbose).await
        }
        "eval" => {
            let script = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: eval <script>"))?;
            backend.js_eval(script).await
        }
        "click" => {
            let sel = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: click <selector>"))?;
            backend.click_element(sel).await
        }
        "fill" => dispatch_desktop_fill(backend, args).await,
        "navigate" => {
            let url = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: navigate <url>"))?;
            backend
                .navigate_page(&NavigateParams {
                    nav_type: "url".to_string(),
                    url: Some(url.to_string()),
                    ..Default::default()
                })
                .await
        }
        "generation" => http_get(&backend.client, "/generation")
            .await
            .map(|b| String::from_utf8_lossy(&b).into_owned()),
        "screenshot" => cli_screenshot_cmd(backend, args).await,
        _ => Ok(desktop_cli_help().to_string()),
    }
}

async fn dispatch_desktop_fill(
    backend: &DesktopHttpBackend,
    args: &[String],
) -> anyhow::Result<String> {
    let sel = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("Usage: fill <selector> <value>"))?;
    let val = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: fill <selector> <value>"))?;
    backend.fill_element(sel, val).await
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let backend = DesktopHttpBackend::new();
    if is_cli_mode(&args) {
        let cmd = args.get(1).map(String::as_str).unwrap_or("help");
        let rest = args.get(2..).unwrap_or(&[]).to_vec();
        match dispatch_desktop_cli(&backend, cmd, &rest).await {
            Ok(out) => {
                if let Err(e) = cli_write(&out) {
                    use std::io::Write as _;
                    let _ = writeln!(std::io::stderr().lock(), "Output error: {e}");
                }
            }
            Err(e) => {
                use std::io::Write as _;
                let _ = writeln!(std::io::stderr().lock(), "Error: {e}");
                std::process::exit(1);
            }
        }
    } else {
        run_mcp_loop(&backend, "poly-devtools-desktop").await;
    }
}
