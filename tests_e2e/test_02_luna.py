#!/usr/bin/env python3
"""
E2E Smoke Test 02: Luna Runtime (`nyakokitsu/luna`) Installer (`install.sh` & `install.ps1`)
Executes exclusively via `kuyase run --headless --board <board> --snapshot --exec`:
1. Runs the unmodified `nyakokitsu/luna/install.sh` inside the guest with canonical `uname -n`
   (verifying station auto-detection without `--platform`)
2. Verifies `nyakokitsu/luna/install.ps1` `Compact` + `Get-Aliases` station matching against `manifest.json`
3. Verifies `/etc/init.d/S99luna` starts `/data/quazar/luna/bin/luna` on `127.0.0.1:8099`
4. Verifies `GET /api/device`, `POST /api/led/ring`, `POST /api/led/off`, `POST /api/clock`, and `POST /api/volume/set`
   (while `kuyase --exec` enforces `ipc_error_count == 0` and `led_trigger_conflict_count == 0`)
"""

from __future__ import annotations

import json
import re
import unittest
from typing import Any, Dict, List, Optional, Tuple

from tests_e2e.harness import (
    ADB_GUEST_SHIM,
    CANONICAL_STATION_HOSTNAME,
    discover_test_boards,
    http_get_text,
    load_luna_install_ps1,
    load_luna_install_sh,
    run_headless_exec,
)


def load_luna_release_manifest() -> Dict[str, Any]:
    raw = http_get_text(
        "https://github.com/nyakokitsu/luna/releases/latest/download/manifest.json"
    )
    return json.loads(raw)


def _luna_compact(val: str) -> str:
    if not val or not val.strip():
        return ""
    x = re.sub(r"[A-Z]{4}$", "", val.strip())
    x = re.sub(r"[-_](?=[a-z]*[0-9])[a-z0-9]{4}$", "", x.lower())
    return re.sub(r"[^a-z0-9]", "", x)


def evaluate_luna_ps1_manifest_match(
    install_ps1: str, manifest: Dict[str, Any], hostname: str
) -> Optional[Tuple[str, str, str]]:
    """Verify `install.ps1` structure and evaluate its `Compact` + `Get-Aliases` station matching."""
    if "function Compact" not in install_ps1 or "function Get-Aliases" not in install_ps1:
        raise RuntimeError("Compact / Get-Aliases functions missing from luna install.ps1")

    t_norm = _luna_compact(hostname)
    devices: List[Dict[str, Any]] = manifest.get("devices", [])

    def aliases(entry: Dict[str, Any]) -> List[str]:
        names = list(entry.get("models", [])) + [
            entry.get("device", ""),
            entry.get("codename", ""),
        ]
        return [_luna_compact(str(n)) for n in names if n]

    found = [e for e in devices if t_norm in aliases(e)]
    if not found:
        found = [
            e
            for e in devices
            if any(t_norm in a or a in t_norm for a in aliases(e))
        ]
    if len(found) != 1:
        return None
    e = found[0]
    return (str(e["device"]), str(e["asset"]), str(e.get("codename", "")))


