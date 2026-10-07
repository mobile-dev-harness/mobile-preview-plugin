# Local control and media contracts

These are MPP's initial framework contracts. They are not MCP, JSON-RPC or the
scrcpy wire protocol. Live capture and input injection are not implemented.

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
| `preview.start` | `{owner, session, generation}` | `UNSUPPORTED` after lease validation |
| `input.send` | `{owner, session, generation, event}` | `UNSUPPORTED` after lease and event validation |

`hello` reports `video_backend` and `input_backend` as `not_implemented`.
There is no screenshot method or media endpoint in this increment.

### Devices and startup

Device records contain `id`, `platform`, `kind`, `state`, `name`, `serial`, `avd`
and `capabilities`. Nullable `serial` and `avd` fields remain present.

- Connected Android IDs use `android:<adb-serial>`.
- Stopped AVD IDs use `android-avd:<avd-name>` and have no serial.
- `state` is `online`, `offline`, `unauthorized`, `stopped` or `unknown`.
- Current Android records advertise `video: false`, `input: false`,
  `screenshot: false` and `lifecycle: true`. Availability and supported methods
  still govern which operations can run; `lifecycle` is not a shutdown API.

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
they are not authentication tokens, cross-process locks or protection from external
adb commands and physical input. No network listener or network authentication is
implemented. A future DSH adapter must authenticate remote access separately.

### Input shapes

Input events are defined and validated, but no event is delivered to a device yet:

```json
{"kind":"touch","phase":"down","x":0.5,"y":0.5,"width":1080,"height":1920}
{"kind":"key","code":4,"phase":"down"}
{"kind":"text","text":"hello"}
```

Touch phases are `down`, `move`, `up`, `cancel`; coordinates must be finite numbers
in `[0, 1]`, and frame dimensions must be in `1..=16384`. Key phases are `down` and
`up`; `code` is a `u32`, with platform mapping to be defined by the input backend.
Text contains 1–16,384 UTF-8 bytes and no null character. Unknown fields are rejected.
These checks do not yet validate a gesture sequence against a live frame.

### Errors

`error` contains a stable `code`, explanatory `message` and actionable `hint`.
Current codes are `INVALID_ARGUMENT`, `NOT_FOUND`, `BUSY`, `STALE_SESSION`,
`PERMISSION_DENIED`, `UNSUPPORTED`, `TOOL_NOT_FOUND`, `COMMAND_FAILED`, `TIMEOUT`
and `IO_ERROR`. Invalid input is rejected even when its backend is unavailable;
for example an out-of-range touch coordinate returns `INVALID_ARGUMENT`.

## Binary media framing: MPP1

The `mpp-core` library encodes and incrementally decodes packets. It neither
captures frames nor provides a transport; no binary video is sent over control
stdout. The intended first codec is H.264 Annex B. Codec bytes are opaque to the
framing library and must be validated by the eventual decoder.

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

The parser accepts split and coalesced packets, buffering at most one header and
one bounded payload internally. Returned packets consume additional memory.
Malformed input invalidates that decoder; no packets from the failed call are
returned. Finishing a truncated stream also fails. Framing does not enforce packet
order, lease ownership, timestamp progression or codec correctness. A future media
consumer must match the active lease generation and handle configuration/keyframe
recovery; passing framing tests alone proves none of those playback behaviors.
