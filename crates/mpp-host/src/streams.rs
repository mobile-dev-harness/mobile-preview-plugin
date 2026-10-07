use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use mpp_android::Android;
use mpp_core::{Error, Result, Session, stream::Geometry};
use serde::Serialize;
use tokio::{sync::oneshot, task::JoinHandle, time::timeout};

#[cfg(unix)]
use {
    mpp_android::{RunningStream, StreamOptions},
    mpp_core::stream::{ControlReply, ControlRequest, MAX_CONTROL_BYTES},
    std::{
        os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
        path::Path,
    },
    tokio::{
        io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
        net::{TcpStream, UnixListener, UnixStream},
    },
};

#[cfg(unix)]
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(unix)]
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(20);

pub struct PreviewOptions {
    pub bootstrap: PathBuf,
    pub library: PathBuf,
    pub socket_dir: PathBuf,
    pub stream_id: String,
    pub token: String,
    pub max_size: u32,
    pub bit_rate: u32,
    pub max_fps: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewDescriptor {
    pub stream_id: String,
    pub epoch: u64,
    pub generation: u64,
    pub geometry: Geometry,
    pub video_socket: PathBuf,
    pub control_socket: PathBuf,
}

struct ActivePreview {
    session: Session,
    epoch: u64,
    cancel: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<()>>,
}

#[derive(Default)]
pub struct PreviewManager {
    epoch: u64,
    active: BTreeMap<String, ActivePreview>,
}

impl PreviewManager {
    pub async fn start(
        &mut self,
        android: &Android,
        session: &Session,
        options: PreviewOptions,
    ) -> Result<PreviewDescriptor> {
        self.reap().await;
        if self.active.contains_key(&options.stream_id)
            || self
                .active
                .values()
                .any(|active| same_lease(&active.session, session))
        {
            return Err(Error::Busy {
                device: session.device.id.clone(),
                owner: session.owner.clone(),
            });
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or_else(|| invalid("capture epoch exhausted"))?;
        #[cfg(unix)]
        {
            let sockets = PrivateSockets::bind(&options.socket_dir)?;
            let mut running = android
                .start_stream(
                    &session.device,
                    StreamOptions {
                        bootstrap: options.bootstrap,
                        library: options.library,
                        token: options.token,
                        stream_id: options.stream_id.clone(),
                        generation: session.generation,
                        epoch: self.epoch,
                        max_size: options.max_size,
                        bit_rate: options.bit_rate,
                        max_fps: options.max_fps,
                    },
                )
                .await?;
            let descriptor = PreviewDescriptor {
                stream_id: options.stream_id.clone(),
                epoch: self.epoch,
                generation: session.generation,
                geometry: running.geometry,
                video_socket: sockets.video_path.path.clone(),
                control_socket: sockets.control_path.path.clone(),
            };
            let (video, control) = match running.take_streams() {
                Ok(channels) => channels,
                Err(error) => {
                    let _ = running.close().await;
                    return Err(error);
                }
            };
            let (cancel, cancelled) = oneshot::channel();
            let epoch = self.epoch;
            let task = tokio::spawn(run_preview(
                running, sockets, video, control, epoch, cancelled,
            ));
            self.active.insert(
                options.stream_id,
                ActivePreview {
                    session: session.clone(),
                    epoch,
                    cancel: Some(cancel),
                    task,
                },
            );
            Ok(descriptor)
        }
        #[cfg(not(unix))]
        {
            let _ = (android, options);
            Err(Error::Unsupported {
                feature: "private preview sockets on this platform".into(),
            })
        }
    }

    pub async fn stop(&mut self, session: &Session, stream_id: &str, epoch: u64) -> Result<()> {
        let active = self.active.get(stream_id).ok_or(Error::StaleSession)?;
        if !same_lease(&active.session, session) || active.epoch != epoch {
            return Err(Error::StaleSession);
        }
        self.stop_id(stream_id).await
    }

    pub async fn stop_session(&mut self, session: &Session) -> Result<()> {
        let ids: Vec<_> = self
            .active
            .iter()
            .filter(|(_, active)| same_lease(&active.session, session))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.stop_id(&id).await?;
        }
        Ok(())
    }

    pub fn signal_all(&mut self) {
        for active in self.active.values_mut() {
            signal(active);
        }
    }

