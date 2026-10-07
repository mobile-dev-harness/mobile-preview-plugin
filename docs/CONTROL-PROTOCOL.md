# Local control and media contracts

MPP has distinct lifecycle, media and live-input transports. The private Rust
protocols are not MCP, JSON-RPC or scrcpy's wire protocol. The DSH adapter provides
authenticated browser routes; the current live backend implements Android API
29–37 adapters and requires arm64 and a Unix host. Both the host and device use
`android_framework(api)`: APIs 29–33 select SurfaceControl/InputManager, and APIs
34–37 select DisplayManager/InputManagerGlobal; other levels are rejected.
Implemented contracts and minimum API 29 build outputs do not establish device
compatibility or application-level input success. Per-target qualification is
recorded in the [Android matrix](ANDROID-COMPATIBILITY.md).

## Stdio transport

Run `mpp serve --stdio` as a persistent local subprocess. Each UTF-8 JSON request
occupies one line; each response is a JSON object followed by a newline. Stdout is
reserved for responses. The host processes requests sequentially and serves later
requests after a request-level error. EOF releases all session leases.

The request limit is 65,536 bytes excluding the newline. Oversized input is drained
through the next newline and rejected without accumulating the entire line. A final
nonempty line without a newline is accepted. Blank lines are malformed requests.

```json
{"id":"hello-1","method":"hello","params":{}}
```

- `id`: unsigned 64-bit integer or a nonempty string of at most 128 UTF-8 bytes.
- `method`: one of the method names below.
- `params`: an object; defaults to `{}` when omitted. Unknown request or parameter
  fields are rejected.

Responses always include `schema`, `id`, `ok`, `result` and `error`. Exactly one of
`result` and `error` is non-null. `id` is null if a request cannot be decoded or its
ID is invalid; one-shot CLI commands also use a null ID.

```json
{"schema":"mpp/v1","id":"bad-method","ok":false,"result":null,"error":{"code":"UNSUPPORTED","message":"...","hint":"..."}}
```

Use strings for request IDs in JavaScript clients when values may exceed JavaScript's
safe integer range. Session generations are `u64` on the wire; clients must preserve
them without rounding.

## Methods

| Method | Parameters | Result |
| --- | --- | --- |
| `hello` | `{}` | Version, protocol names, method names and backend availability |
| `devices.list` | `{}` | `{devices: [...], warnings: [...]}` |
| `emulator.start` | `{avd: string, consent: true}` | Selected Android device after boot completes |
| `session.connect` | `{owner: string, device: string}` | Session after an exact-serial probe |
| `session.status` | `{owner, session, generation}` | Session after another exact-serial probe |
| `session.disconnect` | `{owner, session, generation}` | Disconnection receipt; lease becomes invalid |
| `preview.start` | Lease plus trusted asset/socket options below | Private preview descriptor |
| `preview.stop` | `{owner, session, generation, stream_id, epoch}` | `{stopped: true}` for that exact capture |
| `input.send` | `{owner, session, generation, event}` | `UNSUPPORTED` after lease and event validation |

`hello` reports `video_backend` and `input_backend` as `requires_device_assets`.
There is no screenshot method. The lifecycle-stdio `input.send` remains unsupported
by design: live input travels over its dedicated socket and the DSH HTTP method
of the same name. Input must not wait behind lifecycle operations.

### Private preview lifecycle

`preview.start` requires `owner`, `session`, `generation`, and these trusted adapter
fields: absolute `bootstrap`/`library` asset paths, an existing ordinary mode-0700
`socket_dir`, a fresh 32-character lowercase-hex `stream_id`, and a 64-character
lowercase-hex device `token`. Optional `max_size`, `bit_rate`, `max_fps` default to
1280 pixels, 4,000,000 bits/s and 30 fps. Size must be even and in 256–2048, bitrate
in 100,000–20,000,000, and fps in 1–60. These are requested settings, not measured
performance guarantees. Calling with just a lease returns `UNSUPPORTED`.

