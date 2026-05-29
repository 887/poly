//! `impl DmsAndGroupsBackend for StoatClient` — group DMs, saved messages, mute stubs.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (H.3.c).
//!
//! Stoat (Revolt) supports DM channels, group DMs, saved messages, open/close,
//! leave, and group editing. Mute/unmute requires a per-instance notification
//! schema that is not stable; those two methods return NotSupported.

use crate::api::{self, StoatGroupEdit};
use async_trait::async_trait;
use futures::future;
use poly_client::{BackendType, ClientResult, DmChannel, Group};

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for StoatClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        let (channels, root_config) = future::try_join(
            self.http.fetch_direct_message_channels(),
            self.http.fetch_server_config(),
        )
        .await?;
        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);
        let account_id = self.current_account_metadata()?.0;

        future::try_join_all(
            channels
                .into_iter()
                .filter(api::StoatChannel::is_group)
                .map(|channel| {
                    let autumn_base_url = autumn_base_url.clone();
                    let account_id = account_id.clone();

                    async move {
                        let members = self.http.fetch_group_members(&channel.id).await?;
                        let last_message = self
                            .fetch_last_message_preview(
                                &channel.id,
                                channel.last_message_id.as_deref(),
                                autumn_base_url.as_deref(),
                            )
                            .await?;

                        Ok(Group {
                            id: channel.id,
                            members: members
                                .into_iter()
                                .map(|user| {
                                    user.into_poly_user_with_autumn(autumn_base_url.as_deref())
                                })
                                .collect(),
                            name: channel.name,
                            last_message,
                            backend: BackendType::from(crate::SLUG),
                            account_id: account_id.clone(),
                        })
                    }
                }),
        )
        .await
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let ((channels, unreads, root_config), self_user) = future::try_join(
            future::try_join3(
                self.http.fetch_direct_message_channels(),
                self.http.fetch_unreads(),
                self.http.fetch_server_config(),
            ),
            self.http.fetch_self(),
        )
        .await?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);
        let account_id = self.current_account_metadata()?.0;

        future::try_join_all(
            channels
                .into_iter()
                .filter(|channel| channel.is_direct_message() || channel.is_saved_messages())
                .map(|channel| {
                    let unread_index = unread_index.clone();
                    let autumn_base_url = autumn_base_url.clone();
                    let account_id = account_id.clone();
                    let self_user = self_user.clone();

                    async move {
                        let unread_count =
                            Self::unread_count_for_channel(&unread_index, &channel.id);
                        self.map_dm_like_channel(
                            channel,
                            unread_count,
                            autumn_base_url.as_deref(),
                            &account_id,
                            Some(&self_user),
                        )
                        .await
                    }
                }),
        )
        .await
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        Self::open_direct_message_channel(self, user_id).await
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Self::open_saved_messages_channel(self).await
    }

    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        self.http.add_group_member(group_id, user_id).await
    }

    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        self.http.remove_group_member(group_id, user_id).await
    }

    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()> {
        // Revolt exposes a per-user endpoint; fan out one call per user.
        future::try_join_all(
            user_ids
                .iter()
                .map(|user_id| self.http.add_group_member(channel_id, user_id)),
        )
        .await
        .map(|_| ())
    }

    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()> {
        self.http.close_or_leave_channel(channel_id).await
    }

    async fn mute_conversation(
        &self,
        channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        // SOLID-audit-stoat C.5: Stoat (Revolt) `PATCH /channels/{id}` with a nested
        // `notify` override is the canonical server-side mute path, but the override
        // schema varies between official `stoat.chat` and self-hosted Revolt forks
        // (some accept `"muted"` as a string enum, some require an integer level,
        // some reject the field entirely). Rather than guess per-instance and
        // produce silent 4xx noise, we mirror the same in-memory parity pattern
        // teams C.4 already ships: the `muted_dms` set inside `menu_state` is
        // already the source of truth the context-menu "mute-dm" action reads, so
        // wiring `mute_conversation` to it makes the trait method and the
        // context-action agree without a network round-trip. `_until` (timed
        // mute) is noted but Revolt notification overrides don't carry expiry;
        // we store the mute unconditionally (best-effort parity with teams).
        // When the Stoat notification-override schema stabilises, swap this in-
        // memory toggle for a real `PATCH /channels/{id}` call.
        tracing::debug!(channel_id, "stoat: mute_conversation (in-memory store)");
        if let Ok(mut state) = self.menu_state.lock() {
            state.muted_dms.insert(channel_id.to_string());
        }
        Ok(())
    }

    async fn unmute_conversation(&self, channel_id: &str) -> ClientResult<()> {
        // SOLID-audit-stoat C.5: symmetric with `mute_conversation` above.
        tracing::debug!(channel_id, "stoat: unmute_conversation (in-memory store)");
        if let Ok(mut state) = self.menu_state.lock() {
            state.muted_dms.remove(channel_id);
        }
        Ok(())
    }

    async fn leave_group_dm(&self, channel_id: &str) -> ClientResult<()> {
        self.http.close_or_leave_channel(channel_id).await
    }

    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        if avatar_url.is_some() {
            // Revolt icon updates require an Autumn file upload first; we can't
            // accept a plain URL here.  Log at debug (UI surfaces this — no need
            // to warn-spam server logs).  SOLID-audit-stoat (Phase B.2).
            tracing::debug!(
                channel_id,
                "edit_group_dm: Stoat requires an Autumn upload for icon changes; avatar_url ignored",
            );
        }

        let edit = StoatGroupEdit {
            name: name.map(str::to_string),
            remove: None,
        };

        self.http.edit_group_dm(channel_id, &edit).await
    }
}
