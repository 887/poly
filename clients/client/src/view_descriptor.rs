//! `ViewDescriptorBackend` capability sub-trait (Phase C.1 — ISP split).
//!
//! Carved out of [`IsBackend`] in Phase C.1 of
//! `docs/plans/plan-solid-audit-core-state.md`.  Groups the sidebar /
//! account-overview / channel-view descriptor methods that drive the
//! plugin-controlled UI surface (`D5 / D19 / D23 / D25`).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(vd) = backend.as_view_descriptor() {
//!     let view = vd.get_account_overview_view().await?;
//! }
//! ```
//!
//! The legacy [`IsBackend`] methods (`get_sidebar_declaration`,
//! `invoke_sidebar_action`, `get_account_overview_view`,
//! `get_channel_view`, `get_view_rows`, `get_view_detail`) remain as
//! default-delegating shims so existing call sites in `crates/core/`
//! continue to compile.
//!
//! [`IsBackend`]: crate::IsBackend
//! [`IsBackend::as_view_descriptor`]: crate::IsBackend::as_view_descriptor

use async_trait::async_trait;

use crate::{
    ActionOutcome, CardSpec, ClientError, ClientResult, Cursor, SidebarDeclaration,
    SidebarLayoutKind, ViewBody, ViewDescriptor, ViewDetail, ViewHeader, ViewKind, ViewRowsPage,
};

/// Capability sub-trait for plugin-declared UI view descriptors.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ViewDescriptorBackend: Send + Sync {
    /// D5 / D19 — plugin's current sidebar declaration.
    ///
    /// Default: Custom layout with no sections.
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Custom,
            sections: vec![],
            header_block: None,
        })
    }

    /// D14 / D25 — dispatch a sidebar-item click.
    ///
    /// Default: `Err(NotFound(action_id))`.
    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(action_id.to_string()))
    }

    /// Fetch the account-level overview view descriptor.
    ///
    /// Default: generic CardGrid descriptor.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("overview-default-title".to_string()),
                subtitle_key: Some("overview-default-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    /// D5 — fetch a channel's non-chat view descriptor.
    ///
    /// Default: `Err(NotSupported)`.
    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("get_channel_view".to_string()))
    }

    /// D23 — paged data feed.
    ///
    /// Default: empty page.
    async fn get_view_rows(
        &self,
        _channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        Ok(ViewRowsPage {
            rows: vec![],
            next_cursor: None,
        })
    }

    /// D5 — detail payload for `split` views.
    ///
    /// Default: `Err(NotSupported)`.
    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("get_view_detail".to_string()))
    }
}
