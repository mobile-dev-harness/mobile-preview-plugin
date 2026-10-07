#!/usr/bin/env python3
"""Opt-in native stream smoke test; never boots or selects a device implicitly.

Requires an already authorized device and built MPP assets. By default Home is
sent through MPP; use --no-input to leave a physical device's foreground alone.
Only metadata and aggregate packet counts are retained, never video payloads.
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import queue
import re
import secrets
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time


MAX_PAYLOAD = 8 * 1024 * 1024
MAX_LINE = 1024 * 1024


class SmokeError(Exception):
    """Messages are authored here, never copied from device or host output."""


def require(condition, message):
    if not condition:
        raise SmokeError(message)


def safe_code(value):
    return value if isinstance(value, str) and re.fullmatch(r"[A-Z_]{1,64}", value) else "UNKNOWN"


def bounded_write(operation, timeout=5):
    outcome = queue.Queue(maxsize=1)

    def write():
        try:
            operation()
            outcome.put(True)
        except (OSError, ValueError):
            outcome.put(False)

    threading.Thread(target=write, daemon=True).start()
    try:
        require(outcome.get(timeout=timeout), "Protocol write failed")
    except queue.Empty:
        raise SmokeError("Protocol write timeout") from None


class JsonLines:
    """A dedicated reader avoids select/TextIO read-ahead races; memory is bounded."""

    def __init__(self, stream):
        self.stream = stream
        self.messages = queue.Queue(maxsize=16)
        self.failed = None
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        try:
            while True:
                line = self.stream.readline(MAX_LINE + 1)
                if not line:
                    raise SmokeError("JSON-lines channel closed")
                require(len(line) <= MAX_LINE and line.endswith(b"\n"), "Invalid JSON-lines length")
                value = json.loads(line)
                require(isinstance(value, dict), "JSON-lines response must be an object")
                self.messages.put_nowait(value)
        except (OSError, ValueError, queue.Full, SmokeError):
            self.failed = "JSON-lines reader failed or closed"

    def receive(self, timeout):
        deadline = time.monotonic() + timeout
        while True:
            try:
                return self.messages.get(timeout=min(0.1, max(0.001, deadline - time.monotonic())))
            except queue.Empty:
                if self.failed:
                    raise SmokeError(self.failed)
                if time.monotonic() >= deadline:
                    raise SmokeError("JSON-lines response timeout")


class Host:
    def __init__(self, executable, adb):
        # stderr is discarded: native errors may include credentials or device data.
        self.process = subprocess.Popen(
            [str(executable), "--adb", str(adb), "serve", "--stdio"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self.lines = JsonLines(self.process.stdout)
        self.request_id = 0

    def rpc(self, method, params=None, timeout=60):
        self.request_id += 1
        request = {"id": self.request_id, "method": method, "params": params or {}}
        wire = (json.dumps(request) + "\n").encode()
        bounded_write(lambda: (self.process.stdin.write(wire), self.process.stdin.flush()))
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            reply = self.lines.receive(max(0.001, deadline - time.monotonic()))
            require(reply.get("schema") == "mpp/v1", "Unexpected host schema")
            # A timed-out request can finish before the following cleanup request.
            if reply.get("id") != self.request_id:
                continue
            if reply.get("ok") is not True:
                code = safe_code((reply.get("error") or {}).get("code"))
                raise SmokeError("Host " + method + " failed: " + code)
            return reply.get("result")
        raise SmokeError("Host RPC timeout")

    def close(self):
        forced = False
        try:
            bounded_write(self.process.stdin.close)
            self.process.wait(timeout=35)
        except (SmokeError, subprocess.TimeoutExpired):
            forced = True
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.process.stdout.close()
        return {"exitCode": self.process.returncode, "forced": forced}


def parse_header(header, generation):
    require(len(header) == 28, "Truncated MPP1 header")
    magic, kind, reserved, observed, length, pts = struct.unpack(">4sB3sQIQ", header)
    require(magic == b"MPP1" and reserved == b"\0\0\0", "Invalid MPP1 header")
    require(kind in (0, 1, 2), "Unknown MPP1 packet kind")
    require(observed == generation and observed > 0, "Stale MPP1 generation")
    require((9 if kind == 0 else 1) <= length <= MAX_PAYLOAD, "Invalid MPP1 payload length")
    return kind, length, pts


def parse_configuration(body):
    require(len(body) >= 9, "Truncated MPP1 configuration")
    width, height = struct.unpack(">II", body[:8])
    require(1 <= width <= 16384 and 1 <= height <= 16384, "Invalid MPP1 dimensions")
    return {"width": width, "height": height}


class Video:
    def __init__(self, channel, generation, geometry, packet_timeout=5):
        self.channel = channel
        self.generation = generation
        self.geometry = geometry
        self.stop = threading.Event()
        self.error = None
        self.packet_timeout = packet_timeout
        self.packet_deadline = None
        self.started = time.monotonic()
        self.metrics = {"packets": {"configuration": 0, "key": 0, "delta": 0},
                        "encodedBytes": 0, "firstKeyMs": None, "configurations": []}
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _exact(self, length, deadline=None):
        data = bytearray()
        # An idle screen need not emit frames; a partially delivered packet must finish.
        while len(data) < length:
            if self.stop.is_set():
                raise SmokeError("Video reader stopped")
            if deadline is not None and time.monotonic() >= deadline:
                raise SmokeError("Partial video packet timeout")
            try:
                self.channel.settimeout(0.5 if deadline is None else
                                        max(0.001, min(0.5, deadline - time.monotonic())))
                chunk = self.channel.recv(length - len(data))
            except socket.timeout:
                continue
            require(bool(chunk), "Video channel closed")
            if deadline is None:
                deadline = time.monotonic() + self.packet_timeout
                self.packet_deadline = deadline
            data.extend(chunk)
        return bytes(data), deadline

    def _read(self):
        try:
            while not self.stop.is_set():
                header, deadline = self._exact(28)
                kind, length, _ = parse_header(header, self.generation)
                body, _ = self._exact(length, deadline)
                name = ("configuration", "key", "delta")[kind]
                if kind == 0:
                    geometry = parse_configuration(body)
                    require(geometry == self.geometry, "MPP1 dimensions differ from stream descriptor")
                    if geometry not in self.metrics["configurations"]:
                        self.metrics["configurations"].append(geometry)
                else:
                    require(self.metrics["packets"]["configuration"] > 0, "Frame preceded configuration")
                if kind == 1 and self.metrics["firstKeyMs"] is None:
                    self.metrics["firstKeyMs"] = round((time.monotonic() - self.started) * 1000)
                self.metrics["packets"][name] += 1
                self.metrics["encodedBytes"] += length
                self.packet_deadline = None
        except (OSError, SmokeError) as error:
            if not self.stop.is_set():
                self.error = str(error) if isinstance(error, SmokeError) else "Video socket failed"

    def finish_packet(self):
        # A duration cutoff must not make a truncated trailing packet look successful.
        deadline = self.packet_deadline
        while deadline is not None and self.packet_deadline == deadline:
            require(self.error is None, self.error or "Video failed")
            require(time.monotonic() < deadline, "Partial video packet timeout")
            time.sleep(min(0.01, max(0, deadline - time.monotonic())))
        require(self.error is None, self.error or "Video failed")

    def close(self):
        self.stop.set()
        try:
            self.channel.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.channel.close()
        self.thread.join(timeout=2)
        require(not self.thread.is_alive(), "Video reader did not stop")


def adb_run(adb, serial, *args, timeout=10):
    result = subprocess.run([str(adb), "-s", serial, *args], stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=timeout, check=False)
    require(result.returncode == 0, "ADB read-only query failed")
    return result.stdout.decode("utf-8", errors="replace").strip()


COMPONENT = re.compile(r"\b([A-Za-z0-9_.]+)/(\.?[A-Za-z0-9_.$]+)\b")


def launcher_package(resolution):
    components = COMPONENT.findall(resolution)
    require(bool(components), "Cannot resolve a default Home activity")
    package = components[-1][0]
    require(package != "android", "Home is a chooser; launcher effect cannot be verified")
    return package


def launcher_resumed(activity, package):
    return package in resumed_packages(activity)


def resumed_packages(activity):
    return [match[0] for line in activity.splitlines()
            if "topResumedActivity=" in line or "mResumedActivity:" in line
            for match in COMPONENT.findall(line)]


def discover_adb(explicit):
    if explicit:
        candidates = [Path(explicit)]
    else:
        roots = [Path(os.environ[name]) for name in ("ANDROID_HOME", "ANDROID_SDK_ROOT")
                 if os.environ.get(name)]
        roots.append(Path.home() / ("Library/Android/sdk" if sys.platform == "darwin" else "Android/Sdk"))
        candidates = [root / "platform-tools/adb" for root in roots]
        on_path = shutil.which("adb")
        if on_path:
            candidates.append(Path(on_path))
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate.resolve()
    raise SmokeError("ADB executable not found; set ANDROID_HOME or pass --adb")


def connect_socket(path, timeout):
    channel = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        channel.settimeout(timeout)
        channel.connect(path)
        return channel
    except BaseException:
        channel.close()
        raise


def run(args):
    result = {"schema": "mpp-android-smoke/v1", "passed": False,
              "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "durationSeconds": args.duration, "inputRequested": not args.no_input,
              "control": [], "cleanup": {}, "errors": []}
    host = video = control = control_file = lease = descriptor = None
    socket_dir = None
    stream_id = secrets.token_hex(16)
    stage = "validate"
    try:
        require(bool(args.serial.strip()) and not args.serial.startswith("-"), "An exact device serial is required")
        require(0 < args.duration <= 3600, "Duration must be in (0, 3600] seconds")
        for name in ("executable", "device_assets", "output"):
            require(Path(getattr(args, name)).is_absolute(), name + " must be an absolute path")
        executable, assets = Path(args.executable), Path(args.device_assets)
        require(executable.is_file() and os.access(executable, os.X_OK), "MPP executable is not executable")
        for filename in ("bootstrap.jar", "libmpp_android_device.so"):
            require((assets / filename).is_file(), "Android device assets are incomplete")
        adb = discover_adb(args.adb)
        stage = "device_metadata"
        api = adb_run(adb, args.serial, "shell", "getprop", "ro.build.version.sdk")
        abi = adb_run(adb, args.serial, "shell", "getprop", "ro.product.cpu.abi")
        page_size = adb_run(adb, args.serial, "shell", "getconf", "PAGESIZE")
        require(api.isdecimal() and page_size.isdecimal(), "Invalid SDK or page-size metadata")
        require(re.fullmatch(r"[A-Za-z0-9_-]{1,32}", abi), "Invalid ABI metadata")
        result["device"] = {"api": int(api), "abi": abi, "pageSize": int(page_size)}
        stage = "connect"
        host = Host(executable, adb)
        devices = host.rpc("devices.list")["devices"]
        matches = [device for device in devices if device.get("serial") == args.serial]
        require(len(matches) == 1 and matches[0].get("state") == "online", "Exact requested device is not online")
        result["device"]["kind"] = matches[0].get("kind")
        session = host.rpc("session.connect", {"owner": "android-matrix-smoke", "device": matches[0]["id"]})
        lease = {"owner": session["owner"], "session": session["id"], "generation": session["generation"]}
        socket_dir = tempfile.mkdtemp(prefix="mpp-smoke-", dir="/tmp")
        stage = "preview_start"
        descriptor = host.rpc("preview.start", {**lease, "bootstrap": str(assets / "bootstrap.jar"),
            "library": str(assets / "libmpp_android_device.so"), "socket_dir": socket_dir,
            "stream_id": stream_id, "token": secrets.token_hex(32),
            "max_size": 1280, "bit_rate": 4000000, "max_fps": 30})
        result["geometry"] = {key: descriptor["geometry"][key] for key in
                              ("width", "height", "display_width", "display_height", "rotation")}
        control = connect_socket(descriptor["control_socket"], 5)
        # The reader has no socket timeout: receive() bounds each reply independently.
        control.settimeout(None)
        control_file = control.makefile("rb")
        replies = JsonLines(control_file)
        video = Video(connect_socket(descriptor["video_socket"], 0.5), session["generation"],
                      {key: descriptor["geometry"][key] for key in ("width", "height")})
        sequence = 0

        def command(name, value):
            nonlocal sequence
            sequence += 1
            wire = {"seq": sequence, "epoch": descriptor["epoch"], "command": value}
            bounded_write(lambda: control.sendall((json.dumps(wire) + "\n").encode()))
            reply = replies.receive(5)
            require(reply.get("seq") == sequence, "Control reply sequence mismatch")
            ok = reply.get("ok") is True
            result["control"].append({"command": name, "acknowledged": ok,
                                     "code": None if ok else safe_code(reply.get("code"))})
            require(ok, "Control command rejected: " + name)

        stage = "control"
        command("key_frame", {"kind": "key_frame"})
        if not args.no_input:
            package = launcher_package(adb_run(adb, args.serial, "shell", "cmd", "package", "resolve-activity",
                                              "--brief", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.HOME"))
            before = resumed_packages(adb_run(adb, args.serial, "shell", "dumpsys", "activity", "activities", timeout=3))
            result["homeStartedOutsideLauncher"] = bool(before) and package not in before
            result["homeEffectVerified"] = False
            require(result["homeStartedOutsideLauncher"],
                    "Home verification requires another app to be resumed first; switch to another app or use --no-input")
            for phase in ("down", "up"):
                command("home_" + phase, {"kind": "input", "event": {"kind": "key", "code": 3, "phase": phase}})
            deadline = time.monotonic() + 8
            confirmed = False
            while time.monotonic() < deadline:
                activity = adb_run(adb, args.serial, "shell", "dumpsys", "activity", "activities", timeout=3)
                if launcher_resumed(activity, package):
                    confirmed = True
                    break
                time.sleep(0.2)
            result["homeEffectVerified"] = confirmed
            require(confirmed, "Home ACK received but launcher was not resumed")
        else:
            result["homeStartedOutsideLauncher"] = None
            result["homeEffectVerified"] = None
        stage = "video"
        deadline = time.monotonic() + args.duration
        while time.monotonic() < deadline:
            require(video.error is None, video.error or "Video failed")
            time.sleep(min(0.1, max(0, deadline - time.monotonic())))
        video.finish_packet()
        require(video.error is None, video.error or "Video failed")
        require(video.metrics["packets"]["configuration"] > 0, "No MPP1 configuration received")
        require(video.metrics["packets"]["key"] > 0, "No MPP1 key frame received")
    except BaseException as error:
        result["errors"].append({"stage": stage, "message": str(error) if isinstance(error, SmokeError)
                                 else "Operation failed (" + type(error).__name__ + ")"})
    finally:
        # Stop while the sockets are still open, so peer EOF cannot race normal stop.
        if host and descriptor:
            try:
                stopped = host.rpc("preview.stop", {**lease, "stream_id": stream_id,
                                                    "epoch": descriptor["epoch"]}, timeout=30)
                result["cleanup"]["previewStopped"] = stopped.get("stopped") is True
            except Exception:
                result["cleanup"]["previewStopped"] = False
        if video:
            try:
                video.close()
            except Exception:
                result["errors"].append({"stage": "cleanup", "message": "Video reader cleanup failed"})
            result["video"] = video.metrics
        if control:
            try:
                control.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            control.close()
        if control_file:
            control_file.close()
        if host and lease:
            try:
                disconnected = host.rpc("session.disconnect", lease, timeout=30)
                result["cleanup"]["sessionDisconnected"] = disconnected.get("state") == "disconnected"
            except Exception:
                result["cleanup"]["sessionDisconnected"] = False
        if host:
            try:
                result["cleanup"]["host"] = host.close()
            except Exception:
                result["errors"].append({"stage": "cleanup", "message": "Host shutdown failed"})
        if socket_dir:
            shutil.rmtree(socket_dir, ignore_errors=True)
        if host and lease:
            try:
                reverse = adb_run(adb, args.serial, "reverse", "--list")
                processes = adb_run(adb, args.serial, "shell", "ps", "-A", "-o", "ARGS")
                result["cleanup"]["ownedReverseRemoved"] = "mpp_" + stream_id not in reverse
                result["cleanup"]["ownedDeviceProcessGone"] = "mpp_" + stream_id not in processes
            except Exception:
                result["errors"].append({"stage": "cleanup", "message": "Cannot verify owned device cleanup"})
        cleanup = result["cleanup"]
        if any(value is False for value in cleanup.values()) or (
                "host" in cleanup and (cleanup["host"]["forced"] or cleanup["host"]["exitCode"] != 0)):
            result["errors"].append({"stage": "cleanup", "message": "One or more cleanup checks failed"})
    result["passed"] = not result["errors"]
    return result


def write_result(path, result):
    path = Path(path)
    require(path.is_absolute(), "Output must be an absolute path")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent,
                                         prefix=".mpp-smoke-", delete=False) as output:
            temporary = Path(output.name)
            json.dump(result, output, indent=2)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if temporary and temporary.exists():
            temporary.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True, help="Exact authorized ADB serial; no boot or auto-selection")
    parser.add_argument("--executable", required=True, help="Absolute path to built MPP executable")
    parser.add_argument("--device-assets", required=True, help="Absolute directory containing bootstrap.jar and .so")
    parser.add_argument("--output", required=True, help="Absolute JSON result path (replaced atomically)")
    parser.add_argument("--adb", help="Explicit ADB executable; otherwise locate SDK before PATH")
    parser.add_argument("--duration", type=float, default=5, help="Seconds of stream observation, 0 < duration <= 3600")
    parser.add_argument("--no-input", action="store_true", help="Skip Home injection and launcher verification")
    args = parser.parse_args()
    result = run(args)
    try:
        write_result(args.output, result)
    except (OSError, SmokeError):
        print("Could not write smoke-test JSON result", file=sys.stderr)
        return 1
    print(json.dumps({"passed": result["passed"], "errors": result["errors"]}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
