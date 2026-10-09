//! Apple SDK C ABI. Private selectors are limited to the verified CoreSimulator read path.
use std::ffi::{c_char, c_int, c_void};
pub type Id = *mut c_void;

// CoreMedia/CMTime.h uses four-byte packing, including for embedded CMTime values.
#[repr(C, packed(4))]
#[derive(Clone, Copy, Default)]
pub struct Time {
    pub value: i64,
    pub scale: i32,
    pub flags: u32,
    pub epoch: i64,
}
impl Time {
    pub fn micros(value: u64) -> Self {
        Self {
            value: value as i64,
            scale: 1_000_000,
            flags: 1,
            epoch: 0,
        }
    }
}

#[repr(C)]
#[derive(Default)]
pub struct ProcessInfo {
    pub values: [u32; 12],
    pub command: [u8; 16],
    pub name: [u8; 32],
    pub counters: [u32; 5],
    pub nice: i32,
    pub start_sec: u64,
    pub start_usec: u64,
}

#[link(name = "objc")]
unsafe extern "C" {
    pub fn objc_getClass(name: *const c_char) -> Id;
    pub fn objc_getProtocol(name: *const c_char) -> Id;
    pub fn sel_registerName(name: *const c_char) -> Id;
    pub fn objc_msgSend();
    pub fn objc_retain(object: Id) -> Id;
    pub fn objc_release(object: Id);
    pub fn objc_autoreleasePoolPush() -> Id;
    pub fn objc_autoreleasePoolPop(pool: Id);
}

#[link(name = "System")]
unsafe extern "C" {
    pub fn dlopen(path: *const c_char, flags: c_int) -> Id;
    pub fn getuid() -> u32;
}
#[link(name = "proc")]
unsafe extern "C" {
    pub fn proc_pidinfo(pid: c_int, flavor: c_int, arg: u64, buffer: Id, size: c_int) -> c_int;
}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRelease(value: Id);
    pub fn CFNumberCreate(allocator: Id, kind: isize, value: *const c_void) -> Id;
    pub fn CFDictionaryCreate(
        allocator: Id,
        keys: *const Id,
        values: *const Id,
        count: isize,
        key_callbacks: *const u8,
        value_callbacks: *const u8,
    ) -> Id;
    pub static kCFTypeDictionaryKeyCallBacks: u8;
    pub static kCFTypeDictionaryValueCallBacks: u8;
    pub static kCFBooleanTrue: Id;
    pub static kCFBooleanFalse: Id;
}
#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    pub fn IOSurfaceGetWidth(surface: Id) -> usize;
    pub fn IOSurfaceGetHeight(surface: Id) -> usize;
    pub fn IOSurfaceGetID(surface: Id) -> u32;
    pub fn IOSurfaceGetSeed(surface: Id) -> u32;
    pub fn IOSurfaceLock(surface: Id, options: u32, seed: *mut u32) -> i32;
    pub fn IOSurfaceUnlock(surface: Id, options: u32, seed: *mut u32) -> i32;
}
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    pub fn CVPixelBufferCreateWithIOSurface(
        allocator: Id,
        surface: Id,
        attributes: Id,
        out: *mut Id,
    ) -> i32;
    pub fn CVPixelBufferCreate(
        allocator: Id,
        width: usize,
        height: usize,
        format: u32,
        attributes: Id,
        out: *mut Id,
    ) -> i32;
    pub static kCVPixelBufferIOSurfacePropertiesKey: Id;
}

