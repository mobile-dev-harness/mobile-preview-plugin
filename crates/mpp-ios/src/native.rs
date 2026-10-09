//! macOS FFI boundary. Capture samples the raw IOSurface at a bounded cadence, avoiding
//! private Blocks callbacks and their remote-proxy lifetime hazards. IOSurface seeds do not
//! reliably signal GPU renders, so every cadence samples pixels; seeds only detect torn copies.

#[path = "native/encoder.rs"]
mod encoder;
#[path = "native/ffi.rs"]
mod ffi;

use super::{Control, InputWork, dimensions, failed};
use crate::{CaptureConfig, NativeProbe};
use ffi::*;
use mpp_core::{Error, Result, stream::Geometry};
use std::{
    ffi::{CStr, CString},
    io::Read,
    process::{Command, Stdio},
    ptr::null_mut,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

macro_rules! msg {
    ($object:expr, $selector:expr => $ret:ty $(, $arg:expr => $ty:ty)*) => {{
        // SAFETY: Each call below uses a known selector's exact ABI on its verified receiver.
        unsafe {
            let function: unsafe extern "C" fn(Id, Id $(, $ty)*) -> $ret =
                std::mem::transmute(objc_msgSend as *const ());
            function($object, sel_registerName($selector.as_ptr()) $(, $arg)*)
        }
    }};
}

#[path = "native/input.rs"]
mod input;

struct Object(Id);
impl Object {
    fn retain(object: Id) -> Result<Self> {
        if object.is_null() {
            return Err(failed("CoreSimulator returned a missing object"));
        }
        // SAFETY: Returned Objective-C objects remain valid within the current autorelease pool.
        Ok(Self(unsafe { objc_retain(object) }))
    }
}
impl Drop for Object {
    fn drop(&mut self) {
        unsafe { objc_release(self.0) }
    }
}
struct Pool(Id);
impl Pool {
    fn new() -> Self {
        Self(unsafe { objc_autoreleasePoolPush() })
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        unsafe { objc_autoreleasePoolPop(self.0) }
    }
}

pub(super) struct Cf(pub Id);
impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

pub(super) fn status(code: i32, operation: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(failed(&format!("{operation} failed (Apple status {code})")))
    }
}

pub(super) fn dictionary(pairs: &[(Id, Id)]) -> Result<Cf> {
    let keys: Vec<Id> = pairs.iter().map(|pair| pair.0).collect();
    let values: Vec<Id> = pairs.iter().map(|pair| pair.1).collect();
    // SAFETY: Arrays remain live for this copying constructor; CF callbacks retain their values.
    let dict = unsafe {
        CFDictionaryCreate(
            null_mut(),
            keys.as_ptr(),
            values.as_ptr(),
            pairs.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    };
    if dict.is_null() {
        Err(failed("Could not allocate capture settings"))
    } else {
        Ok(Cf(dict))
    }
}

pub(super) fn number(value: i64) -> Result<Cf> {
    // kCFNumberSInt64Type = 4. The constructor copies the stack value.
    let result = unsafe { CFNumberCreate(null_mut(), 4, (&value as *const i64).cast()) };
    if result.is_null() {
        Err(failed("Could not allocate capture setting"))
    } else {
        Ok(Cf(result))
    }
}

fn class(name: &CStr) -> Result<Id> {
    let class = unsafe { objc_getClass(name.as_ptr()) };
    if class.is_null() {
        return Err(Error::Unsupported {
            feature: format!(
                "Required Xcode class {} is unavailable",
                name.to_string_lossy()
            ),
        });
    }
    Ok(class)
}
fn string(value: &str) -> Result<Id> {
    let value = CString::new(value).map_err(|_| failed("Invalid native string"))?;
    let value = msg!(class(c"NSString")?, c"stringWithUTF8String:" => Id, value.as_ptr() => *const std::ffi::c_char);
    if value.is_null() {
        Err(failed("Could not create native string"))
    } else {
        Ok(value)
    }
}
fn responds(object: Id, selector: &CStr) -> bool {
    let selector = unsafe { sel_registerName(selector.as_ptr()) };
    msg!(object, c"respondsToSelector:" => bool, selector => Id)
}
fn conforms(object: Id, name: &CStr) -> bool {
    let protocol = unsafe { objc_getProtocol(name.as_ptr()) };
    !protocol.is_null() && msg!(object, c"conformsToProtocol:" => bool, protocol => Id)
}

fn load_frameworks() -> Result<()> {
    let developer = if let Ok(path) = std::env::var("DEVELOPER_DIR") {
        path
    } else {
        command_output("/usr/bin/xcode-select", &["-p"])?
            .trim()
            .to_owned()
    };
    for path in [
        "/Library/Developer/PrivateFrameworks/CoreSimulator.framework/CoreSimulator".to_owned(),
        format!("{developer}/Library/PrivateFrameworks/SimulatorKit.framework/SimulatorKit"),
    ] {
        let path = CString::new(path).map_err(|_| failed("Invalid Xcode developer directory"))?;
        // Keep frameworks loaded for the child process lifetime: remote proxy classes can retain code.
        if unsafe { dlopen(path.as_ptr(), 2) }.is_null() {
            return Err(Error::Unsupported {
                feature: "The selected Xcode does not expose Simulator capture frameworks".into(),
            });
        }
    }
    Ok(())
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| failed("Missing subprocess stdout"))?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output.take(65_537).read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let exit = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break Ok(exit),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Error::Timeout {
                    operation: "reading simulator boot identity".into(),
                });
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Error::Io(error));
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| failed("Boot identity reader failed"))??;
    if !exit?.success() {
        return Err(Error::StaleSession);
    }
    if bytes.len() > 65_536 {
        return Err(failed("Boot identity output exceeds its limit"));
    }
    String::from_utf8(bytes).map_err(|_| failed("Boot identity output is not UTF-8"))
}

