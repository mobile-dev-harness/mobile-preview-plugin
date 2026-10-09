//! Local control plane. Each process owns its leases; stdout contains protocol messages only.

use mpp_android::Android;
use mpp_core::{Device, Error, InputEvent, Inventory, LeaseManager, Platform, Result, Session};
use mpp_ios::Ios;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub mod streams;
use streams::{PreviewManager, PreviewOptions};

pub const SCHEMA: &str = "mpp/v1";
pub const MAX_REQUEST_BYTES: usize = 65_536;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(u64),
    Text(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: RequestId,
    method: String,
    #[serde(default = "empty_object")]
    params: Value,
}

fn empty_object() -> Value {
    serde_json::json!({})
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub schema: &'static str,
    pub id: Option<RequestId>,
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<ErrorBody>,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl Response {
    pub fn new(id: Option<RequestId>, result: Result<Value>) -> Self {
        match result {
            Ok(value) => Self {
                schema: SCHEMA,
                id,
                ok: true,
                result: Some(value),
                error: None,
            },
            Err(error) => Self {
                schema: SCHEMA,
                id,
                ok: false,
                result: None,
                error: Some(ErrorBody {
                    code: error.code(),
                    message: error.to_string(),
                    hint: error.hint(),
                }),
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Connect {
    owner: String,
    device: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lease {
    owner: String,
    session: String,
    generation: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendInput {
    owner: String,
    session: String,
    generation: u64,
    event: InputEvent,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Boot {
    avd: String,
    consent: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootSimulator {
    udid: String,
    consent: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListDevices {
    platform: Option<Platform>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewStart {
    owner: String,
    session: String,
    generation: u64,
    bootstrap: Option<std::path::PathBuf>,
    library: Option<std::path::PathBuf>,
    socket_dir: Option<std::path::PathBuf>,
    stream_id: Option<String>,
    token: Option<String>,
    max_size: Option<u32>,
    bit_rate: Option<u32>,
    max_fps: Option<u32>,
}

impl PreviewStart {
    fn options(self, platform: Platform) -> Result<PreviewOptions> {
        if self.bootstrap.is_none()
            && self.library.is_none()
            && self.socket_dir.is_none()
            && self.stream_id.is_none()
            && self.token.is_none()
        {
            return Err(Error::Unsupported {
                feature: "live preview requires configured private preview channels".into(),
            });
        }
        if platform == Platform::Android && (self.bootstrap.is_none() || self.library.is_none()) {
            return Err(Error::InvalidArgument {
                message: "Android preview.start requires bootstrap and library".into(),
            });
        }
        let missing = || Error::InvalidArgument {
            message: "preview.start requires socket_dir, stream_id and token".into(),
        };
        Ok(PreviewOptions {
            bootstrap: self.bootstrap,
            library: self.library,
            socket_dir: self.socket_dir.ok_or_else(missing)?,
            stream_id: self.stream_id.ok_or_else(missing)?,
            token: self.token.ok_or_else(missing)?,
            max_size: self.max_size.unwrap_or(1280),
            bit_rate: self.bit_rate.unwrap_or(4_000_000),
            max_fps: self.max_fps.unwrap_or(30),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewStop {
    owner: String,
    session: String,
    generation: u64,
    stream_id: String,
    epoch: u64,
}

fn params<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| Error::InvalidArgument {
        message: format!("invalid parameters: {error}"),
    })
}

pub fn value<T: Serialize>(data: T) -> Result<Value> {
    serde_json::to_value(data).map_err(|error| Error::InvalidArgument {
        message: format!("could not encode response: {error}"),
    })
}

pub struct Host {
    android: Option<Android>,
    ios: Option<Ios>,
    leases: LeaseManager,
    previews: PreviewManager,
}

impl Host {
    pub fn new(android: Android) -> Self {
        Self::with_backends(Some(android), None)
    }

    pub fn with_backends(android: Option<Android>, ios: Option<Ios>) -> Self {
        Self {
            android,
            ios,
            leases: LeaseManager::new(),
            previews: PreviewManager::default(),
        }
    }

    pub async fn inventory(&self, platform: Option<Platform>) -> Result<Inventory> {
        let mut inventory = Inventory::default();
        if platform != Some(Platform::Ios) {
            match &self.android {
                Some(android) => match android.inventory().await {
                    Ok(mut found) => {
                        inventory.devices.append(&mut found.devices);
                        inventory.warnings.append(&mut found.warnings);
                    }
                    Err(error) => inventory.warnings.push(format!("Android discovery failed: {error}; {}", error.hint())),
                },
                None => inventory.warnings.push("Android SDK tools are unavailable; install the SDK or configure --adb to discover Android devices.".into()),
            }
        }
        if platform != Some(Platform::Android) {
            match &self.ios {
                Some(ios) => match ios.inventory().await {
                    Ok(mut found) => {
                        inventory.devices.append(&mut found.devices);
                        inventory.warnings.append(&mut found.warnings);
                    }
                    Err(error) => inventory.warnings.push(format!("iOS Simulator discovery failed: {error}; {}", error.hint())),
                },
                None => inventory.warnings.push("iOS Simulator tools are unavailable; use macOS with Xcode and select its developer directory with xcode-select.".into()),
            }
        }
        Ok(inventory)
    }

    async fn probe(&self, device: &Device) -> Result<Device> {
        let serial = device
            .serial
            .as_deref()
            .ok_or_else(|| Error::InvalidArgument {
                message: "device is not running; explicitly start it before connecting".into(),
            })?;
        match device.platform {
            Platform::Android => {
                self.android
                    .as_ref()
                    .ok_or_else(|| missing_backend("Android SDK"))?
                    .probe(serial)
                    .await
            }
            Platform::Ios => {
                self.ios
                    .as_ref()
                    .ok_or_else(|| missing_backend("Xcode Simulator"))?
                    .probe(serial)
                    .await
            }
        }
    }

    async fn platform_inventory(&self, platform: Platform) -> Result<Inventory> {
        match platform {
            Platform::Android => {
                self.android
                    .as_ref()
                    .ok_or_else(|| missing_backend("Android SDK"))?
                    .inventory()
                    .await
            }
            Platform::Ios => {
                self.ios
                    .as_ref()
                    .ok_or_else(|| missing_backend("Xcode Simulator"))?
                    .inventory()
                    .await
            }
        }
    }

    pub async fn request(&mut self, bytes: &[u8]) -> Response {
        if bytes.len() > MAX_REQUEST_BYTES {
            return oversized();
        }
        let request: Request = match serde_json::from_slice(bytes) {
            Ok(request) => request,
            Err(_) => {
                return Response::new(
                    None,
                    Err(Error::InvalidArgument {
                        message: "expected a JSON object with id, method and optional params"
                            .into(),
                    }),
                );
            }
        };
        if matches!(&request.id, RequestId::Text(id) if id.is_empty() || id.len() > 128) {
            return Response::new(
                None,
                Err(Error::InvalidArgument {
                    message: "id must contain 1–128 bytes".into(),
                }),
            );
        }
        let result = self.dispatch(&request.method, request.params).await;
        Response::new(Some(request.id), result)
    }

    async fn dispatch(&mut self, method: &str, input: Value) -> Result<Value> {
        match method {
            "hello" => {
                params::<Empty>(input)?;
                Ok(serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "control_protocol": SCHEMA,
                    "media_protocol": "MPP1",
                    "methods": ["hello", "devices.list", "emulator.start", "simulator.start", "session.connect", "session.status", "session.disconnect", "preview.start", "preview.stop", "input.send"],
                    "video_backend": "requires_device_assets",
                    "input_backend": "requires_device_assets",
                    "platforms": {
                        "android": self.android.is_some(),
                        "ios": self.ios.is_some()
                    }
                }))
            }
            "devices.list" => {
                let p: ListDevices = params(input)?;
                value(self.inventory(p.platform).await?)
            }
            "emulator.start" => {
                let p: Boot = params(input)?;
                if !p.consent {
                    return Err(Error::PermissionDenied {
                        message: "starting an emulator requires explicit consent".into(),
                    });
                }
                value(
                    self.android
                        .as_ref()
                        .ok_or_else(|| missing_backend("Android SDK"))?
                        .boot(&p.avd)
                        .await?,
                )
            }
            "simulator.start" => {
                let p: BootSimulator = params(input)?;
                if !p.consent {
                    return Err(Error::PermissionDenied {
                        message: "starting a Simulator requires explicit consent".into(),
                    });
                }
                value(
                    self.ios
                        .as_ref()
                        .ok_or_else(|| missing_backend("Xcode Simulator"))?
                        .boot(&p.udid)
                        .await?,
                )
            }
            "session.connect" => {
                let p: Connect = params(input)?;
                let platform = if p.device.starts_with("ios:") {
                    Platform::Ios
                } else {
                    Platform::Android
                };
                let device = self
                    .platform_inventory(platform)
                    .await?
                    .devices
                    .into_iter()
                    .find(|device| device.id == p.device)
                    .ok_or_else(|| Error::NotFound {
                        what: format!("device {}", p.device),
                    })?;
                let verified = self.probe(&device).await?;
                let mut selected = device.clone();
                if selected.platform == Platform::Ios && selected.transport_id.is_none() {
                    // Inventory knows the UUID; only probing establishes this boot's identity.
                    selected.transport_id = verified.transport_id.clone();
                }
                if !same_device(&selected, &verified) {
                    return Err(Error::StaleSession);
                }
                // &mut self serializes requests. Acquire only after awaiting the probe so
                // cancelling a connect cannot strand a lease whose ID was never returned.
                let session = self.leases.reserve(&p.owner, verified)?;
                value(
                    self.leases
                        .ready(&p.owner, &session.id, session.generation)?,
                )
            }
            "session.status" => {
                let p: Lease = params(input)?;
                let session = self.lease(&p)?;
                match self.probe(&session.device).await {
                    Ok(device) if same_device(&device, &session.device) => {}
                    result => {
                        let _ = self.previews.stop_session(&session).await;
                        let _ = self.leases.disconnect(&p.owner, &p.session, p.generation);
                        return Err(result.err().unwrap_or(Error::StaleSession));
                    }
                }
                value(session)
            }
            "session.disconnect" => {
                let p: Lease = params(input)?;
                let session = self.lease(&p)?;
                let stopped = self.previews.stop_session(&session).await;
                let disconnected = self.leases.disconnect(&p.owner, &p.session, p.generation)?;
                stopped?;
                value(disconnected)
            }
            "preview.start" => {
                let p: PreviewStart = params(input)?;
                let session = self.leases.status(&p.owner, &p.session, p.generation)?;
                value(
                    self.previews
                        .start_with_backends(
                            self.android.as_ref(),
                            self.ios.as_ref(),
                            &session,
                            p.options(session.device.platform)?,
                        )
                        .await?,
                )
            }
            "preview.stop" => {
                let p: PreviewStop = params(input)?;
                let session = self.leases.status(&p.owner, &p.session, p.generation)?;
                self.previews.stop(&session, &p.stream_id, p.epoch).await?;
                Ok(serde_json::json!({"stopped":true}))
            }
            "input.send" => {
                let p: SendInput = params(input)?;
                self.leases.status(&p.owner, &p.session, p.generation)?;
                p.event.validate()?;
                Err(Error::Unsupported {
                    feature: "input over lifecycle stdio; use the active preview control socket"
                        .into(),
                })
            }
            _ => Err(Error::Unsupported {
                feature: "unknown control method; use hello to list methods".into(),
            }),
        }
    }

    fn lease(&self, request: &Lease) -> Result<Session> {
        self.leases
            .status(&request.owner, &request.session, request.generation)
    }

    pub fn close(&mut self) {
        self.previews.signal_all();
        self.leases.disconnect_all();
    }

    pub async fn shutdown(&mut self) {
        self.close();
        let android = async {
            if let Some(android) = &self.android {
                let _ = android.shutdown_stream_starts().await;
            }
        };
        let ios = async {
            if let Some(ios) = &self.ios {
                let _ = ios.shutdown_stream_starts().await;
            }
        };
        tokio::join!(self.previews.shutdown(), android, ios);
    }
}

fn same_device(left: &Device, right: &Device) -> bool {
    left.platform == right.platform
        && left.kind == right.kind
        && left.id == right.id
        && left.serial == right.serial
        && left.transport_id == right.transport_id
        && left.avd == right.avd
}

fn missing_backend(tool: &str) -> Error {
    Error::ToolNotFound { tool: tool.into() }
}

fn oversized() -> Response {
    Response::new(
        None,
        Err(Error::InvalidArgument {
            message: format!("request exceeds {MAX_REQUEST_BYTES} bytes"),
        }),
    )
}

/// Drain an oversized line without ever accumulating it in memory.
async fn read_request<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<Option<Vec<u8>>>> {
    let mut bytes = Vec::new();
    let mut too_large = false;
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Ok(if too_large {
                Some(None)
            } else if bytes.is_empty() {
                None
            } else {
                Some(Some(bytes))
            });
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let length = newline.unwrap_or(buffer.len());
        if !too_large {
            if length > MAX_REQUEST_BYTES - bytes.len() {
                too_large = true;
                bytes.clear();
            } else {
                bytes.extend_from_slice(&buffer[..length]);
            }
        }
        reader.consume(length + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(if too_large { None } else { Some(bytes) }));
        }
    }
}

pub async fn serve<R, W>(host: &mut Host, mut input: R, mut output: W) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let result = async {
        #[cfg(unix)]
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let terminated = async {
            #[cfg(unix)]
            terminate.recv().await;
            #[cfg(not(unix))]
            std::future::pending::<()>().await;
        };
        // Match the adapter's bounded pending limit while continuing to observe owner EOF.
        let (send, mut receive) = tokio::sync::mpsc::channel(64);
        let (eof_send, mut eof_receive) = tokio::sync::watch::channel(false);
        let read = async {
            while let Some(frame) = read_request(&mut input).await? {
                if send.send(frame).await.is_err() {
                    return Ok::<(), Error>(());
                }
            }
            eof_send.send_replace(true);
            drop(send);
            Ok(())
        };
        let respond = async {
            while let Some(frame) = receive.recv().await {
                let response = match frame {
                    Some(bytes) => {
                        let starts_preview = serde_json::from_slice::<Request>(&bytes)
                            .is_ok_and(|request| request.method == "preview.start");
                        if starts_preview {
                            // A preview cannot outlive its owner. Normal buffered requests still drain.
                            tokio::select! {
                                biased;
                                _ = eof_receive.wait_for(|eof| *eof) => return Ok::<(), Error>(()),
                                response = host.request(&bytes) => response,
                            }
                        } else {
                            host.request(&bytes).await
                        }
                    }
                    None => oversized(),
                };
                let mut bytes = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
                bytes.push(b'\n');
                output.write_all(&bytes).await?;
                output.flush().await?;
            }
            Ok(())
        };
        tokio::select! {
            _ = terminated => Ok(()),
            result = async { tokio::try_join!(read, respond)?; Ok(()) } => result,
        }
    }
    .await;
    host.shutdown().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn framing_rejects_oversized_lines_and_recovers() {
        let mut bytes = vec![b'x'; MAX_REQUEST_BYTES + 100];
        bytes.extend_from_slice(b"\n{}\n");
        let mut reader = tokio::io::BufReader::with_capacity(13, bytes.as_slice());
        assert!(read_request(&mut reader).await.unwrap().unwrap().is_none());
        assert_eq!(
            read_request(&mut reader).await.unwrap(),
            Some(Some(b"{}".to_vec()))
        );
        assert!(read_request(&mut reader).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn framing_handles_final_line_without_newline() {
        let mut input = &b"{}"[..];
        assert_eq!(
            read_request(&mut input).await.unwrap(),
            Some(Some(b"{}".to_vec()))
        );
        assert!(read_request(&mut input).await.unwrap().is_none());
    }
}