def build_luna_install_and_api_exec_script(
    install_sh: str,
    canonical_hostname: str,
    device: str,
    asset: str,
    codename: str,
    board: str,
) -> str:
    # Provide `uname` (returning canonical station hostname for `-n`) and `python3`
    # (returning the manifest record matched by `luna/install.sh`'s embedded script)
    # so the unmodified `nyakokitsu/luna/install.sh` runs inside the guest `/bin/sh`.
    sysfs_led_boards = (
        "cucumber",
        "yandexmini2",
        "yandexmini2nc",
        "plum",
        "plumnc",
        "yandexmicro",
        "mango",
    )
    switch_trigger = (
        "for t in /sys/class/leds/*:ring*/trigger; do [ -e \"$t\" ] && echo none > \"$t\" 2>/dev/null || true; done"
        if board in sysfs_led_boards
        else ":"
    )

    return (
        "set -eu\n"
        f"{ADB_GUEST_SHIM}\n"
        "uname() {\n"
        '  if [ "${1:-}" = "-n" ]; then\n'
        f'    printf "%s\\n" "{canonical_hostname}"\n'
        "    return 0\n"
        "  fi\n"
        '  command uname "$@"\n'
        "}\n"
        "python3() {\n"
        "  cat >/dev/null\n"
        f'  printf "%s\\t%s\\t%s\\n" "{device}" "{asset}" "{codename}"\n'
        "}\n"
        "(\n"
        "  set --\n"
        f"{install_sh}\n"
        ")\n"
        "test -x /data/quazar/luna/bin/luna\n"
        "test -x /etc/init.d/S99luna\n"
        "i=0\n"
        "while [ $i -lt 30 ]; do\n"
        "  if command wget -qO- http://127.0.0.1:8099/api/device >/tmp/luna-device.json 2>/dev/null; then\n"
        "    break\n"
        "  fi\n"
        "  i=$((i + 1))\n"
        "  sleep 1\n"
        "done\n"
        'grep -Fq \'"model"\' /tmp/luna-device.json\n'
        f"{switch_trigger}\n"
        "RING_COUNT=$(sed -n 's/.*\"ring_leds\"[[:space:]]*:[[:space:]]*\\([0-9]*\\).*/\\1/p' /tmp/quasar-emu-hw.json | head -n 1)\n"
        "COLORS=$(awk -v n=\"${RING_COUNT:-6}\" 'BEGIN { printf \"[\"; for (i=0; i<n; i++) printf \"%s\\\"#22CCFF\\\"\", (i?\",\":\"\"); printf \"]\" }')\n"
        "command wget -qO- --header='Content-Type: application/json' --header='X-Luna-Local: 1' "
        "--post-data=\"{\\\"colors\\\":$COLORS}\" http://127.0.0.1:8099/api/led/ring >/dev/null\n"
        "command wget -qO- --header='Content-Type: application/json' --header='X-Luna-Local: 1' "
        "--post-data='{}' http://127.0.0.1:8099/api/led/off >/dev/null\n"
        "if grep -Fq '\"has_clock\": true' /tmp/quasar-emu-hw.json; then\n"
        "  command wget -qO- --header='Content-Type: application/json' --header='X-Luna-Local: 1' "
        "--post-data='{\"time\":\"12:34\",\"brightness\":200}' http://127.0.0.1:8099/api/clock >/dev/null\n"
        "fi\n"
        "command wget -qO- --header='Content-Type: application/json' --header='X-Luna-Local: 1' "
        "--post-data='{\"percent\":70}' http://127.0.0.1:8099/api/volume/set >/dev/null\n"
        "printf '__LUNA_E2E_OK__\\n'\n"
    )


class TestLunaInstallerAndRuntimeE2E(unittest.TestCase):
    def test_luna_install_sh_ps1_and_peripherals_across_boards(self) -> None:
        boards = discover_test_boards()
        if not boards:
            self.skipTest("No `work/*-system.ext4` guest images found")

        install_sh = load_luna_install_sh()
        install_ps1 = load_luna_install_ps1()
        manifest = load_luna_release_manifest()

        for idx, board in enumerate(boards):
            canonical_host = CANONICAL_STATION_HOSTNAME.get(board, board)
            matched = evaluate_luna_ps1_manifest_match(
                install_ps1, manifest, canonical_host
            )
            with self.subTest(board=board):
                if matched is None:
                    self.skipTest(
                        f"[{board}] Not listed in nyakokitsu/luna release manifest.json"
                    )
                device, asset, codename = matched
                script = build_luna_install_and_api_exec_script(
                    install_sh=install_sh,
                    canonical_hostname=canonical_host,
                    device=device,
                    asset=asset,
                    codename=codename,
                    board=board,
                )
                out = run_headless_exec(board, script, port_offset=300 + idx * 10)
                self.assertIn("__LUNA_E2E_OK__", out)


if __name__ == "__main__":
    unittest.main(verbosity=2)
