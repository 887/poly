//! Long-press state machine — 500 ms sustained-touch gesture.
//!
//! Extracted from the hand-rolled version in `channel_list.rs:1295–1368`
//! (plan-context-menu-quality-control.md §4.4.1) so every component that
//! opens a context menu uses the same timing, cancellation logic, and
//! haptic-feedback hook.
//!
//! The state machine mirrors iOS Safari's native long-press:
//! - `touchstart` starts a 500 ms timer, stamped with a generation counter.
//! - `touchmove`, `touchend`, `touchcancel` advance the generation,
//!   invalidating any in-flight timer whose generation no longer matches.
//! - If the timer fires with the stamped generation still current, the
//!   `on_fire` callback runs with the touch's client coordinates and
//!   (best-effort) a 10 ms haptic buzz.
//!
//! # Usage
//!
//! ```ignore
//! let long_press = LongPress::new(500, move |x, y| {
//!     app_state.write().context_menu_stack.push(ActiveContextMenu::at(x, y, ...));
//! });
//!
//! rsx! {
//!     div {
//!         ontouchstart: long_press.on_touch_start(),
//!         ontouchend: long_press.on_touch_end(),
//!         ontouchmove: long_press.on_touch_move(),
//!         ontouchcancel: long_press.on_touch_cancel(),
//!         // …
//!     }
//! }
//! ```
//!
//! The callback receives page coordinates suitable for
//! `MenuAnchor::Cursor { x, y }` on desktop. On mobile the anchor is coerced
//! to `Center` by the runtime (plan §4.3.1), but long-press still fires the
//! open event — the coordinates are retained for a future "open-near-touch"
//! mode.

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Reusable long-press detector built on top of a generation-counter
/// `Signal<u64>`. Clone is cheap — the underlying signal and callback are
/// reference-counted.
#[derive(Clone)]
pub struct LongPress {
    duration_ms: u64,
    generation: Signal<u64>,
    on_fire: Rc<RefCell<dyn FnMut(f64, f64)>>,
}

impl LongPress {
    /// Build a detector that fires `on_fire(client_x, client_y)` when a
    /// touch lasts `duration_ms` without moving, ending, or being cancelled.
    pub fn new(duration_ms: u64, on_fire: impl FnMut(f64, f64) + 'static) -> Self {
        Self {
            duration_ms,
            generation: use_signal(|| 0_u64),
            on_fire: Rc::new(RefCell::new(on_fire)),
        }
    }

    /// 500 ms default — matches iOS Safari and the pre-extraction
    /// `channel_list.rs` timer.
    pub fn default_500ms(on_fire: impl FnMut(f64, f64) + 'static) -> Self {
        Self::new(500, on_fire)
    }

    /// `ontouchstart` handler. Captures the first touch's client
    /// coordinates, stamps a fresh generation, and schedules the fire.
    pub fn on_touch_start(&self) -> impl FnMut(TouchEvent) + 'static {
        let duration = self.duration_ms;
        let mut gen_sig = self.generation;
        let on_fire = self.on_fire.clone();
        move |evt: TouchEvent| {
            evt.prevent_default();
            evt.stop_propagation();
            let (x, y) = evt
                .touches()
                .first()
                .map_or((0.0_f64, 0.0_f64), |t| {
                    let c = t.client_coordinates();
                    (c.x, c.y)
                });

            let stamp = gen_sig.peek().wrapping_add(1);
            gen_sig.set(stamp);

            let on_fire = on_fire.clone();
            spawn(async move {
                let mut eval = dioxus::prelude::document::eval(&format!(
                    "setTimeout(() => dioxus.send(true), {duration})"
                ));
                let Ok(true) = eval.recv::<bool>().await else {
                    return;
                };
                if *gen_sig.peek() != stamp {
                    return;
                }
                vibrate_best_effort(10);
                (on_fire.borrow_mut())(x, y);
            });
        }
    }

    /// `ontouchend` / `ontouchcancel` handler. Advances the generation so
    /// the pending timer is invalidated even if it has already been queued.
    pub fn on_touch_end(&self) -> impl FnMut(TouchEvent) + 'static {
        let mut gen_sig = self.generation;
        move |evt: TouchEvent| {
            evt.stop_propagation();
            let next = gen_sig.peek().wrapping_add(1);
            gen_sig.set(next);
        }
    }

    /// `ontouchmove` handler. Same as `on_touch_end` today; a future
    /// version may gate on a per-axis 10 px threshold (plan §4.4.2).
    pub fn on_touch_move(&self) -> impl FnMut(TouchEvent) + 'static {
        self.on_touch_end()
    }

    /// `ontouchcancel` handler — alias for `on_touch_end`.
    pub fn on_touch_cancel(&self) -> impl FnMut(TouchEvent) + 'static {
        self.on_touch_end()
    }
}

/// Best-effort haptic buzz via `navigator.vibrate(ms)` (plan §4.4.2).
///
/// No-op on non-WASM targets and on browsers where the API is missing. The
/// evaluator swallows any error; haptics are a nice-to-have.
fn vibrate_best_effort(ms: u32) {
    let _ = dioxus::prelude::document::eval(&format!(
        "if (navigator && navigator.vibrate) navigator.vibrate({ms});"
    ));
}
