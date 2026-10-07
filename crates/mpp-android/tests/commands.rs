#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use mpp_android::Android;
use mpp_core::{DeviceState, Error};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "mpp android test {} {}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        Self { directory }
    }

    fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.directory.join(name);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nset -eu\ncd '{}'\n{body}\n",
                self.directory.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[tokio::test]
async fn inventory_keeps_unavailable_devices_and_deduplicates_running_avds() {
    let fixture = Fixture::new();
    let adb = fixture.script("adb", r#"
printf '%s\n' "$*" >> adb.log
case "$*" in
  'devices -l') printf 'List of devices attached\nemulator-5554 device model:Pixel_9 transport_id:1\nusb offline transport_id:2\nphone unauthorized transport_id:3\n' ;;
  '-t 1 emu avd name') printf 'Pixel_9\nOK\n' ;;
  *) exit 9 ;;
esac"#);
    let emulator = fixture.script(
        "emulator",
        "[ \"$1\" = '-list-avds' ]\nprintf 'INFO | emulator version\nPixel_9\nTablet_API_32\n'",
    );
    let inventory = Android::with_tools(adb, Some(emulator))
        .inventory()
        .await
        .unwrap();
    assert!(inventory.warnings.is_empty());
    assert_eq!(inventory.devices.len(), 4);
    assert_eq!(inventory.devices[0].avd.as_deref(), Some("Pixel_9"));
    assert_eq!(inventory.devices[3].id, "android-avd:Tablet_API_32");
    assert_eq!(inventory.devices[3].state, DeviceState::Stopped);
    assert!(
        inventory
            .devices
            .iter()
            .all(|device| !device.capabilities.video
                && !device.capabilities.input
                && !device.capabilities.screenshot)
    );
    assert_eq!(
        fs::read_to_string(fixture.directory.join("adb.log")).unwrap(),
        "devices -l\n-t 1 emu avd name\n"
    );
}

#[tokio::test]
async fn probe_requires_the_exact_serial_and_online_state() {
    let fixture = Fixture::new();
    let adb = fixture.script("adb", "if [ \"$*\" = '-t 1 get-state' ]; then echo device; exit 0; fi\n[ \"$*\" = 'devices -l' ]\nprintf 'List of devices attached\nphone device model:Pixel_8 transport_id:1\nphone2 offline transport_id:2\nlocked unauthorized transport_id:3\n'");
    let android = Android::with_tools(adb, None);
    assert_eq!(
        android.probe("phone").await.unwrap().serial.as_deref(),
        Some("phone")
    );
    assert!(matches!(
        android.probe("phone2").await,
        Err(Error::CommandFailed { .. })
    ));
    assert!(matches!(
        android.probe("locked").await,
        Err(Error::PermissionDenied { .. })
    ));
    assert!(matches!(
        android.probe("phon").await,
        Err(Error::NotFound { .. })
    ));
    for invalid in ["", "-s", "phone\nsecond"] {
        assert!(matches!(
            android.probe(invalid).await,
            Err(Error::InvalidArgument { .. })
        ));
    }
}

#[tokio::test]
async fn probe_rechecks_transport_after_online_listing() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        r#"
case "$*" in
  'devices -l') printf 'List of devices attached\nphone device transport_id:1\n' ;;
  '-t 1 get-state') echo offline ;;
  *) exit 9 ;;
esac"#,
    );
    assert!(matches!(
        Android::with_tools(adb, None).probe("phone").await,
        Err(Error::CommandFailed { .. })
    ));
}

#[tokio::test]
async fn recycled_serial_is_distinguished_by_transport_identity() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        r#"
printf '%s\n' "$*" >> adb.log
case "$*" in
  'devices -l')
    if [ -f replaced ]; then id=12; else id=11; fi
    printf 'List of devices attached\nemulator-5554 device transport_id:%s\n' "$id" ;;
  '-t 11 get-state'|'-t 12 get-state') echo device ;;
  '-t 11 emu avd name'|'-t 12 emu avd name') printf 'Pixel_9\nOK\n' ;;
  *) exit 9 ;;
esac"#,
    );
    let android = Android::with_tools(adb, None);
    let first = android.probe("emulator-5554").await.unwrap();
    fs::write(fixture.directory.join("replaced"), "").unwrap();
    let second = android.probe("emulator-5554").await.unwrap();
    assert_eq!(first.serial, second.serial);
    assert_eq!(first.avd, second.avd);
    assert_eq!(first.transport_id.as_deref(), Some("11"));
    assert_eq!(second.transport_id.as_deref(), Some("12"));
    let log = fs::read_to_string(fixture.directory.join("adb.log")).unwrap();
    assert!(log.contains("-t 11 get-state\n"));
    assert!(log.contains("-t 12 get-state\n"));
    assert!(!log.contains("-s "));
}

