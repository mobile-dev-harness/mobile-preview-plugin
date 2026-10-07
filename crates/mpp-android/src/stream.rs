use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use mpp_core::{
    Device, DeviceState, Error, Platform, Result,
    stream::{Channel, DEVICE_PROTOCOL, DeviceConfig, DeviceHello, Geometry, MAX_CONTROL_BYTES},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::{Child, ChildStdin, Command},
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{Instant, timeout, timeout_at},
};

use crate::{Android, process};

const START_TIMEOUT: Duration = Duration::from_secs(15);
const HELLO_TIMEOUT: Duration = Duration::from_secs(2);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_CANDIDATES: usize = 8;
const MAX_DIAGNOSTICS: usize = 8192;
const MAX_PENDING_STARTS: usize = 32;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(25);

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

/// Paths and session secrets come from the trusted host, never the browser.
pub struct StreamOptions {
    pub bootstrap: PathBuf,
    pub library: PathBuf,
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
    pub video: Option<TcpStream>,
    pub control: Option<TcpStream>,
    resources: ResourceGuard,
}

impl RunningStream {
    /// Transferring sockets does not transfer ownership of the device process.
    pub fn take_streams(&mut self) -> Result<(TcpStream, TcpStream)> {
        if self.video.is_none() || self.control.is_none() {
            return Err(invalid("Stream channels have already been taken"));
        }
        Ok((self.video.take().unwrap(), self.control.take().unwrap()))
    }

    /// Callers that took the sockets must close them; stdin EOF also stops the helper.
    pub async fn close(&mut self) -> Result<()> {
        self.video.take();
        self.control.take();
        self.resources.close().await
    }
}

