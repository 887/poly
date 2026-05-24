//! Extracted from voice_bridge.rs as part of SOLID B.2 split.
//!
//! Discord voice WS protocol helpers — handshake, IP discovery, key derivation.
//! Pure structural move — no behaviour change.

use super::*;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use std::cell::RefCell;
use std::time::Duration;

    /// Result of a successful `run_handshake` call.
    pub struct HandshakeResult {
        /// Discord UDP server IP from op 2 Ready.
        pub server_ip: String,
        /// Discord UDP server port from op 2 Ready.
        pub server_port: u16,
        /// Local SSRC assigned by Discord.
        pub ssrc: u32,
        /// Negotiated AEAD mode string.
        pub mode: String,
        /// Opaque WS handle for the `finish_handshake` call.
        /// On wasm32 this is a gloo WebSocket; on native a tungstenite sink.
        /// We use a boxed dynamic type to keep the function signatures clean.
        pub ws_handle: WsHandle,
    }

    /// A bidirectional handle to the voice WebSocket.
    ///
    /// Carries a send closure plus a `recv` channel fed by a background pump
    /// task that forwards every Text frame off the underlying WebSocket. The
    /// receiver is wrapped so it can be `take()`n exactly once (by the
    /// post-handshake listener task) without blocking other handle users.
    ///
    /// On WASM gloo_net WebSocket is `!Send`; the send closure is therefore
    /// `LocalBoxFuture`-bound and uses `Rc<RefCell<_>>` internally. We use
    /// the same shape on native for API symmetry — the underlying tokio-
    /// tungstenite sink is `Send` so `Arc<tokio::sync::Mutex<_>>` could be
    /// used there, but the current native bridge path returns Err from
    /// `run_handshake` anyway, so single-thread is fine for now.
    pub struct WsHandle {
        /// Closure that sends a JSON string on the voice WebSocket.
        ///
        /// Returns a `LocalBoxFuture` so it works for both wasm32 (where the
        /// underlying WebSocket sink is `!Send`) and native (where it would
        /// be `Send` but we keep the type uniform).
        pub send: Box<dyn Fn(String) -> futures::future::LocalBoxFuture<'static, Result<(), String>>>,
        /// Channel receiver fed by the WS pump task. Wrapped in
        /// `RefCell<Option<_>>` so the post-handshake listener task can
        /// `take_recv()` exactly once. Subsequent callers see `None`.
        ///
        /// `RefCell` (not `Mutex`) because on wasm32 the whole handle is
        /// single-thread by construction, and on native the only place we
        /// touch it is in the synchronous `take_recv` accessor.
        pub recv: RefCell<Option<UnboundedReceiver<String>>>,
    }

    impl WsHandle {
        /// Take ownership of the recv channel. Exactly one caller wins; all
        /// others see `None`. Used by the post-handshake listener task.
        pub fn take_recv(&self) -> Option<UnboundedReceiver<String>> {
            self.recv.borrow_mut().take()
        }

        /// Receive the next Text frame from the WS with a timeout.
        ///
        /// Borrows the recv channel; will return an Err if the channel has
        /// already been taken via `take_recv`. Used by `finish_handshake` to
        /// wait for op 4 SESSION_DESCRIPTION before the long-lived listener
        /// task is spawned.
        ///
        /// On wasm32 the timeout is implemented via
        /// `gloo_timers::future::TimeoutFuture` raced via `futures::select`.
        /// On native we use `tokio::time::timeout`. This mirrors the
        /// `BackendHandleExt::read_with_timeout` pattern documented in
        /// CLAUDE.md hang-class #4 mitigation.
        pub async fn recv_text_with_timeout(
            &self,
            dur: Duration,
        ) -> Result<String, String> {
            use futures::StreamExt;

            // Hold an Option<RefMut> across awaits is fine on single-thread
            // WASM; on native this method is only called from the handshake
            // path which runs on one task. Take the receiver out of the
            // RefCell for the duration of the await and put it back after.
            let mut rx = self
                .recv
                .borrow_mut()
                .take()
                .ok_or("WsHandle.recv already taken — finish_handshake must run before the listener spawns")?;

            let result: Result<String, String> = {
                #[cfg(target_arch = "wasm32")]
                {
                    use futures::future::{select, Either};
                    let timeout = gloo_timers::future::TimeoutFuture::new(
                        u32::try_from(dur.as_millis()).unwrap_or(u32::MAX),
                    );
                    let next = rx.next();
                    futures::pin_mut!(timeout);
                    futures::pin_mut!(next);
                    match select(timeout, next).await {
                        Either::Left(_) => Err(format!(
                            "WsHandle.recv_text_with_timeout: timed out after {}ms",
                            dur.as_millis()
                        )),
                        Either::Right((Some(msg), _)) => Ok(msg),
                        Either::Right((None, _)) => Err("WS closed".into()),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match tokio::time::timeout(dur, rx.next()).await {
                        Ok(Some(msg)) => Ok(msg),
                        Ok(None) => Err("WS closed".into()),
                        Err(_) => Err(format!(
                            "WsHandle.recv_text_with_timeout: timed out after {}ms",
                            dur.as_millis()
                        )),
                    }
                }
            };

            // Restore the receiver so the long-lived listener task can take
            // it after the handshake finishes.
            *self.recv.borrow_mut() = Some(rx);
            result
        }
    }

    impl std::fmt::Debug for WsHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WsHandle").finish_non_exhaustive()
        }
    }

    // SAFETY: wasm32 is single-threaded (no `std::thread::spawn`, no Send
    // transfer between OS threads is possible). The `send` closure inside
    // `WsHandle` returns `LocalBoxFuture` which is `!Send` in the type
    // system; on a single-threaded runtime this is fine because there is no
    // other thread to transfer to. The IsBackend trait requires Send + Sync,
    // and we satisfy that on wasm32 only via these unsafe impls. On native,
    // the closure is `Send + Sync` naturally and no unsafe impl is needed.
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    unsafe impl Send for WsHandle {}
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    unsafe impl Sync for WsHandle {}

    /// Build a sender pair used by the WS pump task (both WASM and native).
    pub(super) fn ws_recv_channel() -> (UnboundedSender<String>, UnboundedReceiver<String>) {
        futures::channel::mpsc::unbounded()
    }

    /// Parse the op 4 SESSION_DESCRIPTION payload and extract the 32-byte
    /// `secret_key`. Returns Err if the frame is not op 4 or if `d.secret_key`
    /// is missing / not an array of ints.
    ///
    /// Extracted as a helper so it can be unit-tested without spinning up
    /// the WS / UDP / AEAD stack.
    pub fn parse_session_description(frame: &str) -> Result<Option<Vec<u8>>, String> {
        let v: serde_json::Value = serde_json::from_str(frame)
            .map_err(|e| format!("session_description parse: {e}"))?;
        if v.get("op").and_then(|o| o.as_u64()) != Some(4) {
            return Ok(None);
        }
        let arr = v
            .pointer("/d/secret_key")
            .and_then(|k| k.as_array())
            .ok_or("op 4: missing d.secret_key array")?;
        let key: Vec<u8> = arr
            .iter()
            .filter_map(|n| n.as_u64().map(|x| x as u8))
            .collect();
        if key.is_empty() {
            return Err("op 4: secret_key array is empty".into());
        }
        Ok(Some(key))
    }

    /// Run the Discord voice WS handshake.
    ///
    /// Sequence: op 8 HELLO → op 0 IDENTIFY → op 2 READY.
    ///
    /// Returns a `HandshakeResult` containing the UDP server address,
    /// the SSRC, the negotiated AEAD mode, and a WS handle for subsequent
    /// sends (op 1 SELECT_PROTOCOL).
    pub async fn run_handshake(
        ws_endpoint: &str,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<HandshakeResult, String> {
        // Use plain `ws://` for loopback endpoints (local dev / mock server),
        // `wss://` for all other hosts. Matches POLY_DISCORD_VOICE_WS_INSECURE
        // semantics without requiring an env-var import in WASM.
        let host = ws_endpoint.trim_end_matches(':').trim_end_matches('/');
        let scheme = if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
            "ws"
        } else {
            "wss"
        };
        let ws_url = format!(
            "{scheme}://{host}/voice/ws?v={}",
            super::VOICE_WS_VERSION
        );

        // On wasm32 we use gloo-net WebSocket (browser-native, no FFI).
        #[cfg(target_arch = "wasm32")]
        return run_handshake_wasm(ws_url, ws_token, ws_session_id, guild_id, user_id).await;

        // On native (test / chat-mcp builds) use tokio-tungstenite. Requires
        // the `gateway` feature to pull in tokio-tungstenite.
        #[cfg(all(not(target_arch = "wasm32"), feature = "gateway"))]
        return run_handshake_native(ws_url, ws_token, ws_session_id, guild_id, user_id).await;

        #[cfg(all(not(target_arch = "wasm32"), not(feature = "gateway")))]
        {
            let _ = (ws_url, ws_token, ws_session_id, guild_id, user_id);
            Err("voice_bridge::run_handshake requires either wasm32 target or the `gateway` feature for tokio-tungstenite".into())
        }
    }

    /// IP discovery via `/host/udp/send` + read the response from the UDP SSE stream.
    ///
    /// Sends the 74-byte Discord IP discovery packet and parses the response.
    pub async fn ip_discovery_via_udp(
        udp: &UdpClient,
        session_id: &str,
        ssrc: u32,
        local_port: u16,
    ) -> Result<(String, u16), String> {
        // Build the 74-byte discovery packet.
        let mut buf = [0u8; 74];
        buf[0] = 0x00;
        buf[1] = 0x01;
        buf[2] = 0x00;
        buf[3] = 0x46;
        buf[4] = (ssrc >> 24) as u8;
        buf[5] = (ssrc >> 16) as u8;
        buf[6] = (ssrc >> 8) as u8;
        buf[7] = ssrc as u8;
        // bytes 8..72 are the local IP (zero for request).
        // bytes 72..74 are the local port.
        buf[72] = (local_port >> 8) as u8;
        buf[73] = local_port as u8;

        udp.send(session_id, &buf, None)
            .await
            .map_err(|e| format!("IP discovery send: {e}"))?;

        // Read the response from the SSE stream.
        use futures::StreamExt;
        let mut stream = udp.recv_stream_boxed(session_id.to_string());
        let dgram = stream
            .next()
            .await
            .ok_or("IP discovery: no response from server")?;

        use base64::Engine as _;
        let resp = base64::engine::general_purpose::STANDARD
            .decode(dgram.data.as_bytes())
            .map_err(|e| format!("IP discovery decode: {e}"))?;

        if resp.len() < 74 {
            return Err(format!("IP discovery: short response {} bytes", resp.len()));
        }
        if u16::from_be_bytes([resp[0], resp[1]]) != 0x0002 {
            return Err("IP discovery: unexpected response type".into());
        }
        let addr_end = resp[8..72].iter().position(|&b| b == 0).unwrap_or(64);
        let ip = std::str::from_utf8(&resp[8..8 + addr_end])
            .map_err(|e| format!("IP discovery: bad UTF-8: {e}"))?
            .to_string();
        let port = u16::from_be_bytes([resp[72], resp[73]]);
        Ok((ip, port))
    }

    /// Send op 1 SELECT_PROTOCOL and wait for op 4 SESSION_DESCRIPTION.
    ///
    /// Returns the 32-byte `secret_key`. Loops past unrelated frames
    /// (op 6 HEARTBEAT_ACK, op 5 SPEAKING, etc.) with a 5-second total
    /// timeout per frame read. Discord typically replies within a single
    /// RTT after SELECT_PROTOCOL, so a 5-second per-frame budget is
    /// conservative.
    pub async fn finish_handshake(
        ws_handle: &WsHandle,
        local_ip: &str,
        local_port: u16,
        mode: &str,
    ) -> Result<Vec<u8>, String> {
        let payload = serde_json::json!({
            "op": 1,
            "d": {
                "protocol": "udp",
                "data": { "address": local_ip, "port": local_port, "mode": mode }
            }
        });
        (ws_handle.send)(payload.to_string()).await?;

        // Loop reading from the WS recv channel until op 4 arrives. Skip
        // unrelated ops — they are not fatal here. 5-second per-frame
        // timeout (Phase X.0 F.3).
        loop {
            let msg = ws_handle
                .recv_text_with_timeout(Duration::from_secs(5))
                .await?;
            match parse_session_description(&msg)? {
                Some(secret_key) => return Ok(secret_key),
                None => continue, // not op 4 — keep looping
            }
        }
    }

    // ── WASM-only handshake ────────────────────────────────────────────────────

    #[cfg(target_arch = "wasm32")]
    async fn run_handshake_wasm(
        ws_url: String,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<HandshakeResult, String> {
        use gloo_net::websocket::{Message, futures::WebSocket};
        use futures::{SinkExt, StreamExt};

        let ws = WebSocket::open(&ws_url)
            .map_err(|e| format!("WebSocket::open failed: {e:?}"))?;
        let (mut ws_tx, mut ws_rx) = ws.split();

        // Wait for op 8 HELLO.
        let heartbeat_ms = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("HELLO parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
                        let ms = v
                            .get("d")
                            .and_then(|d| d.get("heartbeat_interval"))
                            .and_then(|i| i.as_u64())
                            .unwrap_or(5000);
                        break ms;
                    }
                }
                None => return Err("WS closed before op 8 HELLO".into()),
                _ => continue,
            }
        };
        let _ = heartbeat_ms; // heartbeat loop is a follow-up

        // Send op 0 IDENTIFY.
        let identify = serde_json::json!({
            "op": 0,
            "d": {
                "server_id": guild_id.unwrap_or(user_id),
                "user_id": user_id,
                "session_id": ws_session_id,
                "token": ws_token,
            }
        });
        ws_tx
            .send(Message::Text(identify.to_string()))
            .await
            .map_err(|e| format!("send IDENTIFY: {e:?}"))?;

        // Wait for op 2 READY.
        let (ssrc, server_ip, server_port, modes) = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("READY parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
                        let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
                        let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
                        let ip = d
                            .get("ip")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let port = d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                        let modes: Vec<String> = d
                            .get("modes")
                            .and_then(|m| m.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        break (ssrc, ip, port, modes);
                    }
                }
                None => return Err("WS closed before op 2 READY".into()),
                _ => continue,
            }
        };

        // Select the preferred AEAD mode.
        let mode = super::PREFERRED_AEAD_MODES
            .iter()
            .find(|&&m| modes.iter().any(|s| s == m))
            .map(|&m| m.to_string())
            .ok_or("no supported AEAD mode in op 2 READY")?;

        // Wrap the sink in Rc<RefCell> — WASM is single-threaded so Rc is fine
        // and avoids the Send requirement that Arc<tokio::sync::Mutex<..>> imposes.
        use std::rc::Rc;
        use std::cell::RefCell;
        let tx_guard = Rc::new(RefCell::new(ws_tx));

        // Phase X.0 F.2 — pump the rest of the WS stream into an unbounded
        // mpsc. ws_rx already had op 8 + op 2 consumed above; everything
        // from here on is op 4 / op 6 / op 5 / etc. and goes to the
        // channel. The pump task is owned by `wasm_bindgen_futures::spawn_local`
        // and terminates when ws_rx returns None (WS closed) or the
        // receiver is dropped.
        let (recv_tx, recv_rx) = ws_recv_channel();
        wasm_bindgen_futures::spawn_local(async move {
            let mut ws_rx = ws_rx;
            while let Some(item) = ws_rx.next().await {
                if let Ok(Message::Text(text)) = item {
                    if recv_tx.unbounded_send(text).is_err() {
                        // receiver dropped — caller no longer cares
                        break;
                    }
                }
                // Binary frames + Err items are skipped on the bridge path.
            }
        });

        let ws_handle = WsHandle {
            send: Box::new(move |msg: String| {
                let tx = Rc::clone(&tx_guard);
                Box::pin(async move {
                    let mut sink = tx.borrow_mut();
                    sink.send(Message::Text(msg))
                        .await
                        .map_err(|e| format!("WS send: {e:?}"))
                }) as futures::future::LocalBoxFuture<'static, Result<(), String>>
            }),
            recv: RefCell::new(Some(recv_rx)),
        };

        Ok(HandshakeResult {
            server_ip,
            server_port,
            ssrc,
            mode,
            ws_handle,
        })
    }

    // ── Native-only handshake (Phase X.0 follow-up) ───────────────────────────

    /// Native counterpart to `run_handshake_wasm`. Drives the Discord voice
    /// gateway v8 handshake via `tokio-tungstenite`, then spawns a tokio task
    /// that pumps Text frames into the recv channel for the lifetime of the
    /// WS. Mirrors the wasm path 1:1 — same op sequence, same channel shape,
    /// same `WsHandle` contract.
    ///
    /// Used by:
    ///   - `clients/discord/tests/voice_bridge_handshake.rs` integration test.
    ///   - any chat-mcp native consumer of `DiscordVoiceBridgeClient`.
    #[cfg(all(not(target_arch = "wasm32"), feature = "gateway"))]
    async fn run_handshake_native(
        ws_url: String,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<HandshakeResult, String> {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let (ws, _resp) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| format!("connect_async failed: {e}"))?;
        let (mut ws_tx, mut ws_rx) = ws.split();

        // Wait for op 8 HELLO.
        let heartbeat_ms = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("HELLO parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
                        let ms = v
                            .get("d")
                            .and_then(|d| d.get("heartbeat_interval"))
                            .and_then(|i| i.as_u64())
                            .unwrap_or(5000);
                        break ms;
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(format!("WS recv before HELLO: {e}")),
                None => return Err("WS closed before op 8 HELLO".into()),
            }
        };
        let _ = heartbeat_ms; // heartbeat loop is a follow-up

        // Send op 0 IDENTIFY.
        let identify = serde_json::json!({
            "op": 0,
            "d": {
                "server_id": guild_id.unwrap_or(user_id),
                "user_id": user_id,
                "session_id": ws_session_id,
                "token": ws_token,
            }
        });
        ws_tx
            .send(Message::Text(identify.to_string().into()))
            .await
            .map_err(|e| format!("send IDENTIFY: {e}"))?;

        // Wait for op 2 READY.
        let (ssrc, server_ip, server_port, modes) = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("READY parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
                        let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
                        let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
                        let ip = d
                            .get("ip")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let port = d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                        let modes: Vec<String> = d
                            .get("modes")
                            .and_then(|m| m.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        break (ssrc, ip, port, modes);
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(format!("WS recv before READY: {e}")),
                None => return Err("WS closed before op 2 READY".into()),
            }
        };

        // Select the preferred AEAD mode.
        let mode = super::PREFERRED_AEAD_MODES
            .iter()
            .find(|&&m| modes.iter().any(|s| s == m))
            .map(|&m| m.to_string())
            .ok_or("no supported AEAD mode in op 2 READY")?;

        // Wrap the sink in Arc<tokio::sync::Mutex<_>> so the send closure can
        // be invoked from multiple sites (finish_handshake, heartbeat, etc.)
        // without ownership headaches. The send closure returns a
        // `LocalBoxFuture` to keep the API symmetric with the wasm path.
        let tx_guard = std::sync::Arc::new(tokio::sync::Mutex::new(ws_tx));

        // Pump the remainder of the WS stream into the recv channel. The
        // task ends when ws_rx returns None (WS closed) or the receiver is
        // dropped (caller no longer cares).
        let (recv_tx, recv_rx) = ws_recv_channel();
        tokio::spawn(async move {
            let mut ws_rx = ws_rx;
            while let Some(item) = ws_rx.next().await {
                if let Ok(Message::Text(text)) = item {
                    if recv_tx.unbounded_send(text.to_string()).is_err() {
                        break;
                    }
                }
                // Binary frames + Err items skipped on the bridge path.
            }
        });

        let ws_handle = WsHandle {
            send: Box::new(move |msg: String| {
                let tx = std::sync::Arc::clone(&tx_guard);
                Box::pin(async move {
                    let mut sink = tx.lock().await;
                    sink.send(Message::Text(msg.into()))
                        .await
                        .map_err(|e| format!("WS send: {e}"))
                })
                    as futures::future::LocalBoxFuture<'static, Result<(), String>>
            }),
            recv: RefCell::new(Some(recv_rx)),
        };

        Ok(HandshakeResult {
            server_ip,
            server_port,
            ssrc,
            mode,
            ws_handle,
        })
    }
