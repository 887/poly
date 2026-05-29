//! [`ClientBackend`] impl for [`RedditClient`].
//!
//! Wires the HTML-scraping Reddit client into the Poly trait surface so the
//! UI can render Reddit accounts alongside Lemmy, Discord, etc.
//!
//! # ID mapping
//!
//! | Reddit concept | Poly ID |
//! |---|---|
//! | Subreddit `<sub>` | Server `r_<sub>`, Channel `c_posts_<sub>` |
//! | Post `<id>` (t3) | Message `t3_<id>` |
//! | Comment `<id>` (t1) | Message `t1_<id>` |
//! | DM `<id>` (t4) | DmChannel `dm_<id>`, Message `t4_<id>` |
//! | Username `<name>` | User `u_<name>` |
//!
//! # Module layout (SOLID-audit-reddit C.3 split)
//!
//! - [`ids`] — fullname / poly-id bijections
//! - [`error`] — `RedditError` → `ClientError`, `NS_*` constants
//! - [`mapping`] — `parser::*` → `poly_client::*` conversions + HTML
//!   sanitisers + sort-key codecs
//! - [`is_backend`] — `IsBackend` trait impl
//! - [`social_graph`] — `SocialGraphBackend` impl
//! - [`dms_and_groups`] — `DmsAndGroupsBackend` impl
//! - [`messaging`] — `MessagingBackend` impl
//! - [`discover`] — `DiscoverBackend` impl
//! - [`settings`] — `SettingsBackend` impl
//! - [`view_descriptor`] — `ViewDescriptorBackend` impl

use poly_client::{Session, SettingsStorageCell, SettingsScope, BackendType, User, PresenceStatus, ClientResult, Message, ClientError, Attachment};

use crate::{RedditClient, SortKind};

pub(crate) mod ids;
pub(crate) mod error;
pub(crate) mod mapping;
pub(crate) mod is_backend;
pub(crate) mod social_graph;
pub(crate) mod dms_and_groups;
pub(crate) mod messaging;
pub(crate) mod discover;
pub(crate) mod settings;
pub(crate) mod view_descriptor;

use ids::user_id_for_name;
use mapping::{
    flatten_comments_into_messages, raw_post_to_message, sort_kind_from_str,
};

// ─── State storage for session ───────────────────────────────────────────────

/// `ClientBackend` adapter wrapping a `RedditClient` + optional session.
pub struct RedditBackend {
    pub(crate) client: RedditClient,
    pub(crate) session: Option<Session>,
    /// In-memory settings storage (mirrors Lemmy's stub pattern, Phase 4).
    pub(crate) settings_storage: SettingsStorageCell,
}

impl RedditBackend {
    /// Create a new backend from an already-constructed `RedditClient`.
    pub fn new(client: RedditClient) -> Self {
        Self { client, session: None, settings_storage: SettingsStorageCell::new() }
    }

    /// Read the `show-media-previews` mechanism state.
    ///
    /// Defaults to `true` (previews shown) when the user has never toggled it.
    pub(crate) fn media_previews_enabled(&self) -> bool {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "show-media-previews")
            .is_none_or(|v| v != "false")
    }

    /// Read the current sort mode.
    ///
    /// Defaults to `SortKind::Hot` when the user has never chosen a sort.
    pub(crate) fn current_sort(&self) -> SortKind {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "current-sort")
            .as_deref()
            .map_or(SortKind::Hot, sort_kind_from_str)
    }

    pub(crate) fn backend_type() -> BackendType {
        BackendType::from(crate::SLUG)
    }

    pub(crate) fn account_id(&self) -> &str {
        self.session.as_ref().map_or("reddit-anon", |s| s.id.as_str())
    }

    pub(crate) fn account_display_name(&self) -> &str {
        self.session
            .as_ref()
            .map_or("Anonymous", |s| s.user.display_name.as_str())
    }

    /// Build a `Session` for the given username.
    pub(crate) fn build_session(&self, username: &str) -> Session {
        // `token` is what gets persisted to KV and replayed via
        // `authenticate(Token(t))` on next app boot. It MUST be the
        // session-cookie value captured during login_with_password, not
        // the bare username — otherwise restore re-authenticates with
        // a string the server doesn't recognise as a session.
        // Falls back to username if (somehow) login didn't capture a
        // session — caller can still re-login with password from the UI.
        let token = self
            .client
            .session_cookie_value()
            .unwrap_or_else(|| username.to_string());
        Session {
            id: format!("reddit-{username}"),
            user: User {
                id: user_id_for_name(username),
                display_name: username.to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: Self::backend_type(),
            },
            token,
            backend: Self::backend_type(),
            icon_emoji: Some("🤖".to_string()),
            instance_id: "old.reddit.com".to_string(),
            backend_url: Some(self.client.base_url().to_string()),
        }
    }

    /// Fetch a post plus its full comment tree and return them as a flat
    /// `Vec<Message>` (OP first, then depth-first comments).
    ///
    /// Extracted from `get_messages` (B.3) to separate the three concerns:
    /// post fetch, gallery-url enrichment, and comment flattening.
    ///
    /// `bare_id` is the Reddit post ID **without** any `t3_` prefix.
    pub(crate) async fn fetch_post_thread_messages(
        &self,
        bare_id: &str,
        bt: &BackendType,
    ) -> ClientResult<Vec<Message>> {
        let (post, comments) = self
            .client
            .get_post(bare_id)
            .await
            .map_err(ClientError::from)?;

        // Always attempt the gallery JSON fetch — for a non-gallery post it
        // returns Ok(empty) cheaply; for a gallery post it gives us the full
        // ordered list of source URLs that the HTML scrape doesn't expose.
        // Append each as an Attachment on the OP message so
        // ForumThreadView renders the carousel.
        let gallery_urls: Vec<String> = if self.media_previews_enabled() {
            self.client.get_gallery_urls(bare_id).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut op_msg = raw_post_to_message(&post, bt);
        if gallery_urls.len() >= 2 {
            op_msg.attachments.clear();
            for (i, url) in gallery_urls.iter().enumerate() {
                op_msg.attachments.push(Attachment::remote(
                    format!("reddit-gallery-{bare_id}-{i}"),
                    format!("gallery_{i}.jpg"),
                    "image/jpeg".to_string(),
                    url.clone(),
                    0,
                ));
            }
            if op_msg.preview_image_url.is_none() {
                op_msg.preview_image_url = gallery_urls.first().cloned();
            }
        }

        let mut messages = Vec::new();
        messages.push(op_msg);
        flatten_comments_into_messages(&comments, bt, &mut messages);
        Ok(messages)
    }
}
