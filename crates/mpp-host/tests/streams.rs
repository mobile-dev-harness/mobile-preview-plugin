#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use mpp_android::Android;
use mpp_core::{
    Capabilities, Device, DeviceKind, DeviceState, Error, Platform, Session, SessionState,
    stream::{Channel, DEVICE_PROTOCOL, DeviceConfig, DeviceHello, Geometry},
};
use mpp_host::{
    Host,
    streams::{PreviewDescriptor, PreviewManager, PreviewOptions},
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpStream, UnixStream},
    time::{sleep, timeout},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "mpp-hs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("bootstrap.jar"), b"fake").unwrap();
        fs::write(root.join("library.so"), b"fake").unwrap();
        let adb = root.join("adb");
        fs::write(
            &adb,
            r##"#!/bin/sh
set -eu
cd "${0%/*}"
case "$*" in
  'devices -l')
    printf 'List of devices attached\n'
    if [ ! -f gone ]; then printf 'phone device model:Test transport_id:77\n'; fi ;;
  '-t 77 get-state') echo device ;;
  '-t 77 shell getprop ro.build.version.sdk') echo 32 ;;
  '-t 77 shell getprop ro.product.cpu.abi') echo arm64-v8a ;;
  '-t 77 shell mkdir '*) : ;;
  '-t 77 push '*) : ;;
  '-t 77 reverse --no-rebind '*) printf '%s\n' "$6" > port ;;
  '-t 77 reverse --remove '*) touch reverse-removed ;;
  '-t 77 shell rm -rf '*)
    if [ -f delay-cleanup ]; then sleep 2; fi
    touch directory-removed ;;
  '-t 77 shell -T '*)
    printf '%s\n' "$$" > helper-pid
    IFS= read -r config
    printf '%s\n' "$config" > config
    cat > /dev/null
    touch helper-closed ;;
  *) exit 9 ;;
