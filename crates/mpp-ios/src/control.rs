//! Ordered private control requests; a successful reply follows the native owner's result.

use std::{sync::mpsc::SyncSender, time::Duration};

use mpp_core::{
    Error, InputEvent, KeyPhase, Result, TouchPhase,
    stream::{
        ControlCommand, ControlReply, ControlRequest, Geometry, InputState, MAX_CONTROL_BYTES,
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::oneshot,
    time::{Instant, timeout, timeout_at},
};

use super::{Control, InputWork, StopOnDrop, failed, invalid};
use std::sync::atomic::Ordering;

const INPUT_TIMEOUT: Duration = Duration::from_secs(2);
const PACKET_TIMEOUT: Duration = Duration::from_secs(2);
const NATIVE_TIMEOUT: Duration = Duration::from_millis(1500);
const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);

pub(super) async fn run(
    mut stream: impl AsyncRead + AsyncWrite + Unpin,
    control: &std::sync::Arc<Control>,
    native: &SyncSender<InputWork>,
    geometry: oneshot::Receiver<Geometry>,
    epoch: u64,
    input_enabled: bool,
) -> Result<()> {
    let _stop_on_drop = StopOnDrop(control.clone());
    let geometry = timeout(Duration::from_secs(10), geometry)
        .await
        .map_err(|_| timed_out("waiting for capture geometry"))?
        .map_err(|_| failed("Native capture ended before publishing geometry"))?;
    geometry.validate()?;
    let mut state = InputState::new(epoch, geometry.width, geometry.height)?;
    let result = control_loop(&mut stream, control, native, &mut state, input_enabled).await;
    state.drain_releases();
    // Native ownership has an independent ledger: even an empty logical state must release it.
    let cleanup = release(native).await;
    result.and(cleanup)
}

#[derive(Default)]
struct Held {
    pointer: bool,
    home: bool,
}

impl Held {
    fn any(&self) -> bool {
        self.pointer || self.home
    }

    fn applied(&mut self, events: &[InputEvent]) {
        for event in events {
            match event {
                InputEvent::Touch { phase, .. } => {
                    self.pointer = matches!(phase, TouchPhase::Down | TouchPhase::Move);
                }
                InputEvent::Key { phase, .. } => self.home = *phase == KeyPhase::Down,
                InputEvent::Text { .. } => {}
            }
        }
    }
}

async fn control_loop(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    control: &Control,
    native: &SyncSender<InputWork>,
    state: &mut InputState,
    input_enabled: bool,
) -> Result<()> {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut line = Line::default();
    let mut held = Held::default();
    let mut last_accepted = Instant::now();
    loop {
        let request = tokio::select! {
            biased;
            _ = tokio::time::sleep_until(last_accepted + INPUT_TIMEOUT), if held.any() => {
                state.drain_releases();
                held = Held::default();
                release(native).await?;
                continue;
            }
            line = line.read(&mut reader) => {
                let Some(bytes) = line? else { return Ok(()) };
                serde_json::from_slice::<ControlRequest>(&bytes)
                    .map_err(|_| invalid("Malformed capture control request"))?
            }
        };
        let rejected_feature = match &request.command {
            ControlCommand::Input { .. } if !input_enabled => Some("iOS Simulator input"),
            ControlCommand::Input {
                event: InputEvent::Key { code, .. },
            } if *code != 3 => Some("iOS Simulator keys other than Home"),
            ControlCommand::Input {
                event: InputEvent::Text { .. },
            } => Some("iOS Simulator text input"),
            _ => None,
        };
        let events = if let Some(feature) = rejected_feature {
            // Consume the sequence without introducing an unsupported key into held state.
            state
                .accept(&ControlRequest {
                    seq: request.seq,
                    epoch: request.epoch,
                    command: ControlCommand::Heartbeat,
                })
                .and_then(|_| {
                    Err(Error::Unsupported {
                        feature: feature.into(),
                    })
                })
        } else {
            state.accept(&request)
        };
        let events = match events {
            Ok(events) => events,
            Err(error) => {
                reply(&mut writer, request.seq, Some(&error)).await?;
                continue;
            }
        };
        let applied = match &request.command {
            ControlCommand::Input { .. } => apply(native, events.clone()).await,
            ControlCommand::Reset | ControlCommand::Stop => release(native).await,
            ControlCommand::KeyFrame => {
                control.key_frame.store(true, Ordering::Release);
                Ok(())
            }
            ControlCommand::Heartbeat => Ok(()),
        };
        if let Err(error) = applied {
            // Injection errors end this attachment; the cleanup path releases the native ledger.
            let _ = reply(&mut writer, request.seq, Some(&error)).await;
            return Err(error);
        }
        last_accepted = Instant::now();
        held.applied(&events);
        reply(&mut writer, request.seq, None).await?;
        if matches!(request.command, ControlCommand::Stop) {
            return Ok(());
        }
    }
}

