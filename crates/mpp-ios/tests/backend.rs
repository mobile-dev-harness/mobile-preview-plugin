#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use mpp_core::{
    DeviceKind, DeviceState, Error, InputEvent, KeyPhase, Platform, TouchPhase,
    media::{Packet, PacketBody, encode},
    stream::{ControlCommand, ControlReply, ControlRequest, Geometry, MEDIA_MAX_PAYLOAD},
};
use mpp_ios::{Ios, StreamOptions, parse_devices};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    time::{sleep, timeout},
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
const UDID: &str = "01234567-89AB-CDEF-0123-456789ABCDEF";
const OTHER: &str = "11234567-89AB-CDEF-0123-456789ABCDEF";
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Fixture {
    directory: PathBuf,
    xcrun: PathBuf,
    capture: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "mpp ios test {} {}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let xcrun = script(
            &directory,
            "xcrun",
            &format!(
                r#"
printf '%s\n' "$*" >> xcrun.log
case "$*" in
  'simctl list devices available --json') cat inventory ;;
  'simctl boot {UDID}') cp booted inventory ;;
  'simctl bootstatus {UDID} -b') : ;;
  *) exit 9 ;;
esac"#
            ),
        );
        let capture = script(
            &directory,
            "capture",
            r#"
printf '%s\n' "$*" >> capture.log
case "$1" in
  --ios-probe)
    if [ -f input ]; then
      printf '{"boot_id":"%s","input":true}\n' "$(cat boot-id)"
    else
      printf '{"boot_id":"%s"}\n' "$(cat boot-id)"
    fi ;;
  --ios-capture)
    echo $$ > helper-pid
    if [ -f replace-boot ]; then echo launchd:456:later > boot-id; fi
    if [ -f block-start ]; then
      cat > /dev/null
      touch stdin-closed
      exit 0
    fi
    if [ -f helper-error ]; then echo 'native helper failed' >&2; exit 7; fi
    cat geometry
    if [ -f backpressure ]; then
      cat backlog &
      media=$!
      trap 'kill "$media" 2>/dev/null || :; wait "$media" 2>/dev/null || :' EXIT
    else
      cat packet
    fi
    while IFS= read -r command; do
      printf '%s\n' "$command" >> commands
      seq=${command#'{"seq":'}
      seq=${seq%%,*}
      if [ -f no-reply ]; then continue; fi
      if [ -f delay-reply ]; then
        touch awaiting-reply
        while [ ! -f release-reply ]; do sleep 0.01; done
      fi
      if [ -f close-video-before-reply ]; then exec 1>&-; sleep 0.05; fi
      if [ -f native-reply ]; then
        cat native-reply >&0
      else
        printf '{"seq":%s,"ok":true,"code":null,"message":null}\n' "$seq" >&0
      fi
      case "$command" in *'"kind":"stop"'*) touch stopped; exit 0 ;; esac
    done
    touch stdin-closed ;;
  *) exit 9 ;;