fn boot_identity(udid: &str) -> Result<String> {
    let uid = unsafe { getuid() };
    let job = format!(
        "user/{uid}/com.apple.CoreSimulator.SimDevice.{}",
        udid.to_ascii_uppercase()
    );
    let output = command_output("/bin/launchctl", &["print", &job])?;
    let pid = parse_pid(&output)?;
    let mut info = ProcessInfo::default();
    let size = std::mem::size_of::<ProcessInfo>();
    // SAFETY: The SDK's proc_bsdinfo ABI is reproduced in ffi.rs; proc_pidinfo bounds the write.
    let read = unsafe {
        proc_pidinfo(
            pid,
            3,
            0,
            (&mut info as *mut ProcessInfo).cast(),
            size as i32,
        )
    };
    let name = info
        .command
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default();
    if read != size as i32
        || info.values[3] != pid as u32
        || name != b"launchd_sim"
        || info.start_sec == 0
    {
        return Err(Error::StaleSession);
    }
    Ok(format!(
        "ios:{}:{pid}:{}:{}",
        udid.to_ascii_uppercase(),
        info.start_sec,
        info.start_usec
    ))
}

fn parse_pid(output: &str) -> Result<i32> {
    if !output.lines().any(|line| line.trim() == "state = running") {
        return Err(Error::StaleSession);
    }
    let pids: Vec<_> = output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pid = "))
        .collect();
    if pids.len() != 1 {
        return Err(Error::StaleSession);
    }
    pids[0]
        .parse::<i32>()
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or(Error::StaleSession)
}

pub(super) fn probe(udid: &str) -> Result<NativeProbe> {
    let boot_id = boot_identity(udid)?;
    let _pool = Pool::new();
    let input = Binding::new(udid)
        .and_then(|binding| input::InputSession::connect(binding.device.0))
        .is_ok();
    if boot_identity(udid)? != boot_id {
        return Err(Error::StaleSession);
    }
    Ok(NativeProbe { boot_id, input })
}

struct Binding {
    _context: Object,
    _set: Object,
    device: Object,
    screen: Object,
    surface: Object,
    surface_id: u32,
    width: usize,
    height: usize,
}

