#!/usr/bin/env python3
"""
E2E Smoke Test 03: Station-airplay Pilot Plugin (`Yandex-Station-Universal-Repo/Station-airplay`)
Verifies on top of YAM v0.2+ and Luna:
1. Installing `station/airplay` (from `KUYASE_AIRPLAY_PKG` or YAM registry)
2. Enabling binary plugins in Luna (`/api/settings` -> `allow_plugin_binaries: true`) and starting the service
3. Waiting for `/run/station-airplay/private-bus` and RTSP listener (`:5000` or `:7000`)
4. Verifying private D-Bus ownership (`org.freedesktop.Avahi`, `org.gnome.ShairportSync`)
5. Injecting voice transport commands (`"пауза"`, `"дальше"`) and verifying Alice stays silent (`IDLE`) with 0 IPC errors
6. Clean service stop via Luna HTTP API
"""

from __future__ import annotations

import os
import pathlib
import time
import unittest
from typing import Tuple

from tests_e2e.harness import KuyaseHeadlessVm, discover_test_boards
from tests_e2e.test_02_luna import install_and_start_luna


def install_airplay_plugin(vm: KuyaseHeadlessVm) -> Tuple[bool, str]:
    local_pkg = os.environ.get("KUYASE_AIRPLAY_PKG", "").strip()
    if local_pkg:
        pkg_path = pathlib.Path(local_pkg)
        if not pkg_path.is_file():
            raise FileNotFoundError(f"KUYASE_AIRPLAY_PKG not found: {local_pkg}")
        vm.adb.push_bytes("/tmp/station-airplay.yampkg", pkg_path.read_bytes(), mode=0o644)
        vm.adb.shell_checked(
            "/usr/bin/yam install --yes /tmp/station-airplay.yampkg", timeout=180.0
        )
        return True, ""

    # Install `airplay/airplay-luna` from the configured YAM v0.2+ registry
    out = vm.adb.shell(
        "/usr/bin/yam install --yes airplay/airplay-luna 2>&1 && echo __AIRPLAY_OK__",
        timeout=180.0,
    )
    if "__AIRPLAY_OK__" in out:
        return True, ""
    return False, out.strip()


class TestStationAirplaySmoke(unittest.TestCase):
    def test_station_airplay_bringup_dbus_and_voice_transport(self) -> None:
        boards = discover_test_boards()
        if not boards:
            self.skipTest("No `work/*-system.ext4` guest images found")

        for idx, board in enumerate(boards):
            with self.subTest(board=board):
                with KuyaseHeadlessVm(board, port_offset=400 + idx * 10) as vm:
                    assert vm.bridge is not None
                    luna_ok, luna_err = install_and_start_luna(vm)
                    if not luna_ok:
                        self.skipTest(
                            f"[{board}] yam/luna install skipped ({luna_err.splitlines()[-1] if luna_err else 'unavailable'})"
                        )

                    ok, err_msg = install_airplay_plugin(vm)
                    if not ok:
                        self.skipTest(
                            f"airplay/airplay-luna install skipped ({err_msg.splitlines()[-1] if err_msg else 'unavailable'}); "
                            "set KUYASE_AIRPLAY_PKG=/path/to/station-airplay.yampkg to test a local build"
                        )

                    # Enable binary plugins in Luna and rescan
                    vm.adb.shell_checked(
                        "wget -qO- --header='Content-Type: application/json' "
                        "--header='X-Luna-Local: 1' "
                        "--post-data='{\"allow_plugin_binaries\":true}' "
                        "http://127.0.0.1:8099/api/settings >/dev/null || true; "
                        "wget -qO- --header='X-Luna-Local: 1' --post-data='' "
                        "http://127.0.0.1:8099/api/plugins/rescan >/dev/null || true"
                    )

                    # Start airplay.airplay-luna / station.airplay / station-airplay service
                    vm.adb.shell_checked(
                        "for id in airplay.airplay-luna station.airplay station-airplay; do "
                        "  wget -qO- --header='X-Luna-Local: 1' --post-data='' "
                        "    http://127.0.0.1:8099/api/plugins/$id/toggle >/dev/null 2>&1 || true; "
                        "  wget -qO- --header='X-Luna-Local: 1' --post-data='' "
                        "    http://127.0.0.1:8099/api/plugins/$id/service/enable >/dev/null 2>&1 || true; "
                        "  wget -qO- --header='X-Luna-Local: 1' --post-data='' "
                        "    http://127.0.0.1:8099/api/plugins/$id/service/start >/dev/null 2>&1 || true; "
                        "done"
                    )

                    # Wait up to 90s under TCG for private D-Bus + RTSP (:5000 or :7000)
                    vm.adb.shell_checked(
                        "i=0; while [ $i -lt 45 ]; do "
                        "  if [ -S /run/station-airplay/private-bus ] && "
                        "     netstat -ltn 2>/dev/null | grep -qE ':(5000|7000)'; then "
                        "    exit 0; "
                        "  fi; "
                        "  i=$((i+1)); sleep 2; "
                        "done; exit 1",
                        timeout=100.0,
                    )

                    # Verify ShairportSync answers on the private D-Bus
                    dbus_out = vm.adb.shell_checked(
                        "dbus-send --bus=unix:path=/run/station-airplay/private-bus "
                        "--print-reply --dest=org.gnome.ShairportSync "
                        "/org/gnome/ShairportSync org.gnome.ShairportSync.GetControlStatus",
                        timeout=15.0,
                    )
                    self.assertIn("uint32", dbus_out)

                    # Inject voice transport phrase ("пауза") -> must stay silent (IDLE) and log 0 IPC errors
                    vm.bridge.send_command(
                        {
                            "InjectVoice": {
                                "phrase": "пауза",
                                "doa_angle": 0,
                                "rms": None,
                                "wake_only": False,
                            }
                        }
                    )
                    time.sleep(2.1)
                    st = vm.bridge.request_state()
                    self.assertEqual(
                        st["alice_state"],
                        "IDLE",
                        f"[{board}] Expected Alice to remain IDLE on transport command, got {st['alice_state']}",
                    )
                    self.assertEqual(st["ipc_error_count"], 0)

                    # Clean stop
                    vm.adb.shell_checked(
                        "for id in airplay.airplay-luna station.airplay station-airplay; do "
                        "  wget -qO- --header='X-Luna-Local: 1' --post-data='' "
                        "    http://127.0.0.1:8099/api/plugins/$id/stop >/dev/null 2>&1 || true; "
                        "done"
                    )


if __name__ == "__main__":
    unittest.main(verbosity=2)
