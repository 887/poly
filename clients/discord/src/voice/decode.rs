//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! UDP RTP receive + Opus decode loop (B.7 + E.9). Pure structural move.

use super::*;

pub(super) async fn udp_decode_loop(
    udp: Arc<UdpSocket>,
    secret_key: [u8; 32],
    mode: String,
    _ssrc_user_map: SsrcUserMap,
    output: Box<dyn poly_audio_backend::AudioOutputStream>,
    bandwidth_ctrl: Option<Arc<rtcp::BandwidthController>>,
) {
    let cipher = match XChaCha20Poly1305::new_from_slice(&secret_key) {
        Ok(c) => c,
        Err(e) => {
            warn!(target: "poly_discord::voice", error = %e, "decode: failed to create cipher");
            return;
        }
    };

    // One Opus decoder per remote SSRC.
    let mut decoders: HashMap<u32, OpusDecoder> = HashMap::new();
    let mut recv_buf = vec![0u8; MAX_UDP_PACKET];
    let mut pcm_buf = vec![0i16; OPUS_FRAME_SAMPLES * 2];

    loop {
        let n = match udp.recv(&mut recv_buf).await {
            Ok(n) => n,
            Err(e) => {
                warn!(target: "poly_discord::voice", error = %e, "UDP recv error");
                break;
            }
        };

        let packet = &recv_buf[..n];

        // Packets shorter than the RTP header are garbage.
        if packet.len() < RTP_HEADER_SIZE {
            continue;
        }

        // E.9 — RTCP bandwidth feedback: route RTCP packets to the bandwidth
        // controller instead of the Opus decode path.
        if rtcp::is_rtcp_packet(packet) {
            if let Some(ref ctrl) = bandwidth_ctrl {
                rtcp::handle_rtcp_datagram(packet, ctrl);
            }
            continue;
        }

        // Parse RTP header.
        let (ssrc, payload_offset) = match parse_rtp_header(packet) {
            Some(r) => r,
            None => continue,
        };

        let rtp_header = &packet[..payload_offset];
        let ciphertext = &packet[payload_offset..];

        // AEAD decrypt.
        let plaintext = match decrypt_rtp(&cipher, rtp_header, ciphertext, &mode) {
            Ok(p) => p,
            Err(_) => {
                // Discord sends keepalives and other non-audio packets — don't log.
                continue;
            }
        };

        // Opus decode.
        let decoder = decoders.entry(ssrc).or_insert_with(|| {
            OpusDecoder::new(OpusSampleRate::Hz48000, OpusChannels::Stereo)
                .expect("OpusDecoder::new never fails with valid params")
        });

        // audiopus wraps &[u8] as Packet and &mut [i16] as MutSignals.
        let packet = match Packet::try_from(plaintext.as_slice()) {
            Ok(p) => p,
            Err(_) => continue, // empty or oversized — skip
        };
        let mut_signals = match MutSignals::try_from(pcm_buf.as_mut_slice()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let decoded_samples = match decoder.decode(Some(packet), mut_signals, false) {
            Ok(n) => n,
            Err(e) => {
                warn!(target: "poly_discord::voice", ssrc, error = %e, "Opus decode error");
                continue;
            }
        };

        let pcm = &pcm_buf[..decoded_samples * 2]; // stereo: 2 i16 per sample
        if let Err(e) = output.push(pcm).await {
            warn!(target: "poly_discord::voice", error = %e, "output.push error");
        }
    }
    debug!(target: "poly_discord::voice", "decode loop exited");
}
