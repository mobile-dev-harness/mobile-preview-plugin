//! A surface-input H.264 encoder using the Android NDK (API 26+).
//!
//! The caller must attach a video producer to the borrowed input surface. Creating
//! this encoder alone does not capture a display. Codec choice and support for
//! requested dimensions remain device-dependent; no hardware encoder is assumed.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::{self, NonNull};

use mpp_core::{Error, Result};

use super::validation::{validate_output_size, validate_settings};

const DEQUEUE_TIMEOUT_US: i64 = 10_000;
const COLOR_FORMAT_SURFACE: i32 = 0x7f00_0789;
const CONFIGURE_FLAG_ENCODE: u32 = 1;
const BUFFER_FLAG_KEY_FRAME: u32 = 1;
const BUFFER_FLAG_CODEC_CONFIG: u32 = 2;
const BUFFER_FLAG_END_OF_STREAM: u32 = 4;
const BUFFER_FLAG_PARTIAL_FRAME: u32 = 8;

/// Owns one codec and its input surface; use it from a single owning thread.
pub struct Encoder {
    codec: NonNull<c_void>,
    input_surface: *mut c_void,
    started: bool,
    ended: bool,
}

#[derive(Debug)]
pub struct EncodedFrame {
    pub bytes: Vec<u8>,
    pub pts_us: u64,
    pub configuration: bool,
    pub key_frame: bool,
}

impl Encoder {
    pub fn new(width: u32, height: u32, bitrate: u32, fps: u32) -> Result<Self> {
        validate_settings(width, height, bitrate, fps)?;
        // SAFETY: the MIME string is static, NUL-terminated, and valid for this call.
        let codec = NonNull::new(unsafe { AMediaCodec_createEncoderByType(c"video/avc".as_ptr()) })
            .ok_or_else(|| Error::Unsupported {
                feature: "Android H.264 surface encoder".into(),
            })?;
        let mut encoder = Self {
            codec,
            input_surface: ptr::null_mut(),
            started: false,
            ended: false,
        };
        let format = Format::new()?;
        format.set_string(c"mime", c"video/avc");
        format.set_i32(c"width", width as i32);
        format.set_i32(c"height", height as i32);
        format.set_i32(c"bitrate", bitrate as i32);
        format.set_i32(c"frame-rate", fps as i32);
        format.set_i32(c"color-format", COLOR_FORMAT_SURFACE);
        format.set_i32(c"i-frame-interval", 1);
        // Older codecs may ignore this key; it must not be treated as a capability guarantee.
        format.set_i32(c"max-bframes", 0);
        // SAFETY: both handles are live, and an encoder has no output surface or crypto object.
        check_status("configure", unsafe {
            AMediaCodec_configure(
                encoder.codec.as_ptr(),
                format.0.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                CONFIGURE_FLAG_ENCODE,
            )
        })?;
        // SAFETY: the codec is configured but not started. Writing directly into the owned
        // field ensures Drop releases a nonnull window even if this call returns an error.
        check_status("create input surface", unsafe {
            AMediaCodec_createInputSurface(encoder.codec.as_ptr(), &mut encoder.input_surface)
        })?;
        if encoder.input_surface.is_null() {
            return Err(codec_error("create input surface returned a null window"));
        }
        // SAFETY: the configured codec has an input surface and is exclusively owned.
        check_status("start", unsafe {
            AMediaCodec_start(encoder.codec.as_ptr())
        })?;
        encoder.started = true;
        Ok(encoder)
    }

    /// Borrowed ANativeWindow, valid only while this encoder lives. Do not release it.
    pub fn surface(&self) -> *mut c_void {
        self.input_surface
    }

    /// Wait up to 10 ms for one output buffer. None also covers codec notifications.
    pub fn next(&mut self) -> Result<Option<EncodedFrame>> {
        if self.ended {
            return Ok(None);
        }
        let mut info = BufferInfo::default();
        // SAFETY: the codec is started and owned by self; info is writable for the call.
        let index = unsafe {
            AMediaCodec_dequeueOutputBuffer(self.codec.as_ptr(), &mut info, DEQUEUE_TIMEOUT_US)
        };
        match index {
            // Retry, output format change, and legacy output buffer change carry no buffer.
            -3..=-1 => return Ok(None),
            value if value < 0 => {
                return Err(codec_error(format!(
                    "dequeue output failed with status {value}"
                )));
            }
            _ => {}
        }
        let mut output = OutputBuffer {
            codec: self.codec,
            index: index as usize,
            acquired: true,
        };
        let frame = output.copy_frame(&info)?;
        output.release()?;
        self.ended = info.flags & BUFFER_FLAG_END_OF_STREAM != 0;
        Ok(frame)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns these resources, and output guards cannot escape
        // next(). Teardown continues even if stopping the codec reports a platform error.
        unsafe {
            if self.started {
                AMediaCodec_stop(self.codec.as_ptr());
            }
            if !self.input_surface.is_null() {
                ANativeWindow_release(self.input_surface);
            }
            AMediaCodec_delete(self.codec.as_ptr());
        }
    }
}

struct Format(NonNull<c_void>);