#[tokio::test]
async fn transport_replacement_between_listing_and_probe_cannot_fall_back_to_serial() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        r#"
printf '%s\n' "$*" >> adb.log
case "$*" in
  'devices -l') printf 'List of devices attached\nphone device transport_id:11\n' ;;
  '-t 11 get-state') echo 'transport disappeared' >&2; exit 1 ;;
  '-s phone get-state'|'-t 12 get-state') echo device ;;
  *) exit 9 ;;
esac"#,
    );
    assert!(matches!(
        Android::with_tools(adb, None).probe("phone").await,
        Err(Error::CommandFailed { .. })
    ));
    assert_eq!(
        fs::read_to_string(fixture.directory.join("adb.log")).unwrap(),
        "devices -l\n-t 11 get-state\n"
    );
}

#[tokio::test]
async fn probe_requires_transport_identity_and_emulator_avd_identity() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        "[ \"$*\" = 'devices -l' ]\nprintf 'List of devices attached\\nphone device\\n'",
    );
    let error = Android::with_tools(adb, None)
        .probe("phone")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no valid adb transport ID"));
    let adb = fixture.script(
        "adb",
        r#"
case "$*" in
  'devices -l') printf 'List of devices attached\nemulator-5554 device transport_id:11\n' ;;
  '-t 11 get-state') echo device ;;
  '-t 11 emu avd name') echo 'KO: console unavailable' ;;
  *) exit 9 ;;
esac"#,
    );
    let error = Android::with_tools(adb, None)
        .probe("emulator-5554")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Could not identify AVD"));
}

#[tokio::test]
async fn boot_does_not_overwrite_a_replaced_devices_transport_bound_avd() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        r#"
case "$*" in
  'devices -l')
    if [ -f replaced ]; then id=12; else id=11; fi
    printf 'List of devices attached\nemulator-5554 device transport_id:%s\n' "$id" ;;
  '-t 11 emu avd name'|'-s emulator-5554 emu avd name') printf 'Pixel_9\nOK\n' ;;
  '-s emulator-5554 get-state'|'-t 12 get-state') echo device ;;
  '-s emulator-5554 shell getprop sys.boot_completed') touch replaced; echo 1 ;;
  '-t 12 emu avd name') printf 'Different_AVD\nOK\n' ;;
  *) exit 9 ;;
esac"#,
    );
    let emulator = fixture.script("emulator", "[ \"$*\" = '-list-avds' ]\necho Pixel_9");
    let error = Android::with_tools(adb, Some(emulator))
        .boot("Pixel_9")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed AVD during boot"));
}

#[tokio::test]
async fn cancelling_boot_kills_only_the_owned_emulator_process() {
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    let fixture = Fixture::new();
    let adb = fixture.script("adb", "if [ \"$*\" = 'devices -l' ]; then printf 'List of devices attached\\n'; exit 0; fi\nexit 1");
    let emulator = fixture.script("emulator", "if [ \"$*\" = '-list-avds' ]; then echo Chosen_AVD; exit 0; fi\necho $$ > emulator.pid\nexec sleep 60");
    let android = Android::with_tools(adb, Some(emulator));
    let mut task = tokio::spawn(async move { android.boot("Chosen_AVD").await });
    let pid_path = fixture.directory.join("emulator.pid");
    let startup = timeout(Duration::from_secs(30), async {
        tokio::select! {
            result = &mut task => panic!("boot ended before cancellation: {result:?}"),
            () = async {
                while !pid_path.exists() { sleep(Duration::from_millis(10)).await; }
            } => {}
        }
    })
    .await;
    if startup.is_err() {
        task.abort();
    }
    startup.unwrap();
    let pid = fs::read_to_string(pid_path).unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    timeout(Duration::from_secs(5), async {
        loop {
            let status = tokio::process::Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .unwrap();
            if !status.success() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn discovery_failures_are_not_reported_as_an_empty_ready_inventory() {
    let fixture = Fixture::new();
    let adb = fixture.script("adb", "echo 'server failed' >&2\nexit 1");
    assert!(matches!(
        Android::with_tools(adb, None).inventory().await,
        Err(Error::CommandFailed { .. })
    ));
    assert!(matches!(
        Android::with_tools(Path::new("/nonexistent/mpp-adb").into(), None)
            .inventory()
            .await,
        Err(Error::ToolNotFound { .. })
    ));
    let adb = fixture.script("empty-adb", "printf 'List of devices attached\n'");
    let emulator = fixture.script("broken-emulator", "exit 2");
    let inventory = Android::with_tools(adb, Some(emulator))
        .inventory()
        .await
        .unwrap();
    assert!(inventory.devices.is_empty());
    assert_eq!(inventory.warnings.len(), 1);
    assert!(inventory.warnings[0].contains("AVD listing failed"));
}

#[tokio::test]
async fn boot_returns_existing_avd_without_launching_or_selecting_another_one() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        r#"
case "$*" in
  'devices -l') printf 'List of devices attached\nemulator-5554 device transport_id:1\n' ;;
  '-s emulator-5554 emu avd name'|'-t 1 emu avd name') printf 'Pixel_9\nOK\n' ;;
  '-s emulator-5554 get-state'|'-t 1 get-state') echo device ;;
  '-s emulator-5554 shell getprop sys.boot_completed')
    if [ -f polled ]; then echo 1; else touch polled; echo 0; fi ;;
  *) exit 9 ;;
