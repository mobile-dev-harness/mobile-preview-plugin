//! Device stream handshakes and ordered input state, independent of I/O and injection APIs.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Error, InputEvent, KeyPhase, Result, TouchPhase};

pub const DEVICE_PROTOCOL: &str = "mpp-device/1";
pub const MAX_CONTROL_BYTES: usize = 16_384;
pub const MAX_BATCH_EVENTS: usize = 64;
pub const MEDIA_MAX_PAYLOAD: usize = 8 * 1024 * 1024;

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    pub protocol: String,
    pub socket_name: String,
    pub token: String,
    pub generation: u64,
    pub epoch: u64,
    pub max_size: u32,
    pub bit_rate: u32,
    pub max_fps: u32,
}

impl DeviceConfig {
    pub fn validate(&self) -> Result<()> {
        validate_identity(&self.protocol, &self.token, self.generation, self.epoch)?;
        if !self
            .socket_name
            .strip_prefix("mpp_")
            .is_some_and(|suffix| lower_hex(suffix, 32))
        {
            return Err(invalid(
                "socket name must be mpp_ followed by 32 lowercase hex digits",
            ));
        }
        if !(256..=2048).contains(&self.max_size) || !self.max_size.is_multiple_of(2) {
            return Err(invalid(
                "max_size must be an even value between 256 and 2048",
            ));
        }
        if !(100_000..=20_000_000).contains(&self.bit_rate) {
            return Err(invalid("bit_rate must be between 100000 and 20000000"));
        }
        if !(1..=60).contains(&self.max_fps) {
            return Err(invalid("max_fps must be between 1 and 60"));
        }
        Ok(())
    }
}

impl fmt::Debug for DeviceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceConfig")
            .field("protocol", &self.protocol)
            .field("socket_name", &self.socket_name)
            .field("token", &"[redacted]")
            .field("generation", &self.generation)
            .field("epoch", &self.epoch)
            .field("max_size", &self.max_size)
            .field("bit_rate", &self.bit_rate)
            .field("max_fps", &self.max_fps)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub rotation: u32,
}