impl Format {
    fn new() -> Result<Self> {
        // SAFETY: creation takes no arguments and transfers ownership to the caller.
        NonNull::new(unsafe { AMediaFormat_new() })
            .map(Self)
            .ok_or_else(|| codec_error("could not allocate encoder format"))
    }

    fn set_i32(&self, name: &CStr, value: i32) {
        // SAFETY: the owned format is live; the NDK copies the NUL-terminated key.
        unsafe { AMediaFormat_setInt32(self.0.as_ptr(), name.as_ptr(), value) }
    }

    fn set_string(&self, name: &CStr, value: &CStr) {
        // SAFETY: the owned format is live; the NDK copies both NUL-terminated strings.
        unsafe { AMediaFormat_setString(self.0.as_ptr(), name.as_ptr(), value.as_ptr()) }
    }
}

impl Drop for Format {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns the format and is destroying it once.
        unsafe { AMediaFormat_delete(self.0.as_ptr()) };
    }
}

/// A successfully dequeued slot must be released, including on validation errors.
struct OutputBuffer {
    codec: NonNull<c_void>,
    index: usize,
    acquired: bool,
}

impl OutputBuffer {
    fn copy_frame(&self, info: &BufferInfo) -> Result<Option<EncodedFrame>> {
        let len = validate_output_size(info.size)?;
        if len == 0 {
            return Ok(None);
        }
        if info.flags & BUFFER_FLAG_PARTIAL_FRAME != 0 {
            return Err(codec_error("partial output frames are unsupported"));
        }
        let configuration = info.flags & BUFFER_FLAG_CODEC_CONFIG != 0;
        // Codec configuration has no presentation timestamp; encoders may leave it unset.
        let pts_us = if configuration {
            0
        } else {
            u64::try_from(info.presentation_time_us)
                .map_err(|_| codec_error("encoder returned a negative presentation timestamp"))?
        };
        let mut reported_size = 0;
        // SAFETY: index is currently dequeued and remains acquired until after the copy.
        let bytes = unsafe {
            AMediaCodec_getOutputBuffer(self.codec.as_ptr(), self.index, &mut reported_size)
        };
        if bytes.is_null() {
            return Err(codec_error("encoder returned a null output buffer"));
        }
        // NDK API <=35 documents both BufferInfo.offset and getOutputBuffer's out_size as
        // invalid. The pointer is already positioned, and BufferInfo.size is authoritative.
        // The same info.size contract remains valid on later API levels.
        // SAFETY: the NDK promises info.size readable bytes at this acquired pointer. len
        // has been checked as nonzero and <=8 MiB; the slot is not released during copying.
        let bytes = unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec();
        Ok(Some(EncodedFrame {
            bytes,
            pts_us,
            configuration,
            key_frame: info.flags & BUFFER_FLAG_KEY_FRAME != 0,
        }))
    }

    fn release(&mut self) -> Result<()> {
        // Never retry a release, including after an error with uncertain ownership.
        self.acquired = false;
        // SAFETY: the slot is acquired exactly once; encoding output is never rendered.
        check_status("release output buffer", unsafe {
            AMediaCodec_releaseOutputBuffer(self.codec.as_ptr(), self.index, false)
        })
    }
}

impl Drop for OutputBuffer {
    fn drop(&mut self) {
        if self.acquired {
            let _ = self.release();
        }
    }
}

fn check_status(operation: &str, status: i32) -> Result<()> {
    if status != 0 {
        return Err(codec_error(format!(
            "{operation} failed with status {status}"
        )));
    }
    Ok(())
}

fn codec_error(message: impl Into<String>) -> Error {
    Error::CommandFailed {
        tool: "Android MediaCodec".into(),
        message: message.into(),
    }
}

#[repr(C)]
#[derive(Default)]
struct BufferInfo {
    offset: i32,
    size: i32,
    presentation_time_us: i64,
    flags: u32,
}

#[link(name = "mediandk")]
unsafe extern "C" {
    fn AMediaCodec_createEncoderByType(mime: *const c_char) -> *mut c_void;
    fn AMediaCodec_configure(
        codec: *mut c_void,
        format: *const c_void,
        surface: *mut c_void,
        crypto: *mut c_void,
        flags: u32,
    ) -> i32;
    fn AMediaCodec_createInputSurface(codec: *mut c_void, surface: *mut *mut c_void) -> i32;
    fn AMediaCodec_start(codec: *mut c_void) -> i32;
    fn AMediaCodec_stop(codec: *mut c_void) -> i32;
    fn AMediaCodec_delete(codec: *mut c_void) -> i32;
    fn AMediaCodec_dequeueOutputBuffer(
        codec: *mut c_void,
        info: *mut BufferInfo,
        timeout_us: i64,
    ) -> isize;
    fn AMediaCodec_getOutputBuffer(codec: *mut c_void, index: usize, size: *mut usize) -> *mut u8;
    fn AMediaCodec_releaseOutputBuffer(codec: *mut c_void, index: usize, render: bool) -> i32;
    fn AMediaFormat_new() -> *mut c_void;
    fn AMediaFormat_delete(format: *mut c_void) -> i32;
    fn AMediaFormat_setInt32(format: *mut c_void, name: *const c_char, value: i32);
    fn AMediaFormat_setString(format: *mut c_void, name: *const c_char, value: *const c_char);
}

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_release(window: *mut c_void);
}
