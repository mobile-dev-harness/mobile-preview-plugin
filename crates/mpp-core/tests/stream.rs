use mpp_core::stream::{
    AndroidFramework, Channel, ControlCommand, ControlReply, ControlRequest, DEVICE_PROTOCOL,
    DeviceConfig, DeviceHello, Geometry, InputState, MAX_BATCH_EVENTS, MAX_CONTROL_BYTES,
    MEDIA_MAX_PAYLOAD, android_framework,
};
use mpp_core::{Error, InputEvent, KeyPhase, TouchPhase};
use serde_json::{Value, json};

#[test]
fn framework_selection_follows_android_release_api_changes() {
    for (api, expected) in [
        (29, AndroidFramework::SurfaceControl),
        (30, AndroidFramework::SurfaceControl),
        (31, AndroidFramework::SurfaceControl),
        (32, AndroidFramework::SurfaceControl),
        (33, AndroidFramework::SurfaceControl),
        (34, AndroidFramework::DisplayManager),
        (35, AndroidFramework::DisplayManager),
        (36, AndroidFramework::DisplayManager),
        (37, AndroidFramework::DisplayManager),
    ] {
        assert_eq!(android_framework(api), Some(expected), "API {api}");
    }
    for api in [0, 28, 38, u32::MAX] {
        assert_eq!(android_framework(api), None, "API {api}");
    }
}

fn config() -> DeviceConfig {
    DeviceConfig {
        protocol: DEVICE_PROTOCOL.to_owned(),
        socket_name: format!("mpp_{}", "0123456789abcdef".repeat(2)),
        token: "abcdef0123456789".repeat(4),
        generation: 1,
        epoch: 2,
        max_size: 1024,
        bit_rate: 4_000_000,
        max_fps: 30,
    }
}

fn geometry() -> Geometry {
    Geometry {
        width: 576,
        height: 1024,
        display_width: 1080,
        display_height: 1920,
        rotation: 0,
    }
}

fn hello() -> DeviceHello {
    DeviceHello {
        protocol: DEVICE_PROTOCOL.to_owned(),
        token: config().token,
        channel: Channel::Video,
        generation: 1,
        epoch: 2,
        geometry: geometry(),
    }
}

fn state() -> InputState {
    InputState::new(2, 576, 1024).unwrap()
}

fn request(seq: u64, command: ControlCommand) -> ControlRequest {
    ControlRequest {
        seq,
        epoch: 2,
        command,
    }
}

fn touch(phase: TouchPhase, x: f64, y: f64) -> InputEvent {
    InputEvent::Touch {
        phase,
        x,
        y,
        width: 576,
        height: 1024,
    }
}

fn key(code: u32, phase: KeyPhase) -> InputEvent {
    InputEvent::Key { code, phase }
}

fn input(seq: u64, event: InputEvent) -> ControlRequest {
    request(seq, ControlCommand::Input { event })
}

#[test]
fn config_bounds_reject_unsafe_remote_names_and_invalid_encoder_settings() {
    config().validate().unwrap();
    let fields = [
        ("protocol", json!("mpp-device/2")),
        ("socket_name", json!("mpp_$(touch remote)")),
        ("socket_name", json!(format!("mpp_{}", "A".repeat(32)))),
        ("socket_name", json!(format!("other_{}", "a".repeat(32)))),
        ("socket_name", json!(format!("mpp_{}", "a".repeat(31)))),
        ("token", json!("a".repeat(63))),
        ("token", json!("F".repeat(64))),
        ("token", json!("z".repeat(64))),
        ("token", json!("\n".repeat(64))),
        ("generation", json!(0)),
        ("epoch", json!(0)),
        ("max_size", json!(254)),
        ("max_size", json!(257)),
        ("max_size", json!(2050)),
        ("bit_rate", json!(99_999)),
        ("bit_rate", json!(20_000_001)),
        ("max_fps", json!(0)),
        ("max_fps", json!(61)),
    ];
    for (field, value) in fields {
        let mut encoded = serde_json::to_value(config()).unwrap();
        encoded[field] = value;
        let candidate: DeviceConfig = serde_json::from_value(encoded).unwrap();
        assert!(candidate.validate().is_err(), "accepted invalid {field}");
    }
    for (max_size, bit_rate, max_fps) in [(256, 100_000, 1), (2048, 20_000_000, 60)] {
        DeviceConfig {
            generation: u64::MAX,
            epoch: u64::MAX,
            max_size,
            bit_rate,
            max_fps,
            ..config()
        }
        .validate()
        .unwrap();
    }
}

