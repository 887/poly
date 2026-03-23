//! Shared runtime JavaScript loader helpers.
//!
//! NOTE(DX-ASSET-JS-1): runtime JS that must participate in Dioxus hot-reload
//! should be declared with `asset!` and loaded through this helper instead of
//! being embedded with `include_str!`.

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

/// Fetch a JS asset and evaluate it in the browser.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn load_js_asset(asset: Asset) -> bool {
    let asset_url = String::from(asset);
    let json_url = match serde_json::to_string(&asset_url) {
        Ok(json) => json,
        Err(err) => {
            tracing::warn!("Failed to serialize JS asset URL: {err}");
            return false;
        }
    };

    let js = format!(
        r#"(async () => {{
            await new Promise((resolve, reject) => {{
                const script = document.createElement('script');
                script.type = 'text/javascript';
                script.src = {json_url};
                script.onload = () => resolve();
                script.onerror = () => reject(new Error('Failed to load JS asset: ' + {json_url}));
                document.head.appendChild(script);
            }});
            return 'ready';
        }})()"#,
    );
    let mut eval = document::eval(&js);
    matches!(eval.recv::<String>().await, Ok(status) if status == "ready")
}

/// No-op on non-wasm targets.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn load_js_asset<T>(_asset: T) -> bool {
    false
}
