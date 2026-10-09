//! Private capture-child protocol. Native frame acquisition never writes to stdout directly.

#[cfg(any(target_os = "macos", test))]
use std::sync::Arc;
#[cfg(any(target_os = "macos", test))]
use std::sync::atomic::{AtomicBool, Ordering};

use mpp_core::{Error, Result};
#[cfg(any(target_os = "macos", test))]
use serde::Deserialize;
#[cfg(any(target_os = "macos", test))]
use tokio::io::AsyncReadExt;
#[cfg(target_os = "macos")]
use tokio::io::AsyncWriteExt;

use crate::{CaptureConfig, NativeProbe};

#[cfg(all(unix, any(target_os = "macos", test)))]
#[path = "control_fd.rs"]
mod control_fd;
#[cfg(any(target_os = "macos", test))]
#[path = "control.rs"]
mod input_control;

#[cfg(any(target_os = "macos", test))]
pub(super) enum InputWork {
    Apply {
        events: Vec<mpp_core::InputEvent>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Release {
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
#[path = "native.rs"]
mod native;

pub fn probe(udid: &str) -> Result<NativeProbe> {
    crate::validate_udid(udid)?;
    #[cfg(target_os = "macos")]
    {
        native::probe(udid)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(unsupported())
    }
}

pub async fn run(config: CaptureConfig) -> Result<()> {
    validate(&config)?;
    #[cfg(not(target_os = "macos"))]
    {
        Err(unsupported())
    }
    #[cfg(target_os = "macos")]
    {
        let control = Arc::new(Control::default());
        let _stop_on_drop = StopOnDrop(control.clone());
        // Each entry contains a complete frame, including its configuration when it is an IDR.
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(3);
        let (input_sender, input_receiver) = std::sync::mpsc::sync_channel(8);
        let (geometry_sender, geometry_receiver) = tokio::sync::oneshot::channel();
        let private_control = config.control_fd.map(control_fd::inherited).transpose()?;
        let input_enabled = config.input_enabled;
        let epoch = config.epoch;
        let native_control = control.clone();
        let mut task = tokio::task::spawn_blocking(move || {
            native::run(
                config,
                native_control,
                sender,
                input_receiver,
                geometry_sender,
            )
        });
        let mut stdout = tokio::io::stdout();
        let inputs = async {
            if let Some(stream) = private_control {
                input_control::run(
                    stream,
                    &control,
                    &input_sender,
                    geometry_receiver,
                    epoch,
                    input_enabled,
                )
                .await
            } else {
                // FD 0 belongs exclusively to the private socket when one was inherited.
                controls(&mut tokio::io::stdin(), &control).await
            }
        };
        let output = async {
            while let Some(bytes) = receiver.recv().await {
                stdout.write_all(&bytes).await?;
                stdout.flush().await?;
            }
            Ok::<(), Error>(())
        };
        let result = tokio::select! {
            result = &mut task => return result.map_err(|_| failed("Native capture task failed"))?,
            result = inputs => result,
            result = output => result,
            result = signals() => result,
        };
        control.stop.store(true, Ordering::Release);
        // The native owner releases the compression session and surfaces before this returns.
        let cleanup = task
            .await
            .map_err(|_| failed("Native capture cleanup failed"))?;
        result.and(cleanup)
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(super) struct Control {
    stop: AtomicBool,
    key_frame: AtomicBool,
}

#[cfg(any(target_os = "macos", test))]
struct StopOnDrop(Arc<Control>);
#[cfg(any(target_os = "macos", test))]
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Release);
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    KeyFrame,
    Stop,
}

#[cfg(any(target_os = "macos", test))]
async fn controls(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    control: &Control,
) -> Result<()> {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        if reader.read(&mut byte).await? == 0 {
            return Ok(());
        }
        if byte[0] == b'\n' {
            match serde_json::from_slice::<Command>(&line)
                .map_err(|_| invalid("Malformed capture control command"))?
            {
                Command::KeyFrame => control.key_frame.store(true, Ordering::Release),
                Command::Stop => return Ok(()),
            }
            line.clear();
        } else if line.len() == 256 {
            return Err(invalid("Capture control command exceeds 256 bytes"));
        } else {
            line.push(byte[0]);
        }
    }
}

#[cfg(target_os = "macos")]
async fn signals() -> Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.map_err(Error::Io),
        _ = terminate.recv() => Ok(()),
    }
}

