//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! Voice WS handshake helpers — op 8 HELLO / op 0 IDENTIFY / op 2 READY /
//! op 1 SELECT_PROTOCOL / op 4 SESSION_DESCRIPTION. Pure structural move.

use super::*;

/// Data from op 2 Ready.
pub(super) struct VoiceReady {
    pub(super) ssrc: u32,
    pub(super) ip: String,
    pub(super) port: u16,
    pub(super) modes: Vec<String>,
}

/// Data from op 4 Session Description.
pub(super) struct SessionDesc {
    pub(super) secret_key: Vec<u8>,
}

pub(super) type WsWrite = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    TMsg,
>;
pub(super) type WsRead = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
>;

/// Read frames until op 8 Hello arrives; return heartbeat_interval_ms.
pub(super) async fn wait_for_hello(read: &mut WsRead) -> Result<u64, VoiceError> {
    while let Some(msg) = read.next().await {
        let text = ws_text(msg)?;
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
            let interval = v
                .get("d")
                .and_then(|d| d.get("heartbeat_interval"))
                .and_then(|i| i.as_u64())
                .unwrap_or(5000);
            return Ok(interval);
        }
    }
    Err(VoiceError::WsConnect("WS closed before op 8 Hello".into()))
}

/// Send op 0 IDENTIFY.
pub(super) async fn send_identify(write: &mut WsWrite, info: &VoiceServerInfo) -> Result<(), VoiceError> {
    let payload = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": info.guild_id.as_deref().unwrap_or(&info.user_id),
            "user_id": info.user_id,
            "session_id": info.session_id,
            "token": info.token,
        }
    });
    write
        .send(TMsg::Text(payload.to_string().into()))
        .await
        .map_err(|e| VoiceError::WsConnect(e.to_string()))
}

/// Read frames until op 2 Ready.
pub(super) async fn wait_for_ready(read: &mut WsRead) -> Result<VoiceReady, VoiceError> {
    while let Some(msg) = read.next().await {
        let text = ws_text(msg)?;
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
            let ip = d.get("ip").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let port = d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
            let modes: Vec<String> = d
                .get("modes")
                .and_then(|m| m.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            return Ok(VoiceReady { ssrc, ip, port, modes });
        }
    }
    Err(VoiceError::WsConnect("WS closed before op 2 Ready".into()))
}

/// Read frames until op 4 Session Description.
pub(super) async fn wait_for_session_description(read: &mut WsRead) -> Result<SessionDesc, VoiceError> {
    while let Some(msg) = read.next().await {
        let text = ws_text(msg)?;
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(4) {
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let key: Vec<u8> = d
                .get("secret_key")
                .and_then(|k| k.as_array())
                .map(|a| a.iter().filter_map(|b| b.as_u64().map(|n| n as u8)).collect())
                .unwrap_or_default();
            return Ok(SessionDesc { secret_key: key });
        }
    }
    Err(VoiceError::WsConnect("WS closed before op 4 Session Description".into()))
}

/// Pick the highest-supported AEAD mode from the ready modes list.
pub(super) fn select_encryption_mode(modes: &[String]) -> Result<String, VoiceError> {
    for preferred in PREFERRED_AEAD_MODES {
        if modes.iter().any(|m| m == preferred) {
            return Ok((*preferred).to_string());
        }
    }
    Err(VoiceError::NoSupportedEncryptionMode)
}

/// Send op 1 SELECT PROTOCOL.
pub(super) async fn send_select_protocol(
    write: &mut WsWrite,
    local_ip: &str,
    local_port: u16,
    mode: &str,
) -> Result<(), VoiceError> {
    let payload = serde_json::json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": {
                "address": local_ip,
                "port": local_port,
                "mode": mode,
            }
        }
    });
    write
        .send(TMsg::Text(payload.to_string().into()))
        .await
        .map_err(|e| VoiceError::WsConnect(e.to_string()))
}