esac"#,
        );
        let fixture = Self {
            directory,
            xcrun,
            capture,
        };
        fixture.write("inventory", &listing("Booted"));
        fixture.write("booted", &listing("Booted"));
        fixture.write("boot-id", "launchd:123:earlier");
        fixture.write("geometry", "{\"width\":640,\"height\":1136,\"display_width\":640,\"display_height\":1136,\"rotation\":0}\n");
        fs::write(fixture.directory.join("packet"), packet()).unwrap();
        fixture
    }
    fn ios(&self) -> Ios {
        Ios::with_tools(self.xcrun.clone(), self.capture.clone())
    }
    fn write(&self, name: &str, contents: &str) {
        fs::write(self.directory.join(name), contents).unwrap();
    }
    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.directory.join(name)).unwrap_or_default()
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn script(directory: &Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\nset -eu\ncd '{}'\n{body}\n", directory.display()),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn listing(state: &str) -> String {
    serde_json::json!({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-26-4": [
        {"udid":UDID,"name":"iPhone 17","state":state,"isAvailable":true}
    ]}})
    .to_string()
}
fn options() -> StreamOptions {
    StreamOptions {
        token: TOKEN.into(),
        stream_id: "abcdef0123456789abcdef0123456789".into(),
        generation: 7,
        epoch: 3,
        max_size: 1280,
        bit_rate: 4_000_000,
        max_fps: 30,
    }
}
fn packet() -> Vec<u8> {
    encode(
        &Packet {
            generation: 7,
            pts_us: 0,
            body: PacketBody::Configuration {
                width: 640,
                height: 1136,
                annex_b: vec![0, 0, 0, 1, 103, 100],
            },
        },
        MEDIA_MAX_PAYLOAD,
    )
    .unwrap()
}
async fn wait_file(path: &Path) {
    timeout(Duration::from_secs(5), async {
        while !path.exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn send_request(control: &mut DuplexStream, seq: u64, epoch: u64, command: ControlCommand) {
    let mut bytes = serde_json::to_vec(&ControlRequest {
        seq,
        epoch,
        command,
    })
    .unwrap();
    bytes.push(b'\n');
    control.write_all(&bytes).await.unwrap();
}

async fn read_reply(control: &mut DuplexStream) -> ControlReply {
    let mut line = Vec::new();
    timeout(Duration::from_secs(3), async {
        loop {
            let byte = control.read_u8().await.unwrap();
            if byte == b'\n' {
                break;
            }
            line.push(byte);
        }
    })
    .await
    .unwrap();
    serde_json::from_slice(&line).unwrap()
}

async fn request(
    control: &mut DuplexStream,
    seq: u64,
    epoch: u64,
    command: ControlCommand,
) -> ControlReply {
    send_request(control, seq, epoch, command).await;
    read_reply(control).await
}

#[test]
fn parser_filters_runtime_availability_and_rejects_missing_identity() {
    let inventory = parse_devices(
        &serde_json::json!({"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-26-4": [
                {"udid":UDID,"name":"iPhone 17","state":"Booted","isAvailable":true},
                {"udid":OTHER,"name":"Unavailable","state":"Shutdown","isAvailable":false},
                {"udid":OTHER,"name":"Missing availability","state":"Shutdown"},
                {"udid":"booted","name":"Alias","state":"Booted","isAvailable":true},
                {"udid":UDID.to_lowercase(),"name":"Duplicate","state":"Booted","isAvailable":true}
            ],
            "com.apple.CoreSimulator.SimRuntime.tvOS-26-4": [
                {"udid":OTHER,"name":"Apple TV","state":"Booted","isAvailable":true}
            ]
        }})
        .to_string(),
    )
    .unwrap();
    assert_eq!(inventory.devices.len(), 1);
    assert_eq!(inventory.warnings.len(), 3);
    let device = &inventory.devices[0];
    assert_eq!(device.id, format!("ios:{UDID}"));
    assert_eq!(device.platform, Platform::Ios);
    assert_eq!(device.kind, DeviceKind::Simulator);
    assert_eq!(device.transport_id, None);
    assert_eq!(device.avd, None);
    assert!(!device.capabilities.input && !device.capabilities.screenshot);
    assert_eq!(device.capabilities.video, cfg!(target_os = "macos"));
    assert_eq!(device.capabilities.lifecycle, cfg!(target_os = "macos"));
    assert!(parse_devices("{}").is_err());
}

#[tokio::test]
async fn probe_requires_online_exact_uuid_and_boot_identity() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let first = ios.probe(UDID).await.unwrap();
    fixture.write("boot-id", "launchd:456:later");
    let second = ios.probe(UDID).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_ne!(first.transport_id, second.transport_id);
    for invalid in ["", "booted", "-x", "0123456789abcdef0123456789abcdef01"] {
        assert!(matches!(
            ios.probe(invalid).await,
            Err(Error::InvalidArgument { .. })
        ));
    }
    assert!(matches!(
        ios.probe(OTHER).await,
        Err(Error::NotFound { .. })
    ));
    for invalid in ["", UDID, "boot identity"] {
        fixture.write("boot-id", invalid);
        assert!(matches!(
            ios.probe(UDID).await,
            Err(Error::CommandFailed { .. })
        ));
    }
    fixture.write("inventory", &listing("Shutdown"));
    assert!(matches!(
        ios.probe(UDID).await,
        Err(Error::CommandFailed { .. })
    ));
    assert!(!fixture.read("xcrun.log").contains("simctl boot "));
}

#[tokio::test]
async fn boot_only_targets_explicit_known_simulator_and_never_shuts_it_down() {
    let fixture = Fixture::new();
    fixture.write("inventory", &listing("Shutdown"));
    let ios = fixture.ios();
    assert!(matches!(ios.boot(OTHER).await, Err(Error::NotFound { .. })));
    assert_eq!(ios.boot(UDID).await.unwrap().state, DeviceState::Online);
    assert_eq!(ios.boot(UDID).await.unwrap().state, DeviceState::Online);
    let log = fixture.read("xcrun.log");
    assert_eq!(log.matches(&format!("simctl boot {UDID}\n")).count(), 1);
    assert!(log.contains(&format!("simctl bootstatus {UDID} -b\n")));
    assert!(!log.contains("shutdown"));
}

#[tokio::test]
async fn streaming_preserves_mpp1_packets_and_read_only_control_contract() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    assert_eq!(running.geometry.width, 640);
    let (mut video, mut control) = running.take_streams().unwrap();
    assert!(running.take_streams().is_err());
    let mut bytes = vec![0; packet().len()];
    video.read_exact(&mut bytes).await.unwrap();
    assert_eq!(bytes, packet());
    assert!(
        request(&mut control, 1, 3, ControlCommand::Heartbeat)
            .await
            .ok
    );
    let response = request(
        &mut control,
        2,
        3,
        ControlCommand::Input {
            event: InputEvent::Text {
                text: "read-only".into(),
            },
        },
    )
    .await;
    assert!(!response.ok);
    assert_eq!(response.code.as_deref(), Some("UNSUPPORTED"));
    assert!(response.message.is_some());
    assert!(request(&mut control, 3, 3, ControlCommand::Reset).await.ok);
    assert!(
        request(&mut control, 4, 3, ControlCommand::KeyFrame)
            .await
            .ok
    );
    let invalid = request(&mut control, 4, 3, ControlCommand::Heartbeat).await;
    assert_eq!(invalid.code.as_deref(), Some("INVALID_ARGUMENT"));
    let stale = request(&mut control, 5, 4, ControlCommand::Heartbeat).await;
    assert_eq!(stale.code.as_deref(), Some("STALE_SESSION"));
    assert!(request(&mut control, 5, 3, ControlCommand::Stop).await.ok);
    running.close().await.unwrap();
    wait_file(&fixture.path("stopped")).await;
    let commands: Vec<ControlRequest> = fixture
        .read("commands")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        commands
            .iter()
            .map(|request| request.seq)
            .collect::<Vec<_>>(),
        [1, 3, 4, 5]
    );
    assert_eq!(commands[0].command, ControlCommand::Heartbeat);
    assert_eq!(commands[1].command, ControlCommand::Reset);
    assert_eq!(commands[2].command, ControlCommand::KeyFrame);
    assert_eq!(commands[3].command, ControlCommand::Stop);
    assert!(fixture.read("capture.log").contains("--control-fd 0"));
    assert!(!fixture.read("capture.log").contains("--enable-input"));
    assert!(!fixture.read("capture.log").contains(TOKEN));
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn disconnect_closes_helper_stdin_and_preserves_simulator() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (video, control) = running.take_streams().unwrap();
    drop(control);
    wait_file(&fixture.path("stdin-closed")).await;
    running.close().await.unwrap();
    drop(video);
    assert!(!fixture.read("xcrun.log").contains("shutdown"));
    assert_eq!(ios.probe(UDID).await.unwrap().state, DeviceState::Online);
}

