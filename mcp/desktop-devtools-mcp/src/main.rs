//! # poly-desktop-devtools-mcp
//!
//! MCP server for the **desktop** devtools backend.
//!
//! Launches the desktop-devtools app via `dx serve` and communicates with the app
//! via its embedded HTTP eval-bridge at `http://127.0.0.1:9223`.
//!
//! ## Hot Reload
//!
//! The app runs under `dx serve` with file-watcher-based hot-reload. When you
//! make changes to poly-core, use the `rebuild_app` MCP tool which touches a
//! source file to trigger a full rebuild.
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
use serde_json::Value;
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

    /// Touch a source file to trigger ``dx serve``'s file watcher, causing a
    /// full rebuild.
    async fn touch_source_file(workspace: &str) -> anyhow::Result<()> {
        // Touch the core lib.rs — this is in the hot-reload watch path and
        // guarantees a recompilation of the devtools binary.
        let trigger = format!("{workspace}/crates/core/src/lib.rs");
        tokio::process::Command::new("touch")
            .arg(&trigger)
            .status()
            .await?;
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
                 Hot reload is active. Call connect_cdp to interact."
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

        // ── Step 3: start dx serve ───────────────────────────────────────
        let app_dir = format!("{workspace}/apps/desktop-devtools");
        let mut child = tokio::process::Command::new("dx")
            .args(["serve", "--platform", "desktop"])
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
                "dx serve started in {app_dir}\n\
                 Eval bridge ready at {BASE}\n\
                 Hot reload is active — file changes trigger automatic rebuild.\n\
                 Use rebuild_app for forced rebuild, kill_app to stop everything."
            )),
            Err(_) => Ok(format!(
                "dx serve started in {app_dir}\n\
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
        // Touch a source file to trigger dx serve's file watcher, causing a
        // full rebuild.
        Self::touch_source_file(workspace).await?;

        // Wait a moment for the rebuild to start, then poll the bridge.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        match self.wait_for_bridge(120).await {
            Ok(()) => Ok("Rebuild triggered (touched crates/core/src/lib.rs).\n\
                 dx serve is recompiling — this takes 10-30s with a warm cache.\n\
                 Eval bridge will reconnect when done."
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

// ─── Entry Point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let backend = DesktopHttpBackend::new();
    run_mcp_loop(&backend, "poly-devtools-desktop").await;
}
