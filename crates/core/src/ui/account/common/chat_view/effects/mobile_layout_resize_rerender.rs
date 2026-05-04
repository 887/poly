use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
pub(in super::super) fn use_mobile_layout_resize_rerender_effect(mobile_layout_resize_tick: Signal<u64>) {
    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only capture (mobile_layout_resize_tick); no non-Signal props
        use std::cell::Cell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        let Some(window) = web_sys::window() else {
            return;
        };

        // RAF-throttle: at most one re-render per animation frame regardless of
        // how many resize events fire (browsers fire resize at ~60 Hz).
        let raf_pending = Rc::new(Cell::new(false));

        // lint-allow-unused: Box<dyn Fn> coercion via `as` is the wasm-bindgen idiom
        #[allow(clippy::as_conversions)]
        let closure = Closure::wrap(Box::new(move |_evt: web_sys::Event| {
            if raf_pending.get() {
                return;
            }
            raf_pending.set(true);
            let raf_pending2 = Rc::clone(&raf_pending);
            let mut tick_signal = mobile_layout_resize_tick;
            // lint-allow-unused: Box<dyn FnOnce> coercion via `as` is the wasm-bindgen idiom
            #[allow(clippy::as_conversions)]
            let raf_cb = Closure::once(Box::new(move || {
                raf_pending2.set(false);
                if let Ok(mut tick) = tick_signal.try_write() {
                    *tick = tick.wrapping_add(1);
                }
            }) as Box<dyn FnOnce()>);
            if let Some(window) = web_sys::window() {
                drop(window.request_animation_frame(raf_cb.as_ref().unchecked_ref()));
            }
            raf_cb.forget();
        }) as Box<dyn FnMut(web_sys::Event)>);

        drop(window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref()));
        closure.forget();
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(in super::super) fn use_mobile_layout_resize_rerender_effect(_mobile_layout_resize_tick: Signal<u64>) {}
