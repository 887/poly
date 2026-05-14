//! `AudioBackend` trait contract tests — Phase K.1 of plan-voice-video-calls.md.
//!
//! These tests exercise the trait surface via `MockAudioBackend` (an
//! instrumented double in `test_support`) and verify the contracts stated
//! in the `AudioBackend` / `AudioOutputStream` doc-comments:
//!
//! - `switch_input` / `switch_output` update the current-device accessor
//!   even when no stream is open (hot-swap without prior `open_*` call).
//! - `last_input_device_key` / `last_output_device_key` produce stable,
//!   well-formed KV keys for device persistence (Phase A.6).
//! - Opening an unknown device returns `AudioError::DeviceNotFound`.
//! - `open_output` → `push` → `close` event sequence is observable.
//! - Hot-swap event emission: calling `switch_input` emits `MockEvent::SwitchInput`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures::StreamExt;
use poly_audio_backend::{
    kv_keys::{last_input_device_key, last_output_device_key},
    test_support::{MockAudioBackend, MockEvent},
    AudioBackend, AudioDevice, AudioDeviceKind, AudioError, AudioFormat,
};

// ── K.1.1 — switch_input without prior open (hot-swap without close) ────────

#[tokio::test]
async fn switch_input_without_open_updates_current() {
    let mock = MockAudioBackend::new();

    // No stream open — current_input starts as None.
    // (The current_input_device call itself emits MockEvent::CurrentInputDevice;
    //  drain that before we start asserting on the switch event.)
    let pre = {
        let r = mock.current_input_device();
        mock.drain_events(); // discard the CurrentInputDevice event
        r
    };
    assert!(pre.is_none(), "expected no current input before any open/switch");

    // Switch without ever calling open_input.
    mock.switch_input("mock-mic-b").await.unwrap();

    let events = mock.drain_events();
    assert!(
        events.contains(&MockEvent::SwitchInput("mock-mic-b".into())),
        "expected SwitchInput event, got {events:?}"
    );

    // current_input_device should now reflect the switched-to device.
    mock.drain_events(); // discard CurrentInputDevice event from previous call
    let cur = mock.current_input_device();
    mock.drain_events();
    assert_eq!(
        cur.map(|d| d.id),
        Some("mock-mic-b".into()),
        "current input should be mock-mic-b after switch"
    );
}

// ── K.1.2 — switch_output without prior open ────────────────────────────────

#[tokio::test]
async fn switch_output_without_open_updates_current() {
    let mock = MockAudioBackend::new();

    mock.drain_events();
    mock.switch_output("mock-speaker-b").await.unwrap();

    let events = mock.drain_events();
    assert!(
        events.contains(&MockEvent::SwitchOutput("mock-speaker-b".into())),
        "expected SwitchOutput event"
    );

    let cur = mock.current_output_device();
    mock.drain_events();
    assert_eq!(
        cur.map(|d| d.id),
        Some("mock-speaker-b".into()),
        "current output should be mock-speaker-b"
    );
}

// ── K.1.3 — KV key format stability (Phase A.6 persistence) ─────────────────

#[test]
fn last_device_kv_keys_are_stable_and_scoped() {
    let in_key = last_input_device_key("acct-discord-abc123");
    let out_key = last_output_device_key("acct-stoat-xyz");

    assert_eq!(
        in_key, "voice.last_input_device.acct-discord-abc123",
        "input key format must be stable"
    );
    assert_eq!(
        out_key, "voice.last_output_device.acct-stoat-xyz",
        "output key format must be stable"
    );

    // Two different accounts must produce different keys.
    let key_a = last_input_device_key("user-a");
    let key_b = last_input_device_key("user-b");
    assert_ne!(key_a, key_b, "keys must be scoped per account");
}

// ── K.1.4 — hot-swap event emission (switch after open) ─────────────────────

#[tokio::test]
async fn hot_swap_emits_switch_event_after_open() {
    let mock = MockAudioBackend::new();

    // Open mock-mic-a (the default).
    let _stream = mock
        .open_input("mock-mic-a", AudioFormat::DISCORD_VOICE)
        .await
        .unwrap();
    mock.drain_events(); // discard OpenInput

    // Hot-swap to mock-mic-b while the "stream" is still alive.
    mock.switch_input("mock-mic-b").await.unwrap();

    let events = mock.drain_events();
    assert_eq!(
        events,
        vec![MockEvent::SwitchInput("mock-mic-b".into())],
        "only SwitchInput event expected after hot-swap"
    );

    // Current device updated.
    let cur = mock.current_input_device();
    mock.drain_events();
    assert_eq!(cur.map(|d| d.id), Some("mock-mic-b".into()));
}

// ── K.1.5 — unknown device → DeviceNotFound ──────────────────────────────────

#[tokio::test]
async fn open_unknown_input_device_returns_not_found() {
    let mock = MockAudioBackend::new();
    let result = mock
        .open_input("no-such-mic", AudioFormat::default())
        .await;
    assert!(
        matches!(result, Err(AudioError::DeviceNotFound(_))),
        "expected DeviceNotFound for unknown input device"
    );
}

