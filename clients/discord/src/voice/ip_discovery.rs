//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! UDP IP-discovery — Discord voice protocol B.4. Pure structural move.

use super::*;

pub(super) async fn ip_discovery(udp: &UdpSocket, ssrc: u32) -> Result<(String, u16), VoiceError> {
    // Build request packet (74 bytes).
    let mut buf = [0u8; 74];
    // type = 0x0001
    buf[0] = 0x00;
    buf[1] = 0x01;
    // length = 70
    buf[2] = 0x00;
    buf[3] = 0x46;
    // ssrc (big-endian)
    buf[4] = (ssrc >> 24) as u8;
    buf[5] = (ssrc >> 16) as u8;
    buf[6] = (ssrc >> 8) as u8;
    buf[7] = ssrc as u8;
    // address[8..72] and port[72..74] are already zero (request leaves them empty).

    udp.send(&buf)
        .await
        .map_err(|e| VoiceError::IpDiscovery(format!("send failed: {e}")))?;

    // Wait for response (up to 5s).
    let mut resp = [0u8; 74];
    let n = time::timeout(Duration::from_secs(5), udp.recv(&mut resp))
        .await
        .map_err(|_| VoiceError::IpDiscovery("timed out".into()))?
        .map_err(|e| VoiceError::IpDiscovery(format!("recv failed: {e}")))?;

    if n < 74 {
        return Err(VoiceError::IpDiscovery(format!("short response: {n} bytes")));
    }

    // Response type should be 0x0002.
    let resp_type = u16::from_be_bytes([resp[0], resp[1]]);
    if resp_type != 0x0002 {
        return Err(VoiceError::IpDiscovery(format!("unexpected type: {resp_type:#x}")));
    }

    // address: null-terminated in bytes 8..72.
    let addr_end = resp[8..72]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(64);
    let ip = std::str::from_utf8(&resp[8..8 + addr_end])
        .map_err(|e| VoiceError::IpDiscovery(format!("bad IP utf8: {e}")))?
        .to_string();
    let port = u16::from_be_bytes([resp[72], resp[73]]);

    Ok((ip, port))
}

