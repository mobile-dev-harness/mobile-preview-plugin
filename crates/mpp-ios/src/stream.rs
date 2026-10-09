use std::{
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{Ios, process};
use mpp_core::{
    Device, DeviceKind, DeviceState, Error, Platform, Result,
    stream::{
        ControlCommand, ControlReply, ControlRequest, DEVICE_PROTOCOL, DeviceConfig, Geometry,
        MAX_CONTROL_BYTES,
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream},
    process::{Child, ChildStdout, Command},
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{Instant, timeout, timeout_at},
};

const START_TIMEOUT: Duration = Duration::from_secs(15);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_DIAGNOSTICS: usize = 8192;
const MAX_PENDING_STARTS: usize = 32;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(25);
const PIPE_CAPACITY: usize = 64 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(unix)]
type NativeControl = tokio::net::UnixStream;
#[cfg(not(unix))]
type NativeControl = DuplexStream;

#[derive(Debug, Default)]
pub(crate) struct StreamStarts {
    registry: Mutex<StartRegistry>,
    shutdown: tokio::sync::Mutex<()>,
}

#[derive(Debug, Default)]
struct StartRegistry {
    shutting_down: bool,
    tasks: Vec<StartTask>,
}

#[derive(Debug)]
struct StartTask {
    cancellation: watch::Sender<bool>,
    task: JoinHandle<()>,
}

/// Session credentials originate in the trusted host; capture runs over private child pipes.
pub struct StreamOptions {
    pub token: String,
    pub stream_id: String,
    pub generation: u64,
    pub epoch: u64,
    pub max_size: u32,
    pub bit_rate: u32,
    pub max_fps: u32,
}

pub struct RunningStream {
    pub geometry: Geometry,
    video: Option<DuplexStream>,
    control: Option<DuplexStream>,
    resources: RunningResources,
}

impl RunningStream {
    pub fn take_streams(&mut self) -> Result<(DuplexStream, DuplexStream)> {
        if self.video.is_none() || self.control.is_none() {
            return Err(invalid("Stream channels have already been taken"));
        }
        Ok((self.video.take().unwrap(), self.control.take().unwrap()))
    }

    pub async fn close(&mut self) -> Result<()> {
        self.video.take();
        self.control.take();
        self.resources.close().await
    }
}

impl Ios {
    pub async fn start_stream(
        &self,
        verified_device: &Device,
        options: StreamOptions,
    ) -> Result<RunningStream> {
        validate_options(&options)?;
        let ios = self.clone();
        let device = verified_device.clone();
        let (cancel, mut cancelled) = watch::channel(false);
        let mut cancellation = CancelOnDrop(Some(cancel.clone()));
        let (ready, result_ready) = oneshot::channel();
        let (claimed, result_claimed) = oneshot::channel();
        let result_slot = Arc::new(Mutex::new(None));
        let task_slot = result_slot.clone();
        {
            let mut registry = self
                .stream_starts
                .registry
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if registry.shutting_down {
                return Err(failed("iOS preview host is shutting down"));
            }
            registry.tasks.retain(|start| !start.task.is_finished());
            if registry.tasks.len() >= MAX_PENDING_STARTS {
                return Err(failed("Too many iOS preview startups are in progress"));
            }
            let task = tokio::spawn(async move {
                let result = ios.start_owned(device, options, cancelled.clone()).await;
                *task_slot.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
                if ready.send(()).is_ok() {
                    // Retain ownership until the caller actually claims the result. A successful
                    // oneshot send alone does not prove its waiting caller was not cancelled.
                    tokio::select! {
                        _ = result_claimed => {}
                        _ = wait_cancelled(&mut cancelled) => {}
                    }
                }
                let abandoned = task_slot
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take();
                if let Some(Ok(mut stream)) = abandoned {
                    let _ = stream.close().await;
                }
            });
            registry.tasks.push(StartTask {
                cancellation: cancel,
                task,
            });
        }
        result_ready
            .await
            .map_err(|_| failed("Stream startup task failed"))?;
        let result = {
            // Claiming a result and beginning shutdown have one ordering across all clones.
            let registry = self
                .stream_starts
                .registry
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if registry.shutting_down {
                return Err(cancelled_error());
            }
            result_slot
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
                .ok_or_else(cancelled_error)?
        };
        let _ = claimed.send(());
        cancellation.0.take();
        result
    }

