//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! Voice WS event loop — heartbeat + op 5 SPEAKING. Pure structural move.

use super::*;

pub(super) async fn voice_ws_loop(
    mut write: WsWrite,
    mut read: WsRead,
    local_ssrc: u32,
    heartbeat_interval_ms: u64,
    ssrc_user_map: SsrcUserMap,
    speaking_flag: Arc<AtomicBool>,
    speaking_tx: Option<(String, tokio::sync::mpsc::UnboundedSender<ClientEvent>)>,
    mut ws_out_rx: mpsc::Receiver<serde_json::Value>,
    bandwidth_ctrl: Arc<rtcp::BandwidthController>,
) {
    let interval = Duration::from_millis(heartbeat_interval_ms);
    let mut heartbeat_tick = time::interval(interval);
    // E.9: slow ramp-up ticker — fires every 2s to gradually recover bitrate
    // after congestion.  The ramp-up is a no-op when no video transport is active
    // (bandwidth_ctrl stays at DEFAULT_BITRATE_BPS = 1 Mbps).
    let mut ramp_up_tick = time::interval(Duration::from_secs(2));
    let mut nonce: u64 = 0;
    let mut last_speaking = false;

    loop {
        tokio::select! {
            _ = heartbeat_tick.tick() => {
                nonce = nonce.wrapping_add(1);
                let hb = serde_json::json!({ "op": 3, "d": nonce });
                if write.send(TMsg::Text(hb.to_string().into())).await.is_err() {
                    break;
                }
            }
            Some(outbound) = ws_out_rx.recv() => {
                // Auxiliary outbound messages (op 12 Video, op 14 Client Connect, etc.)
                if write.send(TMsg::Text(outbound.to_string().into())).await.is_err() {
                    break;
                }
            }
            _ = ramp_up_tick.tick() => {
                // E.9: slow ramp-up — recover bitrate gradually after congestion.
                // No-op if already at max.
                bandwidth_ctrl.ramp_up();
            }
            msg = read.next() => {
                let text = match msg {
                    Some(Ok(TMsg::Text(t))) => t.to_string(),
                    Some(Ok(TMsg::Close(_))) | None => break,
                    _ => continue,
                };
                let v: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(99);
                match op {
                    // op 6 = HEARTBEAT_ACK — no action.
                    6 => {}
                    // op 5 = SPEAKING — update SSRC → user_id map (B.8).
                    // C.4: also emit VoiceSpeakingUpdate so UI speaking rings update.
                    5 => {
                        if let Some(d) = v.get("d") {
                            let ssrc = d.get("ssrc")
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0) as u32;
                            let user_id = d.get("user_id")
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string();
                            // speaking bitmask: 0 = not speaking, non-zero = speaking.
                            let speaking_bitmask = d.get("speaking")
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0);
                            if ssrc != 0 && !user_id.is_empty() {
                                ssrc_user_map.write().await.insert(ssrc, user_id.clone());
                                // C.4 — emit speaking indicator event if wired.
                                if let Some((ref channel_id, ref tx)) = speaking_tx {
                                    let ev = ClientEvent::VoiceSpeakingUpdate {
                                        channel_id: channel_id.clone(),
                                        user_id,
                                        is_speaking: speaking_bitmask != 0,
                                    };
                                    let _ = tx.send(ev);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Emit op 5 SPEAKING when the flag transitions (B.8).
        let now_speaking = speaking_flag.load(Ordering::Relaxed);
        if now_speaking != last_speaking {
            last_speaking = now_speaking;
            let speaking_bitmask: u32 = if now_speaking { 1 } else { 0 };
            let ev = serde_json::json!({
                "op": 5,
                "d": {
                    "speaking": speaking_bitmask,
                    "delay": 0,
                    "ssrc": local_ssrc,
                }
            });
            if write.send(TMsg::Text(ev.to_string().into())).await.is_err() {
                break;
            }
        }
    }
    debug!(target: "poly_discord::voice", "voice WS loop exited");
}