#[test]
fn config_and_hello_debug_and_validation_errors_do_not_expose_tokens() {
    let configuration = config();
    let introduction = hello();
    assert!(!format!("{configuration:?}").contains(&configuration.token));
    assert!(!format!("{introduction:?}").contains(&introduction.token));
    let configuration = DeviceConfig {
        token: "never-print-this-token".to_owned(),
        ..configuration
    };
    let introduction = DeviceHello {
        token: configuration.token.clone(),
        ..introduction
    };
    for error in [
        configuration.validate().unwrap_err(),
        introduction.validate().unwrap_err(),
    ] {
        assert!(!error.to_string().contains(&configuration.token));
    }
}

#[test]
fn geometry_checks_encoded_and_display_bounds_and_rotation() {
    geometry().validate().unwrap();
    for (field, value) in [
        ("width", 0),
        ("height", 4097),
        ("display_width", 0),
        ("display_height", 16385),
        ("rotation", 4),
    ] {
        let mut encoded = serde_json::to_value(geometry()).unwrap();
        encoded[field] = json!(value);
        assert!(
            serde_json::from_value::<Geometry>(encoded)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    Geometry {
        width: 1,
        height: 4096,
        display_width: 1,
        display_height: 16384,
        rotation: 3,
    }
    .validate()
    .unwrap();
    assert!(InputState::new(0, 576, 1024).is_err());
    assert!(InputState::new(1, 0, 1024).is_err());
    assert!(InputState::new(1, 576, 4097).is_err());
}

#[test]
fn hello_validates_both_identity_and_geometry() {
    hello().validate().unwrap();
    for (field, value) in [
        ("protocol", json!("wrong")),
        ("token", json!("bad")),
        ("generation", json!(0)),
        ("epoch", json!(0)),
        (
            "geometry",
            json!({ "width":0,"height":1024,"display_width":1080,"display_height":1920,"rotation":0 }),
        ),
    ] {
        let mut encoded = serde_json::to_value(hello()).unwrap();
        encoded[field] = value;
        assert!(
            serde_json::from_value::<DeviceHello>(encoded)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn strict_wire_types_reject_unknown_fields_and_keep_declared_command_names() {
    let mut encoded = serde_json::to_value(config()).unwrap();
    encoded["shell"] = json!("unexpected");
    assert!(serde_json::from_value::<DeviceConfig>(encoded).is_err());
    let mut encoded = serde_json::to_value(hello()).unwrap();
    encoded["geometry"]["extra"] = json!(true);
    assert!(serde_json::from_value::<DeviceHello>(encoded).is_err());
    let mut encoded = serde_json::to_value(hello()).unwrap();
    encoded["extra"] = json!(true);
    assert!(serde_json::from_value::<DeviceHello>(encoded).is_err());
    assert!(serde_json::from_value::<Channel>(json!("audio")).is_err());
    for command in [
        json!({"kind":"heartbeat","extra":true}),
        json!({"kind":"reset","extra":true}),
        json!({"kind":"stop","extra":true}),
        json!({"kind":"key_frame","extra":true}),
        json!({"kind":"input","event":{"kind":"key","code":4,"phase":"down"},"extra":true}),
        json!({"kind":"input","event":{"kind":"key","code":4,"phase":"down","extra":true}}),
    ] {
        assert!(serde_json::from_value::<ControlCommand>(command).is_err());
    }
    let encoded = serde_json::to_value(request(4, ControlCommand::KeyFrame)).unwrap();
    assert_eq!(encoded["command"]["kind"], "key_frame");
    let mut extra = encoded.clone();
    extra["extra"] = json!(true);
    assert!(serde_json::from_value::<ControlRequest>(extra).is_err());
    assert_eq!(
        serde_json::from_value::<ControlRequest>(encoded).unwrap(),
        request(4, ControlCommand::KeyFrame)
    );
    let receipt = ControlReply {
        seq: 4,
        ok: true,
        code: None,
        message: None,
    };
    let mut encoded = serde_json::to_value(&receipt).unwrap();
    assert_eq!(encoded["code"], Value::Null);
    assert_eq!(
        serde_json::from_value::<ControlReply>(encoded.clone()).unwrap(),
        receipt
    );
    encoded["extra"] = json!(true);
    assert!(serde_json::from_value::<ControlReply>(encoded).is_err());
    assert_eq!(MAX_CONTROL_BYTES, 16384);
    assert_eq!(MAX_BATCH_EVENTS, 64);
    assert_eq!(MEDIA_MAX_PAYLOAD, 8 * 1024 * 1024);
}

#[test]
fn touch_requires_ordered_edges_and_tracks_last_position_for_cleanup() {
    let mut state = state();
    assert!(
        state
            .accept(&input(1, touch(TouchPhase::Move, 0.2, 0.3)))
            .is_err()
    );
    assert!(
        state
            .accept(&input(2, touch(TouchPhase::Up, 0.2, 0.3)))
            .is_err()
    );
    let down = touch(TouchPhase::Down, 0.2, 0.3);
    assert_eq!(state.accept(&input(3, down.clone())).unwrap(), vec![down]);
    assert!(
        state
            .accept(&input(4, touch(TouchPhase::Down, 0.9, 0.9)))
            .is_err()
    );
    let moved = touch(TouchPhase::Move, 0.4, 0.5);
    assert_eq!(state.accept(&input(5, moved.clone())).unwrap(), vec![moved]);
    assert_eq!(
        state.drain_releases(),
        vec![touch(TouchPhase::Cancel, 0.4, 0.5)]
    );
    assert!(state.drain_releases().is_empty());
    assert!(
        state
            .accept(&input(6, touch(TouchPhase::Up, 0.4, 0.5)))
            .is_err()
    );
}

#[test]
fn up_and_cancel_end_gestures_and_idle_cancel_is_idempotent() {
    let mut state = state();
    assert!(
        state
            .accept(&input(1, touch(TouchPhase::Cancel, 0.1, 0.1)))
            .unwrap()
            .is_empty()
    );
    state
        .accept(&input(2, touch(TouchPhase::Down, 0.2, 0.2)))
        .unwrap();
    let up = touch(TouchPhase::Up, 0.3, 0.3);
    assert_eq!(state.accept(&input(3, up.clone())).unwrap(), vec![up]);
    assert!(state.drain_releases().is_empty());
    state
        .accept(&input(4, touch(TouchPhase::Down, 0.4, 0.4)))
        .unwrap();
    let cancel = touch(TouchPhase::Cancel, 0.5, 0.5);
    assert_eq!(
        state.accept(&input(5, cancel.clone())).unwrap(),
        vec![cancel]
    );
    assert!(state.drain_releases().is_empty());
}

#[test]
fn rejected_sequences_or_epochs_cannot_modify_an_active_gesture() {
    let mut state = state();
    state
        .accept(&input(10, touch(TouchPhase::Down, 0.1, 0.2)))
        .unwrap();
    for seq in [0, 1, 10] {
        assert!(
            state
                .accept(&input(seq, touch(TouchPhase::Up, 0.9, 0.9)))
                .is_err()
        );
    }
    for epoch in [0, 1, 3] {
        let request = ControlRequest {
            seq: u64::MAX,
            epoch,
            command: ControlCommand::Stop,
        };
        assert!(matches!(state.accept(&request), Err(Error::StaleSession)));
    }
    state
        .accept(&request(11, ControlCommand::Heartbeat))
        .unwrap();
    assert_eq!(
        state.drain_releases(),
        vec![touch(TouchPhase::Cancel, 0.1, 0.2)]
    );
}

#[test]
fn a_rejected_command_consumes_its_sequence_without_partial_input_changes() {
    let mut state = state();
    assert!(
        state
            .accept(&input(1, touch(TouchPhase::Move, 0.1, 0.1)))
            .is_err()
    );
    assert!(
        state
            .accept(&input(1, touch(TouchPhase::Down, 0.1, 0.1)))
            .is_err()
    );
    state
        .accept(&input(2, touch(TouchPhase::Down, 0.2, 0.3)))
        .unwrap();
    let wrong_frame = InputEvent::Touch {
        phase: TouchPhase::Up,
        x: 0.8,
        y: 0.8,
        width: 1024,
        height: 576,
    };
    assert!(state.accept(&input(3, wrong_frame)).is_err());
    assert!(
        state
            .accept(&input(3, touch(TouchPhase::Up, 0.8, 0.8)))
            .is_err()
    );
    assert!(
        state
            .accept(&input(4, touch(TouchPhase::Move, f64::NAN, 0.5)))
            .is_err()
    );
    assert_eq!(
        state.drain_releases(),
        vec![touch(TouchPhase::Cancel, 0.2, 0.3)]
    );
}

#[test]
fn key_edges_are_tracked_once_and_android_integer_bounds_are_enforced() {
    let mut state = state();
    assert!(state.accept(&input(1, key(4, KeyPhase::Up))).is_err());
    state.accept(&input(2, key(4, KeyPhase::Down))).unwrap();
    assert!(state.accept(&input(3, key(4, KeyPhase::Down))).is_err());
    state.accept(&input(4, key(3, KeyPhase::Down))).unwrap();
    assert!(
        state
            .accept(&input(5, key(i32::MAX as u32 + 1, KeyPhase::Down)))
            .is_err()
    );
    state
        .accept(&input(6, key(i32::MAX as u32, KeyPhase::Down)))
        .unwrap();
    assert_eq!(
        state.accept(&input(7, key(4, KeyPhase::Up))).unwrap(),
        vec![key(4, KeyPhase::Up)]
    );
    assert_eq!(
        state.drain_releases(),
        vec![key(3, KeyPhase::Up), key(i32::MAX as u32, KeyPhase::Up)]
    );
}

#[test]
fn reset_and_stop_release_touch_and_keys_but_only_stop_ends_the_stream() {
    let mut state = state();
    state.accept(&input(1, key(4, KeyPhase::Down))).unwrap();
    state
        .accept(&input(2, touch(TouchPhase::Down, 0.1, 0.2)))
        .unwrap();
    assert_eq!(
        state.accept(&request(3, ControlCommand::Reset)).unwrap(),
        vec![touch(TouchPhase::Cancel, 0.1, 0.2), key(4, KeyPhase::Up)]
    );
    assert!(
        state
            .accept(&request(4, ControlCommand::Reset))
            .unwrap()
            .is_empty()
    );
    state.accept(&input(5, key(3, KeyPhase::Down))).unwrap();
    assert_eq!(
        state.accept(&request(6, ControlCommand::Stop)).unwrap(),
        vec![key(3, KeyPhase::Up)]
    );
    assert!(state.drain_releases().is_empty());
    assert!(matches!(
        state.accept(&input(7, key(4, KeyPhase::Down))),
        Err(Error::StaleSession)
    ));
}

#[test]
fn heartbeat_keyframe_and_unsupported_text_never_inject_or_release_input() {
    let mut state = state();
    state
        .accept(&input(1, touch(TouchPhase::Down, 0.1, 0.2)))
        .unwrap();
    assert!(
        state
            .accept(&request(2, ControlCommand::Heartbeat))
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .accept(&request(3, ControlCommand::KeyFrame))
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        state.accept(&input(
            4,
            InputEvent::Text {
                text: "hello".to_owned()
            }
        )),
        Err(Error::Unsupported { .. })
    ));
    assert_eq!(
        state.drain_releases(),
        vec![touch(TouchPhase::Cancel, 0.1, 0.2)]
    );
}
