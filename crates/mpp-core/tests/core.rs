use mpp_core::{
    Capabilities, Device, DeviceKind, DeviceState, Error, InputEvent, Inventory, KeyPhase,
    LeaseManager, MAX_INPUT_TEXT_BYTES, MAX_OWNER_BYTES, Platform, SessionState, TouchPhase,
};

fn device(id: &str) -> Device {
    Device {
        id: id.to_owned(),
        platform: Platform::Android,
        kind: DeviceKind::Emulator,
        state: DeviceState::Online,
        name: "Test emulator".to_owned(),
        serial: Some(id.to_owned()),
        transport_id: Some("1".into()),
        avd: Some("test_avd".to_owned()),
        capabilities: Capabilities {
            lifecycle: true,
            ..Capabilities::default()
        },
    }
}

#[test]
fn session_transitions_only_after_ready_and_disconnect_releases_device() {
    let mut leases = LeaseManager::new();
    let connecting = leases
        .reserve("conversation-1", device("emulator-5554"))
        .unwrap();
    assert_eq!(connecting.state, SessionState::Connecting);
    assert_eq!(
        leases
            .status(&connecting.owner, &connecting.id, connecting.generation)
            .unwrap(),
        connecting
    );
    let ready = leases
        .ready(&connecting.owner, &connecting.id, connecting.generation)
        .unwrap();
    assert_eq!(ready.state, SessionState::TransportReady);
    assert_eq!(
        leases
            .ready(&ready.owner, &ready.id, ready.generation)
            .unwrap(),
        ready
    );
    let disconnected = leases
        .disconnect(&ready.owner, &ready.id, ready.generation)
        .unwrap();
    assert_eq!(disconnected.state, SessionState::Disconnected);
    assert!(matches!(
        leases.status(&ready.owner, &ready.id, ready.generation),
        Err(Error::StaleSession)
    ));
    assert!(matches!(
        leases.ready(&ready.owner, &ready.id, ready.generation),
        Err(Error::StaleSession)
    ));
    assert!(matches!(
        leases.disconnect(&ready.owner, &ready.id, ready.generation),
        Err(Error::StaleSession)
    ));
    assert!(
        leases
            .reserve("conversation-2", device("emulator-5554"))
            .is_ok()
    );
}

#[test]
fn connecting_session_excludes_other_owners_and_other_devices_for_same_owner() {
    let mut leases = LeaseManager::new();
    let first = leases.reserve("one", device("emulator-5554")).unwrap();
    for (owner, id) in [
        ("two", "emulator-5554"),
        ("one", "emulator-5556"),
        ("one", "emulator-5554"),
    ] {
        assert!(matches!(
            leases.reserve(owner, device(id)),
            Err(Error::Busy { device, owner }) if device == "emulator-5554" && owner == "one"
        ));
    }
    assert_eq!(
        leases.status("one", &first.id, first.generation).unwrap(),
        first
    );
    assert!(leases.reserve("two", device("emulator-5556")).is_ok());
}

#[test]
fn stale_generations_after_reconnect_cannot_modify_current_lease() {
    let mut leases = LeaseManager::new();
    let old = leases.reserve("one", device("emulator-5554")).unwrap();
    leases.disconnect("one", &old.id, old.generation).unwrap();
    let current = leases.reserve("one", device("emulator-5554")).unwrap();
    assert_ne!(old.id, current.id);
    assert!(current.generation > old.generation);
    for (id, generation) in [
        (old.id.as_str(), old.generation),
        (current.id.as_str(), old.generation),
        (old.id.as_str(), current.generation),
    ] {
        assert!(matches!(
            leases.ready("one", id, generation),
            Err(Error::StaleSession)
        ));
        assert!(matches!(
            leases.disconnect("one", id, generation),
            Err(Error::StaleSession)
        ));
    }
    assert_eq!(
        leases
            .status("one", &current.id, current.generation)
            .unwrap(),
        current
    );
}

#[test]
fn wrong_owner_and_unknown_id_do_not_touch_another_lease() {
    let mut leases = LeaseManager::new();
    let one = leases.reserve("one", device("emulator-5554")).unwrap();
    let two = leases.reserve("two", device("emulator-5556")).unwrap();
    for (owner, id) in [("two", one.id.as_str()), ("one", "missing")] {
        assert!(matches!(
            leases.status(owner, id, one.generation),
            Err(Error::StaleSession)
        ));
        assert!(matches!(
            leases.ready(owner, id, one.generation),
            Err(Error::StaleSession)
        ));
        assert!(matches!(
            leases.disconnect(owner, id, one.generation),
            Err(Error::StaleSession)
        ));
    }
    assert_eq!(leases.status("one", &one.id, one.generation).unwrap(), one);
    assert_eq!(leases.status("two", &two.id, two.generation).unwrap(), two);
}

#[test]
fn rollback_and_bulk_disconnect_do_not_reset_generation() {
    let mut leases = LeaseManager::new();
    let failed_probe = leases.reserve("one", device("emulator-5554")).unwrap();
    leases
        .disconnect("one", &failed_probe.id, failed_probe.generation)
        .unwrap();
    let retry = leases.reserve("one", device("emulator-5554")).unwrap();
    let other = leases.reserve("two", device("emulator-5556")).unwrap();
    assert_eq!(leases.disconnect_all(), 2);
    assert_eq!(leases.disconnect_all(), 0);
    assert!(matches!(
        leases.status("one", &retry.id, retry.generation),
        Err(Error::StaleSession)
    ));
    let next = leases.reserve("one", device("emulator-5554")).unwrap();
    assert!(next.generation > other.generation);
}

