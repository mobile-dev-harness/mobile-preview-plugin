use std::collections::HashSet;

use mpp_core::{Capabilities, Device, DeviceKind, DeviceState, Inventory, Platform};

/// Keep unavailable devices visible so discovery never suggests they are ready.
pub fn devices(text: &str) -> Inventory {
    let mut inventory = Inventory::default();
    let mut seen = HashSet::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line == "List of devices attached" || line.starts_with('*') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(serial), Some(state)) = (fields.next(), fields.next()) else {
            inventory
                .warnings
                .push(format!("Unrecognized adb device line: {line}"));
            continue;
        };
        if !seen.insert(serial.to_owned()) {
            inventory
                .warnings
                .push(format!("Duplicate adb serial: {serial}"));
            continue;
        }
        let state = match state {
            "device" => DeviceState::Online,
            "offline" => DeviceState::Offline,
            "unauthorized" => DeviceState::Unauthorized,
            "no" if line.contains("no permissions") => {
                inventory.warnings.push(format!(
                    "Host lacks USB permissions for {serial}. Check Android USB rules."
                ));
                DeviceState::Unauthorized
            }
            _ => {
                inventory
                    .warnings
                    .push(format!("Unrecognized adb state for {serial}: {state}"));
                DeviceState::Unknown
            }
        };
        let kind = if serial.starts_with("emulator-") {
            DeviceKind::Emulator
        } else {
            DeviceKind::Physical
        };
        let metadata: Vec<_> = fields.collect();
        let name = metadata
            .iter()
            .find_map(|field| field.strip_prefix("model:"))
            .filter(|model| !model.is_empty())
            .map(|model| model.replace('_', " "))
            .unwrap_or_else(|| serial.to_owned());
        let ids: Vec<_> = metadata
            .iter()
            .filter_map(|field| field.strip_prefix("transport_id:"))
            .collect();
        let transport_id = match ids.as_slice() {
            [id] if !id.is_empty()
                && id.bytes().all(|byte| byte.is_ascii_digit())
                && id.parse::<u64>().is_ok() =>
            {
                Some((*id).to_owned())
            }
            [] => {
                inventory.warnings.push(format!(
                    "Device {serial} has no adb transport ID; refresh discovery before attaching."
                ));
                None
            }
            _ => {
                inventory.warnings.push(format!("Device {serial} has an invalid or ambiguous adb transport ID; refresh discovery before attaching."));
                None
            }
        };
        inventory.devices.push(Device {
            id: format!("android:{serial}"),
            platform: Platform::Android,
            kind,
            state,
            name,
            serial: Some(serial.to_owned()),
            transport_id,
            avd: None,
            capabilities: Capabilities {
                lifecycle: true,
                ..Capabilities::default()
            },
        });
    }
    inventory
}

pub fn avds(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.contains('|')
                && !line.chars().any(char::is_whitespace)
                && !line.starts_with('-')
                && !line.starts_with('*')
        })
        .filter(|line| seen.insert((*line).to_owned()))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn avd_name(text: &str) -> Option<String> {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    match lines.as_slice() {
        [name] | [name, "OK"] if !name.starts_with("KO:") && *name != "OK" => {
            Some((*name).to_owned())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_devices_and_adb_daemon_noise() {
        let text = "* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\nemulator-5554 device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1\nUSB0123 unauthorized transport_id:2\n192.168.1.20:5555 offline\n???????????? no permissions (missing udev rules); see [http://developer.android.com/tools/device.html]\n";
        let inventory = devices(text);
        assert_eq!(inventory.devices.len(), 4);
        assert_eq!(inventory.devices[0].kind, DeviceKind::Emulator);
        assert_eq!(inventory.devices[0].name, "sdk gphone64 arm64");
        assert_eq!(inventory.devices[0].state, DeviceState::Online);
        assert_eq!(inventory.devices[1].state, DeviceState::Unauthorized);
        assert_eq!(inventory.devices[2].state, DeviceState::Offline);
        assert_eq!(inventory.devices[3].state, DeviceState::Unauthorized);
        assert_eq!(inventory.warnings.len(), 3);
        assert_eq!(inventory.devices[0].transport_id.as_deref(), Some("1"));
        assert!(!inventory.devices[0].capabilities.video);
    }

    #[test]
    fn malformed_and_duplicate_output_is_reported() {
        let inventory = devices("garbage\nabc recovery\nabc device\n");
        assert_eq!(inventory.devices.len(), 1);
        assert_eq!(inventory.devices[0].state, DeviceState::Unknown);
        assert_eq!(inventory.warnings.len(), 4);
    }

    #[test]
    fn transport_ids_are_required_to_be_unambiguous_decimal_values() {
        for metadata in [
            "",
            "transport_id:",
            "transport_id:-1",
            "transport_id:x",
            "transport_id:1 transport_id:2",
            "transport_id:18446744073709551616",
        ] {
            let inventory = devices(&format!("phone device {metadata}\n"));
            assert_eq!(inventory.devices.len(), 1);
            assert_eq!(inventory.devices[0].transport_id, None);
            assert_eq!(inventory.warnings.len(), 1);
        }
    }

    #[test]
    fn avd_listing_omits_logs_and_duplicates() {
        assert_eq!(
            avds(
                "INFO | Android emulator version 36\nWARNING | a warning\nPixel_9\nPixel_9\nTablet_API_32\n"
            ),
            ["Pixel_9", "Tablet_API_32"]
        );
        assert_eq!(avd_name("Pixel_9\nOK\n"), Some("Pixel_9".into()));
        assert_eq!(avd_name("KO: unknown command\n"), None);
        assert_eq!(avd_name("OK\n"), None);
    }
}