esac
"##,
        )
        .unwrap();
        fs::set_permissions(&adb, fs::Permissions::from_mode(0o755)).unwrap();
        Self { root }
    }

    fn android(&self) -> Android {
        Android::with_tools(self.root.join("adb"), None)
    }

    fn options(&self, number: u32) -> PreviewOptions {
        let socket_dir = self.root.join(format!("s{number}"));
        fs::create_dir(&socket_dir).unwrap();
        fs::set_permissions(&socket_dir, fs::Permissions::from_mode(0o700)).unwrap();
        PreviewOptions {
            bootstrap: self.root.join("bootstrap.jar"),
            library: self.root.join("library.so"),
            socket_dir,
            stream_id: format!("{number:032x}"),
            token: "a".repeat(64),
            max_size: 1280,
            bit_rate: 4_000_000,
            max_fps: 30,
        }
    }

    fn peer(&self, number: u32) -> tokio::task::JoinHandle<(TcpStream, TcpStream)> {
        let root = self.root.clone();
        tokio::spawn(async move {
            timeout(Duration::from_secs(8), async {
                let config = loop {
                    if let Ok(bytes) = fs::read(root.join("config"))
                        && let Ok(config) = serde_json::from_slice::<DeviceConfig>(&bytes)
                        && config.socket_name == format!("mpp_{number:032x}")
                    {
                        break config;
                    }
                    sleep(Duration::from_millis(5)).await;
                };
                let port = fs::read_to_string(root.join("port")).unwrap();
                let port: u16 = port.trim().strip_prefix("tcp:").unwrap().parse().unwrap();
                let mut connections = Vec::new();
                for channel in [Channel::Control, Channel::Video] {
                    let hello = DeviceHello {
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
                    };
                    let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                        .await
                        .unwrap();
                    let mut bytes = serde_json::to_vec(&hello).unwrap();
                    bytes.push(b'\n');
                    stream.write_all(&bytes).await.unwrap();
                    let mut ack = [0; 12];
                    stream.read_exact(&mut ack).await.unwrap();
                    assert_eq!(&ack, b"{\"ok\":true}\n");
                    connections.push(stream);
                }
                let video = connections.pop().unwrap();
                let control = connections.pop().unwrap();
                (video, control)
            })
            .await
            .unwrap()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn session() -> Session {
    Session {
        id: "session-1".into(),
        owner: "chat-a".into(),
        generation: 7,
        state: SessionState::TransportReady,
        device: Device {
            id: "android:phone".into(),
            platform: Platform::Android,
            kind: DeviceKind::Physical,
            state: DeviceState::Online,
            name: "Test".into(),
            serial: Some("phone".into()),
            transport_id: Some("77".into()),
            avd: None,
            capabilities: Capabilities::default(),
        },
    }
}

async fn begin(
    fixture: &Fixture,
    manager: &mut PreviewManager,
    number: u32,
) -> (PreviewDescriptor, TcpStream, TcpStream) {
    let options = fixture.options(number);
    let peer = fixture.peer(number);
    let descriptor = manager
        .start(&fixture.android(), &session(), options)
        .await
        .unwrap();
    let (video, control) = peer.await.unwrap();
    (descriptor, video, control)
}

async fn attach(descriptor: &PreviewDescriptor) -> (UnixStream, UnixStream) {
    (
        UnixStream::connect(&descriptor.video_socket).await.unwrap(),
        UnixStream::connect(&descriptor.control_socket)
            .await
            .unwrap(),
    )
}

async fn gone(path: &Path) {
    timeout(Duration::from_secs(5), async {
        while path.exists() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn video_order_and_control_receipts_use_separate_live_channels() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    let (descriptor, mut remote_video, remote_control) = begin(&fixture, &mut manager, 1).await;
    let (mut video, control) = attach(&descriptor).await;
    let payload = vec![0x56; 200_000];
    let sender = tokio::spawn(async move {
        remote_video.write_all(&payload).await.unwrap();
        remote_video
    });
    let mut received = vec![0; 200_000];
    timeout(Duration::from_secs(3), video.read_exact(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, vec![0x56; 200_000]);
    let _remote_video = sender.await.unwrap();
    let request = json!({"seq":1,"epoch":descriptor.epoch,"command":{"kind":"heartbeat"}});
    let mut control = BufReader::new(control);
    control
        .get_mut()
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    let mut remote = BufReader::new(remote_control);
    let mut line = String::new();
    remote.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), request);
    remote
        .get_mut()
        .write_all(b"{\"seq\":1,\"ok\":true,\"code\":null,\"message\":null}\n")
        .await
        .unwrap();
    line.clear();
    control.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["ok"], true);
    manager
        .stop(&session(), &descriptor.stream_id, descriptor.epoch)
        .await
        .unwrap();
    assert!(!descriptor.video_socket.exists());
    assert!(!descriptor.control_socket.exists());
}

#[tokio::test]
async fn old_stop_and_wrong_owner_cannot_stop_a_new_capture_epoch() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    let (first, _v1, _c1) = begin(&fixture, &mut manager, 1).await;
    manager
        .stop(&session(), &first.stream_id, first.epoch)
        .await
        .unwrap();
    fs::remove_file(fixture.root.join("config")).unwrap();
    fs::remove_file(fixture.root.join("port")).unwrap();
    let mut reused = fixture.options(3);
    reused.stream_id = first.stream_id.clone();
    let peer = fixture.peer(1);
    let second = manager
        .start(&fixture.android(), &session(), reused)
        .await
        .unwrap();
    let (_v2, _c2) = peer.await.unwrap();
    assert!(second.epoch > first.epoch);
    assert_eq!(second.generation, first.generation);
    assert!(matches!(
        manager
            .stop(&session(), &first.stream_id, first.epoch)
            .await,
        Err(Error::StaleSession)
    ));
    let mut wrong_owner = session();
    wrong_owner.owner = "chat-b".into();
    assert!(matches!(
        manager
            .stop(&wrong_owner, &second.stream_id, second.epoch)
            .await,
        Err(Error::StaleSession)
    ));
    assert!(second.video_socket.exists());
    manager.shutdown().await;
    assert!(!second.video_socket.exists());
}

#[tokio::test]
async fn any_channel_eof_closes_the_capture_and_removes_both_sockets() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    for number in [1, 2, 3, 4] {
        let (descriptor, remote_video, remote_control) =
            begin(&fixture, &mut manager, number).await;
        let (video, control) = attach(&descriptor).await;
        let (mut video, mut control) = (Some(video), Some(control));
        let (mut remote_video, mut remote_control) = (Some(remote_video), Some(remote_control));
        match number {
            1 => drop(video.take()),
            2 => drop(control.take()),
            3 => drop(remote_video.take()),
            _ => drop(remote_control.take()),
        }
        gone(&descriptor.video_socket).await;
        assert!(!descriptor.control_socket.exists());
    }
    manager.shutdown().await;
}

#[tokio::test]
async fn control_reply_timeout_and_invalid_epoch_fail_closed() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    for number in [1, 2, 3, 4] {
        let (descriptor, _remote_video, _remote_control) =
            begin(&fixture, &mut manager, number).await;
        let (_video, mut control) = attach(&descriptor).await;
        let bytes = match number {
            1 => format!(
                "{}\n",
                json!({"seq":1,"epoch":descriptor.epoch,"command":{"kind":"heartbeat"}})
            )
            .into_bytes(),
            2 => format!(
                "{}\n",
                json!({"seq":1,"epoch":descriptor.epoch+1,"command":{"kind":"heartbeat"}})
            )
            .into_bytes(),
            3 => vec![b'x'; mpp_core::stream::MAX_CONTROL_BYTES + 1],
            _ => b"{".to_vec(),
        };
        control.write_all(&bytes).await.unwrap();
        gone(&descriptor.video_socket).await;
    }
    manager.shutdown().await;
}

