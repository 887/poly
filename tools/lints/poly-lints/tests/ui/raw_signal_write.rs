// Minimal fixture for `poly::raw_signal_write`. We can't depend on the
// real dioxus crate here (it would drag the whole WASM build into
// nightly), so we ship a **shim** with the canonical path
// `dioxus_signals::Signal::write` that matches the lint's path check.
//
// The lint resolves the receiver type by canonical DefPath, so as long
// as the crate name + type name + method name line up, the lint fires
// the same way it would on real dioxus code.

#![allow(dead_code)]

// Fake dioxus_signals crate layout.
mod dioxus_signals {
    pub struct Signal<T>(pub T);

    impl<T> Signal<T> {
        pub fn write(&self) -> &T {
            &self.0
        }
        pub fn batch(&self, _f: impl FnOnce(&T)) {}
    }
}

// RwLock to prove we don't flag `.write().await`.
mod tokio_sync {
    pub struct RwLock<T>(pub T);
    impl<T> RwLock<T> {
        pub async fn write(&self) -> &T {
            &self.0
        }
    }
}

fn flagged(sig: &dioxus_signals::Signal<i32>) {
    let _ = sig.write(); // should lint (raw_signal_write)
}

fn allowed_batch(sig: &dioxus_signals::Signal<i32>) {
    sig.batch(|_| {}); // OK
}

async fn not_rwlock(rw: &tokio_sync::RwLock<i32>) {
    let _ = rw.write().await; // OK — different DefPath
}

fn main() {}