    /// Stop accepting startups, cancel pending ones, and drain their owned cleanup before exit.
    pub async fn shutdown_stream_starts(&self) -> Result<()> {
        let _shutdown = self.stream_starts.shutdown.lock().await;
        let tasks = {
            let mut registry = self
                .stream_starts
                .registry
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            registry.shutting_down = true;
            for start in &registry.tasks {
                start.cancellation.send_replace(true);
            }
            std::mem::take(&mut registry.tasks)
        };
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        let mut failure = None;
        for mut start in tasks {
            match timeout_at(deadline, &mut start.task).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    failure.get_or_insert_with(|| {
                        failed("iOS preview startup task failed while shutting down")
                    });
                }
                Err(_) => {
                    // Do not abort an acquisition/cleanup task in the middle of an owned mutation.
                    failure.get_or_insert_with(|| Error::Timeout {
                        operation: "draining iOS preview startup cleanup".into(),
                    });
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    async fn start_owned(
        &self,
        device: Device,
        options: StreamOptions,
        mut cancelled: watch::Receiver<bool>,
    ) -> Result<RunningStream> {
        let deadline = Instant::now() + START_TIMEOUT;
        tokio::select! {
            _ = wait_cancelled(&mut cancelled) => return Err(cancelled_error()),
            result = timeout_at(deadline, self.verify_attachment(&device)) => {
                result.map_err(|_| startup_timeout())??;
            }
        }
        let mut resources = ChildResources::spawn(self, &device, &options)?;
        let mut stdout = BufReader::new(resources.child.stdout.take().unwrap());
        let ready = async {
            let bytes = read_line(&mut stdout)
                .await?
                .ok_or_else(|| failed("Capture helper exited before geometry"))?;
            let geometry: Geometry = serde_json::from_slice(&bytes)
                .map_err(|_| failed("Capture helper returned malformed geometry"))?;
            geometry.validate()?;
            if geometry.width.max(geometry.height) > options.max_size {
                return Err(failed(
                    "Capture geometry exceeds the negotiated maximum size",
                ));
            }
            self.verify_attachment(&device).await?;
            if let Some(status) = resources.child.try_wait()? {
                return Err(failed(&format!(
                    "Capture helper exited during startup ({status})"
                )));
            }
            Ok(geometry)
        };
        let result = tokio::select! {
            _ = wait_cancelled(&mut cancelled) => Err(cancelled_error()),
            result = timeout_at(deadline, ready) => result.map_err(|_| startup_timeout()).and_then(|result| result),
        };
        let geometry = match result {
            Ok(geometry) => geometry,
            Err(error) => {
                let diagnostics = resources.diagnostics.clone();
                resources.cleanup().await?;
                let message = String::from_utf8_lossy(
                    &diagnostics
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()),
                )
                .trim()
                .to_owned();
                return Err(match error {
                    Error::CommandFailed {
                        tool,
                        message: failure,
                    } if !message.is_empty() => Error::CommandFailed {
                        tool,
                        message: format!("{failure}: {message}"),
                    },
                    other => other,
                });
            }
        };
        let (video, video_peer) = tokio::io::duplex(PIPE_CAPACITY);
        let (control, control_peer) = tokio::io::duplex(MAX_CONTROL_BYTES * 2);
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(supervise(
            resources,
            stdout,
            video_peer,
            control_peer,
            options.epoch,
            device.capabilities.input,
            stopped,
        ));
        Ok(RunningStream {
            geometry,
            video: Some(video),
            control: Some(control),
            resources: RunningResources {
                stop,
                task: Some(task),
            },
        })
    }

    async fn verify_attachment(&self, expected: &Device) -> Result<()> {
        if expected.platform != Platform::Ios
            || expected.kind != DeviceKind::Simulator
            || expected.state != DeviceState::Online
            || expected.transport_id.is_none()
        {
            return Err(Error::StaleSession);
        }
        let current = self
            .probe(expected.serial.as_deref().ok_or(Error::StaleSession)?)
            .await?;
        if current.id != expected.id
            || current.transport_id != expected.transport_id
            || current.kind != expected.kind
            || current.avd != expected.avd
            || (expected.capabilities.input && !current.capabilities.input)
        {
            return Err(Error::StaleSession);
        }
        Ok(())
    }
}

struct ChildResources {
    child: Child,
    control: Option<NativeControl>,
    diagnostics: Arc<Mutex<Vec<u8>>>,
    diagnostic_task: Option<JoinHandle<()>>,
}

impl ChildResources {
    fn spawn(ios: &Ios, device: &Device, options: &StreamOptions) -> Result<Self> {
        let (stdin, control) = control_channel()?;
        let mut command = Command::new(&ios.capture_executable);
        command.args([
            "--ios-capture",
            device.serial.as_deref().ok_or(Error::StaleSession)?,
            "--boot-id",
            device.transport_id.as_deref().ok_or(Error::StaleSession)?,
            "--generation",
            &options.generation.to_string(),
            "--epoch",
            &options.epoch.to_string(),
            "--max-size",
            &options.max_size.to_string(),
            "--bit-rate",
            &options.bit_rate.to_string(),
            "--max-fps",
            &options.max_fps.to_string(),
            "--control-fd",
            "0",
        ]);
        if device.capabilities.input {
            command.arg("--enable-input");
        }
        let mut child = command
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                process::spawn_error(&ios.capture_executable.display().to_string(), error)
            })?;
        let mut stderr = child.stderr.take().unwrap();
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let retained = diagnostics.clone();
        let diagnostic_task = tokio::spawn(async move {
            let mut chunk = [0; 1024];
            while let Ok(length) = stderr.read(&mut chunk).await {
                if length == 0 {
                    break;
                }
                let mut bytes = retained.lock().unwrap_or_else(|error| error.into_inner());
                let count = length.min(MAX_DIAGNOSTICS - bytes.len());
                bytes.extend_from_slice(&chunk[..count]);
            }
        });
        Ok(Self {
            child,
            control: Some(control),
            diagnostics,
            diagnostic_task: Some(diagnostic_task),
        })
    }

    async fn cleanup(mut self) -> Result<()> {
        self.control.take();
        let result = match timeout(CLOSE_TIMEOUT, self.child.wait()).await {
            Ok(result) => result.map(|_| ()).map_err(Error::Io),
            Err(_) => {
                let _ = self.child.start_kill();
                timeout(CLOSE_TIMEOUT, self.child.wait())
                    .await
                    .map_err(|_| Error::Timeout {
                        operation: "reaping iOS capture helper".into(),
                    })?
                    .map(|_| ())
                    .map_err(Error::Io)
            }
        };
        if let Some(mut task) = self.diagnostic_task.take()
            && timeout(CLOSE_TIMEOUT, &mut task).await.is_err()
        {
            task.abort();
            let _ = task.await;
        }
        result
    }
}

