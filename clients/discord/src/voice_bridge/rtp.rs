//! Extracted from voice_bridge.rs as part of SOLID B.2 split.
//!
//! RTP header build/parse + XChaCha20-Poly1305 nonce derivation.
//! Pure structural move — no behaviour change.

use super::*;

pub(super) fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; RTP_HEADER_SIZE] {
    let mut h = [0u8; RTP_HEADER_SIZE];
    h[0] = 0x80;
    h[1] = RTP_PAYLOAD_TYPE_OPUS;
    h[2] = (sequence >> 8) as u8;
    h[3] = sequence as u8;
    h[4] = (timestamp >> 24) as u8;
    h[5] = (timestamp >> 16) as u8;
    h[6] = (timestamp >> 8) as u8;
    h[7] = timestamp as u8;
    h[8] = (ssrc >> 24) as u8;
    h[9] = (ssrc >> 16) as u8;
    h[10] = (ssrc >> 8) as u8;
    h[11] = ssrc as u8;
    h
}

pub(super) fn parse_rtp_header(packet: &[u8]) -> Option<(u32, usize)> {
    if packet.len() < RTP_HEADER_SIZE {
        return None;
    }
    if (packet[0] >> 6) & 0x3 != 2 {
        return None;
    }
    let has_ext = (packet[0] >> 4) & 0x1 == 1;
    let csrc_count = (packet[0] & 0x0F) as usize;
    let ssrc = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let mut offset = RTP_HEADER_SIZE + csrc_count * 4;
    if offset > packet.len() {
        return None;
    }
    if has_ext {
        if offset + 4 > packet.len() {
            return None;
        }
        let ext_len = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]) as usize;
        offset += 4 + ext_len * 4;
    }
    if offset > packet.len() {
        return None;
    }
    Some((ssrc, offset))
}

/// Derive a 24-byte XChaCha20 nonce from the RTP header.
/// The Discord `aead_xchacha20_poly1305_rtpsize` mode uses the first 24 bytes
/// of the RTP header (zero-padded) as the nonce.
pub(super) fn xchacha_nonce_from_rtp(rtp_header: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 24];
    let len = rtp_header.len().min(24);
    nonce[..len].copy_from_slice(&rtp_header[..len]);
    nonce.to_vec()
}
