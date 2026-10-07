use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const MAX_INPUT_TEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchPhase {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPhase {
    Down,
    Up,
}

/// Coordinates are normalized; frame dimensions let the backend reject stale input mapping.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputEvent {
    Touch {
        phase: TouchPhase,
        x: f64,
        y: f64,
        width: u32,
        height: u32,
    },
    Key {
        code: u32,
        phase: KeyPhase,
    },
    Text {
        text: String,
    },
}

impl InputEvent {
    pub fn validate(&self) -> Result<()> {
        let message = match self {
            Self::Touch { x, y, .. }
                if !x.is_finite()
                    || !y.is_finite()
                    || !(0.0..=1.0).contains(x)
                    || !(0.0..=1.0).contains(y) =>
            {
                Some("touch coordinates must be finite numbers between 0 and 1")
            }
            Self::Touch { width, height, .. }
                if !(1..=16_384).contains(width) || !(1..=16_384).contains(height) =>
            {
                Some("touch frame dimensions must be between 1 and 16384")
            }
            Self::Text { text } if text.is_empty() || text.len() > MAX_INPUT_TEXT_BYTES => {
                Some("input text must contain between 1 and 16384 UTF-8 bytes")
            }
            Self::Text { text } if text.contains('\0') => {
                Some("input text must not contain a null character")
            }
            _ => None,
        };
        match message {
            Some(message) => Err(Error::InvalidArgument {
                message: message.to_owned(),
            }),
            None => Ok(()),
        }
    }
}
