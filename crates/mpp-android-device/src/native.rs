use std::{
    io::{BufReader, Read, Write},
    net::Shutdown,
    os::{
        android::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixListener, UnixStream},
    },
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use jni::{Env, EnvUnowned, objects::JClass};
use mpp_core::{
    Error,
    media::{Packet, PacketBody, encode},
    stream::{
        Channel, ControlCommand, ControlReply, ControlRequest, DeviceConfig, DeviceHello, Geometry,
        InputState, MAX_CONTROL_BYTES, MEDIA_MAX_PAYLOAD,
    },
};
use serde::{Deserialize, Serialize};

use crate::{
    codec::Encoder,
    platform::{self, Capture, DisplayInfo, Injector, Result, failure},
    wire::{self, Line, ParameterSets},
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
const PACKET_TIMEOUT: Duration = Duration::from_secs(1);
const INPUT_TIMEOUT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(100);

struct SocketReader {
    inner: BufReader<UnixStream>,
    deadline: Instant,
}

impl Read for SocketReader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "device message deadline exceeded",
                )
            })?;
        if self.inner.buffer().is_empty() {
            self.inner
                .get_ref()
                .set_read_timeout(Some(POLL.min(remaining)))?;
        }
        self.inner.read(bytes)
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_mpp_Bootstrap_run<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> i32 {
    unowned
        .with_env(|env| -> jni::errors::Result<i32> {
            match run(env) {
                Ok(()) => Ok(0),
                Err(error) => {
                    env.exception_clear();
                    eprintln!("MPP device: {error}");
                    Ok(1)
                }
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

fn configuration(stop: Arc<AtomicBool>) -> Result<DeviceConfig> {
    let (send, receive) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("mpp-stdin".into())
        .spawn(move || {
            let mut input = BufReader::new(std::io::stdin());
            let config = (|| -> Result<DeviceConfig> {
                let Line::Ready(bytes) = wire::read_line(
                    &mut input,
                    &mut Vec::new(),
                    MAX_CONTROL_BYTES,
                    Some(Instant::now() + HANDSHAKE_TIMEOUT),
                )?
                else {
                    return Err(failure("device configuration was not supplied"));
                };
                let config: DeviceConfig = serde_json::from_slice(&bytes)
                    .map_err(|_| failure("invalid device configuration JSON"))?;
                config.validate()?;
                Ok(config)
            })();
            if config.is_err() {
                stop.store(true, Ordering::Release);
            }
            if send.send(config).is_err() {
                return;
            }
            let mut extra = [0];
            // Ownership is maintained by an open stdin pipe, never by synthetic input heartbeats.
            loop {
                match input.read(&mut extra) {
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    _ => {
                        stop.store(true, Ordering::Release);
                        break;
                    }
                }
            }
        })?;
    receive
        .recv_timeout(HANDSHAKE_TIMEOUT)
        .map_err(|_| failure("device configuration deadline exceeded"))?
}

fn connect(config: &DeviceConfig, channel: Channel, geometry: Geometry) -> Result<UnixStream> {
    let address = SocketAddr::from_abstract_name(config.socket_name.as_bytes())?;
    let (send, receive) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("mpp-channel".into())
        .spawn(move || {
            let result = UnixStream::connect_addr(&address);
            let _ = send.send(result);
        })?;
    let mut stream = receive
        .recv_timeout(HANDSHAKE_TIMEOUT)
        .map_err(|_| failure("device channel connect deadline exceeded"))??;
    stream.set_read_timeout(Some(POLL))?;
    let hello = DeviceHello {
        protocol: config.protocol.clone(),
        token: config.token.clone(),
        channel,
        generation: config.generation,
        epoch: config.epoch,
        geometry,
    };
    write_json(&mut stream, &hello, HANDSHAKE_TIMEOUT)?;
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut reader = SocketReader {
        // Do not read past ACK: subsequent control bytes belong to the control worker.
        inner: BufReader::with_capacity(1, stream.try_clone()?),
        deadline,
    };
    let mut pending = Vec::new();
    loop {
        match wire::read_line(&mut reader, &mut pending, MAX_CONTROL_BYTES, Some(deadline))? {
            Line::Pending => continue,
            Line::Eof => return Err(failure("host closed the channel before authentication")),
            Line::Ready(bytes) => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Ack {
                    ok: bool,
                }
                let ack: Ack = serde_json::from_slice(&bytes)
                    .map_err(|_| failure("invalid host channel acknowledgement"))?;
                if !ack.ok {
                    return Err(failure("host rejected the device channel"));
                }
                return Ok(stream);
            }
        }
    }
}

/// SO_SNDTIMEO applies per syscall; shrink it after every partial write to bound a whole packet.
fn write_packet(stream: &mut UnixStream, bytes: &[u8], timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut position = 0;
    while position < bytes.len() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| failure("device packet write deadline exceeded"))?;
        stream.set_write_timeout(Some(remaining))?;
        match stream.write(&bytes[position..]) {
            Ok(0) => return Err(failure("host closed the device channel")),
            Ok(written) => position += written,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    if Instant::now() >= deadline {
        return Err(failure("device packet write deadline exceeded"));
    }
    Ok(())
}

fn write_json(stream: &mut UnixStream, message: &impl Serialize, timeout: Duration) -> Result<()> {
    let mut bytes =
        serde_json::to_vec(message).map_err(|_| failure("could not encode device response"))?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(failure("device response exceeds limit"));
    }
    bytes.push(b'\n');
    write_packet(stream, &bytes, timeout)
}

fn run(env: &mut Env<'_>) -> Result<()> {
    platform::check_platform(env)?;
    let stop = Arc::new(AtomicBool::new(false));
    let config = configuration(stop.clone())?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let lock = SocketAddr::from_abstract_name(b"mpp-control-owner-v1")?;
    let _owner = UnixListener::bind_addr(&lock).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            Error::Busy {
                device: "Android main display".into(),
                owner: "another MPP device process".into(),
            }
        } else {
            Error::Io(error)
        }
    })?;
    let display = platform::display_info(env)?;
    let geometry = display.geometry(config.max_size)?;
    let mut video = connect(&config, Channel::Video, geometry)?;
    let control = connect(&config, Channel::Control, geometry)?;
    let control_shutdown = control.try_clone()?;
    let video_shutdown = video.try_clone()?;
    let keyframe = Arc::new(AtomicBool::new(false));
    let vm = env.get_java_vm()?;
    let input_stop = stop.clone();
    let input_keyframe = keyframe.clone();
    let epoch = config.epoch;
    let worker = thread::Builder::new()
        .name("mpp-input".into())
        .spawn(move || {
            let result = vm.attach_current_thread_for_scope(|env| {
                control_loop(
                    env,
                    control,
                    geometry,
                    display,
                    epoch,
                    input_stop.clone(),
                    input_keyframe,
                )
            });
            input_stop.store(true, Ordering::Release);
            let _ = video_shutdown.shutdown(Shutdown::Both);
            if let Err(error) = result {
                eprintln!("MPP input: {error}");
            }
        })?;
    let result = video_loop(
        env, &mut video, &config, display, geometry, &stop, &keyframe,
    );
    stop.store(true, Ordering::Release);
    let _ = control_shutdown.shutdown(Shutdown::Both);
    let _ = video.shutdown(Shutdown::Both);
    if worker.join().is_err() {
        return Err(failure("device input worker failed"));
    }
    result
}