fn validate(config: &CaptureConfig) -> Result<()> {
    crate::validate_udid(&config.udid)?;
    if config.boot_id.is_empty()
        || config.boot_id.len() > 256
        || config
            .boot_id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        || config.control_fd.is_some_and(|fd| fd != 0)
        || (config.input_enabled && config.control_fd.is_none())
        || config.generation == 0
        || config.epoch == 0
        || (!(256..=2048).contains(&config.max_size) || !config.max_size.is_multiple_of(2))
        || !(100_000..=20_000_000).contains(&config.bit_rate)
        || !(1..=60).contains(&config.max_fps)
    {
        return Err(invalid(
            "Invalid native capture limits or attachment identity",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn dimensions(width: usize, height: usize, max_size: u32) -> Result<(u32, u32)> {
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err(failed("Simulator returned invalid framebuffer dimensions"));
    }
    let longest = width.max(height) as u64;
    let limit = u64::from(max_size).min(longest);
    let scale = |side: usize| ((side as u64 * limit / longest) as u32) & !1;
    let result = (scale(width), scale(height));
    if result.0 < 2 || result.1 < 2 {
        return Err(failed("Simulator framebuffer is too narrow to encode"));
    }
    Ok(result)
}

/// Convert AVCC lengths after validating the entire sample; malformed frames never reach the wire.
#[cfg(any(target_os = "macos", test))]
fn annex_b(bytes: &[u8], length_size: usize) -> Result<Vec<u8>> {
    if !matches!(length_size, 1 | 2 | 4) || bytes.len() > MAX_FRAME_BYTES {
        return Err(failed("Invalid H.264 sample size or NAL header"));
    }
    let mut result = Vec::with_capacity(bytes.len());
    let mut remaining = bytes;
    let mut units = 0usize;
    while !remaining.is_empty() {
        units += 1;
        if units > 4096 {
            return Err(failed("H.264 sample contains too many NAL units"));
        }
        if remaining.len() < length_size {
            return Err(failed("Truncated H.264 NAL header"));
        }
        let length = remaining[..length_size]
            .iter()
            .fold(0usize, |n, value| (n << 8) | usize::from(*value));
        remaining = &remaining[length_size..];
        if length == 0
            || length > remaining.len()
            || result.len().saturating_add(length + 4) > MAX_FRAME_BYTES
        {
            return Err(failed("Invalid H.264 NAL length"));
        }
        result.extend_from_slice(&[0, 0, 0, 1]);
        result.extend_from_slice(&remaining[..length]);
        remaining = &remaining[length..];
    }
    if result.is_empty() {
        return Err(failed("Empty H.264 sample"));
    }
    Ok(result)
}

#[cfg(any(target_os = "macos", test))]
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
fn invalid(message: &str) -> Error {
    Error::InvalidArgument {
        message: message.into(),
    }
}
#[cfg(any(target_os = "macos", test))]
fn failed(message: &str) -> Error {
    Error::CommandFailed {
        tool: "iOS capture".into(),
        message: message.into(),
    }
}
#[cfg(not(target_os = "macos"))]
fn unsupported() -> Error {
    Error::Unsupported {
        feature: "iOS Simulator capture requires macOS and Xcode".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_preserves_even_bounded_geometry() {
        assert_eq!(dimensions(1206, 2622, 1280).unwrap(), (588, 1280));
        assert_eq!(dimensions(1206, 2622, 4096).unwrap(), (1206, 2622));
        assert!(dimensions(0, 2622, 1280).is_err());
        assert!(dimensions(1, 16000, 64).is_err());
    }

    #[test]
    fn nal_conversion_rejects_partial_and_oversized_units() {
        assert_eq!(
            annex_b(&[0, 0, 0, 2, 0x65, 0x80, 0, 0, 0, 1, 6], 4).unwrap(),
            [0, 0, 0, 1, 0x65, 0x80, 0, 0, 0, 1, 6]
        );
        for bytes in [
            &[][..],
            &[0, 0, 0][..],
            &[0, 0, 0, 0][..],
            &[0xff, 0xff, 0xff, 0xff][..],
        ] {
            assert!(annex_b(bytes, 4).is_err());
        }
    }

    #[test]
    fn capture_limits_and_boot_identity_are_checked_before_native_work() {
        let good = CaptureConfig {
            udid: "D228B7F4-088B-40F8-ACA6-2083A2810F43".into(),
            boot_id: "ios:verified:42:100:200".into(),
            generation: 1,
            epoch: 1,
            max_size: 1280,
            bit_rate: 2_000_000,
            max_fps: 30,
            control_fd: None,
            input_enabled: false,
        };
        validate(&good).unwrap();
        for size in [0, 254, 257, 2050] {
            let mut c = good.clone();
            c.max_size = size;
            assert!(validate(&c).is_err());
        }
        for fps in [0, 61, u32::MAX] {
            let mut c = good.clone();
            c.max_fps = fps;
            assert!(validate(&c).is_err());
        }
        for rate in [0, 99_999, 20_000_001] {
            let mut c = good.clone();
            c.bit_rate = rate;
            assert!(validate(&c).is_err());
        }
        for identity in ["", "unverified identity", "bad\0identity", "bad\nidentity"] {
            let mut c = good.clone();
            c.boot_id = identity.into();
            assert!(validate(&c).is_err());
        }
        let mut c = good.clone();
        c.generation = 0;
        assert!(validate(&c).is_err());
        c = good.clone();
        c.epoch = 0;
        assert!(validate(&c).is_err());
        c = good;
        c.control_fd = Some(1);
        assert!(validate(&c).is_err());
        c.control_fd = None;
        c.input_enabled = true;
        assert!(validate(&c).is_err());
        c.control_fd = Some(0);
        validate(&c).unwrap();
    }

    #[test]
    fn nal_count_and_length_width_are_bounded() {
        assert_eq!(annex_b(&[1, 0x65], 1).unwrap(), [0, 0, 0, 1, 0x65]);
        assert_eq!(annex_b(&[0, 1, 0x65], 2).unwrap(), [0, 0, 0, 1, 0x65]);
        assert!(annex_b(&[0, 0, 1, 0x65], 3).is_err());
        assert!(annex_b(&[1, 0x65].repeat(4097), 1).is_err());
    }

    #[tokio::test]
    async fn controls_request_keyframe_and_stop_at_eof_or_stop() {
        let state = Control::default();
        controls(
            &mut &b"{\"kind\":\"key_frame\"}\n{\"kind\":\"stop\"}\n"[..],
            &state,
        )
        .await
        .unwrap();
        assert!(state.key_frame.load(Ordering::Acquire));
        assert!(
            controls(&mut &b"{\"kind\":\"unknown\"}\n"[..], &state)
                .await
                .is_err()
        );
        assert!(controls(&mut &vec![b'a'; 257][..], &state).await.is_err());
        controls(&mut &b""[..], &state).await.unwrap();
    }
}