#[tokio::test]
async fn private_sockets_reject_symlinks_permissions_and_existing_paths_without_overwriting() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    let options = fixture.options(1);
    fs::write(options.socket_dir.join("control.sock"), b"keep-me").unwrap();
    let directory = options.socket_dir.clone();
    assert!(
        manager
            .start(&fixture.android(), &session(), options)
            .await
            .is_err()
    );
    assert_eq!(
        fs::read(directory.join("control.sock")).unwrap(),
        b"keep-me"
    );
    assert!(!directory.join("video.sock").exists());
    let options = fixture.options(2);
    fs::set_permissions(&options.socket_dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        manager
            .start(&fixture.android(), &session(), options)
            .await
            .is_err()
    );
    let mut options = fixture.options(3);
    let link = fixture.root.join("link");
    std::os::unix::fs::symlink(&options.socket_dir, &link).unwrap();
    options.socket_dir = link;
    assert!(
        manager
            .start(&fixture.android(), &session(), options)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cleanup_does_not_unlink_a_replacement_socket_path() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    let (descriptor, _video, _control) = begin(&fixture, &mut manager, 1).await;
    fs::remove_file(&descriptor.video_socket).unwrap();
    fs::write(&descriptor.video_socket, b"replacement").unwrap();
    manager
        .stop(&session(), &descriptor.stream_id, descriptor.epoch)
        .await
        .unwrap();
    assert_eq!(fs::read(&descriptor.video_socket).unwrap(), b"replacement");
    assert!(!descriptor.control_socket.exists());
}

async fn call(host: &mut Host, method: &str, params: Value) -> Value {
    serde_json::to_value(
        host.request(
            &serde_json::to_vec(&json!({"id":1,"method":method,"params":params})).unwrap(),
        )
        .await,
    )
    .unwrap()
}

