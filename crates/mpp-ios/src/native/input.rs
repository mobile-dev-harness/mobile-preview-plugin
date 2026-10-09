//! DTUHID wire protocol, pinned to facebook/idb c9ec2501362d97b775d818db8660fdee145f32ea
//! (DTUHIDModels.swift / SimulatorXPCConnection.swift). CoreSimulator 1155.4+ can silently
//! discard legacy Indigo messages. Real events precede an acknowledged no-input barrier;
//! that reply confirms ordered submission to the peer, never application-level completion.
use super::super::failed;
use super::{Object, ffi::*, string};
use mpp_core::{Error, InputEvent, KeyPhase, Result, TouchPhase};
use std::{
    ffi::CStr,
    ptr::null_mut,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const SERVICE: &CStr = c"com.apple.coredevice.feature.remote.hid.digitizer";
const ACK_TIMEOUT: Duration = Duration::from_millis(750);
const RELEASE_TIMEOUT: Duration = Duration::from_millis(200);

struct Xpc(Id);
impl Xpc {
    fn new(pointer: Id) -> Result<Self> {
        if pointer.is_null() {
            Err(failed(
                "Could not allocate Simulator HID connection/message",
            ))
        } else {
            Ok(Self(pointer))
        }
    }
    fn dictionary() -> Result<Self> {
        Self::new(unsafe { xpc_dictionary_create(null_mut(), null_mut(), 0) })
    }
}
impl Drop for Xpc {
    fn drop(&mut self) {
        unsafe { xpc_release(self.0) }
    }
}
struct Queue(Id);
impl Drop for Queue {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { dispatch_release(self.0) }
        }
    }
}

#[derive(Default)]
struct Answer {
    result: Mutex<Option<Result<()>>>,
    ready: Condvar,
    disconnected: AtomicBool,
}

// Blocks copy/dispose owns one Arc only for each copied heap block, so late XPC callbacks
// remain safe after a request timed out and its native owner returned.
#[repr(C)]
struct Block {
    isa: Id,
    flags: i32,
    reserved: i32,
    invoke: unsafe extern "C" fn(*mut Block, Id),
    descriptor: *const Descriptor,
    context: *const Answer,
}
#[repr(C)]
struct Descriptor {
    reserved: usize,
    size: usize,
    copy: unsafe extern "C" fn(*mut Block, *const Block),
    dispose: unsafe extern "C" fn(*mut Block),
    signature: *const std::ffi::c_char,
}
// All descriptor fields are immutable code/static-data addresses.
unsafe impl Sync for Descriptor {}
static DESCRIPTOR: Descriptor = Descriptor {
    reserved: 0,
    size: std::mem::size_of::<Block>(),
    copy: copy_block,
    dispose: dispose_block,
    signature: c"v16@?0@8".as_ptr(),
};
unsafe extern "C" fn copy_block(destination: *mut Block, source: *const Block) {
    unsafe {
        (*destination).context = (*source).context;
        Arc::increment_strong_count((*source).context);
    }
}
unsafe extern "C" fn dispose_block(block: *mut Block) {
    unsafe {
        Arc::decrement_strong_count((*block).context);
    }
}
struct OwnedBlock(Id);
impl OwnedBlock {
    fn new(context: &Arc<Answer>, invoke: unsafe extern "C" fn(*mut Block, Id)) -> Result<Self> {
        let stack = Block {
            isa: (&raw const _NSConcreteStackBlock).cast_mut().cast(),
            flags: (1 << 25) | (1 << 30),
            reserved: 0,
            invoke,
            descriptor: &DESCRIPTOR,
            context: Arc::as_ptr(context),
        };
        // The stack borrow remains live through the synchronous copy, whose helper retains context.
        let copy = unsafe { _Block_copy((&stack as *const Block).cast_mut().cast()) };
        if copy.is_null() {
            Err(failed("Could not allocate Simulator HID callback"))
        } else {
            Ok(Self(copy))
        }
    }
}
impl Drop for OwnedBlock {
    fn drop(&mut self) {
        unsafe { _Block_release(self.0) }
    }
}

