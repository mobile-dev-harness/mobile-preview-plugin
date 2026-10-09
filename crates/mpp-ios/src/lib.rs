//! iOS Simulator preview, native input, and explicit simulator lifecycle.

pub mod capture;
mod parse;
mod process;
mod stream;

use std::{path::PathBuf, sync::Arc, time::Duration};

use mpp_core::{Device, DeviceState, Error, Inventory, Result};
use serde::{Deserialize, Serialize};

pub use parse::devices as parse_devices;
pub use stream::{RunningStream, StreamOptions};

const BOOT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfig {
    pub udid: String,
    pub boot_id: String,
    pub generation: u64,
    pub epoch: u64,
    pub max_size: u32,
    pub bit_rate: u32,
    pub max_fps: u32,
    #[serde(default)]
    pub control_fd: Option<u32>,
    #[serde(default)]
    pub input_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProbe {
    pub boot_id: String,
    #[serde(default)]
    pub input: bool,
}

#[derive(Clone, Debug)]
pub struct Ios {
    xcrun: PathBuf,
    capture_executable: PathBuf,
    stream_starts: Arc<stream::StreamStarts>,
}

impl Ios {
    pub fn discover() -> Result<Self> {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unsupported {
                feature: "iOS Simulator preview requires macOS and Xcode".into(),
            });
        }
        let xcrun = PathBuf::from("/usr/bin/xcrun");
        if !xcrun.is_file() {
            return Err(Error::ToolNotFound {
                tool: "Xcode xcrun".into(),
            });
        }
        Ok(Self::with_tools(xcrun, std::env::current_exe()?))
    }

    pub fn with_tools(xcrun: PathBuf, capture_executable: PathBuf) -> Self {
        Self {
            xcrun,
            capture_executable,
            stream_starts: Arc::default(),
        }
    }

    pub async fn inventory(&self) -> Result<Inventory> {
        let output = process::run(
            &self.xcrun,
            &["simctl", "list", "devices", "available", "--json"],
        )
        .await?;
        parse::devices(&output)
    }

    /// The simulator UUID is reusable; attachment identity must identify this boot.
    pub async fn probe(&self, udid: &str) -> Result<Device> {
        validate_udid(udid)?;
        let mut device = self
            .inventory()
            .await?
            .devices
            .into_iter()
            .find(|device| device.serial.as_deref() == Some(udid))
            .ok_or_else(|| Error::NotFound {
                what: format!("iOS Simulator {udid}"),
            })?;
        if device.state != DeviceState::Online {
            return Err(Error::CommandFailed {
                tool: "simctl".into(),
                message: format!(
                    "Simulator {udid} is {:?}; boot it explicitly before connecting",
                    device.state
                ),
            });
        }
        let output = process::run(&self.capture_executable, &["--ios-probe", udid]).await?;
        let probe: NativeProbe =
            serde_json::from_str(&output).map_err(|_| Error::CommandFailed {
                tool: "iOS preview".into(),
                message: "Capture probe returned invalid boot identity JSON".into(),
            })?;
        if probe.boot_id.is_empty()
            || probe.boot_id.len() > 256
            || probe
                .boot_id
                .chars()
                .any(|value| value.is_control() || value.is_whitespace())
            || probe.boot_id.eq_ignore_ascii_case(udid)
        {
            return Err(Error::CommandFailed {
                tool: "iOS preview".into(),
                message: "Capture probe did not identify the current simulator boot".into(),
            });
        }
        device.transport_id = Some(probe.boot_id);
        device.capabilities.input = probe.input;
        Ok(device)
    }

    /// Call only after the user has explicitly chosen this simulator and authorized its boot.
    pub async fn boot(&self, udid: &str) -> Result<Device> {
        validate_udid(udid)?;
        let device = self
            .inventory()
            .await?
            .devices
            .into_iter()
            .find(|device| device.serial.as_deref() == Some(udid))
            .ok_or_else(|| Error::NotFound {
                what: format!("iOS Simulator {udid}"),
            })?;
        match device.state {
            DeviceState::Online => return self.probe(udid).await,
            DeviceState::Stopped => {}
            _ => {
                return Err(Error::CommandFailed {
                    tool: "simctl".into(),
                    message: format!(
                        "Simulator {udid} is transitioning; refresh discovery before booting it"
                    ),
                });
            }
        }
        process::run(&self.xcrun, &["simctl", "boot", udid]).await?;
        process::run_with_timeout(
            &self.xcrun,
            &["simctl", "bootstatus", udid, "-b"],
            BOOT_TIMEOUT,
        )
        .await?;
        // Failure or disconnect never shuts down a simulator owned by the user.
        self.probe(udid).await
    }
}

pub(crate) fn validate_udid(udid: &str) -> Result<()> {
    let valid = udid.len() == 36
        && udid.bytes().enumerate().all(|(index, value)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                value == b'-'
            } else {
                value.is_ascii_hexdigit()
            }
        });
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidArgument {
            message: "Simulator UDID must be an exact UUID returned by device discovery".into(),
        })
    }
}
