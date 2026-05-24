//! `SettingsBackend` impl for [`super::RedditBackend`].

use async_trait::async_trait;
use poly_client::{ClientResult, SettingsSection, SettingsStorageCell};

use super::RedditBackend;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for RedditBackend {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        // Reddit has no per-server / per-channel settings exposed yet.
        Ok(Vec::new())
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}