#[tokio::test]
async fn preview_rpc_requires_lease_and_complete_assets_and_keeps_legacy_stub_behavior() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.android());
    let connected = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a","device":"android:phone"}),
    )
    .await;
    assert_eq!(connected["ok"], true, "{connected}");
    let lease = json!({"owner":"chat-a","session":connected["result"]["id"],"generation":connected["result"]["generation"]});
    assert_eq!(
        call(&mut host, "preview.start", lease.clone()).await["error"]["code"],
        "UNSUPPORTED"
    );
    let mut partial = lease.clone();
    partial["bootstrap"] = json!(fixture.root.join("bootstrap.jar"));
    assert_eq!(
        call(&mut host, "preview.start", partial).await["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let mut stopped = lease.clone();
    stopped["stream_id"] = json!("0".repeat(32));
    assert_eq!(
        call(&mut host, "preview.stop", stopped.clone()).await["error"]["code"],
        "INVALID_ARGUMENT"
    );
    stopped["epoch"] = json!(1);
    assert_eq!(
        call(&mut host, "preview.stop", stopped).await["error"]["code"],
        "STALE_SESSION"
    );
    let hello = call(&mut host, "hello", json!({})).await;
    assert!(
        hello["result"]["methods"]
            .as_array()
            .unwrap()
            .contains(&json!("preview.stop"))
    );
    assert_eq!(hello["result"]["video_backend"], "requires_device_assets");
    host.shutdown().await;
}

#[tokio::test]
async fn cancelled_startup_removes_private_sockets_and_finishes_backend_cleanup() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    let options = fixture.options(1);
    let directory = options.socket_dir.clone();
    let android = fixture.android();
    let session = session();
    {
        let starting = manager.start(&android, &session, options);
        tokio::pin!(starting);
        timeout(Duration::from_secs(5), async {
            tokio::select! {
                result = &mut starting => panic!("startup did not wait for channels: {result:?}"),
                _ = async { while !fixture.root.join("config").exists() { sleep(Duration::from_millis(5)).await; } } => {}
            }
        }).await.unwrap();
    }
    assert!(!directory.join("video.sock").exists());
    assert!(!directory.join("control.sock").exists());
    timeout(Duration::from_secs(5), async {
        while !fixture.root.join("directory-removed").exists() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(fixture.root.join("helper-closed").exists());
    manager.shutdown().await;
}

#[tokio::test]
async fn slow_video_consumer_terminates_the_capture_with_a_bounded_write_deadline() {
    let fixture = Fixture::new();
    let mut manager = PreviewManager::default();
    let (descriptor, mut remote_video, _remote_control) = begin(&fixture, &mut manager, 1).await;
    let (_video, _control) = attach(&descriptor).await;
    let producer = tokio::spawn(async move {
        let bytes = [0x5a; 64 * 1024];
        while remote_video.write_all(&bytes).await.is_ok() {}
    });
    gone(&descriptor.video_socket).await;
    producer.abort();
    let _ = producer.await;
    assert!(!descriptor.control_socket.exists());
    manager.shutdown().await;
}

async fn host_connect(host: &mut Host) -> Value {
    let connected = call(
        host,
        "session.connect",
        json!({"owner":"chat-a","device":"android:phone"}),
    )
    .await;
    assert_eq!(connected["ok"], true, "{connected}");
    json!({"owner":"chat-a","session":connected["result"]["id"],"generation":connected["result"]["generation"]})
}

async fn host_preview(
    fixture: &Fixture,
    host: &mut Host,
    lease: &Value,
    number: u32,
) -> (PathBuf, PathBuf, TcpStream, TcpStream) {
    let options = fixture.options(number);
    let peer = fixture.peer(number);
    let mut params = lease.clone();
    params["bootstrap"] = json!(options.bootstrap);
    params["library"] = json!(options.library);
    params["socket_dir"] = json!(options.socket_dir);
    params["stream_id"] = json!(options.stream_id);
    params["token"] = json!(options.token);
    let response = call(host, "preview.start", params).await;
    assert_eq!(response["ok"], true, "{response}");
    let (remote_video, remote_control) = peer.await.unwrap();
    (
        response["result"]["video_socket"].as_str().unwrap().into(),
        response["result"]["control_socket"]
            .as_str()
            .unwrap()
            .into(),
        remote_video,
        remote_control,
    )
}

#[tokio::test]
async fn closing_media_keeps_chat_lease_and_stdio_eof_awaits_live_actor_cleanup() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.android());
    let lease = host_connect(&mut host).await;
    let (video_path, control_path, _v1, _c1) = host_preview(&fixture, &mut host, &lease, 1).await;
    let video = UnixStream::connect(&video_path).await.unwrap();
    let _control = UnixStream::connect(&control_path).await.unwrap();
    drop(video);
    gone(&video_path).await;
    assert_eq!(
        call(&mut host, "session.status", lease.clone()).await["ok"],
        true
    );
    let (next_video, next_control, _v2, _c2) = host_preview(&fixture, &mut host, &lease, 2).await;
    let mut output = Vec::new();
    mpp_host::serve(&mut host, &b""[..], &mut output)
        .await
        .unwrap();
    assert!(!next_video.exists());
    assert!(!next_control.exists());
    assert_eq!(
        call(&mut host, "session.status", lease).await["error"]["code"],
        "STALE_SESSION"
    );
}

