// Minimal fixture for `poly::use_effect_spawn_cycle`. Like the
// `raw_signal_write` fixture, we ship shim modules under the canonical
// paths `dioxus_hooks::{use_effect, spawn}` so the lint's DefPath
// matcher resolves the same way it would in production.

#![allow(dead_code, unused_variables)]

mod dioxus_hooks {
    pub fn use_effect<F: FnOnce()>(f: F) {
        f();
    }
    pub fn spawn<F>(_f: F) {}
}

mod dioxus_signals {
    pub struct Signal<T>(pub T);
    impl<T: Clone> Signal<T> {
        pub fn read(&self) -> T {
            self.0.clone()
        }
        pub fn batch(&self, _f: impl FnOnce(&T)) {}
    }
}

use dioxus_hooks::{spawn, use_effect};
use dioxus_signals::Signal;

fn flagged(sig: Signal<i32>) {
    use_effect(move || {
        spawn(async move {
            sig.batch(|_| {}); // should lint (use_effect_spawn_cycle)
        });
    });
}

fn ok_no_spawn(sig: Signal<i32>) {
    use_effect(move || {
        let _ = sig.read(); // OK — no spawn
    });
}

fn ok_spawn_no_write(sig: Signal<i32>) {
    use_effect(move || {
        spawn(async move {
            let _ = sig.read(); // OK — no write inside spawn
        });
    });
}

fn main() {}
