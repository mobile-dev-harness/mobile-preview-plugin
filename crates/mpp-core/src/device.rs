use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Android,
    Ios,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Emulator,
    Physical,
    Simulator,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Online,
    Offline,
    Unauthorized,
    Stopped,
    Unknown,
}

/// Advertise only capabilities the active backend can actually provide.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capabilities {
    pub video: bool,
    pub input: bool,
    pub screenshot: bool,
    pub lifecycle: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Device {
    pub id: String,
    pub platform: Platform,
    pub kind: DeviceKind,
    pub state: DeviceState,
    pub name: String,
    pub serial: Option<String>,
    /// Identifies an attachment instance, not just a reusable device address.
    pub transport_id: Option<String>,
    pub avd: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Inventory {
    pub devices: Vec<Device>,
    pub warnings: Vec<String>,
}