fn video_loop(
    env: &mut Env<'_>,
    stream: &mut UnixStream,
    config: &DeviceConfig,
    display: DisplayInfo,
    geometry: Geometry,
    stop: &AtomicBool,
    keyframe: &AtomicBool,
) -> Result<()> {
    let mut encoder = Encoder::configure(
        geometry.width,
        geometry.height,
        config.bit_rate,
        config.max_fps,
    )?;
    let _capture = Capture::new(env, encoder.surface(), display, geometry)?;
    encoder.start()?;
    encoder.request_key_frame()?;
    let mut sets = ParameterSets::default();
    let mut sent_configuration = None;
    let mut first_frame = false;
    let started = Instant::now();
    let mut checked_geometry = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if checked_geometry.elapsed() >= POLL {
            if platform::display_info(env)? != display {
                return Err(failure("display geometry changed; reconnect preview"));
            }
            checked_geometry = Instant::now();
        }
        if keyframe.swap(false, Ordering::AcqRel) {
            encoder.request_key_frame()?;
        }
        if !first_frame && started.elapsed() > Duration::from_secs(5) {
            return Err(failure("display capture produced no decodable first frame"));
        }
        let Some(frame) = encoder.next()? else {
            if encoder.ended() {
                return Err(failure("encoder ended unexpectedly"));
            }
            continue;
        };
        if frame.configuration {
            sets.update(&frame.bytes)?;
        } else if sent_configuration.is_none()
            && let Some(configuration) = encoder.output_configuration()?
        {
            sets.update(&configuration.bytes)?;
        }
        if let Some(configuration) = sets.configuration()
            && sent_configuration.as_ref() != Some(&configuration)
        {
            if first_frame {
                return Err(failure("encoder configuration changed; reconnect preview"));
            }
            let packet = Packet {
                generation: config.generation,
                pts_us: 0,
                body: PacketBody::Configuration {
                    width: geometry.width,
                    height: geometry.height,
                    annex_b: configuration.clone(),
                },
            };
            write_packet(stream, &encode(&packet, MEDIA_MAX_PAYLOAD)?, PACKET_TIMEOUT)?;
            sent_configuration = Some(configuration);
        }
        if frame.configuration {
            continue;
        }
        if !first_frame && (!frame.key_frame || sent_configuration.is_none()) {
            continue;
        }
        let body = if frame.key_frame {
            PacketBody::KeyFrame(frame.bytes)
        } else {
            PacketBody::DeltaFrame(frame.bytes)
        };
        write_packet(
            stream,
            &encode(
                &Packet {
                    generation: config.generation,
                    pts_us: frame.pts_us,
                    body,
                },
                MEDIA_MAX_PAYLOAD,
            )?,
            PACKET_TIMEOUT,
        )?;
        first_frame = true;
    }
    Ok(())
}

