#![cfg(unix)]

use std::{
    fs,
    future::Future,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
    time::Duration,
};

use mpp_android::{Android, StreamOptions};
use mpp_core::{
    Capabilities, Device, DeviceKind, DeviceState, Error, Platform,
    stream::{Channel, DEVICE_PROTOCOL, DeviceConfig, DeviceHello, Geometry},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{sleep, timeout},
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const STREAM_ID: &str = "abcdef0123456789abcdef0123456789";

struct Fixture {
    directory: PathBuf,
    adb: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "mpp stream test {} {}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&directory).unwrap();
        for (name, value) in [
            ("bootstrap.jar", "fake-bootstrap"),
            ("library.so", "fake-library"),
            ("transport", "77"),
            ("sdk", "32"),
            ("abi", "arm64-v8a"),
        ] {
            fs::write(directory.join(name), value).unwrap();
        }
        let adb = directory.join("adb");
        fs::write(&adb, format!(r#"#!/bin/sh
set -eu
cd '{}'
printf '%s\n' "$*" >> adb.log
case "$*" in
  'devices -l') printf 'List of devices attached\nphone device model:Pixel transport_id:%s\n' "$(cat transport)" ;;
  '-t 77 get-state'|'-t 78 get-state') echo device ;;
  '-t 77 shell getprop ro.build.version.sdk') cat sdk ;;
  '-t 77 shell getprop ro.product.cpu.abi') cat abi ;;
  '-t 77 shell mkdir '*)
    if [ -f directory-exists ]; then exit 7; fi
    touch directory-created ;;
  '-t 77 push '*)
    if [ -f fail-push ]; then exit 7; fi ;;
  '-t 77 reverse --no-rebind '*)
    if [ -f fail-reverse ]; then exit 7; fi
    printf '%s\n' "$6" > port
    if [ -f delay-reverse ]; then
      while [ ! -f release-reverse ]; do sleep 0.01; done
    fi ;;
  '-t 77 reverse --remove '*) touch reverse-removed ;;
  '-t 77 shell rm -rf '*) touch directory-removed ;;
  '-t 77 shell -T '*)
    IFS= read -r config
    printf '%s\n' "$config" > config
    if [ -f fail-helper ]; then printf 'helper error: %s\n' "$config" >&2; exit 8; fi
    cat > /dev/null
    touch stdin-closed ;;
  *) exit 9 ;;
