//! Android discovery and explicit emulator lifecycle. Media and input are negotiated separately.

mod parse;
mod process;
mod sdk;
mod stream;

use std::{
    collections::HashSet,
    net::{Ipv4Addr, TcpListener},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use mpp_core::{Capabilities, Device, DeviceKind, DeviceState, Error, Inventory, Platform, Result};
use tokio::{
    process::{Child, Command},
    time::{sleep, timeout},
};

pub use parse::{avds as parse_avds, devices as parse_devices};
pub use stream::{RunningStream, StreamOptions};

const BOOT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug)]
pub struct Android {
    adb: PathBuf,
    emulator: Option<PathBuf>,
    stream_starts: Arc<stream::StreamStarts>,
}

impl Android {
    pub fn discover() -> Result<Self> {
        let (adb, emulator) = sdk::discover()?;
        Ok(Self::with_tools(adb, emulator))
    }

    pub fn with_tools(adb: PathBuf, emulator: Option<PathBuf>) -> Self {
        Self {
            adb,
            emulator,
            stream_starts: Arc::default(),
        }
    }

    pub fn with_emulator(mut self, emulator: PathBuf) -> Self {
        self.emulator = Some(emulator);
        self
    }

    pub async fn inventory(&self) -> Result<Inventory> {
        let mut inventory = self.connected().await?;
        for device in &mut inventory.devices {
            if device.kind != DeviceKind::Emulator || device.state != DeviceState::Online {
                continue;
            }
            let serial = device
                .serial
                .as_deref()
                .expect("connected devices have a serial");
            let Some(transport_id) = device.transport_id.as_deref() else {
                continue;
            };
            match process::run(&self.adb, &["-t", transport_id, "emu", "avd", "name"]).await {
                Ok(output) => match parse::avd_name(&output) {
                    Some(name) => {
                        device.name = name.clone();
                        device.avd = Some(name);
                    }
                    None => inventory.warnings.push(format!(
                        "Could not identify AVD for {serial}: unexpected emulator console response"
                    )),
                },
                Err(error) => inventory
                    .warnings
                    .push(format!("Could not identify AVD for {serial}: {error}")),
            }
        }
        let Some(emulator) = &self.emulator else {
            inventory.warnings.push("Android emulator tool not found; only connected devices are listed. Install the SDK emulator package to list or boot AVDs.".into());
            return Ok(inventory);
        };
        match process::run(emulator, &["-list-avds"]).await {
            Ok(output) => {
                let running: HashSet<_> = inventory
                    .devices
                    .iter()
                    .filter_map(|device| device.avd.clone())
                    .collect();
                inventory.devices.extend(
                    parse::avds(&output)
                        .into_iter()
                        .filter(|avd| !running.contains(avd))
                        .map(stopped_avd),
                );
            }
            Err(error) => inventory
                .warnings
                .push(format!("AVD listing failed: {error}")),
        }
        Ok(inventory)
    }

    pub async fn probe(&self, serial: &str) -> Result<Device> {
        validate_serial(serial)?;
        let mut device = self
            .connected()
            .await?
            .devices
            .into_iter()
            .find(|device| device.serial.as_deref() == Some(serial))
            .ok_or_else(|| Error::NotFound {
                what: format!("Android device {serial}"),
            })?;
        match device.state {
            DeviceState::Online => {
                let transport_id = device.transport_id.as_deref().ok_or_else(|| Error::CommandFailed {
                    tool: "adb".into(),
                    message: format!("Device {serial} has no valid adb transport ID; refresh discovery before attaching"),
                })?;
                let state = process::run(&self.adb, &["-t", transport_id, "get-state"]).await?;
                if state.trim() != "device" {
                    return Err(Error::CommandFailed {
                        tool: "adb".into(),
                        message: format!(
                            "Device {serial} is no longer online; reconnect it and retry"
                        ),
                    });
                }
                if device.kind == DeviceKind::Emulator {
                    let output =
                        process::run(&self.adb, &["-t", transport_id, "emu", "avd", "name"])
                            .await?;
                    let avd = parse::avd_name(&output).ok_or_else(|| Error::CommandFailed {
                        tool: "adb".into(),
                        message: format!("Could not identify AVD for {serial} on transport {transport_id}; refresh discovery before attaching"),
                    })?;
                    device.name = avd.clone();
                    device.avd = Some(avd);
                }
                Ok(device)
            }
            DeviceState::Unauthorized => Err(Error::PermissionDenied {
                message: format!(
                    "Device {serial} is unauthorized; unlock it and accept the debugging prompt, or check host USB permissions"
                ),
            }),
            state => Err(Error::CommandFailed {
                tool: "adb".into(),
                message: format!("Device {serial} is {state:?}; reconnect it and retry"),
            }),
        }
    }

