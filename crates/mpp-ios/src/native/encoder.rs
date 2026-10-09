use super::super::{MAX_FRAME_BYTES, annex_b, failed};
use super::{Cf, dictionary, ffi::*, number, status};
use crate::CaptureConfig;
use mpp_core::{
    Result,
    media::{Packet, PacketBody, encode},
};
use std::{
    ptr::null_mut,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct Encoder {
    session: Cf,
    transfer: Cf,
    destination: Cf,
    force_properties: Cf,
    callback: Box<Callback>,
}

struct Callback {
    generation: u64,
    width: u32,
    height: u32,
    sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    recovering: AtomicBool,
    error: Mutex<Option<String>>,
}

impl Encoder {
    pub fn new(
        config: &CaptureConfig,
        width: u32,
        height: u32,
        sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<Self> {
        let mut callback = Box::new(Callback {
            generation: config.generation,
            width,
            height,
            sender,
            recovering: AtomicBool::new(true),
            error: Mutex::new(None),
        });
        let empty = dictionary(&[])?;
        let attributes = dictionary(&[(unsafe { kCVPixelBufferIOSurfacePropertiesKey }, empty.0)])?;
        let mut destination = null_mut();
        // Owned NV12 buffer is reused only after CompleteFrames has finished its preceding encode.
        status(
            unsafe {
                CVPixelBufferCreate(
                    null_mut(),
                    width as usize,
                    height as usize,
                    u32::from_be_bytes(*b"420v"),
                    attributes.0,
                    &mut destination,
                )
            },
            "Allocating encoder pixel buffer",
        )?;
        let destination = Cf(destination);
        let mut transfer = null_mut();
        status(
            unsafe { VTPixelTransferSessionCreate(null_mut(), &mut transfer) },
            "Creating pixel transfer session",
        )?;
        let transfer = Cf(transfer);
        let specification = dictionary(&[(
            unsafe { kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder },
            unsafe { kCFBooleanTrue },
        )])?;
        let mut session = null_mut();
        status(
            unsafe {
                VTCompressionSessionCreate(
                    null_mut(),
                    width as i32,
                    height as i32,
                    u32::from_be_bytes(*b"avc1"),
                    specification.0,
                    null_mut(),
                    null_mut(),
                    output_callback,
                    (&mut *callback as *mut Callback).cast(),
                    &mut session,
                )
            },
            "Creating H.264 encoder",
        )?;
        let session = Cf(session);
        let force_properties = dictionary(&[(
            unsafe { kVTEncodeFrameOptionKey_ForceKeyFrame },
            unsafe { kCFBooleanTrue },
        )])?;
        let encoder = Self {
            session,
            transfer,
            destination,
            force_properties,
            callback,
        };
        let bitrate = number(i64::from(config.bit_rate))?;
        let fps = number(i64::from(config.max_fps))?;
        let key_interval = number(i64::from(config.max_fps) * 2)?;
        // All objects remain retained until the synchronous property setters return.
        for (key, value) in unsafe {
            [
                (kVTCompressionPropertyKey_RealTime, kCFBooleanTrue),
                (
                    kVTCompressionPropertyKey_AllowFrameReordering,
                    kCFBooleanFalse,
                ),
                (kVTCompressionPropertyKey_AverageBitRate, bitrate.0),
                (kVTCompressionPropertyKey_ExpectedFrameRate, fps.0),
                (
                    kVTCompressionPropertyKey_MaxKeyFrameInterval,
                    key_interval.0,
                ),
                (
                    kVTCompressionPropertyKey_ProfileLevel,
                    kVTProfileLevel_H264_Baseline_AutoLevel,
                ),
            ]
        } {
            status(
                unsafe { VTSessionSetProperty(encoder.session.0, key, value) },
                "Configuring H.264 encoder",
            )?;
        }
        status(
            unsafe { VTCompressionSessionPrepareToEncodeFrames(encoder.session.0) },
            "Preparing H.264 encoder",
        )?;
        Ok(encoder)
    }

    pub fn copy(&self, source: Id) -> Result<()> {
        status(
            unsafe {
                VTPixelTransferSessionTransferImage(self.transfer.0, source, self.destination.0)
            },
            "Copying Simulator pixels",
        )
    }

    pub fn encode(&mut self, pts: u64, force: bool) -> Result<()> {
        let properties = if force {
            self.force_properties.0
        } else {
            null_mut()
        };
        status(
            unsafe {
                VTCompressionSessionEncodeFrame(
                    self.session.0,
                    self.destination.0,
                    Time::micros(pts),
                    Time::default(),
                    properties,
                    null_mut(),
                    null_mut(),
                )
            },
            "Encoding Simulator frame",
        )?;
        // One frame in flight bounds buffer ownership, encoder latency and callback memory.
        status(
            unsafe { VTCompressionSessionCompleteFrames(self.session.0, Time::default()) },
            "Completing Simulator frame",
        )?;
        self.check_error()
    }

    pub fn needs_keyframe(&self) -> bool {
        self.callback.recovering.load(Ordering::Acquire)
    }
    pub fn check_error(&self) -> Result<()> {
        let error = self
            .callback
            .error
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(message) = error.as_ref() {
            Err(failed(message))
        } else {
            Ok(())
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // The callback context and pixel buffers outlive all completion callbacks.
        unsafe {
            VTCompressionSessionCompleteFrames(self.session.0, Time::default());
            VTCompressionSessionInvalidate(self.session.0);
            VTPixelTransferSessionInvalidate(self.transfer.0);
        }
    }
}

unsafe extern "C" fn output_callback(
    context: Id,
    _frame_context: Id,
    result: i32,
    flags: u32,
    sample: Id,
) {
    // The boxed context is stable from session creation through CompleteFrames + Invalidate.
    let callback = unsafe { &*context.cast::<Callback>() };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if flags & 2 != 0 {
            callback.recovering.store(true, Ordering::Release);
            return Ok(());
        }
        status(result, "H.264 output")?;
        if sample.is_null() {
            return Err(failed("H.264 encoder returned an empty sample"));
        }
        let (frame, parameter_sets, pts, is_key) = sample_bytes(sample)?;
        if callback.recovering.load(Ordering::Acquire) && !is_key {
            return Ok(());
        }
        let mut bytes = Vec::new();
        if is_key {
            bytes.extend(encode(
                &Packet {
                    generation: callback.generation,
                    pts_us: pts,
                    body: PacketBody::Configuration {
                        width: callback.width,
                        height: callback.height,
                        annex_b: parameter_sets,
                    },
                },
                MAX_FRAME_BYTES,
            )?);
        }
        bytes.extend(encode(
            &Packet {
                generation: callback.generation,
                pts_us: pts,
                body: if is_key {
                    PacketBody::KeyFrame(frame)
                } else {
                    PacketBody::DeltaFrame(frame)
                },
            },
            MAX_FRAME_BYTES,
        )?);
        match callback.sender.try_send(bytes) {
            Ok(()) => {
                if is_key {
                    callback.recovering.store(false, Ordering::Release);
                }
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                callback.recovering.store(true, Ordering::Release)
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
        }
        Ok::<(), mpp_core::Error>(())
    }));
    let error = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(_) => Some("H.264 callback failed".into()),
    };
    if let Some(error) = error {
        let mut slot = callback
            .error
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        slot.get_or_insert(error);
    }
}

