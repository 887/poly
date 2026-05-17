# Discord Nitro Feature Gating — Policy and Implementation

> Implemented in Phase E of `docs/plans/plan-discord-anti-ban.md`.

---

## Why we gate Nitro features intentionally

Poly's Discord backend uses a **user token** (not a bot token). Discord's
anti-abuse systems fingerprint anomalous activity — including users exercising
paid-tier features they do not subscribe to. Sending a 50 MB attachment on a
free account, or using animated emoji from another server without Nitro Classic,
are observable signals that a client is behaving non-humanly.

Our policy: **refuse Nitro-gated requests at the client layer** even when the
Discord API would technically accept them. The goal is to give Discord zero
anomalous signal, not to maximise leeched value.

If a contributor sees code that "could send a 50 MB file without Nitro" and is
tempted to "fix" it, please read this document first — the gate is intentional.

---

## Nitro Tiers

`premium_type` on `GET /users/@me` (also `DiscordUser.premium_type`):

| `premium_type` | Rust variant     | Notes                                      |
|----------------|------------------|--------------------------------------------|
| 0 or absent    | `NitroTier::None`    | Free account                               |
| 1              | `NitroTier::Classic` | Animated emoji, stickers, profile banner   |
| 2              | `NitroTier::Full`    | All Classic + 50 MB upload, GIF avatar, super-reactions, 2 server boosts |
| 3              | `NitroTier::Basic`   | Animated emoji use only; no upload bump, no boosts |

Source: discord-api-types v10 `UserPremiumType`.

---

## Per-feature gate table

| Feature                            | Minimum tier      | Where enforced            |
|------------------------------------|-------------------|---------------------------|
| Cross-server stickers              | Classic           | `NitroGate::can_use_cross_server_stickers` |
| Animated cross-server emoji        | Classic           | `NitroGate::can_use_animated_emoji` |
| Profile banners                    | Classic           | `NitroGate::can_set_profile_banner` |
| GIF avatars                        | Full              | `NitroGate::can_use_gif_avatar` / `NitroGate::check_gif_avatar` |
| Super-reactions                    | Full              | `NitroGate::can_use_super_reactions` |
| 50 MB file uploads                 | Full or Boost Tier 2 | `NitroGate::check_upload_size` / `NitroGate::max_upload_bytes` |
| 100 MB file uploads                | Boost Tier 3 (any tier) | Same helpers |

Defaults (no gate):

| Feature                | Notes                                                      |
|------------------------|------------------------------------------------------------|
| Custom status          | Free for all users; match official client behaviour exactly |
| Server discovery       | Not gated by Nitro; gated by server discoverability settings |

---

## Upload boundary

`NitroGate::max_upload_bytes(tier, guild_boost_level)` returns:

- **8 MB** for `NitroTier::None` or `NitroTier::Basic` on an unboosted guild.
- **50 MB** for `NitroTier::Classic` or `NitroTier::Full`, or any tier on a
  Boost Tier 2 guild.
- **100 MB** for any tier on a Boost Tier 3 guild.

`send_message_with_attachments` (when wired) calls
`NitroGate::check_upload_size(tier, boost_level, total_bytes)` before sending.
This returns `Err(ClientError::PermissionDenied("attachment too large: …"))` so
the UI can surface a friendly error rather than letting Discord return a 413 (which
counts toward the IP ban threshold).

---

## Implementation

### Reading the Nitro tier

`DiscordAccountInfo::update_nitro_tier(premium_type: Option<u8>)` is called from
`DiscordClient::authenticate()` after `get_me()` succeeds. The tier is stored in
`DiscordClient::account_info: Mutex<DiscordAccountInfo>`.

Refreshing on app focus: callers should call `http.get_me()` on foreground and
pass the updated `premium_type` to `account_info.lock().unwrap().update_nitro_tier(...)`.

### Reading the tier in HTTP client code

```rust
let tier = self.nitro_tier();  // on DiscordClient
NitroGate::check_upload_size(tier, guild_boost_level, total_bytes)?;
```

### Reading the tier in UI code

The UI accesses `DiscordClient` via `BackendHandle`. Until a typed accessor is
exposed through `poly_client::IsBackend`, the tier is read via the raw client
accessor. See `DiscordClient::nitro_tier()`.

---

## Adding a new Nitro-gated feature

1. Add a `can_<feature>(tier: NitroTier) -> bool` helper to `NitroGate` in
   `clients/discord/src/nitro.rs`.
2. Add a `check_<feature>(tier) -> Result<(), ClientError>` wrapper if the feature
   is enforced at the HTTP layer.
3. Add an entry to the table above.
4. Wire the check at the call site (UI affordance layer: dim/hide; HTTP layer:
   return `Err` before the request is sent).

---

## Testing

Unit tests for `NitroTier` and `NitroGate` live in `clients/discord/src/nitro.rs`
(inline `#[cfg(test)]` module). Run with:

```bash
cargo test -p poly-discord --features native nitro
```