fn control_loop(
    env: &mut Env<'_>,
    mut stream: UnixStream,
    geometry: Geometry,
    display: DisplayInfo,
    epoch: u64,
    stop: Arc<AtomicBool>,
    keyframe: Arc<AtomicBool>,
) -> Result<()> {
    stream.set_read_timeout(Some(POLL))?;
    let mut reader = SocketReader {
        inner: BufReader::new(stream.try_clone()?),
        deadline: Instant::now() + PACKET_TIMEOUT,
    };
    let mut injector = Injector::new(env, geometry)?;
    let mut state = InputState::new(epoch, geometry.width, geometry.height)?;
    let mut pending = Vec::new();
    let mut deadline = None;
    let mut last_valid = Instant::now();
    let result = (|| -> Result<()> {
        while !stop.load(Ordering::Acquire) {
            if platform::display_info(env)? != display {
                return Err(failure("display geometry changed; input stopped"));
            }
            if injector.pressed() && last_valid.elapsed() >= INPUT_TIMEOUT {
                state.drain_releases();
                injector.release_all(env);
                if injector.pressed() {
                    return Err(failure("could not cancel expired input"));
                }
            }
            let message_deadline = *deadline.get_or_insert_with(|| Instant::now() + PACKET_TIMEOUT);
            reader.deadline = message_deadline;
            let bytes = match wire::read_line(
                &mut reader,
                &mut pending,
                MAX_CONTROL_BYTES,
                Some(message_deadline),
            )? {
                Line::Pending => {
                    if pending.is_empty() {
                        deadline = None;
                    }
                    continue;
                }
                Line::Eof => return Ok(()),
                Line::Ready(bytes) => {
                    deadline = None;
                    bytes
                }
            };
            let request: ControlRequest = serde_json::from_slice(&bytes)
                .map_err(|_| failure("invalid device control JSON"))?;
            let actions = match state.accept(&request) {
                Ok(actions) => actions,
                Err(error) => {
                    write_json(
                        &mut stream,
                        &ControlReply {
                            seq: request.seq,
                            ok: false,
                            code: Some(error.code().into()),
                            message: Some(error.to_string()),
                        },
                        PACKET_TIMEOUT,
                    )?;
                    continue;
                }
            };
            last_valid = Instant::now();
            for action in actions {
                if let Err(error) = injector.inject(env, &action) {
                    let _ = write_json(
                        &mut stream,
                        &ControlReply {
                            seq: request.seq,
                            ok: false,
                            code: Some("INPUT_REJECTED".into()),
                            message: Some("Android rejected input; stream stopped".into()),
                        },
                        PACKET_TIMEOUT,
                    );
                    return Err(error);
                }
            }
            if matches!(request.command, ControlCommand::KeyFrame) {
                keyframe.store(true, Ordering::Release);
            }
            write_json(
                &mut stream,
                &ControlReply {
                    seq: request.seq,
                    ok: true,
                    code: None,
                    message: None,
                },
                PACKET_TIMEOUT,
            )?;
            if matches!(request.command, ControlCommand::Stop) {
                return Ok(());
            }
        }
        Ok(())
    })();
    env.exception_clear();
    state.drain_releases();
    injector.release_all(env);
    stop.store(true, Ordering::Release);
    result
}