esac
"#, directory.display())).unwrap();
        fs::set_permissions(&adb, fs::Permissions::from_mode(0o755)).unwrap();
        Self { directory, adb }
    }

    fn android(&self) -> Android {
        Android::with_tools(self.adb.clone(), None)
    }
    fn options(&self) -> StreamOptions {
        StreamOptions {
            bootstrap: self.directory.join("bootstrap.jar"),
            library: self.directory.join("library.so"),
            token: TOKEN.into(),
            stream_id: STREAM_ID.into(),
            generation: 7,
            epoch: 3,
            max_size: 1280,
            bit_rate: 4_000_000,
            max_fps: 30,
        }
    }
    fn mark(&self, name: &str) {
        fs::write(self.directory.join(name), b"").unwrap();
    }
    fn log(&self) -> String {
        fs::read_to_string(self.directory.join("adb.log")).unwrap_or_default()
    }
    fn exists(&self, name: &str) -> bool {
        self.directory.join(name).exists()
    }

    fn peer(&self, fault: Fault) -> tokio::task::JoinHandle<Vec<TcpStream>> {
        let directory = self.directory.clone();
        tokio::spawn(async move {
            let config: DeviceConfig =
                serde_json::from_str(&wait_file(&directory.join("config")).await).unwrap();
            let port = wait_file(&directory.join("port")).await;
            let port: u16 = port.trim().strip_prefix("tcp:").unwrap().parse().unwrap();
            let mut sockets = Vec::new();
            if matches!(fault, Fault::InvalidCandidates) {
                for index in 0..8 {
                    let mut hello = hello(&config, Channel::Video);
                    if index % 2 == 0 {
                        hello.token = "f".repeat(64);
                    } else {
                        hello.epoch += 1;
                    }
                    let mut stream = send_hello(port, &hello).await;
                    let mut byte = [0];
                    assert_eq!(
                        timeout(Duration::from_secs(3), stream.read(&mut byte))
                            .await
                            .unwrap()
                            .unwrap(),
                        0
                    );
                }
                return sockets;
            }
            if matches!(fault, Fault::IdleCandidate) {
                let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                    .await
                    .unwrap();
                stream.write_all(b"{").await.unwrap();
                let mut byte = [0];
                assert_eq!(
                    timeout(Duration::from_secs(3), stream.read(&mut byte))
                        .await
                        .unwrap()
                        .unwrap(),
                    0
                );
            }
            if matches!(fault, Fault::BadThenValid) {
                let mut invalid = hello(&config, Channel::Video);
                invalid.token = "f".repeat(64);
                let mut stream = send_hello(port, &invalid).await;
                let mut byte = [0];
                assert_eq!(stream.read(&mut byte).await.unwrap(), 0);
            }
            // Control-first proves there is no implicit ordering dependency.
            for channel in [Channel::Control, Channel::Video] {
                let mut hello = hello(&config, channel);
                if channel == Channel::Video {
                    match fault {
                        Fault::DuplicateRole => hello.channel = Channel::Control,
                        Fault::Geometry => hello.geometry.width += 2,
                        Fault::ReplacedDevice => {
                            fs::write(directory.join("transport"), b"78").unwrap()
                        }
                        _ => {}
                    }
                }
                let mut stream = send_hello(port, &hello).await;
                let mut bytes = [0; 12];
                if channel == Channel::Video
                    && matches!(fault, Fault::DuplicateRole | Fault::Geometry)
                {
                    assert_eq!(stream.read(&mut bytes).await.unwrap(), 0);
                    break;
                }
                timeout(Duration::from_secs(2), stream.read_exact(&mut bytes))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(&bytes, b"{\"ok\":true}\n");
                sockets.push(stream);
            }
            sockets
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
#[derive(Clone, Copy)]
enum Fault {
    None,
    BadThenValid,
    IdleCandidate,
    DuplicateRole,
    Geometry,
    ReplacedDevice,
    InvalidCandidates,
}

fn device() -> Device {
    Device {
        id: "android:phone".into(),
        platform: Platform::Android,
        kind: DeviceKind::Physical,
        state: DeviceState::Online,
        name: "Pixel".into(),
        serial: Some("phone".into()),
        transport_id: Some("77".into()),
        avd: None,
        capabilities: Capabilities::default(),
    }
}
fn hello(config: &DeviceConfig, channel: Channel) -> DeviceHello {
    DeviceHello {
        protocol: DEVICE_PROTOCOL.into(),
        token: config.token.clone(),
        channel,
        generation: config.generation,
        epoch: config.epoch,
        geometry: Geometry {
            width: 720,
            height: 1280,
            display_width: 1080,
            display_height: 1920,
            rotation: 0,
        },
    }
}
async fn send_hello(port: u16, hello: &DeviceHello) -> TcpStream {
    let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    let mut bytes = serde_json::to_vec(hello).unwrap();
    bytes.push(b'\n');
    // Fragment the hello to exercise framing instead of one-write/one-read assumptions.
    for part in bytes.chunks(13) {
        stream.write_all(part).await.unwrap();
    }
    stream
}
async fn wait_file(path: &Path) -> String {
    timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(value) = fs::read_to_string(path)
                && !value.is_empty()
            {
                return value;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
async fn wait_exists(path: &Path) {
    timeout(Duration::from_secs(30), async {
        while !path.exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn authenticates_channels_in_either_order_and_preserves_media_bytes() {
    let fixture = Fixture::new();
    let peer = fixture.peer(Fault::BadThenValid);
    let mut running = fixture
        .android()
        .start_stream(&device(), fixture.options())
        .await
        .unwrap();
    let mut sockets = peer.await.unwrap();
    assert_eq!(running.geometry.width, 720);
    assert!(!fixture.exists("stdin-closed"));
    assert!(fixture.exists("reverse-removed"));
    let (mut video, mut control) = running.take_streams().unwrap();
    sockets[1].write_all(b"MPP1").await.unwrap();
    let mut magic = [0; 4];
    video.read_exact(&mut magic).await.unwrap();
    assert_eq!(&magic, b"MPP1");
    control.write_all(b"input").await.unwrap();
    let mut input = [0; 5];
    sockets[0].read_exact(&mut input).await.unwrap();
    assert_eq!(&input, b"input");
    assert!(running.take_streams().is_err());
    drop((video, control));
    running.close().await.unwrap();
    running.close().await.unwrap();
    assert!(fixture.exists("stdin-closed"));
    assert!(fixture.exists("directory-removed"));
    let log = fixture.log();
    assert!(!log.contains(TOKEN));
    assert!(!log.contains("-s "));
    assert_eq!(log.matches("reverse --remove").count(), 1);
    assert!(log.contains("reverse --no-rebind localabstract:mpp_"));
    assert!(log.contains("shell -T CLASSPATH=/data/local/tmp/mpp_"));
}
#[tokio::test]
async fn rejects_unqualified_devices_before_deployment() {
    for (field, value) in [("sdk", "33"), ("abi", "x86_64")] {
        let fixture = Fixture::new();
        fs::write(fixture.directory.join(field), value).unwrap();
        assert!(matches!(
            fixture
                .android()
                .start_stream(&device(), fixture.options())
                .await,
            Err(Error::Unsupported { .. })
        ));
        assert!(!fixture.log().contains("mkdir"));
    }
}
#[tokio::test]
async fn rejects_replaced_transport_before_and_after_handshake() {
    let fixture = Fixture::new();
    fs::write(fixture.directory.join("transport"), b"78").unwrap();
    assert!(matches!(
        fixture
            .android()
            .start_stream(&device(), fixture.options())
            .await,
        Err(Error::StaleSession)
    ));
    assert!(!fixture.log().contains("mkdir"));
    let fixture = Fixture::new();
    let peer = fixture.peer(Fault::ReplacedDevice);
    assert!(matches!(
        fixture
            .android()
            .start_stream(&device(), fixture.options())
            .await,
        Err(Error::StaleSession)
    ));
    drop(peer.await.unwrap());
    assert!(fixture.exists("directory-removed"));
    assert!(fixture.exists("reverse-removed"));
    assert!(!fixture.log().contains("-t 78 shell"));
}
#[tokio::test]
async fn rejects_bad_token_epoch_duplicate_role_and_conflicting_geometry() {
    for fault in [
        Fault::InvalidCandidates,
        Fault::DuplicateRole,
        Fault::Geometry,
    ] {
        let fixture = Fixture::new();
        let peer = fixture.peer(fault);
        assert!(
            fixture
                .android()
                .start_stream(&device(), fixture.options())
                .await
                .is_err()
        );
        drop(peer.await.unwrap());
        assert!(fixture.exists("stdin-closed"));
        assert!(fixture.exists("directory-removed"));
        assert!(fixture.exists("reverse-removed"));
    }
}
#[tokio::test]
async fn removes_only_resources_it_created_on_acquisition_failures() {
    for failure in ["directory-exists", "fail-push", "fail-reverse"] {
        let fixture = Fixture::new();
        fixture.mark(failure);
        assert!(
            fixture
                .android()
                .start_stream(&device(), fixture.options())
                .await
                .is_err()
        );
        assert_eq!(
            fixture.exists("directory-removed"),
            failure != "directory-exists"
        );
        assert!(!fixture.exists("reverse-removed"));
    }
}
#[tokio::test]
async fn helper_exit_is_prompt_and_never_returns_the_secret() {
    let fixture = Fixture::new();
    fixture.mark("fail-helper");
    let result = timeout(
        Duration::from_secs(30),
        fixture.android().start_stream(&device(), fixture.options()),
    )
    .await
    .unwrap();
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("helper unexpectedly started"),
    };
    assert!(!error.to_string().contains(TOKEN));
    assert!(fixture.exists("directory-removed"));
    assert!(fixture.exists("reverse-removed"));
}
#[tokio::test]
async fn cancelling_startup_closes_stdin_and_cleans_owned_resources() {
    let fixture = Fixture::new();
    let android = fixture.android();
    let options = fixture.options();
    let task = tokio::spawn(async move { android.start_stream(&device(), options).await });
    wait_file(&fixture.directory.join("config")).await;
    task.abort();
    let _ = task.await;
    wait_exists(&fixture.directory.join("directory-removed")).await;
    assert!(fixture.exists("stdin-closed"));
    assert!(fixture.exists("reverse-removed"));
}
#[tokio::test]
async fn dropping_running_stream_stops_the_owned_helper() {
    let fixture = Fixture::new();
    let peer = fixture.peer(Fault::None);
    let running = fixture
        .android()
        .start_stream(&device(), fixture.options())
        .await
        .unwrap();
    drop(peer.await.unwrap());
    drop(running);
    wait_exists(&fixture.directory.join("directory-removed")).await;
    assert!(fixture.exists("stdin-closed"));
}
#[tokio::test]
async fn validates_assets_and_identifiers_without_invoking_adb() {
    let fixture = Fixture::new();
    let mut options = fixture.options();
    options.stream_id = "bad; id".into();
    assert!(
        fixture
            .android()
            .start_stream(&device(), options)
            .await
            .is_err()
    );
    let mut options = fixture.options();
    options.bootstrap = PathBuf::from("relative.jar");
    assert!(
        fixture
            .android()
            .start_stream(&device(), options)
            .await
            .is_err()
    );
    let mut options = fixture.options();
    options.library = fixture.directory.clone();
    assert!(
        fixture
            .android()
            .start_stream(&device(), options)
            .await
            .is_err()
    );
    let mut options = fixture.options();
    options.token = "secret".into();
    assert!(
        fixture
            .android()
            .start_stream(&device(), options)
            .await
            .is_err()
    );
    assert!(fixture.log().is_empty());
}
#[tokio::test]
async fn idle_candidate_is_timed_out_without_blocking_authenticated_channels() {
    let fixture = Fixture::new();
    let peer = fixture.peer(Fault::IdleCandidate);
    let mut running = timeout(
        Duration::from_secs(30),
        fixture.android().start_stream(&device(), fixture.options()),
    )
    .await
    .unwrap()
    .unwrap();
    drop(peer.await.unwrap());
    running.close().await.unwrap();
    assert!(fixture.exists("directory-removed"));
}
#[tokio::test]
async fn shutdown_waits_for_cancelled_reverse_acquisition_and_owned_cleanup() {
    let fixture = Fixture::new();
    fixture.mark("delay-reverse");
    let android = fixture.android();
    let starter = android.clone();
    let options = fixture.options();
    let request = tokio::spawn(async move { starter.start_stream(&device(), options).await });
    wait_file(&fixture.directory.join("port")).await;
    request.abort();
    let _ = request.await;
    let stopper = android.clone();
    let shutdown = tokio::spawn(async move { stopper.shutdown_stream_starts().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    fixture.mark("release-reverse");
    shutdown.await.unwrap().unwrap();
    assert!(fixture.exists("reverse-removed"));
    assert!(fixture.exists("directory-removed"));
    let log = fixture.log();
    assert_eq!(log.matches("reverse --remove").count(), 1);
    assert_eq!(log.matches("shell rm -rf").count(), 1);
    assert!(
        android
            .start_stream(&device(), fixture.options())
            .await
            .is_err()
    );
    assert_eq!(fixture.log(), log);
    android.shutdown_stream_starts().await.unwrap();
}
#[tokio::test]
async fn shutdown_cancels_pending_handshake_without_requiring_caller_abort() {
    let fixture = Fixture::new();
    let android = fixture.android();
    let starter = android.clone();
    let options = fixture.options();
    let request = tokio::spawn(async move { starter.start_stream(&device(), options).await });
    wait_file(&fixture.directory.join("config")).await;
    android.shutdown_stream_starts().await.unwrap();
    assert!(request.await.unwrap().is_err());
    assert!(fixture.exists("stdin-closed"));
    assert!(fixture.exists("reverse-removed"));
    assert!(fixture.exists("directory-removed"));
    let independent = fixture.android();
    fs::write(fixture.directory.join("sdk"), b"33").unwrap();
    assert!(matches!(
        independent.start_stream(&device(), fixture.options()).await,
        Err(Error::Unsupported { .. })
    ));
    independent.shutdown_stream_starts().await.unwrap();
}
#[tokio::test]
async fn shutdown_drains_unclaimed_success_when_its_caller_is_dropped() {
    let fixture = Fixture::new();
    let android = fixture.android();
    let selected = device();
    let peer = fixture.peer(Fault::None);
    let mut request = Box::pin(android.start_stream(&selected, fixture.options()));
    std::future::poll_fn(|cx| {
        assert!(matches!(request.as_mut().poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    let sockets = peer.await.unwrap();
    wait_exists(&fixture.directory.join("reverse-removed")).await;
    tokio::task::yield_now().await;
    assert!(!fixture.exists("stdin-closed"));
    drop(request);
    android.shutdown_stream_starts().await.unwrap();
    assert!(fixture.exists("stdin-closed"));
    assert!(fixture.exists("directory-removed"));
    drop(sockets);
}
#[tokio::test]
async fn completed_starts_are_reaped_instead_of_exhausting_the_pending_limit() {
    let fixture = Fixture::new();
    let android = fixture.android();
    let mut stale = device();
    stale.state = DeviceState::Stopped;
    for _ in 0..64 {
        assert!(matches!(
            android.start_stream(&stale, fixture.options()).await,
            Err(Error::StaleSession)
        ));
    }
    android.shutdown_stream_starts().await.unwrap();
    assert!(fixture.log().is_empty());
}
#[tokio::test]
async fn shutdown_closes_ready_but_unclaimed_stream_before_returning() {
    let fixture = Fixture::new();
    let android = fixture.android();
    let selected = device();
    let peer = fixture.peer(Fault::None);
    let mut request = Box::pin(android.start_stream(&selected, fixture.options()));
    std::future::poll_fn(|cx| {
        assert!(matches!(request.as_mut().poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    let sockets = peer.await.unwrap();
    wait_exists(&fixture.directory.join("reverse-removed")).await;
    android.shutdown_stream_starts().await.unwrap();
    assert!(fixture.exists("stdin-closed"));
    assert!(fixture.exists("directory-removed"));
    assert!(request.await.is_err());
    drop(sockets);
}
