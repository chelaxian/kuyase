#!/usr/bin/env python3
"""
Shared E2E test harness for KUYASE headless VMs:
- Pure-Python client for the duplex TCP bridge (:18091 -> :8091) with background reader thread
- Pure-Python client for the guest ADB daemon (:15555 -> :5555) with native `sync:` fast file push
- Artifact cache in `work/e2e-cache/` for YAM v0.2+, Luna, and Station-airplay
"""

from __future__ import annotations

import hashlib
import http.server
import io
import json
import os
import pathlib
import platform
import queue
import re
import socket
import struct
import subprocess
import tarfile
import threading
import time
import urllib.request
import uuid
from typing import Any, Dict, List, Optional, Tuple

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
WORK_DIR = REPO_ROOT / "work"
CACHE_DIR = WORK_DIR / "e2e-cache"

KIND_HOST_COMMAND = 1
KIND_STATE_SNAPSHOT = 1
KIND_PCM_AUDIO_CHUNK = 2
KIND_COMMAND_ACK = 3

ADB_CNXN = 0x4E584E43
ADB_OPEN = 0x4E45504F
ADB_OKAY = 0x59414B4F
ADB_WRTE = 0x45545257
ADB_CLSE = 0x45534C43

YAM_OFFICIAL_REGISTRY_URL = "https://repo.yse.wtf/v2"
YAM_OFFICIAL_REGISTRY_KEY = "ed25519:5Rz8F47uftWVU5kLgWYhprkEZ4rpP2kecUge/a7KQas="
YAM_PROFILES = [
    "yandexmicro",
    "plum",
    "yandexplum",
    "yandexmini",
    "yandexmini_2",
    "bergamot",
    "pickle",
    "cucumber",
    "yandexmidi",
    "mango",
    "yandexstation_2",
    "yandexstation",
    "saturn",
    "jbl_link_music",
    "orion",
]


def find_kuyase_binary() -> pathlib.Path:
    exe = "kuyase.exe" if platform.system() == "Windows" else "kuyase"
    for candidate in (
        REPO_ROOT / "target" / "release" / exe,
        REPO_ROOT / "target" / "debug" / exe,
        REPO_ROOT / "Releases" / exe,
        REPO_ROOT / exe,
    ):
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(
        "kuyase binary not found; build it first with `cargo build --release -p kuyase`"
    )


def discover_test_boards() -> List[str]:
    env_board = os.environ.get("KUYASE_E2E_BOARD", "").strip()
    if env_board and env_board.lower() != "all":
        return [b.strip() for b in env_board.split(",") if b.strip()]

    boards: List[str] = []
    if WORK_DIR.is_dir():
        for p in sorted(WORK_DIR.glob("*-system.ext4")):
            name = p.name[: -len("-system.ext4")]
            if not name.endswith("-orig"):
                boards.append(name)
    return boards


