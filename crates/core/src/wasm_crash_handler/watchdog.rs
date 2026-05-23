//! Boot-hang and interaction-hang watchdogs.
//!
//! Contains three independent JS-injected hang detectors:
//!
//! - **Boot watchdog** — a `setTimeout` that fires if the startup overlay has
//!   not dismissed after [`BOOT_HANG_TIMEOUT_MS`] milliseconds.
//! - **Interaction watchdog** — a Web Worker heartbeat that detects post-boot
//!   main-thread deadlocks by measuring the gap between worker pings.
//! - **ServiceWorker force-reloader** — registers `/poly-service-worker.js`
//!   and sends heartbeats so the SW can force-navigate an otherwise-wedged tab.
//!
//! The interaction watchdog JS also embeds the out-of-band trace sink pattern
//! (POST to `/host/kv/set`) documented in CLAUDE.md's "last-resort diagnostic
//! path" section.  That fetch dispatches from the network thread even when the
//! main WASM thread is wedged, making it the only reliable way to log bisect
//! steps during a live hang.

/// How long (ms) the boot watchdog waits before declaring a hang.
/// Normal boots complete in well under a second; 20 s is generous.
// Boot can take >20s with many restored accounts + favorited servers; 60s is
// a more realistic ceiling and still catches genuine boot hangs.
pub(super) const BOOT_HANG_TIMEOUT_MS: u32 = 60_000;


/// How long (ms) the main thread may be unresponsive before the interaction
/// watchdog shows the crash overlay. 12 s fires fast enough to feel responsive
/// while still absorbing the Dog/Teams first-render pass. The overlay then
/// offers a visible "Reload" affordance + an auto-reload countdown so the
/// user never has to guess whether the tab is dead.
///
/// If the main thread is truly wedged (tight WASM loop that never yields),
/// the ServiceWorker-based force-reloader (registered separately from
/// `install_wasm_crash_handler`) kicks in at ~25 s and navigates the client
/// back to its current URL, bypassing the main thread entirely.
///
/// NOTE: The previous value of 30 s was too long — users thought the tab was
/// dead and closed it before the overlay ever showed. 12 s is late enough to
/// absorb Dog-account first-render (E5 regression threshold) without feeling
/// unresponsive.
pub(super) const INTERACTION_HANG_TIMEOUT_MS: u32 = 12_000;

/// How long (ms) the overlay sits before auto-reloading if the user hasn't
/// clicked "Reload" yet. 15 s matches typical attention span after seeing a
/// modal — long enough to read, short enough to recover unattended.
const OVERLAY_AUTO_RELOAD_MS: u32 = 15_000;

/// Register `/poly-service-worker.js` and start posting heartbeats to it.
///
/// The ServiceWorker runs on a separate thread from the page's main thread,
/// so it can observe when main-thread heartbeats stop. When a gap exceeds
/// its own timeout, it calls `client.navigate(client.url)` — a browser-level
/// navigation that bypasses the wedged main thread and force-reloads the tab.
/// This is the only reliable recovery path for truly infinite WASM loops.
pub(super) fn install_service_worker_force_reloader() {
    // language=JavaScript
    let js = r#"(function() {
    if (!('serviceWorker' in navigator)) { return; }
    if (window.__polyServiceWorkerInstalled) { return; }
    window.__polyServiceWorkerInstalled = true;

    navigator.serviceWorker.register('/poly-service-worker.js', { scope: '/' })
        .then(function(reg) {
            function beat() {
                var sw = navigator.serviceWorker.controller || reg.active;
                if (sw) { try { sw.postMessage({ type: 'poly-heartbeat' }); } catch (e) {} }
            }
            beat();
            setInterval(beat, 500);
        })
        .catch(function(err) {
            console.warn('[poly] ServiceWorker registration failed — no hang auto-reload', err);
        });
})();"#;
    drop(js_sys::eval(js));
}

