//! Message-fetch, send, attachment upload, delete, search, and unread sync.
//!
//! - `GET    /sync/unreads`
//! - `GET    /channels/{id}/messages` (+ query params)
//! - `GET    /channels/{id}/messages/{mid}`
//! - `POST   /channels/{id}/messages`
//! - `POST   {autumn}/attachments` (multipart upload)
//! - `DELETE /channels/{id}/messages/{mid}`
//! - `POST   /channels/{id}/search`
//!
//! Split out from the monolithic `http.rs` in SOLID-audit-stoat D.3.

use super::{STOAT_SESSION_TOKEN_HEADER, StoatHttpClient, encode_multipart_file};
use crate::api::{
    StoatAutumnUploadResponse, StoatBulkMessageResponse, StoatChannelUnread, StoatMessage,
    StoatSendMessageRequest,
};
use poly_client::{Attachment, ClientError, ClientResult, MessageQuery};
use poly_host_bridge::http::Method;

impl StoatHttpClient {
    /// Fetch unread metadata for the authenticated account.
    pub async fn fetch_unreads(&self) -> ClientResult<Vec<StoatChannelUnread>> {
        let response = self
            .authenticated_request(Method::GET, "/sync/unreads")?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch messages for a channel using Poly's generic message query.
    pub async fn fetch_messages(
        &self,
        channel_id: &str,
        query: &MessageQuery,
    ) -> ClientResult<StoatBulkMessageResponse> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }

        if let Some(around) = &query.around {
            params.push(("nearby", around.clone()));
        } else {
            if let Some(before) = &query.before {
                params.push(("before", before.clone()));
            }
            if let Some(after) = &query.after {
                params.push(("after", after.clone()));
            }

            let sort = if query.after.is_some() {
                "Oldest"
            } else {
                "Latest"
            };
            params.push(("sort", sort.to_string()));
        }

        params.push(("include_users", "true".to_string()));
        let mut path = format!("/channels/{channel_id}/messages");
        if !params.is_empty() {
            path.push('?');
            path.push_str(
                &params
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }

        let response = self
            .authenticated_request(Method::GET, &path)?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch a single Stoat message by channel and message ID.
    pub async fn fetch_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<StoatMessage> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/channels/{channel_id}/messages/{message_id}"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Send a text/reply message to a Stoat channel.
    pub async fn send_message(
        &self,
        channel_id: &str,
        payload: &StoatSendMessageRequest,
    ) -> ClientResult<StoatMessage> {
        let response = self
            .authenticated_request(Method::POST, &format!("/channels/{channel_id}/messages"))?
            .json(payload)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Upload one outbound attachment to the Stoat Autumn file service.
    pub async fn upload_attachment(
        &self,
        autumn_base_url: &str,
        attachment: &Attachment,
    ) -> ClientResult<String> {
        let token = self.session().map(|session| session.token).ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;
        let upload_bytes = attachment.upload_bytes.clone().ok_or_else(|| {
            ClientError::NotSupported("Stoat attachment send requires raw upload bytes".to_string())
        })?;

        let boundary = format!(
            "----polystoatboundary{}",
            uuid::Uuid::new_v4().simple()
        );
        let body = encode_multipart_file(
            &boundary,
            "file",
            &attachment.filename,
            &attachment.content_type,
            &upload_bytes,
        );

        let response = self
            .http
            .post(format!(
                "{}/attachments",
                autumn_base_url.trim_end_matches('/')
            ))
            .header(STOAT_SESSION_TOKEN_HEADER, token)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response
            .json::<StoatAutumnUploadResponse>()
            .await
            .map(|upload| upload.file_id)
            .map_err(|e| Self::network_error(&e))
    }

    /// Delete a message from a channel (`DELETE /channels/{channel_id}/messages/{message_id}`).
    pub async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/channels/{channel_id}/messages/{message_id}"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Search messages in a channel (`POST /channels/{channel_id}/search`).
    ///
    /// Returns an expanded response containing matching messages and bundled
    /// user records for display-name / avatar resolution without a follow-up
    /// round-trip.  Revolt caps results at 100 per request.
    pub async fn search_messages_channel(
        &self,
        channel_id: &str,
        req: &crate::api::StoatSearchRequest,
    ) -> ClientResult<crate::api::StoatSearchResponse> {
        let response = self
            .authenticated_request(
                Method::POST,
                &format!("/channels/{channel_id}/search"),
            )?
            .json(req)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }
}