    pub async fn shutdown(&mut self) {
        self.signal_all();
        let ids: Vec<_> = self.active.keys().cloned().collect();
        for id in ids {
            let _ = self.stop_id(&id).await;
        }
    }

    async fn stop_id(&mut self, id: &str) -> Result<()> {
        let Some(active) = self.active.get_mut(id) else {
            return Err(Error::StaleSession);
        };
        signal(active);
        // Keep the handle registered during the await so cancellation cannot detach cleanup.
        let result = match timeout(CLOSE_TIMEOUT, &mut active.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(invalid("preview cleanup task failed")),
            Err(_) => {
                active.task.abort();
                let _ = (&mut active.task).await;
                Err(Error::Timeout {
                    operation: "closing the Android preview".into(),
                })
            }
        };
        self.active.remove(id);
        result
    }

    async fn reap(&mut self) {
        let ids: Vec<_> = self
            .active
            .iter()
            .filter(|(_, active)| active.task.is_finished())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.stop_id(&id).await;
        }
    }
}

impl Drop for PreviewManager {
    fn drop(&mut self) {
        self.signal_all();
    }
}

fn same_lease(left: &Session, right: &Session) -> bool {
    left.id == right.id && left.owner == right.owner && left.generation == right.generation
}

fn signal(active: &mut ActivePreview) {
    if let Some(cancel) = active.cancel.take() {
        let _ = cancel.send(());
    }
}

