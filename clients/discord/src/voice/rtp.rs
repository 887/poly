//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! RTP header build / parse (B.6 — roll-our-own per plan decision). Pure structural move.

use super::*;

pub(super) fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; RTP_HEADER_SIZE] {
    let mut header = [0u8; RTP_HEADER_SIZE];
    // Byte 0: V=2, P=0, X=0, CC=0 → 0x80
    header[0] = 0x80;
    // Byte 1: M=0, PT=120 → 0x78
    header[1] = RTP_PAYLOAD_TYPE_OPUS;
    // Bytes 2-3: sequence number (big-endian)
    header[2] = (sequence >> 8) as u8;
    header[3] = sequence as u8;
    // Bytes 4-7: timestamp (big-endian)
    header[4] = (timestamp >> 24) as u8;
    header[5] = (timestamp >> 16) as u8;
    header[6] = (timestamp >> 8) as u8;
    header[7] = timestamp as u8;
    // Bytes 8-11: SSRC (big-endian)
    header[8] = (ssrc >> 24) as u8;
    header[9] = (ssrc >> 16) as u8;
    header[10] = (ssrc >> 8) as u8;
    header[11] = ssrc as u8;
    header
}

/// Parse the SSRC and payload offset from an RTP packet.
///
/// Returns `(ssrc, payload_start_offset)` or `None` for malformed packets.
pub(super) fn parse_rtp_header(packet: &[u8]) -> Option<(u32, usize)> {
    if packet.len() < RTP_HEADER_SIZE {
        return None;
    }
    let version = (packet[0] >> 6) & 0x3;
    if version != 2 {
        return None;
    }
    let has_extension = (packet[0] >> 4) & 0x1 == 1;
    let csrc_count = (packet[0] & 0x0F) as usize;
    let ssrc = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);

    let mut offset = RTP_HEADER_SIZE + csrc_count * 4;
    if offset > packet.len() {
        return None;
    }

    // Skip extension header if present.
    if has_extension {
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