#[tokio::test]
async fn dropping_running_owner_closes_taken_channels_and_helper() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (_video, mut control) = running.take_streams().unwrap();
    drop(running);
    wait_file(&fixture.path("stdin-closed")).await;
    assert_eq!(control.read(&mut [0]).await.unwrap(), 0);
}

#[tokio::test]
async fn invalid_geometry_and_changed_boot_fail_before_returning_channels() {
    for fault in ["geometry", "replace-boot", "helper-error"] {
        let fixture = Fixture::new();
        let ios = fixture.ios();
        let device = ios.probe(UDID).await.unwrap();
        if fault == "geometry" {
            let geometry = Geometry {
                width: 1400,
                height: 1600,
                display_width: 1400,
                display_height: 1600,
                rotation: 0,
            };
            fixture.write(
                "geometry",
                &(serde_json::to_string(&geometry).unwrap() + "\n"),
            );
        } else {
            fixture.write(fault, "");
        }
        let error = ios.start_stream(&device, options()).await.err().unwrap();
        if fault == "helper-error" {
            assert!(error.to_string().contains("native helper failed"));
        }
        ios.shutdown_stream_starts().await.unwrap();
        if fault != "helper-error" {
            wait_file(&fixture.path("stdin-closed")).await;
        }
    }
}