    /// Boot only the AVD explicitly chosen by the user, leaving unrelated devices alone.
    pub async fn boot(&self, avd: &str) -> Result<Device> {
        if avd.is_empty()
            || avd.starts_with('-')
            || avd.chars().any(char::is_whitespace)
            || avd.chars().any(char::is_control)
        {
            return Err(Error::InvalidArgument {
                message: "AVD name must be an exact name returned by device discovery".into(),
            });
        }
        let emulator = self.emulator.as_ref().ok_or_else(|| Error::ToolNotFound {
            tool: "Android SDK emulator".into(),
        })?;
        let names = process::run(emulator, &["-list-avds"]).await?;
        if !parse::avds(&names).iter().any(|name| name == avd) {
            return Err(Error::NotFound {
                what: format!("Android AVD {avd}"),
            });
        }
        let inventory = self.inventory().await?;
        if let Some(device) = inventory.devices.iter().find(|device| {
            device.avd.as_deref() == Some(avd) && device.state == DeviceState::Online
        }) {
            let serial = device
                .serial
                .as_deref()
                .expect("online emulator has a serial");
            return timeout(BOOT_TIMEOUT, self.wait_for_boot(None, serial, avd))
                .await
                .map_err(|_| Error::Timeout {
                    operation: format!("waiting for Android AVD {avd} to finish booting"),
                })?;
        }
        if inventory.devices.iter().any(|device| {
            device.kind == DeviceKind::Emulator && device.serial.is_some() && device.avd.is_none()
        }) {
            return Err(Error::CommandFailed { tool: "adb".into(), message: "An existing emulator could not be identified. Reconnect it before starting another AVD.".into() });
        }
        let (port, reservations) = reserve_port_pair(&inventory)?;
        let serial = format!("emulator-{port}");
        // Emulator must bind these ports itself. Occupied/changed ports cause an exact-serial boot failure.
        drop(reservations);
        let child = Command::new(emulator)
            .args([
                "-avd",
                avd,
                "-port",
                &port.to_string(),
                "-no-window",
                "-no-audio",
                "-no-snapshot-save",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false)
            .spawn()
            .map_err(|error| process::spawn_error(&emulator.display().to_string(), error))?;
        let mut boot = BootProcess {
            child,
            detached: false,
        };
        let result = timeout(
            BOOT_TIMEOUT,
            self.wait_for_boot(Some(&mut boot.child), &serial, avd),
        )
        .await;
        match result {
            Ok(Ok(device)) => {
                boot.detached = true;
                Ok(device)
            }
            Ok(Err(error)) => {
                let _ = boot.child.kill().await;
                Err(error)
            }
            Err(_) => {
                let _ = boot.child.kill().await;
                Err(Error::Timeout {
                    operation: format!("booting Android AVD {avd}"),
                })
            }
        }
    }

    async fn wait_for_boot(
        &self,
        mut child: Option<&mut Child>,
        serial: &str,
        avd: &str,
    ) -> Result<Device> {
        loop {
            if let Some(child) = child.as_mut()
                && let Some(status) = child.try_wait()?
            {
                return Err(Error::CommandFailed {
                    tool: "emulator".into(),
                    message: format!(
                        "AVD {avd} exited before boot completed ({status}); check its SDK image and hardware acceleration"
                    ),
                });
            }
            if let Ok(state) = process::run(&self.adb, &["-s", serial, "get-state"]).await
                && state.trim() == "device"
                && let Ok(completed) = process::run(
                    &self.adb,
                    &["-s", serial, "shell", "getprop", "sys.boot_completed"],
                )
                .await
                && completed.trim() == "1"
            {
                let name = process::run(&self.adb, &["-s", serial, "emu", "avd", "name"]).await?;
                if parse::avd_name(&name).as_deref() != Some(avd) {
                    return Err(Error::CommandFailed {
                        tool: "emulator".into(),
                        message: format!(
                            "Port for {serial} belongs to a different AVD; retry discovery"
                        ),
                    });
                }
                let device = self.probe(serial).await?;
                if device.avd.as_deref() != Some(avd) {
                    return Err(Error::CommandFailed {
                        tool: "emulator".into(),
                        message: format!(
                            "Device {serial} changed AVD during boot; retry discovery"
                        ),
                    });
                }
                return Ok(device);
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    async fn connected(&self) -> Result<Inventory> {
        let output = process::run(&self.adb, &["devices", "-l"]).await?;
        Ok(parse::devices(&output))
    }
}

struct BootProcess {
    child: Child,
    detached: bool,
}

impl Drop for BootProcess {
    fn drop(&mut self) {
        if !self.detached {
            let _ = self.child.start_kill();
        }
    }
}

fn validate_serial(serial: &str) -> Result<()> {
    if serial.is_empty()
        || serial.len() > 256
        || serial.starts_with('-')
        || serial
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(Error::InvalidArgument {
            message: "Android serial must be an exact, nonempty serial from device discovery"
                .into(),
        });
    }
    Ok(())
}

fn stopped_avd(avd: String) -> Device {
    Device {
        id: format!("android-avd:{avd}"),
        platform: Platform::Android,
        kind: DeviceKind::Emulator,
        state: DeviceState::Stopped,
        name: avd.clone(),
        serial: None,
        transport_id: None,
        avd: Some(avd),
        capabilities: Capabilities {
            lifecycle: true,
            ..Capabilities::default()
        },
    }
}

fn reserve_port_pair(inventory: &Inventory) -> Result<(u16, [TcpListener; 2])> {
    let occupied: HashSet<_> = inventory
        .devices
        .iter()
        .filter_map(|device| device.serial.as_deref())
        .collect();
    for port in (5554..=5584).step_by(2) {
        if occupied.contains(format!("emulator-{port}").as_str()) {
            continue;
        }
        if let Ok(console) = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            && let Ok(adb) = TcpListener::bind((Ipv4Addr::LOCALHOST, port + 1))
        {
            return Ok((port, [console, adb]));
        }
    }
    Err(Error::CommandFailed {
        tool: "emulator".into(),
        message: "No free Android emulator console/ADB port pair is available".into(),
    })
}