impl Drop for ChildResources {
    fn drop(&mut self) {
        // The normal owner awaits cleanup; this covers panics and runtime shutdown.
        if let Some(task) = self.diagnostic_task.take() {
            task.abort();
        }
    }
}

/// Let the standard process implementation install the socket at FD 0, including CLOEXEC handling.
#[cfg(unix)]
fn control_channel() -> Result<(Stdio, NativeControl)> {
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    let (parent, child) = UnixStream::pair()?;
    parent.set_nonblocking(true)?;
    Ok((
        Stdio::from(OwnedFd::from(child)),
        NativeControl::from_std(parent)?,
    ))
}

#[cfg(not(unix))]
fn control_channel() -> Result<(Stdio, NativeControl)> {
    Err(Error::Unsupported {
        feature: "iOS Simulator control requires a Unix host".into(),
    })
}

async fn supervise(
    mut resources: ChildResources,
    mut stdout: BufReader<ChildStdout>,
    mut video: DuplexStream,
    control: DuplexStream,
    epoch: u64,
    input_available: bool,
    mut stopped: watch::Receiver<bool>,
) -> Result<()> {
    let pending = AtomicBool::new(false);
    let media_closed = AtomicBool::new(false);
    let diagnostics = resources.diagnostics.clone();
    let result = {
        let native = resources.control.as_mut().unwrap();
        let relay = controls(
            control,
            native,
            epoch,
            input_available,
            &pending,
            &media_closed,
        );
        tokio::pin!(relay);
        tokio::select! {
            biased;
            _ = wait_cancelled(&mut stopped) => Ok(()),
            result = &mut relay => result,
            result = tokio::io::copy(&mut stdout, &mut video) => {
                media_closed.store(true, Ordering::Relaxed);
                let result = result.map(|_| ()).map_err(Error::Io);
                // Stop or an input failure may end video before its native receipt is drained.
                if pending.load(Ordering::Relaxed) { result.and(relay.await) } else { result }
            },
            result = resources.child.wait() => {
                media_closed.store(true, Ordering::Relaxed);
                let result = match result {
                    Ok(status) if status.success() => Ok(()),
                    Ok(status) => {
                        let bytes = diagnostics.lock().unwrap_or_else(|error| error.into_inner());
                        Err(failed(&format!("Capture helper exited ({status}): {}", String::from_utf8_lossy(&bytes).trim())))
                    },
                    Err(error) => Err(Error::Io(error)),
                };
                if pending.load(Ordering::Relaxed) { result.and(relay.await) } else { result }
            }
        }
    };
    // EOF on the private socket tells the native owner to release held input before it exits.
    drop(stdout);
    drop(video);
    let cleanup = resources.cleanup().await;
    match result {
        Err(Error::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            cleanup
        }
        result => result.and(cleanup),
    }
}