impl Android {
    pub async fn start_stream(
        &self,
        verified_device: &Device,
        options: StreamOptions,
    ) -> Result<RunningStream> {
        validate_options(&options)?;
        let android = self.clone();
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
                return Err(failed("Android preview host is shutting down"));
            }
            registry.tasks.retain(|start| !start.task.is_finished());
            if registry.tasks.len() >= MAX_PENDING_STARTS {
                return Err(failed("Too many Android preview startups are in progress"));
            }
            let task = tokio::spawn(async move {
                let result = android
                    .start_owned(device, options, cancelled.clone())
                    .await;
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
                        failed("Android preview startup task failed while shutting down")
                    });
                }
                Err(_) => {
                    // Do not abort an acquisition/cleanup task in the middle of an owned mutation.
                    failure.get_or_insert_with(|| Error::Timeout {
                        operation: "draining Android preview startup cleanup".into(),
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
        tokio::select! {
            _ = wait_cancelled(&mut cancelled) => return Err(cancelled_error()),
            result = self.verify_attachment(&device) => result?,
        }
        check_cancelled(&mut cancelled)?;
        let transport = device.transport_id.as_deref().ok_or(Error::StaleSession)?;
        let sdk = process::run(
            &self.adb,
            &["-t", transport, "shell", "getprop", "ro.build.version.sdk"],
        )
        .await?;
        check_cancelled(&mut cancelled)?;
        let abi = process::run(
            &self.adb,
            &["-t", transport, "shell", "getprop", "ro.product.cpu.abi"],
        )
        .await?;
        check_cancelled(&mut cancelled)?;
        if sdk.trim() != "32" || abi.trim() != "arm64-v8a" {
            return Err(Error::Unsupported {
                feature: "Live Android preview currently requires API 32 and arm64-v8a".into(),
            });
        }

        let remote = format!("/data/local/tmp/mpp_{}", options.stream_id);
        let socket_name = format!("mpp_{}", options.stream_id);
        let mut guard = ResourceGuard(Some(Resources {
            adb: self.adb.clone(),
            transport: transport.into(),
            remote,
            socket_name,
            owns_directory: false,
            owns_reverse: false,
            child: None,
            stdin: None,
            diagnostics: Arc::new(Mutex::new(Vec::new())),
            diagnostic_task: None,
            token: options.token.clone(),
        }));
        let operation = async {
            let resources = guard.0.as_mut().unwrap();
            resources
                .run(&["shell", "mkdir", "-m", "700", &resources.remote])
                .await?;
            resources.owns_directory = true;
            check_cancelled(&mut cancelled)?;
            let bootstrap = format!("{}/bootstrap.jar", resources.remote);
            let library = format!("{}/libmpp_android_device.so", resources.remote);
            resources
                .run(&["push", path_arg(&options.bootstrap)?, &bootstrap])
                .await?;
            check_cancelled(&mut cancelled)?;
            resources
                .run(&["push", path_arg(&options.library)?, &library])
                .await?;
            check_cancelled(&mut cancelled)?;

            let deadline = Instant::now() + START_TIMEOUT;
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
            let port = format!("tcp:{}", listener.local_addr()?.port());
            let abstract_name = format!("localabstract:{}", resources.socket_name);
            // Finish the acquisition before observing cancellation or the startup deadline.
            resources
                .run(&["reverse", "--no-rebind", &abstract_name, &port])
                .await?;
            resources.owns_reverse = true;
            check_cancelled(&mut cancelled)?;
            let ready = async {
                let config = DeviceConfig {
                    protocol: DEVICE_PROTOCOL.into(),
                    socket_name: resources.socket_name.clone(),
                    token: options.token.clone(),
                    generation: options.generation,
                    epoch: options.epoch,
                    max_size: options.max_size,
                    bit_rate: options.bit_rate,
                    max_fps: options.max_fps,
                };
                resources.spawn(&bootstrap, &library)?;
                let mut bytes = serde_json::to_vec(&config)
                    .map_err(|_| failed("Cannot encode device configuration"))?;
                bytes.push(b'\n');
                resources.stdin.as_mut().unwrap().write_all(&bytes).await?;
                let channels = tokio::select! {
                    _ = wait_cancelled(&mut cancelled) => return Err(cancelled_error()),
                    result = accept_channels(&listener, &config) => result?,
                    status = resources.child.as_mut().unwrap().wait() => {
                        return Err(failed(&format!("Device helper exited during startup ({}). {}", status?, resources.diagnostic())));
                    }
                };
                tokio::select! {
                    _ = wait_cancelled(&mut cancelled) => return Err(cancelled_error()),
                    result = self.verify_attachment(&device) => result?,
                }
                check_cancelled(&mut cancelled)?;
                // Removing the mapping does not close its existing connections.
                resources.remove_reverse().await?;
                if let Some(status) = resources.child.as_mut().unwrap().try_wait()? {
                    return Err(failed(&format!(
                        "Device helper exited before becoming ready ({status}). {}",
                        resources.diagnostic()
                    )));
                }
                Ok::<_, Error>(channels)
            };
            timeout_at(deadline, ready)
                .await
                .map_err(|_| Error::Timeout {
                    operation: "starting authenticated Android preview channels".into(),
                })?
        };
        match operation.await {
            Ok((geometry, video, control)) => Ok(RunningStream {
                geometry,
                video: Some(video),
                control: Some(control),
                resources: guard,
            }),
            Err(error) => {
                let _ = guard.close().await;
                Err(error)
            }
        }
    }

    async fn verify_attachment(&self, expected: &Device) -> Result<()> {
        if expected.platform != Platform::Android || expected.state != DeviceState::Online {
            return Err(Error::StaleSession);
        }
        let transport = expected
            .transport_id
            .as_deref()
            .ok_or(Error::StaleSession)?;
        if transport.is_empty() || !transport.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::StaleSession);
        }
        let current = self
            .probe(expected.serial.as_deref().ok_or(Error::StaleSession)?)
            .await?;
        if current.id != expected.id
            || current.transport_id != expected.transport_id
            || current.kind != expected.kind
            || current.avd != expected.avd
        {
            return Err(Error::StaleSession);
        }
        Ok(())
    }
}

