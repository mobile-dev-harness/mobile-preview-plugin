use std::collections::{BTreeMap, HashSet};

use mpp_core::{Capabilities, Device, DeviceKind, DeviceState, Error, Inventory, Platform, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct Listing {
    devices: BTreeMap<String, Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct Simulator {
    udid: String,
    name: String,
    state: String,
    #[serde(rename = "isAvailable")]
    is_available: bool,
}

/// Ignore non-iOS runtimes and unavailable devices; neither can provide this backend's preview.
pub fn devices(output: &str) -> Result<Inventory> {
    let listing: Listing = serde_json::from_str(output).map_err(|_| Error::CommandFailed {
        tool: "simctl".into(),
        message: "Device listing was not valid simctl JSON".into(),
    })?;
    let mut inventory = Inventory::default();
    let mut seen = HashSet::new();
    for (runtime, simulators) in listing.devices {
        if !runtime.starts_with("com.apple.CoreSimulator.SimRuntime.iOS-") {
            continue;
        }
        for simulator in simulators {
            let Ok(simulator) = serde_json::from_value::<Simulator>(simulator) else {
                inventory
                    .warnings
                    .push(format!("Skipped malformed simulator entry in {runtime}"));
                continue;
            };
            if !simulator.is_available {
                continue;
            }
            if crate::validate_udid(&simulator.udid).is_err() || simulator.name.is_empty() {
                inventory
                    .warnings
                    .push(format!("Skipped invalid simulator identity in {runtime}"));
                continue;
            }
            if !seen.insert(simulator.udid.to_ascii_uppercase()) {
                inventory
                    .warnings
                    .push(format!("Skipped duplicate simulator {}", simulator.udid));
                continue;
            }
            let state = match simulator.state.as_str() {
                "Booted" => DeviceState::Online,
                "Shutdown" => DeviceState::Stopped,
                "Booting" | "Shutting Down" => DeviceState::Offline,
                _ => DeviceState::Unknown,
            };
            inventory.devices.push(Device {
                id: format!("ios:{}", simulator.udid),
                platform: Platform::Ios,
                kind: DeviceKind::Simulator,
                state,
                name: simulator.name,
                serial: Some(simulator.udid),
                transport_id: None,
                avd: None,
                capabilities: Capabilities {
                    video: cfg!(target_os = "macos"),
                    lifecycle: cfg!(target_os = "macos"),
                    ..Capabilities::default()
                },
            });
        }
    }
    Ok(inventory)
}