The host verifies the adb attachment, implemented API (29–37), arm64 ABI
and device channels before
returning `{stream_id, epoch, generation, geometry, video_socket, control_socket}`.
`geometry` contains encoded `width`/`height`, logical `display_width`/`display_height`
and rotation 0–3. The sockets are `video.sock` and `control.sock` in the canonical
private directory. Node validates all descriptor fields and paths. Browsers receive
an opaque stream handle, generation, epoch and geometry; private socket paths and
device authentication tokens stay on the host. A successful descriptor does not
prove a decoded first frame.

The trusted adapter connects control immediately and video when its sole media
consumer subscribes. Stopping closes those local FDs before queued lifecycle cleanup,
then sends `preview.stop` with the lease, stream ID and capture epoch. It deletes
only its private directory. The chat's transport lease survives preview stop.
The host also stops captures when a session disconnects or its attachment changes.

EOF and SIGTERM cancel pending startup and await owned capture/artifact cleanup.
The adapter allows a 30-second shutdown grace before forcefully terminating the
host. SIGKILL or a host crash cannot guarantee remote artifact cleanup; device
socket/ownership loss still triggers input release and capture shutdown.

### Devices and startup

Device records contain `id`, `platform`, `kind`, `state`, `name`, `serial`,
`transport_id`, `avd` and `capabilities`. Nullable fields remain present.

- Connected Android IDs use `android:<adb-serial>`.
- Stopped AVD IDs use `android-avd:<avd-name>` and have no serial.
- `state` is `online`, `offline`, `unauthorized`, `stopped` or `unknown`.
- Current Android records advertise `video: false`, `input: false`,
  `screenshot: false` and `lifecycle: true`. Availability and supported methods
  still govern which operations can run; `lifecycle` is not a shutdown API. These
  discovery flags do not advertise a configured capture. The DSH adapter separately
  reports built asset availability and qualifies an actual preview on start.

Discovery does not silently choose, start or stop devices. Connecting requires a
running device; use `emulator.start` with the exact discovered AVD name and explicit
consent to boot it first. Startup is headless and waits up to 120 seconds for boot
completion. A successful emulator remains running across disconnect and host exit.
The `mpp boot --avd NAME` CLI command is itself an explicit startup request.

### Session lifetime

Use an owner label such as a DSH chat ID: 1–128 UTF-8 bytes, not entirely whitespace,
with no control characters. Each host allows at most one active session per owner
and one owner per device. A conflicting connect returns `BUSY`.

Example session returned by `session.connect` for a running emulator:

```json
{
  "id": "session-1",
  "owner": "chat-a",
  "device": {
    "id": "android:emulator-5554",
    "platform": "android",
    "kind": "emulator",
    "state": "online",
    "name": "Pixel_9",
    "serial": "emulator-5554",
    "transport_id": "1",
    "avd": "Pixel_9",
    "capabilities": {
      "video": false,
      "input": false,
      "screenshot": false,
      "lifecycle": true
    }
  },
  "generation": 1,
  "state": "transport_ready"
}
```

`transport_ready` means adb was reachable and a local lease is held. It does not
mean preview or input is ready. Copy `owner`, `id` (as `session`) and `generation`
from this result into later calls:

```json
{"id":2,"method":"session.status","params":{"owner":"chat-a","session":"session-1","generation":1}}
{"id":3,"method":"session.disconnect","params":{"owner":"chat-a","session":"session-1","generation":1}}
```

The sequential host probes before reserving, so cancelling a connection attempt
cannot leave an unreachable reservation. Status probes compare the observed adb
transport ID and AVD identity with the lease; a missing or replacement device
invalidates it. Disconnect returns state `disconnected`; subsequent calls on that
lease return `STALE_SESSION`. Reconnecting allocates a new generation.

Android transport IDs are valid within one adb server lifetime. Restart the MPP
host if the adb server is restarted; a serial alone is not an attachment identity.

IDs and generations are scoped to one host process and can repeat after restart.
Discard all saved leases on subprocess exit. These fields coordinate local clients;
they are not authentication tokens or cross-process transport locks. The native
capture process separately acquires Android's `mpp-control-owner-v1` abstract-socket
lock to reject competing MPP helpers. External adb and physical input are not locked
out. Device channels authenticate their separate token/lease/epoch identities;
browser requests use DSH's existing authentication rather than a public Rust API.