#[tokio::test]
async fn cancellation_and_shutdown_drain_pending_startup_helpers() {
    let fixture = Fixture::new();
    fixture.write("block-start", "");
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let start_ios = ios.clone();
    let start = tokio::spawn(async move { start_ios.start_stream(&device, options()).await });
    wait_file(&fixture.path("helper-pid")).await;
    start.abort();
    let _ = start.await;
    ios.shutdown_stream_starts().await.unwrap();
    wait_file(&fixture.path("stdin-closed")).await;
    let device = ios.probe(UDID).await.unwrap();
    assert!(ios.start_stream(&device, options()).await.is_err());
}

#[tokio::test]
async fn rejects_stale_device_or_invalid_options_before_spawning_capture() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let mut device = ios.probe(UDID).await.unwrap();
    device.transport_id = Some("previous-boot".into());
    assert!(matches!(
        ios.start_stream(&device, options()).await,
        Err(Error::StaleSession)
    ));
    let mut invalid = options();
    invalid.epoch = 0;
    assert!(matches!(
        ios.start_stream(&device, invalid).await,
        Err(Error::InvalidArgument { .. })
    ));
    assert!(!fixture.path("helper-pid").exists());
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn startup_deadline_reaps_a_helper_that_never_sends_geometry() {
    let fixture = Fixture::new();
    fixture.write("block-start", "");
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let result = timeout(
        Duration::from_secs(22),
        ios.start_stream(&device, options()),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(Error::Timeout { .. })));
    wait_file(&fixture.path("stdin-closed")).await;
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn malformed_geometry_is_bounded_and_control_sequence_zero_is_rejected() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (_video, mut control) = running.take_streams().unwrap();
    let reply = request(&mut control, 0, 3, ControlCommand::Heartbeat).await;
    assert_eq!(reply.code.as_deref(), Some("INVALID_ARGUMENT"));
    running.close().await.unwrap();
    fixture.write("geometry", &("x".repeat(16_385) + "\n"));
    let error = ios.start_stream(&device, options()).await.err().unwrap();
    assert!(error.to_string().contains("size limit"));
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn dropping_both_consumers_is_a_clean_disconnect() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (video, control) = running.take_streams().unwrap();
    drop(video);
    drop(control);
    wait_file(&fixture.path("stdin-closed")).await;
    running.close().await.unwrap();
    ios.shutdown_stream_starts().await.unwrap();
}

fn touch(phase: TouchPhase) -> ControlCommand {
    ControlCommand::Input {
        event: InputEvent::Touch {
            phase,
            x: 0.4,
            y: 0.6,
            width: 640,
            height: 1136,
        },
    }
}

