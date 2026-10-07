//! Device-side Android backend. Platform resources never escape their owning process.

#[cfg(target_os = "android")]
#[allow(unsafe_code)]
mod codec;

#[cfg(any(target_os = "android", test))]
mod validation;

#[derive(Debug, serde::Serialize)]
pub struct EncoderProbe {
    pub encoder: &'static str,
    pub input_surface: bool,
    pub capture_verified: bool,
    pub first_output: Option<ProbeOutput>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProbeOutput {
    pub bytes: usize,
    pub pts_us: u64,
    pub configuration: bool,
    pub key_frame: bool,
}

/// Verify the NDK encoder lifecycle without claiming a working screen capture.
pub fn probe_encoder() -> mpp_core::Result<EncoderProbe> {
    #[cfg(target_os = "android")]
    {
        let mut encoder = codec::Encoder::new(480, 1066, 2_000_000, 30)?;
        let first_output = encoder.next()?.map(|frame| ProbeOutput {
            bytes: frame.bytes.len(),
            pts_us: frame.pts_us,
            configuration: frame.configuration,
            key_frame: frame.key_frame,
        });
        Ok(EncoderProbe {
            encoder: "video/avc",
            input_surface: !encoder.surface().is_null(),
            capture_verified: false,
            first_output,
        })
    }
    #[cfg(not(target_os = "android"))]
    Err(mpp_core::Error::Unsupported {
        feature: "the device encoder probe must run on Android".into(),
    })
}
