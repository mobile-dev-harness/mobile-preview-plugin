//! Shared device, session, and wire types without platform I/O.

mod device;
mod error;
mod input;
mod lease;

pub mod media;
pub mod stream;

pub use device::{Capabilities, Device, DeviceKind, DeviceState, Inventory, Platform};
pub use error::{Error, Result};
pub use input::{InputEvent, KeyPhase, MAX_INPUT_TEXT_BYTES, TouchPhase};
pub use lease::{LeaseManager, MAX_OWNER_BYTES, Session, SessionState};