#[tokio::test]
async fn lease_disconnect_and_probe_failure_close_capture_before_releasing_the_lease() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.android());
    let lease = host_connect(&mut host).await;
    let (video, control, _v1, _c1) = host_preview(&fixture, &mut host, &lease, 1).await;
    assert_eq!(
        call(&mut host, "session.disconnect", lease).await["ok"],
        true
    );
    assert!(!video.exists());
    assert!(!control.exists());
    let lease = host_connect(&mut host).await;
    let (video, control, _v2, _c2) = host_preview(&fixture, &mut host, &lease, 2).await;
    fs::write(fixture.root.join("gone"), b"").unwrap();
    assert_eq!(
        call(&mut host, "session.status", lease.clone()).await["error"]["code"],
        "NOT_FOUND"
    );
    assert!(!video.exists());
    assert!(!control.exists());
    assert_eq!(
        call(&mut host, "session.status", lease).await["error"]["code"],
        "STALE_SESSION"
    );
    host.shutdown().await;
}

#[tokio::test]
async fn native_host_eof_and_sigterm_cancel_startup_and_reap_owned_resources_before_exit() {
    use std::process::Stdio;
    use tokio::process::Command;

    for terminate in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("delay-cleanup"), b"").unwrap();
        let options = fixture.options(1);
        let mut child = Command::new(env!("CARGO_BIN_EXE_mpp"))
            .env("TOKIO_WORKER_THREADS", "2")
            .arg("--adb")
            .arg(fixture.root.join("adb"))
            .args(["serve", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut input = Some(child.stdin.take().unwrap());
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let connect = json!({"id":"connect","method":"session.connect","params":{"owner":"chat-a","device":"android:phone"}});
        input
            .as_mut()
            .unwrap()
            .write_all(format!("{connect}\n").as_bytes())
            .await
            .unwrap();
        let mut line = String::new();
        timeout(Duration::from_secs(15), output.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let connected: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(connected["ok"], true, "{connected}");
        let start = json!({"id":"start","method":"preview.start","params":{
            "owner":"chat-a","session":connected["result"]["id"],"generation":connected["result"]["generation"],
            "bootstrap":options.bootstrap,"library":options.library,"socket_dir":options.socket_dir,
            "stream_id":options.stream_id,"token":options.token,
        }});
        input
            .as_mut()
            .unwrap()
            .write_all(format!("{start}\n").as_bytes())
            .await
            .unwrap();
        timeout(Duration::from_secs(15), async {
            while !fixture.root.join("config").exists() {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(fixture.root.join("port").exists());
        let started = std::time::Instant::now();
        if terminate {
            let status = Command::new("/bin/kill")
                .args(["-TERM", &child.id().unwrap().to_string()])
                .status()
                .await
                .unwrap();
            assert!(status.success());
        } else {
            drop(input.take());
        }
        // This must interrupt the pending 15-second channel handshake, yet await slow cleanup.
        let status = timeout(Duration::from_secs(8), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success(), "host must exit gracefully: {status}");
        assert!(started.elapsed() >= Duration::from_secs(2));
        assert!(fixture.root.join("helper-closed").exists());
        assert!(fixture.root.join("reverse-removed").exists());
        assert!(fixture.root.join("directory-removed").exists());
        assert!(!options.socket_dir.join("video.sock").exists());
        assert!(!options.socket_dir.join("control.sock").exists());
        let helper_pid = fs::read_to_string(fixture.root.join("helper-pid")).unwrap();
        let alive = Command::new("/bin/kill")
            .args(["-0", helper_pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .unwrap();
        assert!(!alive.success(), "helper must be reaped before host exit");
    }
}