class BridgeTcpClient:
    """Duplex TCP bridge client speaking `[u8 kind][u32_le len][payload]`."""

    def __init__(self, host: str, port: int, timeout: float = 180.0) -> None:
        self.host = host
        self.port = port
        self.sock: Optional[socket.socket] = None
        self._lock = threading.Lock()
        self.latest_state: Dict[str, Any] = {}
        self._ack_queue: "queue.Queue[Dict[str, Any]]" = queue.Queue()
        self._alive = False
        self._reader_thread: Optional[threading.Thread] = None
        self._connect(timeout)

    def _connect(self, timeout: float) -> None:
        deadline = time.monotonic() + timeout
        last_err: Optional[Exception] = None
        while time.monotonic() < deadline:
            s: Optional[socket.socket] = None
            try:
                s = socket.create_connection((self.host, self.port), timeout=2.5)
                s.settimeout(2.5)
                for _ in range(16):
                    kind, payload = self._read_packet_from(s)
                    if kind == KIND_STATE_SNAPSHOT:
                        self.latest_state = json.loads(payload.decode("utf-8"))
                        s.settimeout(None)
                        self.sock = s
                        self._alive = True
                        self._reader_thread = threading.Thread(
                            target=self._reader_loop, daemon=True
                        )
                        self._reader_thread.start()
                        return
                s.close()
            except Exception as e:
                last_err = e
                if s is not None:
                    try:
                        s.close()
                    except Exception:
                        pass
                time.sleep(0.25)
        raise TimeoutError(
            f"Timed out connecting to qemu-hw-bridge on {self.host}:{self.port}: {last_err}"
        )

    def _reader_loop(self) -> None:
        assert self.sock is not None
        while self._alive:
            try:
                kind, payload = self._read_packet_from(self.sock)
                if kind == KIND_STATE_SNAPSHOT:
                    snap = json.loads(payload.decode("utf-8"))
                    with self._lock:
                        self.latest_state = snap
                elif kind == KIND_COMMAND_ACK:
                    ack = json.loads(payload.decode("utf-8"))
                    self._ack_queue.put(ack)
                elif kind == KIND_PCM_AUDIO_CHUNK:
                    pass
            except Exception:
                self._alive = False
                break

    @staticmethod
    def _read_exact(s: socket.socket, n: int) -> bytes:
        buf = bytearray()
        while len(buf) < n:
            chunk = s.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("EOF from qemu-hw-bridge")
            buf.extend(chunk)
        return bytes(buf)

    def _read_packet_from(self, s: socket.socket) -> Tuple[int, bytes]:
        hdr = self._read_exact(s, 5)
        kind = hdr[0]
        (length,) = struct.unpack("<I", hdr[1:5])
        payload = self._read_exact(s, length) if length > 0 else b""
        return kind, payload

    def send_command(self, cmd: Any, timeout: float = 15.0) -> Dict[str, Any]:
        if self.sock is None or not self._alive:
            raise RuntimeError("BridgeTcpClient is closed")
        raw = json.dumps(cmd).encode("utf-8")
        frame = struct.pack("<BI", KIND_HOST_COMMAND, len(raw)) + raw
        self.sock.sendall(frame)
        return self._ack_queue.get(timeout=timeout)

    def request_state(self) -> Dict[str, Any]:
        ack = self.send_command("RequestState")
        if not ack.get("ok", False):
            raise RuntimeError(f"request_state failed: {ack}")
        with self._lock:
            return dict(self.latest_state)

    def close(self) -> None:
        self._alive = False
        if self.sock is not None:
            try:
                self.sock.shutdown(socket.SHUT_RDWR)
            except Exception:
                pass
            try:
                self.sock.close()
            except Exception:
                pass
            self.sock = None


class _BlobHttpServer:
    """In-memory HTTP server reachable from QEMU guest SLIRP at `http://10.0.2.2:<port>`."""

    _instance: Optional["_BlobHttpServer"] = None
    _lock = threading.Lock()

    def __init__(self) -> None:
        self.blobs: Dict[str, bytes] = {}
        blobs_ref = self.blobs

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                data = blobs_ref.get(self.path)
                if data is None:
                    self.send_response(404)
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, format: str, *args: Any) -> None:
                pass

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    @classmethod
    def instance(cls) -> "_BlobHttpServer":
        with cls._lock:
            if cls._instance is None:
                cls._instance = _BlobHttpServer()
            return cls._instance