#[tokio::test]
async fn open_unknown_output_device_returns_not_found() {
    let mock = MockAudioBackend::new();
    let result = mock
        .open_output("no-such-speaker", AudioFormat::default())
        .await;
    assert!(
        matches!(result, Err(AudioError::DeviceNotFound(_))),
        "expected DeviceNotFound for unknown output device"
    );
}

#[tokio::test]
async fn switch_unknown_input_device_returns_not_found() {
    let mock = MockAudioBackend::new();
    let result = mock.switch_input("ghost-device").await;
    assert!(
        matches!(result, Err(AudioError::DeviceNotFound(_))),
        "expected DeviceNotFound on switch to unknown device"
    );
    // No SwitchInput event should be emitted for a failed switch.
    let events = mock.drain_events();
    assert!(
        !events.iter().any(|e| matches!(e, MockEvent::SwitchInput(_))),
        "SwitchInput must not emit on failure"
    );
}

// ── K.1.6 — open_output → push → close event sequence ───────────────────────

#[tokio::test]
async fn output_stream_push_and_close_events() {
    let mock = MockAudioBackend::new();

    let output = mock
        .open_output("mock-speaker-a", AudioFormat::DISCORD_VOICE)
        .await
        .unwrap();
    mock.drain_events(); // discard OpenOutput

    // Discord 20 ms stereo frame = 1920 samples.
    let frame: Vec<i16> = vec![0i16; 1920];
    output.push(&frame).await.unwrap();
    output.push(&frame).await.unwrap();
    output.close().await.unwrap();

    let events = mock.drain_events();
    assert_eq!(
        events,
        vec![
            MockEvent::PushSamples(1920),
            MockEvent::PushSamples(1920),
            MockEvent::CloseOutput,
        ],
        "expected push×2 + close events"
    );

    // Total pushed should reflect both frames.
    assert_eq!(mock.total_pushed(), 3840, "3840 samples total (2 × 1920)");
}

// ── K.1.7 — list_input/output_devices returns all advertised devices ─────────

#[tokio::test]
async fn list_devices_returns_all_advertised() {
    let mock = MockAudioBackend::new();

    let inputs = mock.list_input_devices().await.unwrap();
    let outputs = mock.list_output_devices().await.unwrap();

    assert_eq!(inputs.len(), 2, "expected 2 input devices");
    assert!(
        inputs.iter().any(|d| d.id == "mock-mic-a" && d.is_default),
        "mock-mic-a must be default"
    );
    assert!(
        inputs.iter().any(|d| d.id == "mock-mic-b" && !d.is_default),
        "mock-mic-b must be non-default"
    );

    assert_eq!(outputs.len(), 2, "expected 2 output devices");
    assert!(
        outputs.iter().any(|d| d.id == "mock-speaker-a" && d.is_default),
        "mock-speaker-a must be default"
    );
}

// ── K.1.8 — open_input returns a stream (may be empty for the mock) ──────────

#[tokio::test]
async fn open_input_returns_stream() {
    let mock = MockAudioBackend::new();
    let mut stream = mock
        .open_input("mock-mic-a", AudioFormat::DISCORD_VOICE)
        .await
        .unwrap();

    // The mock returns an empty stream (silence), so next() → None.
    let item = stream.next().await;
    assert!(item.is_none(), "mock input stream should be empty (silence)");
}

// ── K.1.9 — custom device list injection ────────────────────────────────────

#[tokio::test]
async fn custom_device_list_is_advertised() {
    let custom_inputs = vec![
        AudioDevice::new_default("usb-mic", "USB Microphone", AudioDeviceKind::Input),
        AudioDevice::new("bt-headset", "Bluetooth Headset Mic", AudioDeviceKind::Input),
    ];
    let custom_outputs =
        vec![AudioDevice::new_default("usb-dac", "USB DAC", AudioDeviceKind::Output)];

    let mock = MockAudioBackend::with_devices(custom_inputs, custom_outputs);

    let inputs = mock.list_input_devices().await.unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].id, "usb-mic");
    assert_eq!(inputs[1].id, "bt-headset");

    let outputs = mock.list_output_devices().await.unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].id, "usb-dac");

    // switch_input to one of the custom devices should succeed.
    mock.switch_input("bt-headset").await.unwrap();
    let cur = mock.current_input_device();
    mock.drain_events();
    assert_eq!(cur.map(|d| d.id), Some("bt-headset".into()));
}

// ── K.1.10 — empty-string device_id selects the default ──────────────────────

#[tokio::test]
async fn empty_device_id_selects_default_input() {
    let mock = MockAudioBackend::new();
    let _stream = mock
        .open_input("", AudioFormat::STOAT_VOICE)
        .await
        .unwrap();

    // The default input is mock-mic-a.
    let cur = mock.current_input_device();
    mock.drain_events();
    assert_eq!(
        cur.map(|d| d.id),
        Some("mock-mic-a".into()),
        "empty device_id should select mock-mic-a (default)"
    );
}

#[tokio::test]
async fn empty_device_id_selects_default_output() {
    let mock = MockAudioBackend::new();
    let _out = mock
        .open_output("", AudioFormat::STOAT_VOICE)
        .await
        .unwrap();

    let cur = mock.current_output_device();
    mock.drain_events();
    assert_eq!(
        cur.map(|d| d.id),
        Some("mock-speaker-a".into()),
        "empty device_id should select mock-speaker-a (default)"
    );
}