## DSH browser API

POST `/api/mobile-preview/v1` with `{method, params}`. Success returns
`{ok: true, result}`; failure returns `{ok: false, error: {code, message, hint}}`.
JSON request bodies are limited to 64 KiB. DSH authenticates the route before the
adapter validates the client, conversation and binding.

| Method | Parameters | Result / purpose |
| --- | --- | --- |
| `client.open` | `{}` | Opaque client token, host, timing budgets and asset availability |
| `client.heartbeat` | `{client}` | Renews the browser client and audits its conversations |
| `client.close` | `{client}` | Releases that client's bindings and previews |
| `devices.list` | `{client}` | Inventory from the DSH Host machine |
| `emulator.start` | `{client, avd, consent: true}` | Explicitly boots one AVD |
| `session.connect` | `{client, sessionId, device}` | Opaque binding and transport session |
| `session.list` | `{client, sessionId}` | Existing conversation binding or null |
| `session.status` | `{client, binding}` | Revalidates the device/conversation binding |
| `session.disconnect` | `{client, binding}` | Releases preview and transport lease |
| `preview.start` | `{client, binding}` | `{stream, epoch, generation, geometry, capabilities}` |
| `preview.stop` | `{client, binding, stream}` | Stops only the matching capture; old tokens cannot stop a new one |
| `input.send` | `{client, binding, stream, requests}` | Ordered input replies from the dedicated control socket |

Client, binding and stream handles are distinct opaque 43-character tokens. The
server derives Rust owners and leases from validated DSH context. Browser requests
cannot select executable paths, device secrets, socket paths or Rust owners.
An available asset flag is not evidence that a particular device/codec will work.

POST `/api/mobile-preview/v1/media` with `{client, binding, stream}` returns
`application/octet-stream` MPP1 bytes through streaming Fetch. It uses the same
authentication and binding checks. Only one media consumer is allowed per capture;
abort/cancellation stops that capture, not its chat lease. No frame is inserted
into model context and no agent/MCP tools are registered.

Default browser-client heartbeats are 15 seconds with 45-second expiry. These are
distinct from the pressed-input watchdog. The browser owns per-chat preview intent;
a new connection defaults to automatic preview when an eligible canvas is visible.
Hiding/pagehide suspends capture and releases input but retains that intent and a
still-valid binding. Visible remount/return starts a new capture only after old
startup/cleanup work settles. An old view or late response cannot revive an obsolete
capture. These are client lifecycle rules, not new wire methods.

Explicit Pause remains paused until Resume. Genuine failures latch in the current
visible view/visibility period. Explicit Resume/Retry or a new visible view/return
can make one fresh attempt if wanted intent and binding remain valid; ordinary
rerenders and heartbeats do not clear the latch. There is no timer/hot retry loop
or input replay. Conversation deletion,
disconnect, binding/client expiry, plugin disposal and host failure invalidate
the corresponding resources and clear intent. Returning after lease expiry does
not automatically reconnect a device.

## Dedicated live input

Each batch contains 1–64 requests. Each request is one UTF-8 JSON line of at most
16,384 bytes excluding newline, within the HTTP body's overall 64 KiB limit:

```json
{"seq":1,"epoch":1,"command":{"kind":"input","event":{"kind":"touch","phase":"down","x":0.5,"y":0.5,"width":480,"height":1066}}}
{"seq":2,"epoch":1,"command":{"kind":"reset"}}
```

Use the current capture epoch and encoded frame dimensions. `seq` is positive,
strictly increasing and allocated solely by the browser, including `reset`,
`heartbeat`, `stop` and `key_frame` commands. Node and Rust do not synthesize competing
sequence numbers or retry uncertain input. The DSH adapter rejects integers that
JavaScript cannot represent exactly. Only one batch may be in flight per capture.

Input events include:

```json
{"kind":"touch","phase":"down","x":0.5,"y":0.5,"width":480,"height":1066}
{"kind":"key","code":4,"phase":"down"}
```

