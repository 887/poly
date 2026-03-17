# Memory: Stoat attachment upload blocked by current outbound attachment shape

*Stored: 2026-03-17T10:17:17.677052193+00:00*

---

Important implementation finding on 2026-03-17 while starting the remaining attachment half of Stoat `3.1.2.7`:

- The current chat composer in `crates/core/src/ui/account/common/chat_view.rs` converts pending files into `poly_client::Attachment` using only:
  - `id`
  - `filename`
  - `content_type`
  - `size`
  - `url` (preview URL or empty string)
- `build_attachment_previews(...)` only keeps a base64 data URL preview for some small images; it does **not** persist the original file bytes for general attachments.
- By the time `send_message(...)` reaches a backend, the Stoat client no longer has the raw file contents needed for the required Autumn upload step (`POST {autumn}/{tag}` multipart `file`).

Implication:
- Full Stoat attachment send support is not just a `clients/stoat` HTTP change.
- It requires shared outbound attachment plumbing changes in `poly-core` / possibly `poly-client` so real file bytes or a durable upload source survive until backend send time.

Because of that, continuing with Stoat user/profile/presence support is the clean next slice unless we explicitly choose a broader shared attachment-pipeline refactor.