fn sample_bytes(sample: Id) -> Result<(Vec<u8>, Vec<u8>, u64, bool)> {
    // CoreMedia owns these borrowed objects for the duration of the callback; payloads are copied.
    let data = unsafe { CMSampleBufferGetDataBuffer(sample) };
    let format = unsafe { CMSampleBufferGetFormatDescription(sample) };
    if data.is_null() || format.is_null() {
        return Err(failed("H.264 sample is missing its data or format"));
    }
    let length = unsafe { CMBlockBufferGetDataLength(data) };
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(failed("H.264 sample exceeds frame size limit"));
    }
    let mut bytes = vec![0; length];
    status(
        unsafe { CMBlockBufferCopyDataBytes(data, 0, length, bytes.as_mut_ptr().cast()) },
        "Reading H.264 sample",
    )?;
    let mut count = 0;
    let mut length_size = 0;
    status(
        unsafe {
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                format,
                0,
                null_mut(),
                null_mut(),
                &mut count,
                &mut length_size,
            )
        },
        "Reading H.264 format",
    )?;
    if count == 0 || count > 16 {
        return Err(failed("Invalid H.264 parameter set count"));
    }
    let frame = annex_b(&bytes, length_size as usize)?;
    let is_key = frame
        .windows(5)
        .any(|bytes| bytes[..4] == [0, 0, 0, 1] && bytes[4] & 0x1f == 5);
    let mut parameters = Vec::new();
    if is_key {
        for index in 0..count {
            let mut pointer = std::ptr::null();
            let mut size = 0;
            status(
                unsafe {
                    CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                        format,
                        index,
                        &mut pointer,
                        &mut size,
                        null_mut(),
                        null_mut(),
                    )
                },
                "Reading H.264 parameter set",
            )?;
            if pointer.is_null()
                || size == 0
                || size > 65_536
                || parameters.len() + size + 4 > 65_536
            {
                return Err(failed("H.264 configuration exceeds its size limit"));
            }
            parameters.extend_from_slice(&[0, 0, 0, 1]);
            parameters.extend_from_slice(unsafe { std::slice::from_raw_parts(pointer, size) });
        }
    }
    let time = unsafe { CMSampleBufferGetPresentationTimeStamp(sample) };
    if time.value < 0 || time.scale <= 0 {
        return Err(failed("H.264 encoder returned an invalid timestamp"));
    }
    let pts = (time.value as u128 * 1_000_000 / time.scale as u128) as u64;
    Ok((frame, parameters, pts, is_key))
}