Touch phases are `down`, `move`, `up`, `cancel`; coordinates must be finite numbers
in `[0, 1]`. Dimensions must match the active encoded geometry. The state machine
allows one pointer and requires matching down/move/up sequences. Key phases are
`down` and `up`; codes must fit an Android signed integer, with matching press/release
state. The panel exposes Home/Back and focused-canvas arrows, Enter, Backspace, Tab
and Space. The core event type retains `text`, but live text/IME injection is
unsupported. Unknown fields, stale epochs and stale geometry are rejected.

Every complete control reply has a two-second deadline and matches its request:

```json
{"seq":1,"ok":true,"code":null,"message":null}
```

`ok` means OS submission or accepted control command, not application success.
Negative replies contain nonempty `code`/`message`; the adapter returns received
replies, stops preview and sends no remaining batch events. Malformed, mismatched,
oversized, timed-out or cancelled exchanges terminate the control channel. No
automatic replay occurs.

Held controls generate browser-origin heartbeats every 750 ms. The device cancels
pressed input after two seconds without valid controller traffic; an open adb stdin
pipe only maintains process ownership and does not renew that watchdog. Reset,
EOF, geometry change and shutdown release tracked input. Rotation or size changes
end the capture and require a fresh epoch/preview.

### Error codes

`error` contains a stable `code`, explanatory `message` and actionable `hint`.
Core codes are `INVALID_ARGUMENT`, `NOT_FOUND`, `BUSY`, `STALE_SESSION`,
`PERMISSION_DENIED`, `UNSUPPORTED`, `TOOL_NOT_FOUND`, `COMMAND_FAILED`, `TIMEOUT`
and `IO_ERROR`. The adapter additionally reports lifecycle/transport failures such
as `ABORTED`, `CLIENT_EXPIRED`, `CLOSED`, `PROTOCOL_ERROR` and `CONNECTION_FAILED`.
Invalid input never becomes an application-success receipt.

## Binary media framing: MPP1

The `mpp-core` library encodes and incrementally decodes packets. It neither
captures frames nor provides a transport; no binary video is sent over control
stdout. The live backend sends H.264 Annex B access units. Codec bytes are opaque
to the framing library; the browser additionally validates configuration and IDR
recovery before passing chunks to WebCodecs.

Each packet has a fixed 28-byte header followed by its payload. Multibyte integers
are big-endian:

| Offset | Bytes | Field |
| --- | --- | --- |
| 0 | 4 | ASCII `MPP1` |
| 4 | 1 | Kind: configuration `0`, key frame `1`, delta frame `2` |
| 5 | 3 | Reserved; must be zero |
| 8 | 8 | Nonzero session generation (`u64`) |
| 16 | 4 | Payload length (`u32`), excluding this header |
| 20 | 8 | Presentation timestamp in microseconds (`u64`) |

A configuration payload starts with width (`u32`) and height (`u32`), followed by
nonempty Annex B configuration bytes. Dimensions must be in `1..=16384`. Key and
delta payloads contain nonempty encoded bytes. The caller selects an inclusive
payload limit between 9 bytes and 16 MiB; configuration dimensions count toward it.
The current live path uses an 8 MiB payload limit.

The parser accepts split and coalesced packets, buffering at most one header and
one bounded payload internally. Returned packets consume additional memory.
Malformed input invalidates that decoder; no packets from the failed call are
returned. Finishing a truncated stream also fails. Framing does not enforce packet
order, lease ownership, timestamp progression or codec correctness. The browser's
stream player matches the lease generation, uses a decoder scoped to the capture
epoch, derives its codec from SPS, and requires configuration plus an IDR keyframe.
When `decodeQueueSize >= 4`, ordered media consumption waits for a `dequeue` event
with a two-second stall deadline. Normal backlog does not reset the decoder or
discard dependent deltas. Actual decoder-error recovery still resets the decoder
and requests an IDR with the required configuration; a static producer may need a
new frame or preview restart to supply it. Only the latest decoded frame awaits
rendering; replaced, displayed and late frames are closed. First-frame decoding
has a separate ten-second deadline. Protocol tests alone establish neither
real-device input effects nor sustained playback performance.