async fn accept_channels(
    listener: &TcpListener,
    config: &DeviceConfig,
) -> Result<(Geometry, TcpStream, TcpStream)> {
    let mut video = None;
    let mut control = None;
    let mut geometry = None;
    for _ in 0..MAX_CANDIDATES {
        let (mut stream, _) = listener.accept().await?;
        let hello = match timeout(HELLO_TIMEOUT, read_hello(&mut stream)).await {
            Ok(Ok(hello)) => hello,
            _ => continue,
        };
        if hello.protocol != DEVICE_PROTOCOL
            || !token_matches(&hello.token, &config.token)
            || hello.generation != config.generation
            || hello.epoch != config.epoch
            || hello.geometry.validate().is_err()
        {
            continue;
        }
        let channel = match hello.channel {
            Channel::Video => &mut video,
            Channel::Control => &mut control,
        };
        if channel.is_some()
            || geometry
                .as_ref()
                .is_some_and(|value| *value != hello.geometry)
        {
            return Err(failed(
                "Device helper supplied conflicting channel metadata",
            ));
        }
        stream.set_nodelay(true)?;
        // The device may wait for this ACK before it opens its other channel.
        timeout(HELLO_TIMEOUT, stream.write_all(b"{\"ok\":true}\n"))
            .await
            .map_err(|_| failed("Device channel acknowledgement timed out"))??;
        geometry = Some(hello.geometry);
        *channel = Some(stream);
        if video.is_some() && control.is_some() {
            break;
        }
    }
    match (geometry, video, control) {
        (Some(geometry), Some(video), Some(control)) => Ok((geometry, video, control)),
        _ => Err(failed("Too many invalid device channel connections")),
    }
}

async fn read_hello(stream: &mut TcpStream) -> Result<DeviceHello> {
    let mut bytes = Vec::new();
    loop {
        let byte = stream.read_u8().await?;
        if byte == b'\n' {
            return serde_json::from_slice(&bytes)
                .map_err(|_| failed("Malformed device channel hello"));
        }
        if bytes.len() == MAX_CONTROL_BYTES {
            return Err(failed("Device channel hello exceeds its size limit"));
        }
        bytes.push(byte);
    }
}

fn token_matches(actual: &str, expected: &str) -> bool {
    actual.len() == expected.len()
        && actual
            .bytes()
            .zip(expected.bytes())
            .fold(0, |diff, (a, b)| diff | (a ^ b))
            == 0
}

struct Resources {
    adb: PathBuf,
    transport: String,
    remote: String,
    socket_name: String,
    owns_directory: bool,
    owns_reverse: bool,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    diagnostics: Arc<Mutex<Vec<u8>>>,
    diagnostic_task: Option<JoinHandle<()>>,
    token: String,
}

impl Resources {
    async fn run(&self, args: &[&str]) -> Result<String> {
        let mut targeted = vec!["-t", &self.transport];
        targeted.extend_from_slice(args);
        process::run(&self.adb, &targeted).await
    }