impl Geometry {
    pub fn validate(&self) -> Result<()> {
        validate_video_size(self.width, self.height)?;
        if !(1..=16_384).contains(&self.display_width)
            || !(1..=16_384).contains(&self.display_height)
        {
            return Err(invalid("display dimensions must be between 1 and 16384"));
        }
        if self.rotation > 3 {
            return Err(invalid("rotation must be between 0 and 3"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Video,
    Control,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceHello {
    pub protocol: String,
    pub token: String,
    pub channel: Channel,
    pub generation: u64,
    pub epoch: u64,
    pub geometry: Geometry,
}

impl DeviceHello {
    /// Validate syntax and bounds; the receiver must also compare the expected identity and role.
    pub fn validate(&self) -> Result<()> {
        validate_identity(&self.protocol, &self.token, self.generation, self.epoch)?;
        self.geometry.validate()
    }
}

impl fmt::Debug for DeviceHello {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceHello")
            .field("protocol", &self.protocol)
            .field("token", &"[redacted]")
            .field("channel", &self.channel)
            .field("generation", &self.generation)
            .field("epoch", &self.epoch)
            .field("geometry", &self.geometry)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    pub seq: u64,
    pub epoch: u64,
    pub command: ControlCommand,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlCommand {
    Input { event: InputEvent },
    Reset,
    Heartbeat,
    Stop,
    KeyFrame,
}

impl<'de> Deserialize<'de> for ControlCommand {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        // Empty struct variants enforce unknown-field rejection; serde unit variants do not.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Input { event: InputEvent },
            Reset {},
            Heartbeat {},
            Stop {},
            KeyFrame {},
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Input { event } => Self::Input { event },
            Wire::Reset {} => Self::Reset,
            Wire::Heartbeat {} => Self::Heartbeat,
            Wire::Stop {} => Self::Stop,
            Wire::KeyFrame {} => Self::KeyFrame,
        })
    }
}

/// Success acknowledges submission or protocol acceptance, never application-level completion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlReply {
    pub seq: u64,
    pub ok: bool,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct Pointer {
    x: f64,
    y: f64,
}

/// Tracks possibly submitted input so callers can release it after errors or lost connections.
#[derive(Debug)]
pub struct InputState {
    epoch: u64,
    width: u32,
    height: u32,
    last_seq: u64,
    pointer: Option<Pointer>,
    keys: BTreeSet<u32>,
    stopped: bool,
}

impl InputState {
    pub fn new(epoch: u64, width: u32, height: u32) -> Result<Self> {
        if epoch == 0 {
            return Err(invalid("capture epoch must be positive"));
        }
        validate_video_size(width, height)?;
        Ok(Self {
            epoch,
            width,
            height,
            last_seq: 0,
            pointer: None,
            keys: BTreeSet::new(),
            stopped: false,
        })
    }

    /// A new sequence in the current epoch is consumed even if its command is rejected.
    /// Old epochs and sequences never mutate this state. Injection failure requires caller cleanup.
    pub fn accept(&mut self, request: &ControlRequest) -> Result<Vec<InputEvent>> {
        if self.stopped || request.epoch != self.epoch {
            return Err(Error::StaleSession);
        }
        if request.seq == 0 || request.seq <= self.last_seq {
            return Err(invalid(
                "control sequence must be positive and strictly increasing",
            ));
        }
        self.last_seq = request.seq;
        match &request.command {
            ControlCommand::Input { event } => self.input(event),
            ControlCommand::Reset => Ok(self.drain_releases()),
            ControlCommand::Stop => {
                self.stopped = true;
                Ok(self.drain_releases())
            }
            ControlCommand::Heartbeat | ControlCommand::KeyFrame => Ok(Vec::new()),
        }
    }

    /// Independent of incoming frame metadata so a stale frame cannot prevent cancellation.
    pub fn drain_releases(&mut self) -> Vec<InputEvent> {
        let mut releases =
            Vec::with_capacity(usize::from(self.pointer.is_some()) + self.keys.len());
        if let Some(pointer) = self.pointer.take() {
            releases.push(InputEvent::Touch {
                phase: TouchPhase::Cancel,
                x: pointer.x,
                y: pointer.y,
                width: self.width,
                height: self.height,
            });
        }
        for code in std::mem::take(&mut self.keys) {
            releases.push(InputEvent::Key {
                code,
                phase: KeyPhase::Up,
            });
        }
        releases
    }

    fn input(&mut self, event: &InputEvent) -> Result<Vec<InputEvent>> {
        event.validate()?;
        match event {
            InputEvent::Touch {
                phase,
                x,
                y,
                width,
                height,
            } => {
                if *width != self.width || *height != self.height {
                    return Err(invalid(
                        "touch dimensions do not match the active video frame",
                    ));
                }
                match phase {
                    TouchPhase::Down => {
                        if self.pointer.is_some() {
                            return Err(invalid("a pointer is already pressed"));
                        }
                        self.pointer = Some(Pointer { x: *x, y: *y });
                    }
                    TouchPhase::Move => {
                        if self.pointer.is_none() {
                            return Err(invalid("pointer move requires a preceding down"));
                        }
                        self.pointer = Some(Pointer { x: *x, y: *y });
                    }
                    TouchPhase::Up => {
                        if self.pointer.take().is_none() {
                            return Err(invalid("pointer up requires a preceding down"));
                        }
                    }
                    TouchPhase::Cancel => {
                        if self.pointer.take().is_none() {
                            return Ok(Vec::new());
                        }
                    }
                }
            }
            InputEvent::Key { code, phase } => {
                if *code > i32::MAX as u32 {
                    return Err(invalid("key code must fit an Android signed integer"));
                }
                match phase {
                    KeyPhase::Down if !self.keys.insert(*code) => {
                        return Err(invalid("the key is already pressed"));
                    }
                    KeyPhase::Up if !self.keys.remove(code) => {
                        return Err(invalid("key up requires a preceding down"));
                    }
                    _ => {}
                }
            }
            InputEvent::Text { .. } => {
                return Err(Error::Unsupported {
                    feature: "text injection".to_owned(),
                });
            }
        }
        Ok(vec![event.clone()])
    }
}

fn validate_identity(protocol: &str, token: &str, generation: u64, epoch: u64) -> Result<()> {
    if protocol != DEVICE_PROTOCOL {
        return Err(invalid("unsupported device protocol"));
    }
    if !lower_hex(token, 64) {
        return Err(invalid("token must contain 64 lowercase hex digits"));
    }
    if generation == 0 || epoch == 0 {
        return Err(invalid(
            "lease generation and capture epoch must be positive",
        ));
    }
    Ok(())
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_video_size(width: u32, height: u32) -> Result<()> {
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
        return Err(invalid("encoded dimensions must be between 1 and 4096"));
    }
    Ok(())
}

fn invalid(message: &str) -> Error {
    Error::InvalidArgument {
        message: message.to_owned(),
    }
}
