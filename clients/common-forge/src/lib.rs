//! Shared helpers for GitHub and Forgejo forge clients.
//!
//! Both clients need:
//! - base64 decoding (API responses embed file content as newline-interspersed b64)
//! - `FileKind` mapping from the API string
//! - `owner~repo` segment splitting (Dioxus router-safe separator)

use poly_client::{ClientError, ClientResult, FileKind};

/// Map an API `type` / `kind` string to [`FileKind`].
///
/// Used by both GitHub (`/contents` API) and Forgejo (`/repos/{owner}/{repo}/git/trees`).
pub fn kind_from_string(s: &str) -> FileKind {
    match s {
        "dir" => FileKind::Directory,
        "symlink" => FileKind::Symlink,
        "submodule" => FileKind::Submodule,
        _ => FileKind::File,
    }
}

/// Split `"{owner}~{repo}"` at the first `~`.
///
/// `~` is used instead of `/` because the host's Dioxus router treats `/`
/// as a path delimiter and truncates the channel_id segment. `~` is
/// URL-safe and disallowed in GitHub / Forgejo owner and repo names.
pub fn split_owner_repo(s: &str) -> ClientResult<(String, String)> {
    s.split_once('~')
        .map(|(o, r)| (o.to_string(), r.to_string()))
        .ok_or_else(|| ClientError::NotFound(format!("malformed owner/repo segment: {s}")))
}

/// Decode a base64-encoded string, stripping embedded whitespace first.
///
/// GitHub and Forgejo both return file contents as base64 with embedded
/// newlines (every 60 or 76 chars). This function strips all whitespace
/// before decoding.
pub fn decode_b64(s: &str) -> Vec<u8> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    decode_b64_simple(&cleaned)
}

/// Decode a whitespace-free standard base64 string.
///
/// Uses a hand-rolled lookup table to avoid pulling in an external `base64`
/// crate. Invalid characters are silently skipped; padding `=` stops decoding.
pub fn decode_b64_simple(input: &str) -> Vec<u8> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        if let Some(slot) = lookup.get_mut(usize::from(b)) {
            // lint-allow-unused: i is bounded by TABLE.len() = 64 < u8::MAX
            #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
            {
                *slot = i as u8;
            }
        }
    }
    let bytes = input.as_bytes();
    // lint-allow-unused: base64 expansion ratio 3/4 — capacity hint; truncation harmless
    #[allow(clippy::integer_division, clippy::arithmetic_side_effects)]
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0_u32;
    let mut bits = 0_u32;
    for &b in bytes {
        if b == b'=' {
            break;
        }
        let v = lookup.get(usize::from(b)).copied().unwrap_or(255);
        if v == 255 {
            continue;
        }
        buf = (buf << 6_u32) | u32::from(v);
        // lint-allow-unused: base64 6-bit chunk accumulator; bits stays in [0, 13]
        #[allow(clippy::arithmetic_side_effects)]
        {
            bits += 6;
        }
        if bits >= 8 {
            // lint-allow-unused: guarded by `bits >= 8` above; never underflows
            #[allow(clippy::arithmetic_side_effects)]
            {
                bits -= 8;
            }
            // lint-allow-unused: masked with 0xff so cast to u8 is exact
            #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    out
}