    fn spawn(&mut self, bootstrap: &str, library: &str) -> Result<()> {
        let mut child = Command::new(&self.adb)
            .args([
                "-t",
                &self.transport,
                "shell",
                "-T",
                &format!("CLASSPATH={bootstrap}"),
                "app_process",
                "/",
                "dev.mpp.Bootstrap",
                library,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| process::spawn_error(&self.adb.display().to_string(), error))?;
        // Child::wait closes its own stdin; keep the ownership pipe separately.
        self.stdin = child.stdin.take();
        let mut stderr = child.stderr.take().unwrap();
        let diagnostics = self.diagnostics.clone();
        self.diagnostic_task = Some(tokio::spawn(async move {
            let mut chunk = [0; 1024];
            while let Ok(length) = stderr.read(&mut chunk).await {
                if length == 0 {
                    break;
                }
                let mut bytes = diagnostics
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let retained = length.min(MAX_DIAGNOSTICS - bytes.len());
                bytes.extend_from_slice(&chunk[..retained]);
            }
        }));
        self.child = Some(child);
        Ok(())
    }

    fn diagnostic(&self) -> String {
        let bytes = self
            .diagnostics
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // A truncated final line may contain only part of a secret: omit it entirely.
        let end = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        String::from_utf8_lossy(&bytes[..end])
            .replace(&self.token, "[redacted]")
            .trim()
            .to_owned()
    }

    async fn remove_reverse(&mut self) -> Result<()> {
        if self.owns_reverse {
            self.run(&[
                "reverse",
                "--remove",
                &format!("localabstract:{}", self.socket_name),
            ])
            .await?;
            self.owns_reverse = false;
        }
        Ok(())
    }

    async fn cleanup(mut self) -> Result<()> {
        let mut error = None;
        self.stdin.take();
        if let Some(child) = &mut self.child {
            match timeout(CLOSE_TIMEOUT, child.wait()).await {
                Ok(Ok(_)) => {}
                _ => {
                    let _ = child.start_kill();
                    let _ = timeout(CLOSE_TIMEOUT, child.wait()).await;
                }
            }
        }
        if let Some(task) = self.diagnostic_task.take() {
            task.abort();
        }
        match timeout(CLOSE_TIMEOUT, self.remove_reverse()).await {
            Ok(Ok(())) => {}
            Ok(Err(failure)) => error = Some(failure),
            Err(_) => {
                error = Some(Error::Timeout {
                    operation: "removing owned Android preview reverse mapping".into(),
                })
            }
        }
        if self.owns_directory {
            let result = timeout(
                CLOSE_TIMEOUT,
                self.run(&["shell", "rm", "-rf", &self.remote]),
            )
            .await
            .map_err(|_| Error::Timeout {
                operation: "removing owned Android preview artifacts".into(),
            })
            .and_then(|result| result);
            if error.is_none() {
                error = result.err();
            }
        }
        error.map_or(Ok(()), Err)
    }
}

struct ResourceGuard(Option<Resources>);

impl ResourceGuard {
    async fn close(&mut self) -> Result<()> {
        if let Some(resources) = self.0.take() {
            // Cleanup continues if the request waiting for close is itself cancelled.
            tokio::spawn(resources.cleanup())
                .await
                .map_err(|_| failed("Android preview cleanup task failed"))??;
        }
        Ok(())
    }
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        let Some(resources) = self.0.take() else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(resources.cleanup());
        } else {
            // Explicit close is normal; this also gives non-async owners a bounded fallback.
            std::thread::spawn(move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    let _ = runtime.block_on(resources.cleanup());
                }
            });
        }
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

fn check_cancelled(cancelled: &mut watch::Receiver<bool>) -> Result<()> {
    if *cancelled.borrow() {
        Err(cancelled_error())
    } else {
        Ok(())
    }
}

async fn wait_cancelled(cancelled: &mut watch::Receiver<bool>) {
    while !*cancelled.borrow_and_update() {
        if cancelled.changed().await.is_err() {
            return;
        }
    }
}

fn cancelled_error() -> Error {
    failed("Android preview startup was cancelled")
}

fn validate_options(options: &StreamOptions) -> Result<()> {
    if options.stream_id.len() != 32
        || !options
            .stream_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "Stream ID must contain 32 lowercase hexadecimal characters",
        ));
    }
    for path in [&options.bootstrap, &options.library] {
        if !path.is_absolute() || !path.is_file() {
            return Err(invalid(
                "Android preview assets must be absolute paths to regular files",
            ));
        }
        std::fs::File::open(path)?;
        path_arg(path)?;
    }
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

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| invalid("Android preview asset paths must be UTF-8"))
}

fn invalid(message: &str) -> Error {
    Error::InvalidArgument {
        message: message.into(),
    }
}

fn failed(message: &str) -> Error {
    Error::CommandFailed {
        tool: "Android preview".into(),
        message: message.into(),
    }
}