/// Inject a JS heartbeat that detects post-boot main-thread deadlocks.
///
/// A Web Worker sends a `ping` every 500 ms. The main-thread listener
/// records `Date.now()` on each message. A second `setInterval` — also on
/// the main thread — checks the gap between *now* and the last heartbeat;
/// if the gap exceeds `timeout_ms`, the main thread processed no messages
/// in that window (definition of a hang) and we show the crash overlay.
///
/// The worker itself runs on a separate OS thread so it ticks independently
/// of main-thread load. The main-thread interval is what actually notices
/// the gap, which only fires when the main thread resumes — but then it
/// sees that `Date.now() - lastPing > timeout_ms` and shows the overlay
/// retroactively. That's fine for the user-visible case: either the thread
/// recovers (and we warn them the app was just unresponsive) or the thread
/// stays dead forever and the worker logs to console.
pub(super) fn install_interaction_hang_watchdog(timeout_ms: u32) {
    // language=JavaScript
    let js = format!(
        r#"(function() {{
    if (window.__polyInteractionWatchdogInstalled) {{ return; }}
    window.__polyInteractionWatchdogInstalled = true;

    var TIMEOUT = {timeout};
    var AUTO_RELOAD_MS = {auto_reload};
    window.__polyLastHeartbeat = Date.now();

    // Reset heartbeat on visibility change (resume from suspend, tab
    // switch). Without this, Date.now() jumps across the suspend and
    // the watchdog sees a fake multi-hour "hang". This replaces the
    // previous MAX_REAL_HANG upper-bound filter, which also hid genuine
    // long hangs (>60s) from the user.
    document.addEventListener('visibilitychange', function() {{
        if (document.visibilityState === 'visible') {{
            window.__polyLastHeartbeat = Date.now();
        }}
    }});

    try {{
        var workerSrc = 'setInterval(function(){{postMessage(1)}}, 500);';
        var blob = new Blob([workerSrc], {{ type: 'application/javascript' }});
        var worker = new Worker(URL.createObjectURL(blob));
        worker.onmessage = function() {{
            var now = Date.now();
            var gap = now - window.__polyLastHeartbeat;
            window.__polyLastHeartbeat = now;
            if (gap > TIMEOUT) {{
                showHangOverlay(gap);
            }}
        }};
    }} catch (e) {{
        console.warn('Poly interaction watchdog: worker unavailable', e);
    }}

    setInterval(function() {{
        var gap = Date.now() - window.__polyLastHeartbeat;
        if (gap > TIMEOUT) {{
            showHangOverlay(gap);
        }}
    }}, 1000);

    function showHangOverlay(gapMs) {{
        // Don't double-show if the crash overlay is already visible
        // (e.g. from a Rust panic or another hang report).
        var OVERLAY_ID = 'poly-wasm-crash-overlay';
        if (document.getElementById(OVERLAY_ID)) {{ return; }}
        // Reset the heartbeat so we don't spam the overlay every tick
        // while the user is reading it.
        window.__polyLastHeartbeat = Date.now();

        var overlay = document.createElement('div');
        overlay.id = OVERLAY_ID;
        overlay.style.cssText = 'position:fixed;inset:0;z-index:2147483647;overflow:auto;padding:28px;background:rgba(10,12,16,0.96);color:#fff;font-family:Inter,system-ui,sans-serif;';
        var card = document.createElement('div');
        card.style.cssText = 'max-width:920px;margin:0 auto;background:#1a1f2b;border:1px solid rgba(255,255,255,0.14);border-radius:16px;padding:24px;box-shadow:0 16px 48px rgba(0,0,0,0.45);';
        var h1 = document.createElement('h1');
        h1.style.cssText = 'margin:0 0 8px 0;font-size:28px;line-height:1.2;';
        h1.textContent = 'App not responding';
        var p1 = document.createElement('p');
        p1.style.cssText = 'margin:0 0 12px 0;color:#d8dee9;font-size:15px;line-height:1.5;';
        p1.textContent = 'Poly’s main thread was blocked for ' + Math.round(gapMs/1000) + ' seconds. This usually means an infinite render loop, a deadlocked Dioxus signal, or a missing async yield.';
        var p2 = document.createElement('p');
        p2.style.cssText = 'margin:0 0 18px 0;color:#8fbcff;font-size:14px;font-weight:600;';
        p2.textContent = 'Type: interaction-hang (' + gapMs + 'ms unresponsive)';
        var countdown = document.createElement('p');
        countdown.style.cssText = 'margin:0 0 18px 0;color:#d8dee9;font-size:13px;';
        var remaining = Math.round(AUTO_RELOAD_MS / 1000);
        countdown.textContent = 'Auto-reloading in ' + remaining + 's…';
        var btnRow = document.createElement('div');
        btnRow.style.cssText = 'display:flex;gap:10px;';
        var dismissBtn = document.createElement('button');
        dismissBtn.style.cssText = 'border:1px solid rgba(255,255,255,0.18);border-radius:10px;padding:12px 16px;background:transparent;color:#d8dee9;font-size:14px;font-weight:600;cursor:pointer;';
        dismissBtn.textContent = 'Dismiss (cancel auto-reload)';
        dismissBtn.onclick = function() {{
            clearInterval(ticker);
            var o = document.getElementById(OVERLAY_ID);
            if (o && o.parentNode) o.parentNode.removeChild(o);
        }};
        var btn = document.createElement('button');
        btn.style.cssText = 'border:0;border-radius:10px;padding:12px 16px;background:#4f8cff;color:white;font-size:14px;font-weight:600;cursor:pointer;';
        btn.textContent = 'Reload now';
        btn.onclick = function() {{ window.location.reload(); }};
        btnRow.appendChild(dismissBtn);
        btnRow.appendChild(btn);
        card.appendChild(h1);
        card.appendChild(p1);
        card.appendChild(p2);
        card.appendChild(countdown);
        card.appendChild(btnRow);
        overlay.appendChild(card);
        document.body && document.body.appendChild(overlay);
        console.error('Poly interaction hang: main thread blocked for ' + gapMs + 'ms');

        var ticker = setInterval(function() {{
            remaining -= 1;
            if (remaining <= 0) {{
                clearInterval(ticker);
                window.location.reload();
            }} else {{
                countdown.textContent = 'Auto-reloading in ' + remaining + 's…';
            }}
        }}, 1000);
    }}
}})();"#,
        timeout = timeout_ms,
        auto_reload = OVERLAY_AUTO_RELOAD_MS,
    );
    drop(js_sys::eval(&js));
}