#[cfg(unix)]
struct OwnedSocketPath {
    path: PathBuf,
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl OwnedSocketPath {
    fn record(path: PathBuf) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(&path)?;
        Ok(Self {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[cfg(unix)]
impl Drop for OwnedSocketPath {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path)
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
            && metadata.file_type().is_socket()
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
struct PrivateSockets {
    video: UnixListener,
    control: UnixListener,
    video_path: OwnedSocketPath,
    control_path: OwnedSocketPath,
}

#[cfg(unix)]
impl PrivateSockets {
    fn bind(directory: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(directory)?;
        if !directory.is_absolute() || !metadata.is_dir() || metadata.mode() & 0o7777 != 0o700 {
            return Err(invalid(
                "preview socket directory must be an absolute ordinary mode-0700 directory",
            ));
        }
        let directory = directory.canonicalize()?;
        if directory.to_str().is_none() {
            return Err(invalid("preview socket directory must be UTF-8"));
        }
        let video_path = directory.join("video.sock");
        let video = UnixListener::bind(&video_path)?;
        let video_path = OwnedSocketPath::record(video_path)?;
        std::fs::set_permissions(&video_path.path, std::fs::Permissions::from_mode(0o600))?;
        let control_path = directory.join("control.sock");
        let control = UnixListener::bind(&control_path)?;
        let control_path = OwnedSocketPath::record(control_path)?;
        std::fs::set_permissions(&control_path.path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            video,
            control,
            video_path,
            control_path,
        })
    }

    async fn accept(&self) -> Result<(UnixStream, UnixStream)> {
        let ((video, _), (control, _)) = timeout(ACCEPT_TIMEOUT, async {
            tokio::try_join!(self.video.accept(), self.control.accept())
        })
        .await
        .map_err(|_| Error::Timeout {
            operation: "connecting the local preview client".into(),
        })??;
        Ok((video, control))
    }
}

#[cfg(unix)]
async fn run_preview(
    mut running: RunningStream,
    sockets: PrivateSockets,
    video: TcpStream,
    control: TcpStream,
    epoch: u64,
    cancelled: oneshot::Receiver<()>,
) -> Result<()> {
    let relay = async {
        let (local_video, local_control) = sockets.accept().await?;
        tokio::select! {
            result = relay_video(video, local_video) => result,
            result = relay_control(control, local_control, epoch) => result,
        }
    };
    // Dropping either relay future closes both socket pairs before backend cleanup begins.
    tokio::select! { _ = cancelled => {}, _ = relay => {} }
    let result = running.close().await;
    drop(sockets);
    result
}

#[cfg(unix)]
async fn relay_video(mut device: TcpStream, local: UnixStream) -> Result<()> {
    let (mut local_read, mut local_write) = local.into_split();
    let copy = async {
        let mut chunk = [0; 64 * 1024];
        loop {
            let count = device.read(&mut chunk).await?;
            if count == 0 {
                return Ok(());
            }
            write_bounded(&mut local_write, &chunk[..count]).await?;
        }
    };
    tokio::select! {
        result = copy => result,
        result = local_read.read_u8() => match result {
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(()),
            Err(error) => Err(error.into()),
            Ok(_) => Err(invalid("video channel is receive-only")),
        }
    }
}

#[cfg(unix)]
async fn relay_control(device: TcpStream, local: UnixStream, epoch: u64) -> Result<()> {
    let (device_read, mut device_write) = device.into_split();
    let (local_read, mut local_write) = local.into_split();
    let mut device_read = FrameReader::new(device_read);
    let mut local_read = FrameReader::new(local_read);
    let mut last_seq = 0;
    loop {
        let bytes = tokio::select! {
            frame = local_read.next() => match frame? { Some(bytes) => bytes, None => return Ok(()) },
            unsolicited = device_read.reader.fill_buf() => {
                if unsolicited?.is_empty() { return Ok(()); }
                return Err(invalid("unsolicited device control response"));
            }
        };
        let request: ControlRequest =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid control request"))?;
        if request.epoch != epoch || request.seq == 0 || request.seq <= last_seq {
            return Err(Error::StaleSession);
        }
        last_seq = request.seq;
        let mut frame = bytes;
        frame.push(b'\n');
        timeout(IO_TIMEOUT, async {
            device_write.write_all(&frame).await?;
            let reply = device_read
                .next()
                .await?
                .ok_or_else(|| invalid("device control channel closed before its response"))?;
            let response: ControlReply =
                serde_json::from_slice(&reply).map_err(|_| invalid("invalid control response"))?;
            if response.seq != request.seq
                || (response.ok && (response.code.is_some() || response.message.is_some()))
                || (!response.ok
                    && (response.code.as_ref().is_none_or(String::is_empty)
                        || response.message.as_ref().is_none_or(String::is_empty)))
            {
                return Err(invalid("device input response does not match its request"));
            }
            let mut frame = reply;
            frame.push(b'\n');
            local_write.write_all(&frame).await?;
            Ok::<_, Error>(())
        })
        .await
        .map_err(|_| Error::Timeout {
            operation: "forwarding device input and its submission receipt".into(),
        })??;
    }
}

#[cfg(unix)]
struct FrameReader<R> {
    reader: BufReader<R>,
    bytes: Vec<u8>,
    deadline: Option<tokio::time::Instant>,
}

#[cfg(unix)]
impl<R: AsyncRead + Unpin> FrameReader<R> {
    fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            bytes: Vec::new(),
            deadline: None,
        }
    }

    async fn next(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            let buffer = match self.deadline {
                Some(deadline) => tokio::time::timeout_at(deadline, self.reader.fill_buf())
                    .await
                    .map_err(|_| Error::Timeout {
                        operation: "reading a complete control frame".into(),
                    })??,
                None => self.reader.fill_buf().await?,
            };
            if buffer.is_empty() {
                return if self.bytes.is_empty() {
                    Ok(None)
                } else {
                    Err(invalid("truncated control frame"))
                };
            }
            let newline = buffer.iter().position(|byte| *byte == b'\n');
            let length = newline.unwrap_or(buffer.len());
            if length > MAX_CONTROL_BYTES - self.bytes.len() {
                return Err(invalid("control frame exceeds its size limit"));
            }
            self.bytes.extend_from_slice(&buffer[..length]);
            self.reader.consume(length + usize::from(newline.is_some()));
            if newline.is_some() {
                self.deadline = None;
                return Ok(Some(std::mem::take(&mut self.bytes)));
            }
            if self.deadline.is_none() {
                self.deadline = Some(tokio::time::Instant::now() + IO_TIMEOUT);
            }
        }
    }
}

#[cfg(unix)]
async fn write_bounded(writer: &mut (impl AsyncWrite + Unpin), bytes: &[u8]) -> Result<()> {
    timeout(IO_TIMEOUT, writer.write_all(bytes))
        .await
        .map_err(|_| Error::Timeout {
            operation: "forwarding a preview channel".into(),
        })??;
    Ok(())
}

fn invalid(message: &str) -> Error {
    Error::InvalidArgument {
        message: message.into(),
    }
}
