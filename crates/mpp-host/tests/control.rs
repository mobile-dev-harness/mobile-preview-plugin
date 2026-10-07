#![cfg(unix)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use mpp_android::Android;
use mpp_host::Host;
use serde_json::{Value, json};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "mpp host test {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("state"), "device").unwrap();
        std::fs::write(root.join("serial"), "usb-1").unwrap();
        std::fs::write(root.join("transport"), "1").unwrap();
        std::fs::write(root.join("avd"), "Original_AVD").unwrap();
        let adb = root.join("adb");
        std::fs::write(
            &adb,
            r##"#!/bin/sh
root=${0%/*}
state=$(cat "$root/state")
serial=$(cat "$root/serial")
transport=$(cat "$root/transport")
case "$*" in
  "devices -l")
    printf 'List of devices attached\n'
    if [ "$state" != gone ]; then printf '%s %s model:Framework_Test transport_id:%s\n' "$serial" "$state" "$transport"; fi
    ;;
  "-t $transport get-state"|"-s $serial get-state")
    if [ -f "$root/delay" ]; then printf 'entered' > "$root/entered"; exec sleep 60; fi
    printf '%s\n' "$state" ;;
  "-t $transport emu avd name"|"-s $serial emu avd name") cat "$root/avd"; printf '\nOK\n' ;;
  *) exit 9 ;;
esac
"##,
        )
        .unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { root }
    }

    fn host(&self) -> Host {
        Host::new(Android::with_tools(self.root.join("adb"), None))
    }

    fn state(&self, state: &str) {
        std::fs::write(self.root.join("state"), state).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn call(host: &mut Host, method: &str, params: Value) -> Value {
    serde_json::to_value(
        host.request(
            &serde_json::to_vec(&json!({"id": 7, "method": method, "params": params})).unwrap(),
        )
        .await,
    )
    .unwrap()
}

fn lease(session: &Value, owner: &str) -> Value {
    json!({"owner": owner, "session": session["id"], "generation": session["generation"]})
}

#[tokio::test]
async fn ownership_generation_and_unimplemented_capabilities_are_enforced() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let connected = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a", "device":"android:usb-1"}),
    )
    .await;
    assert_eq!(connected["ok"], true, "{connected}");
    let session = &connected["result"];
    assert_eq!(session["state"], "transport_ready");
    assert_eq!(session["device"]["capabilities"]["video"], false);
    assert_eq!(session["device"]["capabilities"]["input"], false);

    let busy = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-b", "device":"android:usb-1"}),
    )
    .await;
    assert_eq!(busy["error"]["code"], "BUSY");
    let wrong_owner = call(&mut host, "session.status", lease(session, "chat-b")).await;
    assert_eq!(wrong_owner["error"]["code"], "STALE_SESSION");
    let preview = call(&mut host, "preview.start", lease(session, "chat-a")).await;
    assert_eq!(preview["error"]["code"], "UNSUPPORTED");
    let mut input = lease(session, "chat-a");
    input["event"] =
        json!({"kind":"touch","phase":"down","x":0.5,"y":0.5,"width":1080,"height":1920});
    let control = call(&mut host, "input.send", input.clone()).await;
    assert_eq!(control["error"]["code"], "UNSUPPORTED");
    input["event"]["x"] = json!(2);
    assert_eq!(
        call(&mut host, "input.send", input).await["error"]["code"],
        "INVALID_ARGUMENT"
    );

    assert_eq!(
        call(&mut host, "session.disconnect", lease(session, "chat-a")).await["ok"],
        true
    );
    let new = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a", "device":"android:usb-1"}),
    )
    .await;
    assert_eq!(new["ok"], true);
    assert_ne!(new["result"]["generation"], session["generation"]);
    assert_eq!(
        call(&mut host, "session.disconnect", lease(session, "chat-a")).await["error"]["code"],
        "STALE_SESSION"
    );
    assert_eq!(
        call(&mut host, "session.status", lease(&new["result"], "chat-a")).await["ok"],
        true
    );
}

#[tokio::test]
async fn failed_probe_and_disappeared_device_release_reservations() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    fixture.state("unauthorized");
    let denied = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a","device":"android:usb-1"}),
    )
    .await;
    assert_eq!(denied["error"]["code"], "PERMISSION_DENIED");
    fixture.state("device");
    let connected = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-b","device":"android:usb-1"}),
    )
    .await;
    assert_eq!(connected["ok"], true);
    fixture.state("gone");
    assert_eq!(
        call(
            &mut host,
            "session.status",
            lease(&connected["result"], "chat-b")
        )
        .await["error"]["code"],
        "NOT_FOUND"
    );
    fixture.state("device");
    assert_eq!(
        call(
            &mut host,
            "session.connect",
            json!({"owner":"chat-a","device":"android:usb-1"})
        )
        .await["ok"],
        true
    );
}

#[tokio::test]
async fn stdio_recovers_from_bad_requests_and_releases_leases_on_eof() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let connected = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a","device":"android:usb-1"}),
    )
    .await;
    let mut lines = b"not json\n".to_vec();
    lines.extend(std::iter::repeat_n(b'x', mpp_host::MAX_REQUEST_BYTES + 1));
    lines.extend_from_slice(b"\n{\"id\":\"last\",\"method\":\"hello\"}\n");
    let mut output = Vec::new();
    mpp_host::serve(
        &mut host,
        tokio::io::BufReader::with_capacity(17, lines.as_slice()),
        &mut output,
    )
    .await
    .unwrap();
    let responses: Vec<Value> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["ok"], false);
    assert_eq!(responses[1]["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(responses[2]["id"], "last");
    assert_eq!(responses[2]["ok"], true);
    assert_eq!(
        call(
            &mut host,
            "session.status",
            lease(&connected["result"], "chat-a")
        )
        .await["error"]["code"],
        "STALE_SESSION"
    );
}

#[tokio::test]
async fn boot_requires_consent_and_unknown_parameters_are_not_ignored() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    assert_eq!(
        call(
            &mut host,
            "emulator.start",
            json!({"avd":"Any","consent":false})
        )
        .await["error"]["code"],
        "PERMISSION_DENIED"
    );
    assert_eq!(
        call(&mut host, "hello", json!({"typo":true})).await["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        call(&mut host, "does.not.exist", json!({})).await["error"]["code"],
        "UNSUPPORTED"
    );
}

#[tokio::test]
async fn cancelling_connect_does_not_strand_a_lease() {
    use std::time::Duration;
    let fixture = Fixture::new();
    let mut host = fixture.host();
    std::fs::write(fixture.root.join("delay"), "").unwrap();
    {
        let request = call(
            &mut host,
            "session.connect",
            json!({"owner":"chat-a","device":"android:usb-1"}),
        );
        tokio::pin!(request);
        tokio::time::timeout(Duration::from_secs(15), async {
            tokio::select! {
                response = &mut request => panic!("probe did not stall: {response}"),
                _ = async {
                    while !fixture.root.join("entered").exists() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                } => {}
            }
        })
        .await
        .expect("probe must start before cancelling");
    }
    std::fs::remove_file(fixture.root.join("delay")).unwrap();
    let connected = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a","device":"android:usb-1"}),
    )
    .await;
    assert_eq!(connected["ok"], true, "{connected}");
}

#[tokio::test]
async fn recycled_serial_cannot_keep_the_previous_device_lease() {
    let fixture = Fixture::new();
    std::fs::write(fixture.root.join("serial"), "emulator-5554").unwrap();
    let mut host = fixture.host();
    let connected = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-a","device":"android:emulator-5554"}),
    )
    .await;
    assert_eq!(connected["ok"], true, "{connected}");
    assert_eq!(connected["result"]["device"]["transport_id"], "1");
    std::fs::write(fixture.root.join("transport"), "2").unwrap();
    std::fs::write(fixture.root.join("avd"), "Replacement_AVD").unwrap();
    let stale = call(
        &mut host,
        "session.status",
        lease(&connected["result"], "chat-a"),
    )
    .await;
    assert_eq!(stale["error"]["code"], "STALE_SESSION", "{stale}");
    let replacement = call(
        &mut host,
        "session.connect",
        json!({"owner":"chat-b","device":"android:emulator-5554"}),
    )
    .await;
    assert_eq!(replacement["ok"], true, "{replacement}");
    assert_eq!(replacement["result"]["device"]["transport_id"], "2");
    assert_eq!(replacement["result"]["device"]["avd"], "Replacement_AVD");
}