class AdbTcpClient:
    """Pure-Python ADB TCP client (:15555 -> :5555) with persistent connection & SLIRP HTTP push."""

    def __init__(self, host: str = "127.0.0.1", port: int = 15555) -> None:
        self.host = host
        self.port = port
        self._sock: Optional[socket.socket] = None
        self._next_local_id = 1

    @staticmethod
    def _pack_msg(cmd: int, arg0: int, arg1: int, payload: bytes = b"") -> bytes:
        checksum = sum(payload) & 0xFFFFFFFF
        magic = cmd ^ 0xFFFFFFFF
        return struct.pack("<6I", cmd, arg0, arg1, len(payload), checksum, magic) + payload

    @staticmethod
    def _read_exact(s: socket.socket, n: int) -> bytes:
        buf = bytearray()
        while len(buf) < n:
            chunk = s.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("EOF from adbd")
            buf.extend(chunk)
        return bytes(buf)

    def _read_msg(self, s: socket.socket) -> Tuple[int, int, int, bytes]:
        hdr = self._read_exact(s, 24)
        cmd, arg0, arg1, length, _crc, _magic = struct.unpack("<6I", hdr)
        payload = self._read_exact(s, length) if length > 0 else b""
        return cmd, arg0, arg1, payload

    def _ensure_connected(self, timeout: float) -> socket.socket:
        if self._sock is not None:
            return self._sock
        deadline = time.monotonic() + max(timeout, 5.0)
        last_err: Optional[Exception] = None
        while time.monotonic() < deadline:
            s: Optional[socket.socket] = None
            try:
                s = socket.create_connection((self.host, self.port), timeout=2.5)
                s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
                s.settimeout(4.0)
                s.sendall(
                    self._pack_msg(ADB_CNXN, 0x01000000, 256 * 1024, b"host::kuyase-e2e\x00")
                )
                cmd, _a0, _a1, _p = self._read_msg(s)
                if cmd == ADB_CNXN:
                    self._sock = s
                    self._next_local_id = 1
                    return s
                s.close()
            except Exception as e:
                last_err = e
                if s is not None:
                    try:
                        s.close()
                    except Exception:
                        pass
                time.sleep(0.15)
        raise ConnectionError(f"Failed to connect to adbd on {self.host}:{self.port}: {last_err}")

    def close(self) -> None:
        if self._sock is not None:
            try:
                self._sock.close()
            except Exception:
                pass
            self._sock = None

    def shell(self, command: str, timeout: float = 60.0) -> str:
        deadline = time.monotonic() + timeout
        last_err: Optional[Exception] = None
        while time.monotonic() < deadline:
            try:
                s = self._ensure_connected(deadline - time.monotonic())
                s.settimeout(max(deadline - time.monotonic(), 2.0))
                local_id = self._next_local_id
                self._next_local_id += 1
                open_payload = f"shell:{command}\x00".encode("utf-8")
                s.sendall(self._pack_msg(ADB_OPEN, local_id, 0, open_payload))

                remote_id = 0
                out = bytearray()
                while True:
                    cmd, arg0, arg1, payload = self._read_msg(s)
                    if cmd == ADB_OKAY and arg1 == local_id:
                        remote_id = arg0
                    elif cmd == ADB_WRTE and arg1 == local_id:
                        remote_id = arg0
                        out.extend(payload)
                        s.sendall(self._pack_msg(ADB_OKAY, local_id, remote_id))
                    elif cmd == ADB_CLSE and (arg1 == local_id or arg1 == 0):
                        s.sendall(self._pack_msg(ADB_CLSE, local_id, remote_id))
                        break
                return out.decode("utf-8", errors="replace").replace("\r\n", "\n")
            except Exception as e:
                last_err = e
                self.close()
                time.sleep(0.15)
        raise RuntimeError(f"ADB shell timed out ({command}): {last_err}")

    def shell_checked(self, command: str, timeout: float = 60.0) -> str:
        wrapped = f'( {command} ); __rc=$?; printf "\\n__EXIT_CODE=%d\\n" "$__rc"'
        raw = self.shell(wrapped, timeout=timeout)
        rc = 1
        lines: List[str] = []
        for line in raw.splitlines():
            if line.startswith("__EXIT_CODE="):
                try:
                    rc = int(line.split("=", 1)[1].strip())
                except ValueError:
                    rc = 1
            else:
                lines.append(line)
        clean = "\n".join(lines)
        if rc != 0:
            raise RuntimeError(
                f"Guest command failed (exit {rc}): {command}\nOutput:\n{clean}"
            )
        return clean

    def push_bytes(self, remote_path: str, data: bytes, mode: int = 0o755) -> None:
        """Push file into the guest via `wget http://10.0.2.2:<port>/...`."""
        parent = os.path.dirname(remote_path) or "/"
        srv = _BlobHttpServer.instance()
        blob_key = f"/blob/{uuid.uuid4().hex}"
        srv.blobs[blob_key] = data
        try:
            oct_mode = oct(mode)[2:]
            self.shell_checked(
                f"mkdir -p '{parent}' && "
                f"wget -qO '{remote_path}' 'http://10.0.2.2:{srv.port}{blob_key}' && "
                f"chmod {oct_mode} '{remote_path}'",
                timeout=60.0,
            )
        finally:
            srv.blobs.pop(blob_key, None)


