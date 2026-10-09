#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use mpp_android::Android;
use mpp_core::media::{Packet, PacketBody, encode};
use mpp_host::Host;
use mpp_ios::Ios;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    time::{sleep, timeout},
};

const UDID: &str = "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "mpp-hi-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("state"), "Booted").unwrap();
        fs::write(root.join("boot-id"), "boot-1").unwrap();
        for (name, script) in [
            (
                "xcrun",
                r##"#!/bin/sh
set -eu
cd "${0%/*}"
case "$*" in
  'simctl list devices available --json')
    state=$(cat state)
    printf '{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-18-0":[{"udid":"AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE","name":"Test iPhone","state":"%s","isAvailable":true}]}}\n' "$state" ;;
  'simctl boot AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE')
    printf 'Booted' > state
    touch booted ;;
  'simctl bootstatus AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE -b') : ;;
  *) exit 9 ;;
esac
"##,
            ),
            (
                "capture",
                r##"#!/bin/sh
set -eu
cd "${0%/*}"
case "$1" in
  --ios-probe)
    if [ -f input ]; then
      printf '{"boot_id":"%s","input":%s}\n' "$(cat boot-id)" "$(cat input)"
    else
      printf '{"boot_id":"%s"}\n' "$(cat boot-id)"
    fi ;;
  --ios-capture)
    case "$*" in *'--control-fd 0'|*'--control-fd 0 --enable-input') : ;; *) exit 8 ;; esac
    printf '%s\n' "$*" > capture-args
    printf '{"width":720,"height":1280,"display_width":1170,"display_height":2532,"rotation":0}\n'
    cat media
    while IFS= read -r command; do
      printf '%s\n' "$command" >> controls
      seq=$(printf '%s' "$command" | sed -n 's/.*"seq":\([0-9][0-9]*\).*/\1/p')
      printf '{"seq":%s,"ok":true,"code":null,"message":null}\n' "$seq" >&0
      case "$command" in *'"kind":"stop"'*) break ;; esac
    done
    touch closed ;;
  *) exit 9 ;;
