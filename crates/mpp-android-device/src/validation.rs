//! Platform-independent bounds checked before entering the NDK.

use mpp_core::{Error, Result};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn validate_settings(width: u32, height: u32, bitrate: u32, fps: u32) -> Result<()> {
    if !(2..=4096).contains(&width)
        || !(2..=4096).contains(&height)
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
    {
        return Err(Error::InvalidArgument {
            message: "encoder dimensions must be even numbers in 2..=4096".into(),
        });
    }
    if !(100_000..=20_000_000).contains(&bitrate) {
        return Err(Error::InvalidArgument {
            message: "encoder bitrate must be in 100000..=20000000 bits per second".into(),
        });
    }
    if !(1..=60).contains(&fps) {
        return Err(Error::InvalidArgument {
            message: "encoder frame rate must be in 1..=60".into(),
        });
    }
    Ok(())
}

pub(super) fn validate_output_size(size: i32) -> Result<usize> {
    let len = usize::try_from(size).map_err(|_| output_error("negative output buffer size"))?;
    if len > MAX_FRAME_BYTES {
        return Err(output_error("encoded output exceeds the 8 MiB limit"));
    }
    Ok(len)
}

fn output_error(message: &str) -> Error {
    Error::CommandFailed {
        tool: "Android MediaCodec".into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_bounds_are_inclusive() {
        validate_settings(2, 4096, 100_000, 1).unwrap();
        validate_settings(4096, 2, 20_000_000, 60).unwrap();
        validate_settings(480, 1066, 2_000_000, 30).unwrap();
    }

    #[test]
    fn rejects_invalid_settings_before_calling_ndk() {
        for dimension in [0, 1, 3, 4095, 4097, u32::MAX] {
            assert!(validate_settings(dimension, 2, 100_000, 30).is_err());
            assert!(validate_settings(2, dimension, 100_000, 30).is_err());
        }
        for bitrate in [0, 99_999, 20_000_001, u32::MAX] {
            assert!(validate_settings(2, 2, bitrate, 30).is_err());
        }
        for fps in [0, 61, u32::MAX] {
            assert!(validate_settings(2, 2, 100_000, fps).is_err());
        }
    }

    #[test]
    fn output_size_is_bounded_before_copying() {
        assert_eq!(validate_output_size(0).unwrap(), 0);
        assert_eq!(validate_output_size(1).unwrap(), 1);
        assert_eq!(
            validate_output_size(8 * 1024 * 1024).unwrap(),
            MAX_FRAME_BYTES
        );
        for size in [-1, i32::MIN, 8 * 1024 * 1024 + 1, i32::MAX] {
            assert!(validate_output_size(size).is_err());
        }
    }
}