pub type OutputCallback = unsafe extern "C" fn(Id, Id, i32, u32, Id);
#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    pub fn VTPixelTransferSessionCreate(allocator: Id, out: *mut Id) -> i32;
    pub fn VTPixelTransferSessionTransferImage(session: Id, source: Id, destination: Id) -> i32;
    pub fn VTPixelTransferSessionInvalidate(session: Id);
    pub fn VTCompressionSessionCreate(
        allocator: Id,
        width: i32,
        height: i32,
        codec: u32,
        specification: Id,
        attributes: Id,
        compressed_allocator: Id,
        callback: OutputCallback,
        context: Id,
        out: *mut Id,
    ) -> i32;
    pub fn VTSessionSetProperty(session: Id, key: Id, value: Id) -> i32;
    pub fn VTCompressionSessionPrepareToEncodeFrames(session: Id) -> i32;
    pub fn VTCompressionSessionEncodeFrame(
        session: Id,
        image: Id,
        pts: Time,
        duration: Time,
        properties: Id,
        context: Id,
        flags: *mut u32,
    ) -> i32;
    pub fn VTCompressionSessionCompleteFrames(session: Id, until: Time) -> i32;
    pub fn VTCompressionSessionInvalidate(session: Id);
    pub static kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder: Id;
    pub static kVTCompressionPropertyKey_RealTime: Id;
    pub static kVTCompressionPropertyKey_AllowFrameReordering: Id;
    pub static kVTCompressionPropertyKey_AverageBitRate: Id;
    pub static kVTCompressionPropertyKey_ExpectedFrameRate: Id;
    pub static kVTCompressionPropertyKey_MaxKeyFrameInterval: Id;
    pub static kVTCompressionPropertyKey_ProfileLevel: Id;
    pub static kVTProfileLevel_H264_Baseline_AutoLevel: Id;
    pub static kVTEncodeFrameOptionKey_ForceKeyFrame: Id;
}
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    pub fn CMSampleBufferGetDataBuffer(sample: Id) -> Id;
    pub fn CMSampleBufferGetFormatDescription(sample: Id) -> Id;
    pub fn CMSampleBufferGetPresentationTimeStamp(sample: Id) -> Time;
    pub fn CMBlockBufferGetDataLength(buffer: Id) -> usize;
    pub fn CMBlockBufferCopyDataBytes(buffer: Id, offset: usize, length: usize, out: Id) -> i32;
    pub fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        description: Id,
        index: usize,
        pointer: *mut *const u8,
        size: *mut usize,
        count: *mut usize,
        length_size: *mut i32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::Time;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn core_media_time_matches_apple_sdk_layout() {
        assert_eq!(size_of::<Time>(), 24);
        assert_eq!(align_of::<Time>(), 4);
        assert_eq!(offset_of!(Time, value), 0);
        assert_eq!(offset_of!(Time, scale), 8);
        assert_eq!(offset_of!(Time, flags), 12);
        assert_eq!(offset_of!(Time, epoch), 16);
    }
}

#[link(name = "System")]
unsafe extern "C" {
    pub fn dlsym(handle: Id, name: *const c_char) -> Id;
    pub fn free(pointer: Id);
    pub fn dispatch_queue_create(label: *const c_char, attributes: Id) -> Id;
    pub fn dispatch_release(queue: Id);
    pub fn _Block_copy(block: Id) -> Id;
    pub fn _Block_release(block: Id);
    pub static _NSConcreteStackBlock: u8;
    pub fn xpc_release(object: Id);
    pub fn xpc_get_type(object: Id) -> Id;
    pub fn xpc_copy_description(object: Id) -> Id;
    pub static _xpc_type_dictionary: u8;
    pub static _xpc_type_error: u8;
    pub fn xpc_dictionary_create(keys: Id, values: Id, count: usize) -> Id;
    pub fn xpc_dictionary_get_count(dictionary: Id) -> usize;
    #[cfg(test)]
    pub fn xpc_dictionary_get_bool(dictionary: Id, key: *const c_char) -> bool;
    pub fn xpc_connection_send_message(connection: Id, message: Id);
    pub fn xpc_dictionary_set_string(dictionary: Id, key: *const c_char, value: *const c_char);
    pub fn xpc_dictionary_set_uint64(dictionary: Id, key: *const c_char, value: u64);
    pub fn xpc_dictionary_set_double(dictionary: Id, key: *const c_char, value: f64);
    pub fn xpc_dictionary_set_bool(dictionary: Id, key: *const c_char, value: bool);
    pub fn xpc_dictionary_set_value(dictionary: Id, key: *const c_char, value: Id);
    pub fn xpc_connection_create_from_endpoint(endpoint: Id) -> Id;
    pub fn xpc_connection_set_target_queue(connection: Id, queue: Id);
    pub fn xpc_connection_set_event_handler(connection: Id, block: Id);
    pub fn xpc_connection_resume(connection: Id);
    pub fn xpc_connection_cancel(connection: Id);
    pub fn xpc_connection_send_message_with_reply(
        connection: Id,
        message: Id,
        queue: Id,
        block: Id,
    );
}