esac
"##,
            ),
            ("adb", "#!/bin/sh\nexit 9\n"),
        ] {
            fs::write(root.join(name), script).unwrap();
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { root }
    }

    fn ios(&self) -> Ios {
        Ios::with_tools(self.root.join("xcrun"), self.root.join("capture"))
    }

    fn host(&self) -> Host {
        Host::with_backends(None, Some(self.ios()))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn call(host: &mut Host, method: &str, params: Value) -> Value {
    serde_json::to_value(
        host.request(
            &serde_json::to_vec(&json!({
                "id": 1, "method": method, "params": params,
            }))
            .unwrap(),
        )
        .await,
    )
    .unwrap()
}

async fn connect(host: &mut Host) -> Value {
    let response = call(
        host,
        "session.connect",
        json!({
            "owner":"chat-ios", "device":format!("ios:{UDID}"),
        }),
    )
    .await;
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}

fn lease(session: &Value) -> Value {
    json!({"owner":"chat-ios", "session":session["id"], "generation":session["generation"]})
}

#[tokio::test]
async fn discovery_keeps_ios_when_android_fails_and_filters_by_platform() {
    let fixture = Fixture::new();
    let mut host = Host::with_backends(
        Some(Android::with_tools(fixture.root.join("adb"), None)),
        Some(fixture.ios()),
    );
    let all = call(&mut host, "devices.list", json!({})).await;
    assert_eq!(all["ok"], true, "{all}");
    assert_eq!(all["result"]["devices"].as_array().unwrap().len(), 1);
    assert_eq!(all["result"]["devices"][0]["platform"], "ios");
    assert_eq!(all["result"]["devices"][0]["capabilities"]["input"], false);
    assert!(
        all["result"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .unwrap()
                    .contains("Android discovery failed")
            })
    );
    let unavailable = call(
        &mut host,
        "session.connect",
        json!({
            "owner":"chat-android", "device":"android:phone",
        }),
    )
    .await;
    assert_eq!(unavailable["error"]["code"], "COMMAND_FAILED");
    let ios = call(&mut host, "devices.list", json!({"platform":"ios"})).await;
    assert_eq!(ios["result"]["warnings"], json!([]));
    assert_eq!(
        call(&mut host, "devices.list", json!({"platform":"watchos"})).await["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let hello = call(&mut host, "hello", json!({})).await;
    assert!(
        hello["result"]["methods"]
            .as_array()
            .unwrap()
            .contains(&json!("simulator.start"))
    );
    host.shutdown().await;
}

#[tokio::test]
async fn simulator_boot_requires_consent_and_session_is_invalid_after_reboot() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    fs::write(fixture.root.join("state"), "Shutdown").unwrap();
    assert_eq!(
        call(
            &mut host,
            "simulator.start",
            json!({"udid":UDID,"consent":false})
        )
        .await["error"]["code"],
        "PERMISSION_DENIED"
    );
    assert!(!fixture.root.join("booted").exists());
    assert_eq!(
        call(
            &mut host,
            "simulator.start",
            json!({"udid":UDID,"consent":true})
        )
        .await["ok"],
        true
    );
    assert!(fixture.root.join("booted").exists());
    let session = connect(&mut host).await;
    assert_eq!(session["device"]["transport_id"], "boot-1");
    assert_eq!(session["device"]["capabilities"]["input"], false);
    let mut input = lease(&session);
    input["event"] =
        json!({"kind":"touch","phase":"down","x":0.5,"y":0.5,"width":720,"height":1280});
    let direct_input = call(&mut host, "input.send", input).await;
    assert_eq!(direct_input["error"]["code"], "UNSUPPORTED");
    assert!(
        direct_input["error"]["message"]
            .as_str()
            .unwrap()
            .contains("active preview control socket")
    );
    fs::write(fixture.root.join("boot-id"), "boot-2").unwrap();
    assert_eq!(
        call(&mut host, "session.status", lease(&session)).await["error"]["code"],
        "STALE_SESSION"
    );
    let reconnected = connect(&mut host).await;
    assert_ne!(session["generation"], reconnected["generation"]);
    assert_eq!(reconnected["device"]["transport_id"], "boot-2");
    host.shutdown().await;
}

#[tokio::test]
async fn ios_preview_relays_media_and_control_without_android_assets() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("input"), "true").unwrap();
    let mut host = fixture.host();
    let session = connect(&mut host).await;
    assert_eq!(session["device"]["capabilities"]["input"], true);
    let packet = encode(
        &Packet {
            generation: session["generation"].as_u64().unwrap(),
            pts_us: 0,
            body: PacketBody::Configuration {
                width: 720,
                height: 1280,
                annex_b: vec![0, 0, 0, 1, 0x67, 0x42, 0, 0x1e],
            },
        },
        1024,
    )
    .unwrap();
    fs::write(fixture.root.join("media"), &packet).unwrap();
    let socket_dir = fixture.root.join("sockets");
    fs::create_dir(&socket_dir).unwrap();
    fs::set_permissions(&socket_dir, fs::Permissions::from_mode(0o700)).unwrap();
    let mut params = lease(&session);
    params["socket_dir"] = json!(socket_dir);
    params["stream_id"] = json!("a".repeat(32));
    params["token"] = json!("b".repeat(64));
    let started = call(&mut host, "preview.start", params).await;
    assert_eq!(started["ok"], true, "{started}");
    assert!(
        fs::read_to_string(fixture.root.join("capture-args"))
            .unwrap()
            .trim()
            .ends_with("--control-fd 0 --enable-input")
    );
    let descriptor = &started["result"];
    assert_eq!(descriptor.as_object().unwrap().len(), 6);
    let mut video = UnixStream::connect(descriptor["video_socket"].as_str().unwrap())
        .await
        .unwrap();
    let mut control = BufReader::new(
        UnixStream::connect(descriptor["control_socket"].as_str().unwrap())
            .await
            .unwrap(),
    );
    let mut received = vec![0; packet.len()];
    timeout(Duration::from_secs(3), video.read_exact(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, packet);
    let mut requests = Vec::new();
    for (index, command) in [json!({"kind":"heartbeat"}), json!({"kind":"key_frame"}),
        json!({"kind":"input","event":{"kind":"touch","phase":"down","x":0.5,"y":0.5,"width":720,"height":1280}}),
        json!({"kind":"input","event":{"kind":"touch","phase":"up","x":0.5,"y":0.5,"width":720,"height":1280}}),
        json!({"kind":"input","event":{"kind":"key","code":3,"phase":"down"}}),
        json!({"kind":"input","event":{"kind":"key","code":3,"phase":"up"}}),
        json!({"kind":"stop"}),
    ].into_iter().enumerate() {
        let request = json!({"seq":index + 1, "epoch":descriptor["epoch"], "command":command});
        requests.push(request.clone());
        control.get_mut().write_all(format!("{request}\n").as_bytes()).await.unwrap();
        let mut line = String::new();
        timeout(Duration::from_secs(3), control.read_line(&mut line)).await.unwrap().unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["seq"], index + 1);
        assert_eq!(reply["ok"], true, "{reply}");
    }
    timeout(Duration::from_secs(5), async {
        while socket_dir.join("video.sock").exists() || !fixture.root.join("closed").exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let controls = fs::read_to_string(fixture.root.join("controls")).unwrap();
    let forwarded: Vec<Value> = controls
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(forwarded, requests);
    assert_eq!(
        call(&mut host, "session.status", lease(&session)).await["ok"],
        true
    );
    host.shutdown().await;
}

#[tokio::test]
async fn malformed_internal_capture_arguments_never_write_json_into_media_stdout() {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_mpp"))
        .args(["--ios-capture", UDID])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[tokio::test]
async fn stdio_lists_simulators_even_when_android_tool_is_unavailable() {
    use std::process::Stdio;

    let fixture = Fixture::new();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_mpp"))
        .arg("--adb")
        .arg(fixture.root.join("absent-adb"))
        .arg("--xcrun")
        .arg(fixture.root.join("xcrun"))
        .args(["serve", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(b"{\"id\":1,\"method\":\"devices.list\",\"params\":{\"platform\":\"ios\"}}\n")
        .await
        .unwrap();
    drop(stdin);
    let output = timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        response["result"]["devices"][0]["id"],
        format!("ios:{UDID}")
    );
    assert!(output.stderr.is_empty());
}