class KuyaseHeadlessVm:
    """Context manager launching `kuyase run --headless --keep-alive`."""

    def __init__(self, board: str, port_offset: int = 200) -> None:
        self.board = board
        self.port_offset = port_offset
        self.bridge_port = 18091 + port_offset
        self.adb_port = 15555 + port_offset
        self.proc: Optional[subprocess.Popen[str]] = None
        self.bridge: Optional[BridgeTcpClient] = None
        self.adb = AdbTcpClient("127.0.0.1", self.adb_port)

    def __enter__(self) -> "KuyaseHeadlessVm":
        kuyase_bin = find_kuyase_binary()
        cmd = [
            str(kuyase_bin),
            "run",
            "--headless",
            "--board",
            self.board,
            "--snapshot",
            "--keep-alive",
            "--port-offset",
            str(self.port_offset),
        ]
        self.proc = subprocess.Popen(
            cmd,
            cwd=REPO_ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        try:
            assert self.proc.stdout is not None
            deadline = time.monotonic() + 180.0
            ready = False
            output_lines: List[str] = []
            while time.monotonic() < deadline:
                line = self.proc.stdout.readline()
                if not line:
                    if self.proc.poll() is not None:
                        break
                    time.sleep(0.1)
                    continue
                output_lines.append(line.rstrip())
                if "READY (" in line:
                    ready = True
                    break
            if not ready:
                tail = "\n".join(output_lines[-20:])
                raise RuntimeError(
                    f"Headless VM '{self.board}' failed to become READY:\n{tail}"
                )
            # Drain remaining stdout in background so pipe never blocks
            stdout_pipe = self.proc.stdout
            threading.Thread(
                target=lambda: [None for _ in stdout_pipe], daemon=True
            ).start()
            self.bridge = BridgeTcpClient("127.0.0.1", self.bridge_port, timeout=30.0)
        except Exception:
            self.stop()
            raise
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        self.stop()

    def stop(self) -> None:
        self.adb.close()
        if self.bridge is not None:
            try:
                self.bridge.send_command("PowerOff", timeout=3.0)
            except Exception:
                pass
            self.bridge.close()
            self.bridge = None
        if self.proc is not None:
            try:
                self.proc.wait(timeout=4.0)
            except subprocess.TimeoutExpired:
                if platform.system() == "Windows":
                    subprocess.run(
                        ["taskkill", "/F", "/T", "/PID", str(self.proc.pid)],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                    )
                else:
                    self.proc.kill()
                try:
                    self.proc.wait(timeout=3.0)
                except Exception:
                    pass
            if self.proc.stdout is not None:
                try:
                    self.proc.stdout.close()
                except Exception:
                    pass
            self.proc = None


def _http_get_bytes(url: str, timeout: float = 30.0) -> bytes:
    req = urllib.request.Request(
        url,
        headers={"User-Agent": "kuyase-e2e-harness/1.0"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return resp.read()


def ensure_yam_v2_cached() -> pathlib.Path:
    """Download and cache the latest YAM v0.2+ release and platform definitions."""
    yam_cache = CACHE_DIR / "yam"
    yam_cache.mkdir(parents=True, exist_ok=True)
    bin_path = yam_cache / "yam-aarch64-unknown-linux-musl"
    profiles_dir = yam_cache / "platforms"
    profiles_dir.mkdir(parents=True, exist_ok=True)

    env_yam_bin = os.environ.get("KUYASE_YAM_BIN", "").strip()
    if env_yam_bin:
        src = pathlib.Path(env_yam_bin)
        if src.is_file():
            bin_path.write_bytes(src.read_bytes())

    if not bin_path.is_file() or bin_path.stat().st_size == 0:
        releases_raw = _http_get_bytes(
            "https://api.github.com/repos/nyakokitsu/yam/releases?per_page=30"
        ).decode("utf-8")
        m = re.search(r'"tag_name"\s*:\s*"(yam-v[^"]+)"', releases_raw)
        if not m:
            raise RuntimeError("Could not find a `yam-v*` release in nyakokitsu/yam")
        tag = m.group(1)
        base_url = f"https://github.com/nyakokitsu/yam/releases/download/{tag}"
        sums_txt = _http_get_bytes(f"{base_url}/SHA256SUMS", timeout=30.0).decode("utf-8")
        asset_bytes = _http_get_bytes(
            f"{base_url}/yam-aarch64-unknown-linux-musl", timeout=120.0
        )
        expected_sha = None
        for line in sums_txt.splitlines():
            parts = line.split()
            if len(parts) >= 2 and parts[1] == "yam-aarch64-unknown-linux-musl":
                expected_sha = parts[0].lower()
                break
        if expected_sha:
            actual_sha = hashlib.sha256(asset_bytes).hexdigest().lower()
            if actual_sha != expected_sha:
                raise RuntimeError(
                    f"SHA-256 mismatch for yam-aarch64-unknown-linux-musl: {actual_sha} != {expected_sha}"
                )
        bin_path.write_bytes(asset_bytes)

    for prof in YAM_PROFILES:
        prof_file = profiles_dir / f"{prof}.yaml"
        if not prof_file.is_file() or prof_file.stat().st_size == 0:
            raw = _http_get_bytes(
                f"https://raw.githubusercontent.com/nyakokitsu/yam/main/platforms/{prof}.yaml",
                timeout=20.0,
            )
            prof_file.write_bytes(raw)

    return yam_cache


def detect_yam_platform_on_vm(vm: KuyaseHeadlessVm) -> str:
    """Detect YAM v0.2+ platform by SoC (`/proc/device-tree/compatible`) + `install.sh`."""
    dt_compat = (
        vm.adb.shell("tr '\\000' ' ' </proc/device-tree/compatible 2>/dev/null || true")
        .lower()
        .replace(" ", "")
    )
    # Disambiguate by SoC (HackAlice):
    # - Retail Station 2 (`YNDX-00051`) has DT `yandex,station-midi` on SoC `amlogic,axg`
    #   (whereas `quasar.cfg` has `deviceType: "yandexmidi"`).
    # - Retail Station Midi (`YNDX-00054`) is `cucumber` on SoC `amlogic,a5`.
    if "yandex,station-midi" in dt_compat and "amlogic,axg" in dt_compat:
        return "yandexstation_2"
    if "cucumber" in dt_compat and "amlogic,a5" in dt_compat:
        return "cucumber"

    install_sh = _http_get_bytes(
        "https://raw.githubusercontent.com/nyakokitsu/yam/main/install.sh", timeout=20.0
    ).decode("utf-8")
    id_match = re.search(r"(device_identity\(\)\s*\{.*?\n\})", install_sh, re.DOTALL)
    det_match = re.search(r"(detect_platform\(\)\s*\{.*?\n\})", install_sh, re.DOTALL)
    if not id_match or not det_match:
        raise RuntimeError("device_identity/detect_platform not found in yam install.sh")
    script = (
        'adb() { shift 3; sh -c "$1"; }\n'
        "DEVICE=local\n"
        f"{id_match.group(1)}\n"
        f"{det_match.group(1)}\n"
        "DEVICE_IDENTITY=$(device_identity)\n"
        "detect_platform\n"
    )
    return vm.adb.shell_checked(script, timeout=20.0).strip().splitlines()[-1].strip()


def install_yam_v2_on_vm(vm: KuyaseHeadlessVm) -> str:
    """Provision YAM v0.2+ into `/data/yam` and `/usr/bin/yam` on the running VM."""
    yam_cache = ensure_yam_v2_cached()
    yam_platform = detect_yam_platform_on_vm(vm)

    vm.adb.shell_checked("mkdir -p /data/yam/bin /data/yam/platforms /tmp/yam")
    vm.adb.push_bytes(
        "/data/yam/bin/yam",
        (yam_cache / "yam-aarch64-unknown-linux-musl").read_bytes(),
        mode=0o755,
    )
    tar_buf = io.BytesIO()
    with tarfile.open(fileobj=tar_buf, mode="w") as tf:
        for prof_file in sorted((yam_cache / "platforms").glob("*.yaml")):
            data = prof_file.read_bytes()
            ti = tarfile.TarInfo(name=prof_file.name)
            ti.size = len(data)
            ti.mode = 0o644
            tf.addfile(ti, io.BytesIO(data))
    vm.adb.push_bytes("/tmp/yam-platforms.tar", tar_buf.getvalue(), mode=0o644)
    vm.adb.shell_checked(
        "tar -xf /tmp/yam-platforms.tar -C /data/yam/platforms && rm -f /tmp/yam-platforms.tar"
    )

    yam_run = (
        "#!/bin/sh\n"
        f"export YAM_PLATFORM='{yam_platform}'\n"
        "export YAM_PROFILES='/data/yam/platforms'\n"
        "export YAM_ROOT='/data/yam'\n"
        "export YAM_RUN='/tmp/yam'\n"
        "exec '/data/yam/bin/yam' \"$@\"\n"
    ).encode("utf-8")
    vm.adb.push_bytes("/data/yam/bin/yam-run", yam_run, mode=0o755)
    vm.adb.shell_checked(
        "mount -o remount,rw / 2>/dev/null || true; "
        "if ! grep -q '^nameserver ' /etc/resolv.conf 2>/dev/null; then "
        "  printf 'nameserver 10.0.2.3\\nnameserver 1.1.1.1\\nnameserver 8.8.8.8\\n' > /tmp/resolv.conf; "
        "  rm -f /etc/resolv.conf 2>/dev/null || true; "
        "  cp -f /tmp/resolv.conf /etc/resolv.conf 2>/dev/null || ln -sfn /tmp/resolv.conf /etc/resolv.conf; "
        "fi; "
        "ln -sfn /data/yam/bin/yam-run /usr/bin/yam; "
        "mkdir -p /etc/profile.d; "
        "printf '# Managed by YAM installer\\nexport PATH=\"/data/yam/bin:$PATH\"\\n' > /etc/profile.d/yam.sh"
    )
    vm.adb.shell_checked(
        f"/usr/bin/yam registry add official {YAM_OFFICIAL_REGISTRY_URL} --key {YAM_OFFICIAL_REGISTRY_KEY} "
        "|| true"
    )
    return yam_platform