async fn apply(native: &SyncSender<InputWork>, events: Vec<InputEvent>) -> Result<()> {
    let (reply, result) = oneshot::channel();
    native
        .try_send(InputWork::Apply { events, reply })
        .map_err(|_| failed("Native input queue is unavailable"))?;
    native_result(result).await
}

async fn release(native: &SyncSender<InputWork>) -> Result<()> {
    let (reply, result) = oneshot::channel();
    native
        .try_send(InputWork::Release { reply })
        .map_err(|_| failed("Native input queue is unavailable during release"))?;
    native_result(result).await
}

async fn native_result(result: oneshot::Receiver<Result<()>>) -> Result<()> {
    timeout(NATIVE_TIMEOUT, result)
        .await
        .map_err(|_| timed_out("waiting for native input acknowledgement"))?
        .map_err(|_| failed("Native input owner stopped before acknowledgement"))?
}

async fn reply(
    writer: &mut (impl AsyncWrite + Unpin),
    seq: u64,
    error: Option<&Error>,
) -> Result<()> {
    let reply = ControlReply {
        seq,
        ok: error.is_none(),
        code: error.map(|error| error.code().to_owned()),
        message: error.map(ToString::to_string),
    };
    let mut bytes =
        serde_json::to_vec(&reply).map_err(|_| failed("Cannot serialize capture control reply"))?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(failed("Capture control reply exceeds its byte limit"));
    }
    bytes.push(b'\n');
    timeout(REPLY_TIMEOUT, async {
        writer.write_all(&bytes).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| timed_out("writing capture control reply"))??;
    Ok(())
}

#[derive(Default)]
struct Line {
    bytes: Vec<u8>,
    deadline: Option<Instant>,
}

impl Line {
    // Both the partial frame and its deadline survive cancellation by the held-input watchdog.
    async fn read(&mut self, reader: &mut (impl AsyncRead + Unpin)) -> Result<Option<Vec<u8>>> {
        loop {
            let mut byte = [0];
            let read = match self.deadline {
                Some(deadline) => timeout_at(deadline, reader.read(&mut byte))
                    .await
                    .map_err(|_| timed_out("reading capture control request"))??,
                None => reader.read(&mut byte).await?,
            };
            if read == 0 {
                return if self.bytes.is_empty() {
                    Ok(None)
                } else {
                    Err(invalid("Truncated capture control request"))
                };
            }
            if byte[0] == b'\n' {
                self.deadline = None;
                return Ok(Some(std::mem::take(&mut self.bytes)));
            }
            if self.bytes.len() == MAX_CONTROL_BYTES {
                return Err(invalid("Capture control request exceeds 16384 bytes"));
            }
            self.deadline
                .get_or_insert_with(|| Instant::now() + PACKET_TIMEOUT);
            self.bytes.push(byte[0]);
        }
    }
}

