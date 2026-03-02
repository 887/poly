//! Backend trait for devtools implementations.
//!
//! Each backend (desktop HTTP bridge, Chrome CDP, etc.) implements this trait.
//! The MCP main loop dispatches tool calls to the active backend.
//!
//! Tool naming and descriptions follow the patterns established by
//! [chrome-devtools-mcp](https://github.com/nicobailey/chrome-devtools-mcp).
//!
//! ## Design
//!
//! The trait has two tiers of methods:
//!
//! 1. **Required** — backends MUST implement: [`DevtoolsBackend::name`],
//!    [`DevtoolsBackend::launch_app`], [`DevtoolsBackend::kill_app`],
//!    [`DevtoolsBackend::connect`], [`DevtoolsBackend::js_eval`],
//!    [`DevtoolsBackend::take_screenshot`].
//!
//! 2. **Defaulted** — all other methods have sensible defaults that delegate
//!    to [`DevtoolsBackend::js_eval`]. Backends may override any method for
//!    better native integration (e.g. CDP commands instead of JS eval).

use async_trait::async_trait;

// ─── Data Structures ──────────────────────────────────────────────────────────

/// Result of a screenshot capture.
pub struct ScreenshotResult {
    /// Raw image data (PNG, JPEG, or WebP).
    pub image_bytes: Vec<u8>,
    /// MIME type of the image (e.g. `"image/png"`).
    pub mime_type: String,
}

/// Parameters for [`DevtoolsBackend::take_screenshot`].
#[derive(Debug, Clone)]
pub struct ScreenshotParams {
    /// Image format: `"png"`, `"jpeg"`, or `"webp"`. Default: `"png"`.
    pub format: String,
    /// Compression quality (0–100) for JPEG/WebP. Ignored for PNG.
    pub quality: Option<u32>,
    /// Capture the full scrollable page instead of just the visible viewport.
    pub full_page: bool,
    /// Save the screenshot to this file path instead of returning it inline.
    pub file_path: Option<String>,
}

impl Default for ScreenshotParams {
    fn default() -> Self {
        Self {
            format: "png".to_string(),
            quality: None,
            full_page: false,
            file_path: None,
        }
    }
}

/// Parameters for [`DevtoolsBackend::navigate_page`].
#[derive(Debug, Clone)]
pub struct NavigateParams {
    /// `"url"`, `"back"`, `"forward"`, or `"reload"`.
    pub nav_type: String,
    /// Target URL (required when `nav_type == "url"`).
    pub url: Option<String>,
    /// Bypass cache on reload.
    pub ignore_cache: bool,
    /// Navigation timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for NavigateParams {
    fn default() -> Self {
        Self {
            nav_type: "url".to_string(),
            url: None,
            ignore_cache: false,
            timeout_ms: 30_000,
        }
    }
}

/// A single console log entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsoleEntry {
    pub level: String,
    pub text: String,
    #[serde(default)]
    pub timestamp: Option<f64>,
}

// ─── JavaScript Snippets ──────────────────────────────────────────────────────
// Used by default trait implementations. Backends can override methods to use
// native commands (e.g. CDP) instead.

/// Compact text snapshot of the DOM tree (roles, IDs, aria-labels, text).
const SNAPSHOT_JS: &str = r#"(function(){
  var SK={SCRIPT:1,STYLE:1,NOSCRIPT:1,LINK:1,META:1,BR:1,HEAD:1};
  function w(el,d){
    if(!el||el.nodeType!==1)return '';
    var t=el.tagName;if(SK[t])return '';
    try{if(window.getComputedStyle(el).display==='none')return '';}catch(e){}
    var i='  '.repeat(d),p=[t.toLowerCase()];
    var r=el.getAttribute('role');if(r)p[0]=r;
    if(el.id)p.push('#'+el.id);
    var al=el.getAttribute('aria-label');if(al)p.push('aria-label="'+al+'"');
    if(t==='A'){var h=el.getAttribute('href');if(h)p.push('href="'+h+'"');}
    if(t==='INPUT'||t==='TEXTAREA'||t==='SELECT'){
      var tp=el.getAttribute('type');if(tp)p.push('type="'+tp+'"');
      var nm=el.getAttribute('name');if(nm)p.push('name="'+nm+'"');
      var ph=el.getAttribute('placeholder');if(ph)p.push('placeholder="'+ph+'"');
      if(el.value)p.push('value="'+el.value.slice(0,40)+'"');
    }
    if(t==='BUTTON'||t==='INPUT'){if(el.disabled)p.push('[disabled]');}
    var tx='';
    for(var j=0;j<el.childNodes.length;j++){
      if(el.childNodes[j].nodeType===3){var s=el.childNodes[j].textContent.trim();if(s)tx+=s+' ';}
    }
    tx=tx.trim();if(tx.length>80)tx=tx.slice(0,77)+'...';
    var ln=i+'['+p.join(' ')+']';if(tx)ln+=' "'+tx+'"';ln+='\n';
    for(var j=0;j<el.children.length;j++){ln+=w(el.children[j],d+1);}
    return ln;
  }
  return '[page] '+(document.title||'')+'\n'+w(document.body,1);
})()"#;