#[tokio::test]
async fn input_capability_requires_native_probe_and_acknowledgment() {
    let fixture = Fixture::new();
    let ios = fixture.ios();
    assert!(!ios.probe(UDID).await.unwrap().capabilities.input);
    fixture.write("input", "");
    fixture.write("delay-reply", "");
    let device = ios.probe(UDID).await.unwrap();
    assert!(device.capabilities.input);
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (_video, mut control) = running.take_streams().unwrap();
    send_request(&mut control, 1, 3, touch(TouchPhase::Down)).await;
    wait_file(&fixture.path("awaiting-reply")).await;
    assert!(
        timeout(Duration::from_millis(100), control.read_u8())
            .await
            .is_err(),
        "input must not be acknowledged before native submission completes"
    );
    fixture.write("release-reply", "");
    assert!(read_reply(&mut control).await.ok);
    assert!(
        request(&mut control, 2, 3, touch(TouchPhase::Move))
            .await
            .ok
    );
    assert!(request(&mut control, 3, 3, touch(TouchPhase::Up)).await.ok);
    assert!(
        request(
            &mut control,
            4,
            3,
            ControlCommand::Input {
                event: InputEvent::Key {
                    code: 3,
                    phase: KeyPhase::Down
                }
            }
        )
        .await
        .ok
    );
    assert!(
        request(
            &mut control,
            5,
            3,
            ControlCommand::Input {
                event: InputEvent::Key {
                    code: 3,
                    phase: KeyPhase::Up
                }
            }
        )
        .await
        .ok
    );
    assert!(request(&mut control, 6, 3, ControlCommand::Reset).await.ok);
    running.close().await.unwrap();
    wait_file(&fixture.path("stdin-closed")).await;
    assert!(
        fixture
            .read("capture.log")
            .contains("--control-fd 0 --enable-input")
    );
    assert_eq!(fixture.read("commands").lines().count(), 6);
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn native_rejection_is_preserved_without_a_successful_receipt() {
    let fixture = Fixture::new();
    fixture.write("input", "");
    let reply = ControlReply {
        seq: 1,
        ok: false,
        code: Some("INPUT_REJECTED".into()),
        message: Some("Simulator rejected native submission".into()),
    };
    fixture.write(
        "native-reply",
        &(serde_json::to_string(&reply).unwrap() + "\n"),
    );
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (_video, mut control) = running.take_streams().unwrap();
    assert_eq!(
        request(&mut control, 1, 3, touch(TouchPhase::Down)).await,
        reply
    );
    running.close().await.unwrap();
    wait_file(&fixture.path("stdin-closed")).await;
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn native_timeout_closes_socket_and_reaps_the_helper() {
    let fixture = Fixture::new();
    fixture.write("input", "");
    fixture.write("no-reply", "");
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (_video, mut control) = running.take_streams().unwrap();
    send_request(&mut control, 1, 3, touch(TouchPhase::Down)).await;
    let length = timeout(Duration::from_secs(5), control.read(&mut [0]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        length, 0,
        "timeout must not produce a fabricated native receipt"
    );
    assert!(matches!(running.close().await, Err(Error::Timeout { .. })));
    wait_file(&fixture.path("stdin-closed")).await;
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn video_backpressure_does_not_block_native_input_receipts() {
    let fixture = Fixture::new();
    fixture.write("input", "");
    fixture.write("backpressure", "");
    fs::write(fixture.path("backlog"), packet().repeat(10000)).unwrap();
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (_unread_video, mut control) = running.take_streams().unwrap();
    assert!(
        request(&mut control, 1, 3, touch(TouchPhase::Down))
            .await
            .ok
    );
    assert!(request(&mut control, 2, 3, ControlCommand::Reset).await.ok);
    running.close().await.unwrap();
    wait_file(&fixture.path("stdin-closed")).await;
    ios.shutdown_stream_starts().await.unwrap();
}

#[tokio::test]
async fn native_reply_must_match_sequence_and_complete_response_shape() {
    for reply in [
        "{\"seq\":1,\"ok\":true}",
        "{\"seq\":2,\"ok\":true,\"code\":null,\"message\":null}",
        "{\"seq\":1,\"ok\":true,\"code\":\"ERROR\",\"message\":null}",
        "{\"seq\":1,\"ok\":false,\"code\":null,\"message\":\"no code\"}",
        "{\"seq\":1,\"ok\":false,\"code\":\"ERROR\",\"message\":\"\"}",
        "{\"seq\":1,\"ok\":true,\"code\":null,\"message\":null,\"unknown\":1}",
    ] {
        let fixture = Fixture::new();
        fixture.write("native-reply", &(reply.to_owned() + "\n"));
        let ios = fixture.ios();
        let device = ios.probe(UDID).await.unwrap();
        let mut running = ios.start_stream(&device, options()).await.unwrap();
        let (_video, mut control) = running.take_streams().unwrap();
        send_request(&mut control, 1, 3, ControlCommand::Heartbeat).await;
        assert_eq!(
            timeout(Duration::from_secs(3), control.read(&mut [0]))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(matches!(
            running.close().await,
            Err(Error::CommandFailed { .. })
        ));
        wait_file(&fixture.path("stdin-closed")).await;
        ios.shutdown_stream_starts().await.unwrap();
    }
}

#[tokio::test]
async fn stop_receipt_is_drained_when_native_video_closes_first() {
    let fixture = Fixture::new();
    fixture.write("close-video-before-reply", "");
    let ios = fixture.ios();
    let device = ios.probe(UDID).await.unwrap();
    let mut running = ios.start_stream(&device, options()).await.unwrap();
    let (mut video, mut control) = running.take_streams().unwrap();
    let mut bytes = vec![0; packet().len()];
    video.read_exact(&mut bytes).await.unwrap();
    assert!(request(&mut control, 1, 3, ControlCommand::Stop).await.ok);
    running.close().await.unwrap();
    wait_file(&fixture.path("stopped")).await;
    ios.shutdown_stream_starts().await.unwrap();
}