async fn controls(
    mut control: DuplexStream,
    native: &mut NativeControl,
    epoch: u64,
    input_available: bool,
    pending: &AtomicBool,
    media_closed: &AtomicBool,
) -> Result<()> {
    let (native_read, mut native_write) = tokio::io::split(native);
    let mut native_read = BufReader::new(native_read);
    let mut last_seq = 0;
    loop {
        let bytes = tokio::select! {
            bytes = read_line(&mut control) => match bytes? { Some(bytes) => bytes, None => return Ok(()) },
            unsolicited = native_read.fill_buf() => {
                if unsolicited?.is_empty() { return Ok(()); }
                return Err(failed("Unsolicited native control response"));
            }
        };
        let request: ControlRequest =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Malformed iOS control request"))?;
        let rejected = if request.epoch != epoch {
            Some(Error::StaleSession)
        } else if request.seq == 0 || request.seq <= last_seq {
            Some(invalid(
                "Control sequence must be positive and strictly increasing",
            ))
        } else {
            last_seq = request.seq;
            if !input_available && matches!(request.command, ControlCommand::Input { .. }) {
                Some(Error::Unsupported {
                    feature: "iOS Simulator native input is unavailable".into(),
                })
            } else {
                None
            }
        };
        if let Some(error) = rejected {
            let reply = ControlReply {
                seq: request.seq,
                ok: false,
                code: Some(error.code().into()),
                message: Some(error.to_string()),
            };
            timeout(CONTROL_TIMEOUT, write_reply(&mut control, &reply))
                .await
                .map_err(|_| control_timeout())??;
            continue;
        }
        pending.store(true, Ordering::Relaxed);
        timeout(CONTROL_TIMEOUT, async {
            let mut bytes = serde_json::to_vec(&request)
                .map_err(|_| failed("Cannot encode native control request"))?;
            bytes.push(b'\n');
            native_write.write_all(&bytes).await?;
            let bytes = read_line(&mut native_read)
                .await?
                .ok_or_else(|| failed("Native control channel closed before its response"))?;
            let reply: ControlReply = serde_json::from_slice(&bytes)
                .map_err(|_| failed("Malformed native control response"))?;
            // Optional Rust fields still have required keys in the private wire protocol.
            let fields: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|_| failed("Malformed native control response"))?;
            if fields.get("code").is_none() || fields.get("message").is_none() {
                return Err(failed("Native control response omitted required fields"));
            }
            if reply.seq != request.seq
                || (reply.ok && (reply.code.is_some() || reply.message.is_some()))
                || (!reply.ok
                    && (reply.code.as_ref().is_none_or(String::is_empty)
                        || reply.message.as_ref().is_none_or(String::is_empty)))
            {
                return Err(failed("Native input response does not match its request"));
            }
            write_reply(&mut control, &reply).await
        })
        .await
        .map_err(|_| control_timeout())??;
        pending.store(false, Ordering::Relaxed);
        if matches!(request.command, ControlCommand::Stop) || media_closed.load(Ordering::Relaxed) {
            return Ok(());
        }
    }
}