/// Verbose snapshot including CSS classes, data attributes, img sources.
const SNAPSHOT_VERBOSE_JS: &str = r#"(function(){
  var SK={SCRIPT:1,STYLE:1,NOSCRIPT:1,LINK:1,META:1,BR:1,HEAD:1};
  function w(el,d){
    if(!el||el.nodeType!==1)return '';
    var t=el.tagName;if(SK[t])return '';
    try{if(window.getComputedStyle(el).display==='none')return '';}catch(e){}
    var i='  '.repeat(d),p=[t.toLowerCase()];
    var r=el.getAttribute('role');if(r)p[0]=r;
    if(el.id)p.push('#'+el.id);
    if(el.className&&typeof el.className==='string'){
      var cls=el.className.trim().split(/\s+/).slice(0,5);
      if(cls[0])p.push('.'+cls.join('.'));
    }
    var al=el.getAttribute('aria-label');if(al)p.push('aria-label="'+al+'"');
    if(t==='A'){var h=el.getAttribute('href');if(h)p.push('href="'+h+'"');}
    if(t==='IMG'){
      var s=el.getAttribute('src');if(s)p.push('src="'+s.slice(0,60)+'"');
      var a=el.getAttribute('alt');if(a)p.push('alt="'+a+'"');
    }
    if(t==='INPUT'||t==='TEXTAREA'||t==='SELECT'){
      var tp=el.getAttribute('type');if(tp)p.push('type="'+tp+'"');
      var nm=el.getAttribute('name');if(nm)p.push('name="'+nm+'"');
      var ph=el.getAttribute('placeholder');if(ph)p.push('placeholder="'+ph+'"');
      if(el.value)p.push('value="'+el.value.slice(0,40)+'"');
    }
    if(el.disabled)p.push('[disabled]');
    var ds=el.dataset;
    for(var k in ds){if(ds.hasOwnProperty(k))p.push('data-'+k+'="'+String(ds[k]).slice(0,30)+'"');}
    var tx='';
    for(var j=0;j<el.childNodes.length;j++){
      if(el.childNodes[j].nodeType===3){var s=el.childNodes[j].textContent.trim();if(s)tx+=s+' ';}
    }
    tx=tx.trim();if(tx.length>120)tx=tx.slice(0,117)+'...';
    var ln=i+'['+p.join(' ')+']';if(tx)ln+=' "'+tx+'"';ln+='\n';
    for(var j=0;j<el.children.length;j++){ln+=w(el.children[j],d+1);}
    return ln;
  }
  return '[page] '+(document.title||'')+'\n'+w(document.body,1);
})()"#;

/// JS to install console capture hook and retrieve buffered messages.
const CONSOLE_CAPTURE_JS: &str = r#"(function(){
  if(!window.__polyConsoleLogs){
    window.__polyConsoleLogs=[];
    var orig={};
    ['log','warn','error','info','debug'].forEach(function(lvl){
      orig[lvl]=console[lvl];
      console[lvl]=function(){
        var args=Array.from(arguments).map(function(a){try{return JSON.stringify(a);}catch(e){return String(a);}});
        window.__polyConsoleLogs.push({level:lvl,text:args.join(' '),timestamp:Date.now()});
        if(window.__polyConsoleLogs.length>500)window.__polyConsoleLogs.shift();
        orig[lvl].apply(console,arguments);
      };
    });
  }
  return JSON.stringify(window.__polyConsoleLogs);
})()"#;