esac"#,
    );
    let emulator = fixture.script("emulator", "[ \"$*\" = '-list-avds' ]\nprintf 'Pixel_9\n'");
    let android = Android::with_tools(adb, Some(emulator));
    assert_eq!(
        android.boot("Pixel_9").await.unwrap().serial.as_deref(),
        Some("emulator-5554")
    );
    assert!(matches!(
        android.boot("Different_AVD").await,
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        android.boot("-wipe-data").await,
        Err(Error::InvalidArgument { .. })
    ));
}

#[tokio::test]
async fn boot_uses_only_the_requested_avd_and_waits_for_its_exact_serial() {
    let fixture = Fixture::new();
    let adb = fixture.script(
        "adb",
        r#"
printf '%s\n' "$*" >> adb.log
if [ "$*" = 'devices -l' ]; then
  printf 'List of devices attached\nusb device transport_id:2\n'
  if [ -f port ]; then printf 'emulator-%s device transport_id:1\n' "$(cat port)"; fi
  exit 0
fi
[ -f port ] || exit 1
if [ "$1" = '-s' ]; then
  [ "$2" = "emulator-$(cat port)" ]
else
  [ "$1" = '-t' ] && [ "$2" = '1' ]
fi
shift 2
case "$*" in
  'get-state') echo device ;;
  'shell getprop sys.boot_completed') echo 1 ;;
  'emu avd name') printf 'Chosen_AVD\nOK\n' ;;
  *) exit 9 ;;
esac"#,
    );
    let emulator = fixture.script(
        "emulator",
        r#"
if [ "$*" = '-list-avds' ]; then printf 'Chosen_AVD\nOther_AVD\n'; exit 0; fi
printf '%s\n' "$@" > emulator.args
[ "$1" = '-avd' ] && [ "$2" = 'Chosen_AVD' ] && [ "$3" = '-port' ]
printf '%s' "$4" > port
exec sleep 2"#,
    );
    let device = Android::with_tools(adb, Some(emulator))
        .boot("Chosen_AVD")
        .await
        .unwrap();
    let port = fs::read_to_string(fixture.directory.join("port")).unwrap();
    assert_eq!(
        device.serial.as_deref(),
        Some(format!("emulator-{port}").as_str())
    );
    assert_eq!(device.avd.as_deref(), Some("Chosen_AVD"));
    assert_eq!(device.transport_id.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(fixture.directory.join("emulator.args")).unwrap(),
        format!("-avd\nChosen_AVD\n-port\n{port}\n-no-window\n-no-audio\n-no-snapshot-save\n")
    );
    assert!(
        fs::read_to_string(fixture.directory.join("adb.log"))
            .unwrap()
            .contains(&format!(
                "-s emulator-{port} shell getprop sys.boot_completed"
            ))
    );
}

#[tokio::test]
async fn boot_failure_does_not_issue_global_device_shutdown() {
    let fixture = Fixture::new();
    let adb = fixture.script("adb", "printf '%s\\n' \"$*\" >> adb.log\n[ \"$*\" = 'devices -l' ] && printf 'List of devices attached\\n'\nexit 0");
    let emulator = fixture.script(
        "emulator",
        "if [ \"$*\" = '-list-avds' ]; then echo Chosen_AVD; exit 0; fi\nexit 42",
    );
    let result = Android::with_tools(adb, Some(emulator))
        .boot("Chosen_AVD")
        .await;
    assert!(matches!(result, Err(Error::CommandFailed { .. })));
    let log = fs::read_to_string(fixture.directory.join("adb.log")).unwrap();
    assert!(!log.contains("kill"));
}