unsafe extern "C" fn connection_event(block: *mut Block, event: Id) {
    let context = unsafe { &*(*block).context };
    if event.is_null()
        || unsafe { xpc_get_type(event) } == (&raw const _xpc_type_error).cast_mut().cast()
    {
        context.disconnected.store(true, Ordering::Release);
    }
}
unsafe extern "C" fn reply_event(block: *mut Block, reply: Id) {
    let context = unsafe { &*(*block).context };
    let outcome = std::panic::catch_unwind(|| validate_reply(reply))
        .unwrap_or_else(|_| Err(failed("Simulator HID reply callback failed")));
    let mut result = context
        .result
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if result.is_none() {
        *result = Some(outcome);
        context.ready.notify_all();
    }
}
fn validate_reply(reply: Id) -> Result<()> {
    // An empty dictionary is DTUHID's positive reply. Decode failures carry a nonempty error
    // dictionary; XPC itself reports disconnected peers with a distinct error object.
    if !reply.is_null()
        && unsafe { xpc_get_type(reply) } == (&raw const _xpc_type_dictionary).cast_mut().cast()
        && unsafe { xpc_dictionary_get_count(reply) } == 0
    {
        return Ok(());
    }
    let detail = if reply.is_null() {
        "empty reply".to_owned()
    } else {
        let description = unsafe { xpc_copy_description(reply) };
        if description.is_null() {
            "unrecognized reply".to_owned()
        } else {
            let detail: String = unsafe { CStr::from_ptr(description.cast()) }
                .to_string_lossy()
                .chars()
                .take(512)
                .collect();
            unsafe { free(description) };
            detail
        }
    };
    Err(failed(&format!(
        "Simulator HID rejected the event: {detail}"
    )))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HeldTouch {
    x: f64,
    y: f64,
    edge: u64,
}

impl HeldTouch {
    fn start(x: f64, y: f64) -> Self {
        // MPP policy: the bottom 2% is an edge origin, independent of video scaling.
        // idb's SimulatorHIDTypes.swift maps bottom to 3; SimulatorHIDEvent.swift requires
        // that origin on every sample, even after the contact moves into the screen interior.
        Self {
            x,
            y,
            edge: if y >= 0.98 { 3 } else { 0 },
        }
    }

    fn prepare(
        held: &mut Option<Self>,
        phase: TouchPhase,
        x: f64,
        y: f64,
    ) -> Result<Option<(Self, u64)>> {
        let (contact, event_type) = match (phase, *held) {
            (TouchPhase::Down, None) => (Self::start(x, y), 0),
            (TouchPhase::Move, Some(contact)) => (Self { x, y, ..contact }, 1),
            // DTUHID exposes end, not UIKit cancel. Releasing can complete a tap.
            (TouchPhase::Up | TouchPhase::Cancel, Some(contact)) => (Self { x, y, ..contact }, 2),
            (TouchPhase::Cancel, None) => return Ok(None),
            _ => return Err(failed("Simulator touch sequence is invalid")),
        };
        // Possible submissions remain in the ledger until an end is acknowledged, including
        // the original edge so reset or partial-failure cleanup finishes the same contact.
        if event_type != 2 {
            *held = Some(contact);
        }
        Ok(Some((contact, event_type)))
    }
}

pub(super) struct InputSession {
    // Device retained with the connection: the endpoint is permanently scoped to this boot.
    _device: Object,
    connection: Xpc,
    queue: Queue,
    lifecycle: Arc<Answer>,
    pointer: Option<HeldTouch>,
    home_down: bool,
    failed: bool,
    last_send: Option<Instant>,
}

impl InputSession {
    pub fn connect(device: Id) -> Result<Self> {
        type Endpoint = unsafe extern "C" fn(u32, u64, u64) -> Id;
        type Enable = unsafe extern "C" fn(Id);
        // RTLD_DEFAULT on macOS is -2. Private symbols are capability-probed, never linked strongly.
        let handle = (-2isize) as Id;
        let endpoint_symbol =
            unsafe { dlsym(handle, c"xpc_endpoint_create_mach_port_4sim".as_ptr()) };
        let enable_symbol =
            unsafe { dlsym(handle, c"xpc_connection_enable_sim2host_4sim".as_ptr()) };
        if endpoint_symbol.is_null() || enable_symbol.is_null() {
            return Err(Error::Unsupported {
                feature: "The installed CoreSimulator does not expose DTUHID input".into(),
            });
        }
        let endpoint_from_port: Endpoint = unsafe { std::mem::transmute(endpoint_symbol) };
        let enable_simulator: Enable = unsafe { std::mem::transmute(enable_symbol) };
        let retained_device = Object::retain(device)?;
        let queue = Queue(unsafe { dispatch_queue_create(c"mpp.ios.hid".as_ptr(), null_mut()) });
        if queue.0.is_null() {
            return Err(failed("Cannot create Simulator HID callback queue"));
        }
        let lifecycle = Arc::new(Answer::default());
        let handler = OwnedBlock::new(&lifecycle, connection_event)?;
        let mut error = null_mut();
        let port = msg!(device, c"lookup:error:" => u32, string(SERVICE.to_str().unwrap_or_default())? => Id, &mut error => *mut Id);
        if port == 0 {
            return Err(Error::Unsupported {
                feature: "This simulator boot does not provide DTUHID input".into(),
            });
        }
        // The endpoint consumes the send right obtained from SimDevice.lookup.
        let endpoint = Xpc::new(unsafe { endpoint_from_port(port, 0, 0) })?;
        let connection = Xpc::new(unsafe { xpc_connection_create_from_endpoint(endpoint.0) })?;
        unsafe {
            enable_simulator(connection.0);
            xpc_connection_set_target_queue(connection.0, queue.0);
            xpc_connection_set_event_handler(connection.0, handler.0);
            xpc_connection_resume(connection.0);
        }
        let session = Self {
            _device: retained_device,
            connection,
            queue,
            lifecycle,
            pointer: None,
            home_down: false,
            failed: false,
            last_send: None,
        };
        // Usage zero means "no event indicated". This activates/proves the peer without input.
        session.request(&barrier()?, Duration::from_secs(2))?;
        // DTUHID acknowledges activation before opening its virtual device. Match idb's bounded
        // activation tail; this is transport readiness, not an application-completion guarantee.
        std::thread::sleep(Duration::from_millis(200));
        Ok(session)
    }

    fn request(&self, message: &Xpc, timeout: Duration) -> Result<()> {
        if self.lifecycle.disconnected.load(Ordering::Acquire) {
            return Err(failed("Simulator HID connection closed; reconnect preview"));
        }
        let answer = Arc::new(Answer::default());
        let callback = OwnedBlock::new(&answer, reply_event)?;
        unsafe {
            xpc_connection_send_message_with_reply(
                self.connection.0,
                message.0,
                self.queue.0,
                callback.0,
            );
        }
        let deadline = Instant::now() + timeout;
        let mut result = answer
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while result.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Timeout {
                    operation: "waiting for Simulator HID acknowledgement".into(),
                });
            }
            let wait = answer
                .ready
                .wait_timeout(result, remaining)
                .unwrap_or_else(|error| error.into_inner());
            result = wait.0;
        }
        result
            .take()
            .unwrap_or_else(|| Err(failed("Simulator HID acknowledgement disappeared")))
    }

    fn deliver(&mut self, message: &Xpc, timeout: Duration) -> Result<()> {
        if self.lifecycle.disconnected.load(Ordering::Acquire) {
            return Err(failed("Simulator HID connection closed; reconnect preview"));
        }
        // Real events are one-way with isBarrier=false. A separate no-event barrier gets the
        // daemon's positive reply after the preceding message; a barrier carrying touch is ignored.
        unsafe { xpc_connection_send_message(self.connection.0, message.0) };
        self.last_send = Some(Instant::now());
        self.request(&barrier()?, timeout)
    }

    pub fn apply(&mut self, events: &[InputEvent]) -> Result<()> {
        if self.failed {
            return Err(failed("Simulator HID session failed; reconnect preview"));
        }
        for event in events {
            validate_event(event)?;
        }
        for event in events {
            let result = self.apply_one(event);
            if let Err(error) = result {
                self.failed = true;
                let _ = self.release_all();
                return Err(error);
            }
        }
        Ok(())
    }

    fn apply_one(&mut self, event: &InputEvent) -> Result<()> {
        match event {
            InputEvent::Touch { phase, x, y, .. } => {
                let Some((contact, event_type)) =
                    HeldTouch::prepare(&mut self.pointer, *phase, *x, *y)?
                else {
                    return Ok(());
                };
                self.deliver(&touch(contact, event_type)?, ACK_TIMEOUT)?;
                if event_type == 2 {
                    self.pointer = None;
                }
            }
            InputEvent::Key { phase, .. } => {
                let down = *phase == KeyPhase::Down;
                if down == self.home_down {
                    return Err(failed("Simulator Home button sequence is invalid"));
                }
                if down {
                    self.home_down = true;
                }
                self.deliver(&home(down)?, ACK_TIMEOUT)?;
                if !down {
                    self.home_down = false;
                }
            }
            InputEvent::Text { .. } => unreachable!("validated above"),
        }
        Ok(())
    }

    pub fn release_all(&mut self) -> Result<()> {
        let mut failure = None;
        if let Some(contact) = self.pointer {
            match touch(contact, 2).and_then(|message| self.deliver(&message, RELEASE_TIMEOUT)) {
                Ok(()) => self.pointer = None,
                Err(error) => {
                    failure = Some(error);
                }
            }
        }
        if self.home_down {
            match home(false).and_then(|message| self.deliver(&message, RELEASE_TIMEOUT)) {
                Ok(()) => self.home_down = false,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
impl Drop for InputSession {
    fn drop(&mut self) {
        let _ = self.release_all();
        // The daemon reply precedes HID device dispatch; preserve the last event before closing.
        if let Some(remaining) = self
            .last_send
            .and_then(|last| Duration::from_millis(80).checked_sub(last.elapsed()))
        {
            std::thread::sleep(remaining);
        }
        unsafe { xpc_connection_cancel(self.connection.0) };
    }
}

fn validate_event(event: &InputEvent) -> Result<()> {
    event.validate()?;
    match event {
        InputEvent::Touch { .. } | InputEvent::Key { code: 3, .. } => Ok(()),
        _ => Err(Error::Unsupported {
            feature: "iOS Simulator input currently supports single-pointer gestures and Home only"
                .into(),
        }),
    }
}
fn envelope(kind: &CStr, payload: &Xpc, is_barrier: bool) -> Result<Xpc> {
    let message = Xpc::dictionary()?;
    unsafe {
        xpc_dictionary_set_string(message.0, c"messageType".as_ptr(), kind.as_ptr());
        xpc_dictionary_set_bool(message.0, c"isBarrier".as_ptr(), is_barrier);
        xpc_dictionary_set_string(message.0, c"featureIdentifier".as_ptr(), SERVICE.as_ptr());
        xpc_dictionary_set_value(message.0, c"payload".as_ptr(), payload.0);
    }
    Ok(message)
}
fn touch(contact: HeldTouch, phase: u64) -> Result<Xpc> {
    let point = Xpc::dictionary()?;
    let payload = Xpc::dictionary()?;
    unsafe {
        xpc_dictionary_set_double(point.0, c"x".as_ptr(), contact.x);
        xpc_dictionary_set_double(point.0, c"y".as_ptr(), contact.y);
        xpc_dictionary_set_value(payload.0, c"pointOne".as_ptr(), point.0);
        xpc_dictionary_set_uint64(payload.0, c"eventType".as_ptr(), phase);
        xpc_dictionary_set_uint64(payload.0, c"edge".as_ptr(), contact.edge);
        xpc_dictionary_set_uint64(payload.0, c"target".as_ptr(), 0);
    }
    envelope(c"IndigoDigitizerEvent", &payload, false)
}
fn home(down: bool) -> Result<Xpc> {
    let payload = Xpc::dictionary()?;
    unsafe {
        xpc_dictionary_set_uint64(payload.0, c"usagePage".as_ptr(), 12);
        xpc_dictionary_set_uint64(payload.0, c"usageCode".as_ptr(), 64);
        xpc_dictionary_set_uint64(payload.0, c"state".as_ptr(), if down { 1 } else { 2 });
    }
    envelope(c"IndigoButtonEvent", &payload, false)
}
fn barrier() -> Result<Xpc> {
    let payload = Xpc::dictionary()?;
    unsafe {
        xpc_dictionary_set_uint64(payload.0, c"usageCode".as_ptr(), 0);
        xpc_dictionary_set_uint64(payload.0, c"state".as_ptr(), 2);
    }
    envelope(c"IndigoKeyboardButtonEvent", &payload, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[link(name = "System")]
    unsafe extern "C" {
        fn xpc_dictionary_get_value(dictionary: Id, key: *const std::ffi::c_char) -> Id;
        fn xpc_dictionary_get_uint64(dictionary: Id, key: *const std::ffi::c_char) -> u64;
    }

    fn touch_edge(message: &Xpc) -> u64 {
        unsafe {
            let payload = xpc_dictionary_get_value(message.0, c"payload".as_ptr());
            assert!(!payload.is_null());
            assert_eq!(xpc_dictionary_get_uint64(payload, c"target".as_ptr()), 0);
            xpc_dictionary_get_uint64(payload, c"edge".as_ptr())
        }
    }

    #[test]
    fn bottom_touch_down_carries_the_system_edge_flag() {
        assert_eq!(
            touch_edge(&touch(HeldTouch::start(0.5, 0.98), 0).unwrap()),
            3
        );
    }

    fn sample(held: &mut Option<HeldTouch>, phase: TouchPhase, y: f64) -> (HeldTouch, u64) {
        HeldTouch::prepare(held, phase, 0.5, y).unwrap().unwrap()
    }

    #[test]
    fn bottom_origin_survives_moves_end_and_failed_end_cleanup() {
        for end in [TouchPhase::Up, TouchPhase::Cancel] {
            let mut held = None;
            for (phase, y, expected_type) in [
                (TouchPhase::Down, 0.9969, 0),
                (TouchPhase::Move, 0.513, 1),
                (end, 0.4, 2),
            ] {
                let (contact, event_type) = sample(&mut held, phase, y);
                assert_eq!(event_type, expected_type);
                assert_eq!(touch_edge(&touch(contact, event_type).unwrap()), 3);
            }
            // No acknowledgement: the real release_all path serializes this retained ledger.
            let retained = held.expect("end must retain possible held state until acknowledgement");
            assert_eq!(retained.y, 0.513);
            assert_eq!(touch_edge(&touch(retained, 2).unwrap()), 3);
        }
    }

    #[test]
    fn interior_origin_stays_ordinary_when_moved_to_the_bottom() {
        let mut held = None;
        for (phase, y) in [
            (TouchPhase::Down, 0.5),
            (TouchPhase::Move, 0.999),
            (TouchPhase::Up, 1.0),
        ] {
            let (contact, event_type) = sample(&mut held, phase, y);
            assert_eq!(touch_edge(&touch(contact, event_type).unwrap()), 0);
        }
        assert_eq!(touch_edge(&touch(held.unwrap(), 2).unwrap()), 0);
    }

    #[test]
    fn bottom_origin_threshold_is_inclusive_and_does_not_leak_to_new_gestures() {
        for (y, edge) in [(0.98 - f64::EPSILON, 0), (0.98, 3), (1.0, 3)] {
            let mut held = None;
            let (contact, phase) = sample(&mut held, TouchPhase::Down, y);
            assert_eq!(touch_edge(&touch(contact, phase).unwrap()), edge);
            sample(&mut held, TouchPhase::Up, 0.5);
            held = None; // Successful end acknowledgement, as in InputSession::apply_one.
            let (next, phase) = sample(&mut held, TouchPhase::Down, 0.5);
            assert_eq!(touch_edge(&touch(next, phase).unwrap()), 0);
        }
    }

    #[test]
    #[ignore = "requires explicit authorization and MPP_IOS_INPUT_TEST_UDID"]
    fn native_home_ack() {
        let udid =
            std::env::var("MPP_IOS_INPUT_TEST_UDID").expect("select the authorized simulator");
        crate::validate_udid(&udid).unwrap();
        let _pool = super::super::Pool::new();
        let binding = super::super::Binding::new(&udid).unwrap();
        let mut session = InputSession::connect(binding.device.0).unwrap();
        session
            .apply(&[
                InputEvent::Key {
                    code: 3,
                    phase: KeyPhase::Down,
                },
                InputEvent::Key {
                    code: 3,
                    phase: KeyPhase::Up,
                },
            ])
            .unwrap();
        session.release_all().unwrap();
        println!("Native Home down/up acknowledged by DTUHID");
    }

    #[test]
    fn heap_block_retains_context_until_last_copy_is_released() {
        let state = Arc::new(Answer::default());
        let block = OwnedBlock::new(&state, reply_event).unwrap();
        assert_eq!(Arc::strong_count(&state), 2);
        let copied = OwnedBlock(unsafe { _Block_copy(block.0) });
        drop(block);
        assert_eq!(Arc::strong_count(&state), 2);
        let reply = Xpc::dictionary().unwrap();
        let raw = copied.0.cast::<Block>();
        unsafe { ((*raw).invoke)(raw, reply.0) };
        assert!(state.result.lock().unwrap().as_ref().unwrap().is_ok());
        drop(copied);
        assert_eq!(Arc::strong_count(&state), 1);
    }
    #[test]
    fn late_reply_keeps_context_alive_after_the_waiter_is_dropped() {
        let state = Arc::new(Answer::default());
        let weak = Arc::downgrade(&state);
        let block = OwnedBlock::new(&state, reply_event).unwrap();
        drop(state);
        assert!(weak.upgrade().is_some());
        let reply = Xpc::dictionary().unwrap();
        let raw = block.0.cast::<Block>();
        unsafe { ((*raw).invoke)(raw, reply.0) };
        assert!(
            weak.upgrade()
                .unwrap()
                .result
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .is_ok()
        );
        drop(block);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn real_events_are_not_barriers_and_the_no_event_probe_is() {
        let touch = touch(HeldTouch::start(0.5, 0.5), 0).unwrap();
        let home = home(true).unwrap();
        let barrier = barrier().unwrap();
        unsafe {
            assert!(!xpc_dictionary_get_bool(touch.0, c"isBarrier".as_ptr()));
            assert!(!xpc_dictionary_get_bool(home.0, c"isBarrier".as_ptr()));
            assert!(xpc_dictionary_get_bool(barrier.0, c"isBarrier".as_ptr()));
        }
    }

    #[test]
    fn only_empty_dictionary_is_a_positive_acknowledgement() {
        assert!(validate_reply(null_mut()).is_err());
        let reply = Xpc::dictionary().unwrap();
        validate_reply(reply.0).unwrap();
        unsafe { xpc_dictionary_set_bool(reply.0, c"error".as_ptr(), true) };
        assert!(validate_reply(reply.0).is_err());
    }
    #[test]
    fn unsupported_keys_and_text_are_rejected_before_submission() {
        assert!(
            validate_event(&InputEvent::Key {
                code: 3,
                phase: KeyPhase::Down
            })
            .is_ok()
        );
        assert!(
            validate_event(&InputEvent::Key {
                code: 4,
                phase: KeyPhase::Down
            })
            .is_err()
        );
        assert!(
            validate_event(&InputEvent::Text {
                text: "private".into()
            })
            .is_err()
        );
    }
}