impl Binding {
    fn new(udid: &str) -> Result<Self> {
        load_frameworks()?;
        let developer = match std::env::var("DEVELOPER_DIR") {
            Ok(path) => path,
            Err(_) => command_output("/usr/bin/xcode-select", &["-p"])?
                .trim()
                .to_owned(),
        };
        let mut error: Id = null_mut();
        let context = Object::retain(
            msg!(class(c"SimServiceContext")?, c"sharedServiceContextForDeveloperDir:error:" => Id,
            string(&developer)? => Id, &mut error => *mut Id),
        )?;
        let set = Object::retain(
            msg!(context.0, c"defaultDeviceSetWithError:" => Id, &mut error => *mut Id),
        )?;
        let allocated = msg!(class(c"NSUUID")?, c"alloc" => Id);
        let uuid = Object(msg!(allocated, c"initWithUUIDString:" => Id, string(udid)? => Id));
        if uuid.0.is_null() {
            return Err(failed("Cannot construct Simulator UUID"));
        }
        let devices = msg!(set.0, c"devicesByUDID" => Id);
        let device = Object::retain(msg!(devices, c"objectForKey:" => Id, uuid.0 => Id))?;
        if msg!(device.0, c"state" => u64) != 3 {
            return Err(Error::StaleSession);
        }
        let io = msg!(device.0, c"io" => Id);
        let ports = msg!(io, c"ioPorts" => Id);
        let count = msg!(ports, c"count" => usize).min(128);
        for index in 0..count {
            let port = msg!(ports, c"objectAtIndex:" => Id, index => usize);
            let descriptor = msg!(port, c"descriptor" => Id);
            if !conforms(descriptor, c"SimDisplayIOSurfaceRenderable")
                || !conforms(descriptor, c"SimDisplayRenderable")
                || !conforms(descriptor, c"SimScreen")
                || !responds(descriptor, c"state")
            {
                continue;
            }
            let state = msg!(descriptor, c"state" => Id);
            if msg!(state, c"displayClass" => u16) != 0 {
                continue;
            }
            portrait(descriptor)?;
            let screen = Object::retain(descriptor)?;
            let surface = Object::retain(msg!(descriptor, c"framebufferSurface" => Id))?;
            let (width, height, surface_id) = unsafe {
                (
                    IOSurfaceGetWidth(surface.0),
                    IOSurfaceGetHeight(surface.0),
                    IOSurfaceGetID(surface.0),
                )
            };
            return Ok(Self {
                _context: context,
                _set: set,
                device,
                screen,
                surface,
                surface_id,
                width,
                height,
            });
        }
        Err(Error::Unsupported {
            feature: "Xcode did not expose a primary Simulator framebuffer".into(),
        })
    }

    fn validate(&self) -> Result<()> {
        if msg!(self.device.0, c"state" => u64) != 3 {
            return Err(Error::StaleSession);
        }
        portrait(self.screen.0)?;
        let surface = msg!(self.screen.0, c"framebufferSurface" => Id);
        if surface.is_null() {
            return Err(Error::StaleSession);
        }
        if unsafe { IOSurfaceGetID(surface) } != self.surface_id
            || unsafe { IOSurfaceGetWidth(surface) } != self.width
            || unsafe { IOSurfaceGetHeight(surface) } != self.height
        {
            return Err(failed(
                "Simulator display changed; reconnect to refresh preview geometry",
            ));
        }
        Ok(())
    }
}

fn portrait(screen: Id) -> Result<()> {
    let properties = msg!(screen, c"screenProperties" => Id);
    if properties.is_null()
        || !responds(properties, c"uiOrientation")
        || msg!(properties, c"uiOrientation" => u32) != 1
    {
        return Err(Error::Unsupported { feature: "iOS preview currently requires upright portrait orientation; rotate back and reconnect".into() });
    }
    Ok(())
}