async fn write_reply(control: &mut DuplexStream, reply: &ControlReply) -> Result<()> {
    let mut bytes =
        serde_json::to_vec(reply).map_err(|_| failed("Cannot encode control response"))?;
    bytes.push(b'\n');
    control.write_all(&bytes).await?;
    Ok(())
}

fn control_timeout() -> Error {
    Error::Timeout {
        operation: "waiting for native iOS input submission receipt".into(),
    }
}

async fn read_line(reader: &mut (impl AsyncRead + Unpin)) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        if reader.read(&mut byte).await? == 0 {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(failed("Truncated helper protocol line"))
            };
        }
        if byte[0] == b'\n' {
            return Ok(Some(bytes));
        }
        if bytes.len() == MAX_CONTROL_BYTES {
            return Err(failed("Helper protocol line exceeds its size limit"));
        }
        bytes.push(byte[0]);
    }
}

struct RunningResources {
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Result<()>>>,
}

impl RunningResources {
    async fn close(&mut self) -> Result<()> {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            // The supervisor retains child ownership even if the caller cancels this await.
            task.await
                .map_err(|_| failed("iOS preview supervisor failed"))??;
        }
        Ok(())
    }
}

impl Drop for RunningResources {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

struct CancelOnDrop(Option<watch::Sender<bool>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            sender.send_replace(true);
        }
    }
}

async fn wait_cancelled(cancelled: &mut watch::Receiver<bool>) {
    while !*cancelled.borrow_and_update() {
        if cancelled.changed().await.is_err() {
            return;
        }
    }
}

fn validate_options(options: &StreamOptions) -> Result<()> {
    DeviceConfig {
        protocol: DEVICE_PROTOCOL.into(),
        socket_name: format!("mpp_{}", options.stream_id),
        token: options.token.clone(),
        generation: options.generation,
        epoch: options.epoch,
        max_size: options.max_size,
        bit_rate: options.bit_rate,
        max_fps: options.max_fps,
    }
    .validate()
}

fn startup_timeout() -> Error {
    Error::Timeout {
        operation: "starting iOS Simulator preview".into(),
    }
}
fn cancelled_error() -> Error {
    failed("iOS preview startup was cancelled")
}
fn invalid(message: &str) -> Error {
    Error::InvalidArgument {
        message: message.into(),
    }
}
fn failed(message: &str) -> Error {
    Error::CommandFailed {
        tool: "iOS preview".into(),
        message: message.into(),
    }
}