#[test]
fn owner_identifiers_are_bounded_and_rejected_before_reservation() {
    let mut leases = LeaseManager::new();
    for owner in [
        "".to_owned(),
        " \t".to_owned(),
        "one\n".to_owned(),
        "a".repeat(MAX_OWNER_BYTES + 1),
    ] {
        assert!(matches!(
            leases.reserve(&owner, device("emulator-5554")),
            Err(Error::InvalidArgument { .. })
        ));
    }
    let valid = leases
        .reserve(&"a".repeat(MAX_OWNER_BYTES), device("emulator-5554"))
        .unwrap();
    assert_eq!(valid.generation, 1);
    for id in ["", "  ", "bad\nid"] {
        assert!(matches!(
            leases.reserve("two", device(id)),
            Err(Error::InvalidArgument { .. })
        ));
    }
}

#[test]
fn devices_on_different_platforms_have_independent_identity() {
    let mut leases = LeaseManager::new();
    leases.reserve("android", device("same-id")).unwrap();
    let mut ios = device("same-id");
    ios.platform = Platform::Ios;
    ios.kind = DeviceKind::Simulator;
    assert!(leases.reserve("ios", ios).is_ok());
}

fn touch(x: f64, y: f64, width: u32, height: u32) -> InputEvent {
    InputEvent::Touch {
        phase: TouchPhase::Down,
        x,
        y,
        width,
        height,
    }
}

#[test]
fn touch_validation_rejects_nonfinite_out_of_range_and_zero_dimensions() {
    for (x, y) in [
        (f64::NAN, 0.5),
        (0.5, f64::NAN),
        (f64::INFINITY, 0.5),
        (0.5, f64::NEG_INFINITY),
        (-0.1, 0.5),
        (0.5, 1.1),
    ] {
        assert!(touch(x, y, 1080, 1920).validate().is_err());
    }
    assert!(touch(0.5, 0.5, 0, 1920).validate().is_err());
    assert!(touch(0.5, 0.5, 1080, 0).validate().is_err());
    assert!(touch(0.5, 0.5, 16_385, 1920).validate().is_err());
    assert!(touch(0.5, 0.5, 1080, u32::MAX).validate().is_err());
    assert!(touch(0.0, 1.0, 1080, 1920).validate().is_ok());
    assert!(touch(1.0, 0.0, 1080, 1920).validate().is_ok());
}

#[test]
fn text_validation_bounds_utf8_bytes_without_exposing_content_in_errors() {
    let text = |text: String| InputEvent::Text { text };
    assert!(text(String::new()).validate().is_err());
    assert!(text("a".repeat(MAX_INPUT_TEXT_BYTES)).validate().is_ok());
    assert!(
        text("a".repeat(MAX_INPUT_TEXT_BYTES + 1))
            .validate()
            .is_err()
    );
    assert!(
        text("界".repeat(MAX_INPUT_TEXT_BYTES / 3 + 1))
            .validate()
            .is_err()
    );
    assert!(text("Hello, 世界\n".to_owned()).validate().is_ok());
    let error = text("secret\0password".to_owned()).validate().unwrap_err();
    assert!(!error.to_string().contains("secret"));
    assert!(
        InputEvent::Key {
            code: 4,
            phase: KeyPhase::Down
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn wire_types_use_snake_case_and_reject_unknown_input_fields() {
    let value = serde_json::to_value(touch(0.25, 0.75, 1080, 1920)).unwrap();
    assert_eq!(value["kind"], "touch");
    assert_eq!(value["phase"], "down");
    assert_eq!(
        serde_json::from_value::<InputEvent>(value).unwrap(),
        touch(0.25, 0.75, 1080, 1920)
    );
    assert!(
        serde_json::from_str::<InputEvent>(r#"{"kind":"text","text":"hello","command":"ignored"}"#)
            .is_err()
    );
    let inventory = Inventory {
        devices: vec![device("emulator-5554")],
        warnings: vec![],
    };
    let value = serde_json::to_value(&inventory).unwrap();
    assert_eq!(value["devices"][0]["platform"], "android");
    assert_eq!(value["devices"][0]["state"], "online");
    assert_eq!(
        serde_json::from_value::<Inventory>(value).unwrap(),
        inventory
    );
}

#[test]
fn every_error_has_a_stable_code_and_actionable_hint() {
    let cases = [
        (
            Error::InvalidArgument {
                message: "bad".to_owned(),
            },
            "INVALID_ARGUMENT",
        ),
        (
            Error::NotFound {
                what: "device".to_owned(),
            },
            "NOT_FOUND",
        ),
        (
            Error::Busy {
                device: "device".to_owned(),
                owner: "owner".to_owned(),
            },
            "BUSY",
        ),
        (Error::StaleSession, "STALE_SESSION"),
        (
            Error::PermissionDenied {
                message: "denied".to_owned(),
            },
            "PERMISSION_DENIED",
        ),
        (
            Error::Unsupported {
                feature: "video".to_owned(),
            },
            "UNSUPPORTED",
        ),
        (
            Error::ToolNotFound {
                tool: "adb".to_owned(),
            },
            "TOOL_NOT_FOUND",
        ),
        (
            Error::CommandFailed {
                tool: "adb".to_owned(),
                message: "offline".to_owned(),
            },
            "COMMAND_FAILED",
        ),
        (
            Error::Timeout {
                operation: "connect".to_owned(),
            },
            "TIMEOUT",
        ),
        (Error::from(std::io::Error::other("closed")), "IO_ERROR"),
    ];
    for (error, code) in cases {
        assert_eq!(error.code(), code);
        assert!(!error.hint().is_empty());
    }
}
