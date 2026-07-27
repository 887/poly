//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! Opus encode + RTP send loop (B.7). Pure structural move.

// Codec/DSP math: as-casts and arithmetic in the encode pipeline are intentional
#![allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::arithmetic_side_effects,
    clippy::default_numeric_fallback,
    clippy::future_not_send, // voice tasks run on single-threaded tokio executor
    clippy::cognitive_complexity,
    clippy::too_many_arguments,
)]

use super::*;

pub(super) async fn udp_encode_loop(
    udp: Arc<UdpSocket>,
    mut input_stream: BoxInputStream,
    secret_key: [u8; 32],
    mode: String,
    local_ssrc: u32,
    sequence: Arc<AtomicU16>,
    timestamp: Arc<AtomicU32>,
    is_speaking: Arc<AtomicBool>,
    transmit_mode: TransmitMode,
    aead_nonce: Arc<AtomicU32>,
    mut shutdown: ShutdownRx,
) {
    let encoder = match OpusEncoder::new(
        OpusSampleRate::Hz48000,
        OpusChannels::Stereo,
        OpusApplication::Voip,
    ) {
        Ok(e) => e,
        Err(e) => {
            warn!(target: "poly_discord::voice", error = %e, "failed to create Opus encoder");
            return;
        }
    };

    let cipher = match XChaCha20Poly1305::new_from_slice(&secret_key) {
        Ok(c) => c,
        Err(e) => {
            warn!(target: "poly_discord::voice", error = %e, "failed to create AEAD cipher");
            return;
        }
    };

    let mut opus_buf = vec![0u8; 4000];
    let mut pcm_accumulator: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);

    loop {
        // Race the mic against the shutdown signal.  `changed()` also resolves
        // (with an error) when the sender is dropped, so a dropped
        // DiscordVoiceConnection tears this loop down too.
        let frame = tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            next = input_stream.next() => match next {
                Some(f) => f,
                None => break,
            },
        };
        pcm_accumulator.extend_from_slice(&frame);

        while pcm_accumulator.len() >= OPUS_FRAME_SAMPLES {
            let pcm_frame: Vec<i16> = pcm_accumulator.drain(..OPUS_FRAME_SAMPLES).collect();

            // VAD / PTT gate (B.9).
            let transmitting = transmit_mode.should_transmit(&pcm_frame);
            is_speaking.store(transmitting, Ordering::Relaxed);
            if !transmitting {
                continue;
            }

            // Opus encode.
            let encoded_len = match encoder.encode(&pcm_frame, &mut opus_buf) {
                Ok(n) => n,
                Err(e) => {
                    warn!(target: "poly_discord::voice", error = %e, "Opus encode error");
                    continue;
                }
            };
            let opus_data = &opus_buf[..encoded_len];

            // Build RTP header (B.6).
            let seq = sequence.fetch_add(1, Ordering::Relaxed);
            // Per-channel sample-frame count (960), not the interleaved sample
            // count (1920) — see OPUS_TIMESTAMP_INCREMENT.
            let ts = timestamp.fetch_add(OPUS_TIMESTAMP_INCREMENT, Ordering::Relaxed);
            let rtp_header = build_rtp_header(seq, ts, local_ssrc);

            // AEAD encrypt (B.6).
            // aead_xchacha20_poly1305_rtpsize: the nonce is a 32-bit counter,
            // appended unencrypted to the packet; the RTP header is AAD only.
            // The counter is shared with the video transport (same session key).
            let nonce_counter = aead_nonce.fetch_add(1, Ordering::Relaxed);
            let encrypted = match encrypt_rtp(&cipher, &rtp_header, opus_data, &mode, nonce_counter) {
                Ok(e) => e,
                Err(e) => {
                    warn!(target: "poly_discord::voice", error = %e, "AEAD encrypt error");
                    continue;
                }
            };

            // Send RTP header + encrypted payload.
            let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + encrypted.len());
            packet.extend_from_slice(&rtp_header);
            packet.extend_from_slice(&encrypted);

            if let Err(e) = udp.send(&packet).await {
                warn!(target: "poly_discord::voice", error = %e, "UDP send error");
            }
        }
    }

    is_speaking.store(false, Ordering::Relaxed);
    debug!(target: "poly_discord::voice", "encode loop exited");
}