pub(super) fn run(
    config: CaptureConfig,
    control: Arc<Control>,
    sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    input_rx: std::sync::mpsc::Receiver<InputWork>,
    geometry_ready: tokio::sync::oneshot::Sender<Geometry>,
) -> Result<()> {
    let mut pending = PendingInput {
        receiver: input_rx,
        deferred: None,
    };
    let _pool = Pool::new();
    if boot_identity(&config.udid)? != config.boot_id {
        return Err(Error::StaleSession);
    }
    let binding = Binding::new(&config.udid)?;
    let mut input = if config.input_enabled {
        Some(input::InputSession::connect(binding.device.0)?)
    } else {
        None
    };
    let (width, height) = dimensions(binding.width, binding.height, config.max_size)?;
    let mut source = null_mut();
    status(
        unsafe {
            CVPixelBufferCreateWithIOSurface(null_mut(), binding.surface.0, null_mut(), &mut source)
        },
        "Wrapping Simulator surface",
    )?;
    let source = Cf(source);
    let mut encoder = encoder::Encoder::new(&config, width, height, sender.clone())?;
    if boot_identity(&config.udid)? != config.boot_id {
        return Err(Error::StaleSession);
    }
    binding.validate()?;
    let geometry = Geometry {
        width,
        height,
        display_width: binding.width as u32,
        display_height: binding.height as u32,
        rotation: 0,
    };
    geometry.validate()?;
    let mut line =
        serde_json::to_vec(&geometry).map_err(|_| failed("Cannot serialize capture geometry"))?;
    line.push(b'\n');
    sender
        .try_send(line)
        .map_err(|_| failed("Capture output closed during startup"))?;
    geometry_ready
        .send(geometry)
        .map_err(|_| failed("Capture geometry receiver closed"))?;
    let start = Instant::now();
    let cadence = Duration::from_secs_f64(1.0 / f64::from(config.max_fps));
    let mut first_frame = true;
    let mut last_keyframe = start;
    let mut check_boot_at = start;
    let mut next_frame_at = start;
    while !control.stop.load(Ordering::Acquire) && !sender.is_closed() {
        let tick = Instant::now();
        let _iteration_pool = Pool::new();
        for _ in 0..8 {
            let Some(work) = pending.next() else {
                break;
            };
            match work {
                InputWork::Apply { events, reply } => {
                    if reply.is_closed() {
                        continue;
                    }
                    let result = (|| {
                        if control.stop.load(Ordering::Acquire)
                            || boot_identity(&config.udid)? != config.boot_id
                        {
                            return Err(Error::StaleSession);
                        }
                        binding.validate()?;
                        // Preflight may outlast the caller's deadline. Never inject an abandoned
                        // request after waiting on launchctl or a CoreSimulator proxy.
                        if reply.is_closed() || control.stop.load(Ordering::Acquire) {
                            return Err(Error::StaleSession);
                        }
                        let input = input.as_mut().ok_or_else(|| Error::Unsupported {
                            feature: "This preview has no Simulator input connection".into(),
                        })?;
                        input.apply(&events)
                    })();
                    let _ = reply.send(result);
                }
                InputWork::Release { reply } => {
                    let result = input.as_mut().map_or(Ok(()), |input| input.release_all());
                    let _ = reply.send(result);
                }
            }
        }
        binding.validate()?;
        if tick >= check_boot_at {
            if boot_identity(&config.udid)? != config.boot_id {
                return Err(Error::StaleSession);
            }
            check_boot_at = tick + Duration::from_millis(500);
        }
        encoder.check_error()?;
        let frame_tick = Instant::now();
        if frame_tick >= next_frame_at {
            next_frame_at = frame_tick + cadence;
            let keyframe = first_frame
                || control.key_frame.swap(false, Ordering::AcqRel)
                || encoder.needs_keyframe()
                || frame_tick.duration_since(last_keyframe) >= Duration::from_secs(2);
            // The lock is advisory; seed changes detect some concurrent writes, not all GPU writes.
            status(
                unsafe { IOSurfaceLock(binding.surface.0, 1, null_mut()) },
                "Locking Simulator surface",
            )?;
            let before = unsafe { IOSurfaceGetSeed(binding.surface.0) };
            let copied = encoder.copy(source.0);
            let after = unsafe { IOSurfaceGetSeed(binding.surface.0) };
            let unlocked = unsafe { IOSurfaceUnlock(binding.surface.0, 1, null_mut()) };
            copied?;
            status(unlocked, "Unlocking Simulator surface")?;
            if before == after {
                binding.validate()?;
                encoder.encode(
                    frame_tick.duration_since(start).as_micros() as u64,
                    keyframe,
                )?;
                first_frame = false;
                if keyframe {
                    last_keyframe = frame_tick;
                }
            }
        }
        // Input wakes the owner independently of the video cadence. At 1 FPS, a Home-up must
        // not wait a second and turn into a long press; capture still obeys next_frame_at.
        pending.wait(
            next_frame_at
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50)),
        );
    }
    encoder.check_error()?;
    input.as_mut().map_or(Ok(()), |input| input.release_all())
}

struct PendingInput {
    receiver: std::sync::mpsc::Receiver<InputWork>,
    deferred: Option<InputWork>,
}
impl PendingInput {
    fn next(&mut self) -> Option<InputWork> {
        self.deferred
            .take()
            .or_else(|| self.receiver.try_recv().ok())
    }
    fn wait(&mut self, duration: Duration) {
        match self.receiver.recv_timeout(duration) {
            Ok(work) => self.deferred = Some(work),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            // The stop flag is observed next iteration; avoid spinning if the producer vanished.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => thread::sleep(duration),
        }
    }
}
impl Drop for PendingInput {
    fn drop(&mut self) {
        while let Some(work) = self.next() {
            let reply = match work {
                InputWork::Apply { reply, .. } | InputWork::Release { reply } => reply,
            };
            let _ = reply.send(Err(Error::StaleSession));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn work_that_woke_the_owner_is_rejected_if_capture_then_stops() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        let (reply, mut result) = tokio::sync::oneshot::channel();
        sender.send(InputWork::Release { reply }).unwrap();
        let mut pending = PendingInput {
            receiver,
            deferred: None,
        };
        pending.wait(Duration::from_secs(1));
        assert!(pending.deferred.is_some());
        drop(pending);
        assert!(matches!(result.try_recv(), Ok(Err(Error::StaleSession))));
    }

    #[test]
    fn boot_pid_parser_rejects_stopped_missing_and_duplicate_identity() {
        assert_eq!(parse_pid(" state = running\n pid = 42\n").unwrap(), 42);
        for bad in [
            "state = stopped\npid = 42",
            "state = running",
            "state = running\npid = 1\npid = 2",
            "state = running\npid = -1",
        ] {
            assert!(parse_pid(bad).is_err());
        }
    }
}