fn timed_out(operation: &str) -> Error {
    Error::Timeout {
        operation: operation.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::{
        io::{AsyncBufReadExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
        sync::mpsc::UnboundedReceiver,
        task::JoinHandle,
    };

    struct Harness {
        writer: WriteHalf<DuplexStream>,
        reader: BufReader<ReadHalf<DuplexStream>>,
        work: UnboundedReceiver<InputWork>,
        task: JoinHandle<Result<()>>,
        control: Arc<Control>,
    }

    impl Harness {
        fn new(input_enabled: bool) -> Self {
            Self::with_capacity(input_enabled, 64 * 1024)
        }

        fn with_capacity(input_enabled: bool, capacity: usize) -> Self {
            let (client, server) = tokio::io::duplex(capacity);
            let (reader, writer) = tokio::io::split(client);
            let (native, incoming) = std::sync::mpsc::sync_channel(8);
            let (work, requests) = tokio::sync::mpsc::unbounded_channel();
            tokio::task::spawn_blocking(move || {
                while let Ok(request) = incoming.recv() {
                    if work.send(request).is_err() {
                        break;
                    }
                }
            });
            let (geometry, ready) = oneshot::channel();
            geometry
                .send(Geometry {
                    width: 588,
                    height: 1280,
                    display_width: 1206,
                    display_height: 2622,
                    rotation: 0,
                })
                .unwrap();
            let control = Arc::new(Control::default());
            let running = control.clone();
            let task = tokio::spawn(async move {
                run(server, &running, &native, ready, 7, input_enabled).await
            });
            Self {
                writer,
                reader: BufReader::new(reader),
                work: requests,
                task,
                control,
            }
        }

        async fn send(&mut self, seq: u64, command: ControlCommand) {
            self.request(ControlRequest {
                seq,
                epoch: 7,
                command,
            })
            .await;
        }

        async fn request(&mut self, request: ControlRequest) {
            let mut bytes = serde_json::to_vec(&request).unwrap();
            bytes.push(b'\n');
            self.writer.write_all(&bytes).await.unwrap();
        }

        async fn response(&mut self) -> ControlReply {
            let mut bytes = Vec::new();
            timeout(
                Duration::from_secs(3),
                self.reader.read_until(b'\n', &mut bytes),
            )
            .await
            .unwrap()
            .unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        async fn no_response(&mut self) {
            let mut byte = [0];
            assert!(
                timeout(Duration::from_millis(30), self.reader.read(&mut byte))
                    .await
                    .is_err()
            );
        }

        async fn next(&mut self) -> InputWork {
            timeout(Duration::from_secs(3), self.work.recv())
                .await
                .unwrap()
                .unwrap()
        }

        async fn release(&mut self) {
            let InputWork::Release { reply } = self.next().await else {
                panic!("expected native release");
            };
            reply.send(Ok(())).unwrap();
        }

        async fn finish(mut self) -> Result<()> {
            self.writer.shutdown().await.unwrap();
            self.release().await;
            let result = self.task.await.unwrap();
            assert!(self.control.stop.load(Ordering::Acquire));
            result
        }
    }

    fn touch(phase: TouchPhase) -> ControlCommand {
        ControlCommand::Input {
            event: InputEvent::Touch {
                phase,
                x: 0.25,
                y: 0.5,
                width: 588,
                height: 1280,
            },
        }
    }

    fn key(code: u32, phase: KeyPhase) -> ControlCommand {
        ControlCommand::Input {
            event: InputEvent::Key { code, phase },
        }
    }

    #[tokio::test]
    async fn success_waits_for_native_apply_and_eof_releases_held_input() {
        let mut h = Harness::new(true);
        h.send(1, touch(TouchPhase::Down)).await;
        let InputWork::Apply { events, reply } = h.next().await else {
            panic!("expected native apply")
        };
        assert!(matches!(
            events.as_slice(),
            [InputEvent::Touch {
                phase: TouchPhase::Down,
                ..
            }]
        ));
        h.no_response().await;
        reply.send(Ok(())).unwrap();
        assert!(h.response().await.ok);
        h.finish().await.unwrap();
    }

    #[tokio::test]
    async fn delayed_native_failure_is_negative_then_releases_and_stops() {
        let mut h = Harness::new(true);
        h.send(19, key(3, KeyPhase::Down)).await;
        let InputWork::Apply { reply, .. } = h.next().await else {
            panic!("expected native apply")
        };
        h.no_response().await;
        reply.send(Err(failed("Native injection refused"))).unwrap();
        let response = h.response().await;
        assert_eq!(response.seq, 19);
        assert!(!response.ok);
        assert!(
            response
                .message
                .unwrap()
                .contains("Native injection refused")
        );
        h.release().await;
        assert!(h.task.await.unwrap().is_err());
        assert!(h.control.stop.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn reset_and_stop_release_native_state_before_reply_even_when_empty() {
        let mut h = Harness::new(true);
        for (seq, command) in [(1, ControlCommand::Reset), (2, ControlCommand::Stop)] {
            h.send(seq, command).await;
            let InputWork::Release { reply } = h.next().await else {
                panic!("expected native release")
            };
            h.no_response().await;
            reply.send(Ok(())).unwrap();
            assert!(h.response().await.ok);
        }
        h.release().await;
        h.task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn stale_sequences_epochs_geometry_and_gestures_never_reach_native() {
        let mut h = Harness::new(true);
        h.send(1, ControlCommand::Heartbeat).await;
        assert!(h.response().await.ok);
        h.send(1, touch(TouchPhase::Down)).await;
        assert!(!h.response().await.ok);
        h.request(ControlRequest {
            seq: 2,
            epoch: 6,
            command: touch(TouchPhase::Down),
        })
        .await;
        assert!(!h.response().await.ok);
        h.send(
            2,
            ControlCommand::Input {
                event: InputEvent::Touch {
                    phase: TouchPhase::Down,
                    x: 0.2,
                    y: 0.3,
                    width: 1206,
                    height: 2622,
                },
            },
        )
        .await;
        assert!(!h.response().await.ok);
        h.send(3, touch(TouchPhase::Move)).await;
        assert!(!h.response().await.ok);
        h.send(4, touch(TouchPhase::Up)).await;
        assert!(!h.response().await.ok);
        h.send(5, touch(TouchPhase::Down)).await;
        let InputWork::Apply { reply, .. } = h.next().await else {
            panic!("expected only valid down")
        };
        reply.send(Ok(())).unwrap();
        assert!(h.response().await.ok);
        h.send(6, touch(TouchPhase::Down)).await;
        assert!(!h.response().await.ok);
        assert!(h.work.try_recv().is_err());
        h.finish().await.unwrap();
    }

    #[tokio::test]
    async fn unsupported_keys_consume_sequence_without_entering_held_state() {
        let mut h = Harness::new(true);
        h.send(1, key(4, KeyPhase::Down)).await;
        let rejected = h.response().await;
        assert_eq!(rejected.code.as_deref(), Some("UNSUPPORTED"));
        h.send(1, ControlCommand::Heartbeat).await;
        assert!(!h.response().await.ok);
        h.send(2, key(4, KeyPhase::Up)).await;
        assert_eq!(h.response().await.code.as_deref(), Some("UNSUPPORTED"));
        h.send(3, key(3, KeyPhase::Down)).await;
        let InputWork::Apply { events, reply } = h.next().await else {
            panic!("expected Home")
        };
        assert_eq!(
            events,
            vec![InputEvent::Key {
                code: 3,
                phase: KeyPhase::Down
            }]
        );
        reply.send(Ok(())).unwrap();
        assert!(h.response().await.ok);
        h.send(4, ControlCommand::Reset).await;
        h.release().await;
        assert!(h.response().await.ok);
        h.send(5, key(3, KeyPhase::Up)).await;
        assert!(!h.response().await.ok);
        h.finish().await.unwrap();
    }

    #[tokio::test]
    async fn read_only_capture_accepts_media_commands_and_rejects_all_input() {
        let mut h = Harness::new(false);
        for (seq, command) in [
            (1, touch(TouchPhase::Down)),
            (2, key(3, KeyPhase::Down)),
            (
                3,
                ControlCommand::Input {
                    event: InputEvent::Text {
                        text: "hello".into(),
                    },
                },
            ),
        ] {
            h.send(seq, command).await;
            assert_eq!(h.response().await.code.as_deref(), Some("UNSUPPORTED"));
        }
        h.send(4, ControlCommand::KeyFrame).await;
        assert!(h.response().await.ok);
        assert!(h.control.key_frame.load(Ordering::Acquire));
        h.send(5, ControlCommand::Reset).await;
        h.release().await;
        assert!(h.response().await.ok);
        h.finish().await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_cancels_held_input_and_rejects_the_old_gesture() {
        let mut h = Harness::new(true);
        h.send(1, touch(TouchPhase::Down)).await;
        let InputWork::Apply { reply, .. } = h.next().await else {
            panic!("expected down")
        };
        reply.send(Ok(())).unwrap();
        assert!(h.response().await.ok);
        // Unsupported input must not renew this deadline or create another held button.
        h.send(2, key(4, KeyPhase::Down)).await;
        assert!(!h.response().await.ok);
        h.release().await;
        h.send(3, touch(TouchPhase::Move)).await;
        assert!(!h.response().await.ok);
        h.finish().await.unwrap();
    }

    #[tokio::test]
    async fn malformed_oversized_and_truncated_requests_release_before_exit() {
        for bytes in [
            b"{bad json}\n".to_vec(),
            vec![b'a'; MAX_CONTROL_BYTES + 1],
            b"{\"seq\":1".to_vec(),
        ] {
            let mut h = Harness::new(true);
            h.writer.write_all(&bytes).await.unwrap();
            h.writer.shutdown().await.unwrap();
            h.release().await;
            assert!(h.task.await.unwrap().is_err());
        }
    }

    #[tokio::test]
    async fn missing_native_ack_is_negative_then_attempts_release() {
        let mut h = Harness::new(true);
        h.send(1, touch(TouchPhase::Down)).await;
        let InputWork::Apply {
            reply: pending_reply,
            ..
        } = h.next().await
        else {
            panic!("expected native apply");
        };
        let response = h.response().await;
        assert!(!response.ok);
        assert_eq!(response.code.as_deref(), Some("TIMEOUT"));
        h.release().await;
        assert!(h.task.await.unwrap().is_err());
        assert!(pending_reply.send(Ok(())).is_err());
    }

    #[tokio::test]
    async fn failed_reply_write_releases_the_native_held_input() {
        let mut h = Harness::new(true);
        h.send(1, touch(TouchPhase::Down)).await;
        let InputWork::Apply { reply, .. } = h.next().await else {
            panic!("expected native apply");
        };
        drop(h.reader);
        drop(h.writer);
        reply.send(Ok(())).unwrap();
        let InputWork::Release { reply } = h.work.recv().await.unwrap() else {
            panic!("expected release after failed reply");
        };
        reply.send(Ok(())).unwrap();
        assert!(h.task.await.unwrap().is_err());
        assert!(h.control.stop.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn blocked_reply_has_a_deadline_and_releases_native_input() {
        let mut h = Harness::with_capacity(true, 1);
        h.send(1, touch(TouchPhase::Down)).await;
        let InputWork::Apply { reply, .. } = h.next().await else {
            panic!("expected native apply");
        };
        reply.send(Ok(())).unwrap();
        // Leave the one-byte reply buffer full to exercise bounded control writeback.
        h.release().await;
        let error = h.task.await.unwrap().unwrap_err();
        assert_eq!(error.code(), "TIMEOUT");
        assert!(h.control.stop.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn cancelled_control_future_stops_native_owner_for_ledger_cleanup() {
        let mut h = Harness::new(true);
        h.send(1, key(3, KeyPhase::Down)).await;
        let InputWork::Apply { reply, .. } = h.next().await else {
            panic!("expected native apply");
        };
        reply.send(Ok(())).unwrap();
        assert!(h.response().await.ok);
        h.task.abort();
        assert!(h.task.await.unwrap_err().is_cancelled());
        assert!(h.control.stop.load(Ordering::Acquire));
        assert!(
            timeout(Duration::from_secs(1), h.work.recv())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn line_limit_and_partial_deadline_survive_future_cancellation() {
        let (mut client, mut server) = tokio::io::duplex(MAX_CONTROL_BYTES + 2);
        let mut line = Line::default();
        client
            .write_all(&vec![b'a'; MAX_CONTROL_BYTES])
            .await
            .unwrap();
        assert!(
            timeout(Duration::from_millis(20), line.read(&mut server))
                .await
                .is_err()
        );
        assert_eq!(line.bytes.len(), MAX_CONTROL_BYTES);
        let deadline = line.deadline.unwrap();
        client.write_all(b"\n").await.unwrap();
        assert_eq!(
            line.read(&mut server).await.unwrap().unwrap().len(),
            MAX_CONTROL_BYTES
        );
        assert!(line.deadline.is_none());
        line.bytes.push(b'{');
        line.deadline = Some(deadline - PACKET_TIMEOUT);
        assert!(line.read(&mut server).await.is_err());
    }
}