/// Inject a JS `setTimeout` that shows the crash overlay if the startup
/// overlay has not dismissed after `timeout_ms` milliseconds.
///
/// Two-layer check (any one of these = "the app is fine, do NOT alert"):
///   1. `data-poly-startup-phase === "revealed"` — the App component sets
///      this when the startup overlay hides.
///   2. The favourites sidebar has rendered at least one account icon
///      (`.server-sidebar img[alt]`) — proves the UI is alive even if the
///      phase attribute slipped through (false-positive guard).
///
/// Both checks fail → app may be wedged. Overlay shows TWO buttons:
/// "Dismiss" (close, leave the app alone — useful for false positives)
/// and "Reload".
pub(super) fn install_boot_hang_watchdog(timeout_ms: u32) {
    // language=JavaScript
    let js = format!(
        r#"(function() {{
    var t = {timeout};
    window.__polyBootWatchdog = setTimeout(function() {{
        var phase = document.documentElement.getAttribute('data-poly-startup-phase');
        if (phase === 'revealed') {{ return; }}
        // Second-chance probe: even if the phase attribute didn't flip,
        // the app is healthy as long as the sidebar mounted SOMETHING.
        var sidebarIcons = document.querySelectorAll('.server-sidebar img[alt]');
        if (sidebarIcons.length > 0) {{
            console.warn('Poly boot watchdog: phase=' + phase + ' but ' + sidebarIcons.length + ' sidebar icons rendered, suppressing false-positive overlay');
            // Mark phase revealed so we don't keep checking.
            document.documentElement.setAttribute('data-poly-startup-phase', 'revealed');
            return;
        }}
        var OVERLAY_ID = 'poly-wasm-crash-overlay';
        if (document.getElementById(OVERLAY_ID)) {{ return; }}
        var overlay = document.createElement('div');
        overlay.id = OVERLAY_ID;
        overlay.style.cssText = 'position:fixed;inset:0;z-index:2147483647;overflow:auto;padding:28px;background:rgba(10,12,16,0.96);color:#fff;font-family:Inter,system-ui,sans-serif;';
        var card = document.createElement('div');
        card.style.cssText = 'max-width:920px;margin:0 auto;background:#1a1f2b;border:1px solid rgba(255,255,255,0.14);border-radius:16px;padding:24px;box-shadow:0 16px 48px rgba(0,0,0,0.45);';
        var h1 = document.createElement('h1');
        h1.style.cssText = 'margin:0 0 8px 0;font-size:28px;line-height:1.2;';
        h1.textContent = 'App may be stuck';
        var p1 = document.createElement('p');
        p1.style.cssText = 'margin:0 0 12px 0;color:#d8dee9;font-size:15px;line-height:1.5;';
        p1.textContent = 'Poly’s loading screen has been visible for over ' + (t/1000) + ' seconds. This usually means a render loop or missing data prevented startup, but it can also be a slow first boot. If the UI looks fine behind this overlay, just dismiss it.';
        var p2 = document.createElement('p');
        p2.style.cssText = 'margin:0 0 18px 0;color:#8fbcff;font-size:14px;font-weight:600;';
        p2.textContent = 'Type: boot-hang (startup overlay not dismissed after ' + (t/1000) + 's)';
        var btnRow = document.createElement('div');
        btnRow.style.cssText = 'display:flex;gap:10px;';
        var dismissBtn = document.createElement('button');
        dismissBtn.style.cssText = 'border:1px solid rgba(255,255,255,0.18);border-radius:10px;padding:12px 16px;background:transparent;color:#d8dee9;font-size:14px;font-weight:600;cursor:pointer;';
        dismissBtn.textContent = 'Dismiss';
        dismissBtn.onclick = function() {{
            var o = document.getElementById(OVERLAY_ID);
            if (o && o.parentNode) o.parentNode.removeChild(o);
            // Mark the phase as revealed so the watchdog (or any sibling
            // observer) doesn't immediately re-fire after dismiss.
            document.documentElement.setAttribute('data-poly-startup-phase', 'revealed');
        }};
        var reloadBtn = document.createElement('button');
        reloadBtn.style.cssText = 'border:0;border-radius:10px;padding:12px 16px;background:#4f8cff;color:white;font-size:14px;font-weight:600;cursor:pointer;';
        reloadBtn.textContent = 'Reload';
        reloadBtn.onclick = function() {{ window.location.reload(); }};
        btnRow.appendChild(dismissBtn);
        btnRow.appendChild(reloadBtn);
        card.appendChild(h1);
        card.appendChild(p1);
        card.appendChild(p2);
        card.appendChild(btnRow);
        overlay.appendChild(card);
        document.body && document.body.appendChild(overlay);
        console.error('Poly boot hang: startup overlay still visible after ' + t + 'ms (phase=' + phase + ', sidebar icons=' + sidebarIcons.length + ')');
    }}, t);
}})();"#,
        timeout = timeout_ms,
    );
    drop(js_sys::eval(&js));
}