// ─── Trait ────────────────────────────────────────────────────────────────────

/// Trait implemented by each devtools backend.
///
/// Methods return `anyhow::Result` — the MCP layer converts errors to
/// `isError: true` tool results automatically.
#[async_trait]
pub trait DevtoolsBackend: Send + Sync {
    /// Human-readable backend name (e.g. `"desktop-http"`, `"web-cdp"`).
    fn name(&self) -> &str;

    // ═══════════════════════════════════════════════════════════════════
    //  Lifecycle  (Poly-specific — not in chrome-devtools-mcp)
    // ═══════════════════════════════════════════════════════════════════

    /// Build (if needed) and launch the application under test.
    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String>;

    /// Gracefully stop the running application.
    async fn kill_app(&self) -> anyhow::Result<String>;

    /// Verify connectivity to the running application.
    async fn connect(&self) -> anyhow::Result<String>;

    /// Force-kill the app and `dx serve` process with SIGKILL.
    async fn hard_kill(&self) -> anyhow::Result<String> {
        anyhow::bail!("hard_kill not supported by this backend")
    }

    /// Trigger a Dioxus full rebuild (recompile + app restart).
    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        let _ = workspace;
        anyhow::bail!("rebuild_app not supported by this backend")
    }

    /// Delete the local database and restart at the setup wizard.
    async fn reset_app(&self) -> anyhow::Result<String> {
        anyhow::bail!("reset_app not supported by this backend")
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Core primitives  (backends MUST implement)
    // ═══════════════════════════════════════════════════════════════════

    /// Evaluate a JavaScript expression in the app's webview.
    ///
    /// This is the fundamental primitive — most default method
    /// implementations delegate to it.
    async fn js_eval(&self, expression: &str) -> anyhow::Result<String>;

    /// Take a screenshot of the current page.
    ///
    /// Backends that only support PNG may ignore format/quality params.
    async fn take_screenshot(&self, params: &ScreenshotParams) -> anyhow::Result<ScreenshotResult>;

    // ═══════════════════════════════════════════════════════════════════
    //  Snapshot  (cf. chrome-devtools-mcp `take_snapshot`)
    // ═══════════════════════════════════════════════════════════════════

    /// Take a text snapshot of the page based on the DOM tree.
    ///
    /// The snapshot lists page elements in a tree format showing tags,
    /// attributes, and text content. Prefer taking a snapshot over a
    /// screenshot for understanding page content and structure.
    ///
    /// Default implementation walks the DOM via JS.
    async fn take_snapshot(&self, verbose: bool) -> anyhow::Result<String> {
        let js = if verbose {
            SNAPSHOT_VERBOSE_JS
        } else {
            SNAPSHOT_JS
        };
        self.js_eval(js).await
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Script  (cf. chrome-devtools-mcp `evaluate_script`)
    // ═══════════════════════════════════════════════════════════════════

    /// Evaluate a JavaScript function inside the app.
    ///
    /// The `function_body` should be a function declaration like
    /// `() => { return document.title }`. It is wrapped in an IIFE and
    /// executed. Returns the result as a string.
    async fn evaluate_script(&self, function_body: &str) -> anyhow::Result<String> {
        let wrapped = format!("({function_body})()");
        self.js_eval(&wrapped).await
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Console  (cf. chrome-devtools-mcp `list_console_messages`)
    // ═══════════════════════════════════════════════════════════════════

    /// List all console messages captured since page load.
    ///
    /// Installs a capture hook on first call; subsequent calls return
    /// accumulated messages. Default implementation uses JS interception.
    async fn list_console_messages(&self) -> anyhow::Result<String> {
        self.js_eval(CONSOLE_CAPTURE_JS).await
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Navigation  (cf. chrome-devtools-mcp `navigate_page`, `wait_for`)
    // ═══════════════════════════════════════════════════════════════════

    /// Navigate the page by URL, back, forward, or reload.
    ///
    /// Mirrors the `navigate_page` tool from chrome-devtools-mcp.
    async fn navigate_page(&self, params: &NavigateParams) -> anyhow::Result<String> {
        match params.nav_type.as_str() {
            "url" => {
                let url = params.url.as_deref().unwrap_or("");
                if url.is_empty() {
                    anyhow::bail!("A URL is required for navigation of type=url.");
                }
                let escaped = url.replace('\'', "\\'");
                self.js_eval(&format!(
                    "(function(){{ window.location.href = '{escaped}'; return 'Navigating to {escaped}'; }})()"
                ))
                .await
            }
            "back" => {
                self.js_eval("(function(){ window.history.back(); return 'Navigated back'; })()")
                    .await
            }
            "forward" => {
                self.js_eval(
                    "(function(){ window.history.forward(); return 'Navigated forward'; })()",
                )
                .await
            }
            "reload" => {
                let js = if params.ignore_cache {
                    "(function(){ window.location.reload(true); return 'Reloaded (no cache)'; })()"
                } else {
                    "(function(){ window.location.reload(); return 'Reloaded'; })()"
                };
                self.js_eval(js).await
            }
            other => anyhow::bail!(
                "Unknown navigation type: {other}. Use url, back, forward, or reload."
            ),
        }
    }

    /// Wait for any of the specified texts to appear on the page.
    ///
    /// Polls every 250 ms up to `timeout_ms` milliseconds.
    /// Returns a JSON object `{"found": "<matched text>"}` on success.
    async fn wait_for_text(&self, texts: &[String], timeout_ms: u64) -> anyhow::Result<String> {
        let texts_json = serde_json::to_string(texts).unwrap_or_default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);

        loop {
            let check_js = format!(
                r#"(function(){{
                    var texts = {texts_json};
                    var body = document.body ? document.body.innerText : '';
                    for (var i = 0; i < texts.length; i++) {{
                        if (body.indexOf(texts[i]) !== -1) return JSON.stringify({{found: texts[i]}});
                    }}
                    return JSON.stringify({{found: null}});
                }})()"#
            );

            if let Ok(result) = self.js_eval(&check_js).await {
                // The bridge may return the JSON directly or wrapped
                let cleaned = result.trim().trim_matches('"');
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(cleaned)
                    && v.get("found").and_then(|f| f.as_str()).is_some()
                {
                    return Ok(format!("Element matching one of {texts_json} found."));
                }
                // Direct match check (different bridge serialisation)
                if result.contains("\"found\":\"") {
                    return Ok(format!("Element matching one of {texts_json} found."));
                }
            }

            if std::time::Instant::now() >= deadline {
                anyhow::bail!("Timeout after {timeout_ms}ms waiting for any of: {texts_json}");
            }

            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Input  (cf. chrome-devtools-mcp click, click_at, hover, fill,
    //          type_text, handle_dialog)
    // ═══════════════════════════════════════════════════════════════════

    /// Click on an element matching a CSS selector.
    ///
    /// The element is scrolled into view before clicking.
    async fn click_element(&self, selector: &str) -> anyhow::Result<String> {
        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        self.js_eval(&format!(
            r#"(function(){{
                var el = document.querySelector('{escaped}');
                if (!el) return 'Error: No element found for selector: {escaped}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                el.click();
                var tag = el.tagName.toLowerCase();
                var id = el.id ? '#' + el.id : '';
                var txt = (el.textContent||'').trim().slice(0,40);
                return 'Clicked ' + tag + id + (txt ? ' "' + txt + '"' : '');
            }})()"#
        ))
        .await
    }

    /// Click at the provided (x, y) coordinates.
    ///
    /// Dispatches pointer and mouse events at the given position.
    /// Set `dbl_click` to `true` for double-clicks.
    async fn click_at(&self, x: f64, y: f64, dbl_click: bool) -> anyhow::Result<String> {
        let count = if dbl_click { 2 } else { 1 };
        self.js_eval(&format!(
            r#"(function(){{
                var x={x},y={y};
                var el=document.elementFromPoint(x,y);
                if(!el)return 'No element at ('+x+','+y+')';
                var opts={{bubbles:true,cancelable:true,clientX:x,clientY:y,screenX:x,screenY:y,view:window}};
                for(var i=0;i<{count};i++){{
                    el.dispatchEvent(new PointerEvent('pointerdown',Object.assign({{pointerId:1,isPrimary:true}},opts)));
                    el.dispatchEvent(new MouseEvent('mousedown',opts));
                    el.dispatchEvent(new PointerEvent('pointerup',Object.assign({{pointerId:1,isPrimary:true}},opts)));
                    el.dispatchEvent(new MouseEvent('mouseup',opts));
                    el.dispatchEvent(new MouseEvent('click',opts));
                }}
                var tag=el.tagName.toLowerCase();
                var id=el.id?'#'+el.id:'';
                var cls=(el.className||'').toString().trim().replace(/\s+/g,'.');
                var txt=(el.textContent||'').trim().slice(0,40);
                return 'Clicked '+tag+(id?id:(cls?'.'+cls:''))+' at ('+x+','+y+')';
            }})()"#
        ))
        .await
    }

    /// Hover over an element matching a CSS selector.
    ///
    /// Dispatches mouseenter/mouseover/mousemove events.
    async fn hover_element(&self, selector: &str) -> anyhow::Result<String> {
        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        self.js_eval(&format!(
            r#"(function(){{
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
        ))
        .await
    }

    /// Fill an input, textarea, or select element by CSS selector.
    ///
    /// For `<select>` elements, matches by option value or visible text.
    /// For text inputs, uses the native value setter to trigger framework
    /// change handlers (React, Dioxus, etc.).
    async fn fill_element(&self, selector: &str, value: &str) -> anyhow::Result<String> {
        let sel = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let val = value.replace('\\', "\\\\").replace('\'', "\\'");
        self.js_eval(&format!(
            r#"(function(){{
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
                el.value='{val}';
                el.dispatchEvent(new Event('input',{{bubbles:true}}));
                el.dispatchEvent(new Event('change',{{bubbles:true}}));
                return 'Filled '+el.tagName.toLowerCase()+(el.id?'#'+el.id:'')+' with "'+'{val}'.slice(0,40)+'"';
            }})()"#
        ))
        .await
    }

    /// Type text using keyboard into a previously focused element.
    ///
    /// For input/textarea elements, appends to the current value and
    /// dispatches input/change events. Optionally presses a key after
    /// typing (e.g. `"Enter"`, `"Tab"`).
    async fn type_text(&self, text: &str, submit_key: Option<&str>) -> anyhow::Result<String> {
        let escaped = text.replace('\\', "\\\\").replace('\'', "\\'");
        let mut js = format!(
            r#"(function(){{
                var el=document.activeElement||document.body;
                var t='{escaped}';
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{
                    el.value+=t;
                    el.dispatchEvent(new Event('input',{{bubbles:true}}));
                    el.dispatchEvent(new Event('change',{{bubbles:true}}));
                }}else{{
                    for(var i=0;i<t.length;i++){{
                        var c=t[i];
                        el.dispatchEvent(new KeyboardEvent('keydown',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keypress',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keyup',{{key:c,bubbles:true}}));
                    }}
                }}"#
        );
        if let Some(key) = submit_key {
            let ek = key.replace('\\', "\\\\").replace('\'', "\\'");
            js.push_str(&format!(
                "\n                el.dispatchEvent(new KeyboardEvent('keydown',{{key:'{ek}',bubbles:true}}));\
                 \n                el.dispatchEvent(new KeyboardEvent('keyup',{{key:'{ek}',bubbles:true}}));"
            ));
        }
        let display = match submit_key {
            Some(k) => format!("Typed \\\"{escaped}\\\" + {k}"),
            None => format!("Typed \\\"{escaped}\\\""),
        };
        js.push_str(&format!(
            "\n                return '{display}';\n            }})()"
        ));
        self.js_eval(&js).await
    }

    /// Handle a browser dialog (alert, confirm, prompt).
    ///
    /// `action` is `"accept"` or `"dismiss"`.
    /// `prompt_text` is optional text to enter for prompt dialogs.
    async fn handle_dialog(
        &self,
        _action: &str,
        _prompt_text: Option<&str>,
    ) -> anyhow::Result<String> {
        anyhow::bail!("handle_dialog not supported by this backend")
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Extension point
    // ═══════════════════════════════════════════════════════════════════

    /// Handle a backend-specific tool call not covered by the standard set.
    async fn handle_extension_tool(
        &self,
        _name: &str,
        _args: &serde_json::Value,
    ) -> Option<anyhow::Result<String>> {
        None
    }

    /// Return extra tool definitions specific to this backend.
    fn extension_tools(&self) -> Vec<serde_json::Value> {
        vec![]
    }
}
