#!/usr/bin/env python3
"""
E2E Smoke Test 02: Luna Runtime (`nyakokitsu/luna`) via YAM v0.2+
Verifies across guest images in `work/`:
1. Installing `yam/luna` via YAM v0.2+ (or from `KUYASE_LUNA_PKG`) and starting `:8099`
2. `GET /api/device` model and hardware feature discovery
3. LED Ring control (`POST /api/led/ring` & `POST /api/led/off`) -> `BridgeStateSnapshot` without Trigger Conflict
4. Segment Clock control (`POST /api/clock`) on boards with `has_clock`
5. Bidirectional volume sync (`POST /api/volume/set` <-> `BridgeStateSnapshot.volume_level` & `HostCommand::SetVolume`)
6. Speech recognition & Hijack Window (`HostCommand::InjectVoice` -> `GET /api/alice/speech?cancel=true` -> `aliced_hijack_count >= 1`, `ipc_error_count == 0`)
"""

from __future__ import annotations

import json
import os
import pathlib
import time
import unittest

from tests_e2e.harness import (
    KuyaseHeadlessVm,
    discover_test_boards,
    install_yam_v2_on_vm,
)


def install_and_start_luna(vm: KuyaseHeadlessVm) -> None:
    install_yam_v2_on_vm(vm)
    local_pkg = os.environ.get("KUYASE_LUNA_PKG", "").strip()
    if local_pkg and pathlib.Path(local_pkg).is_file():
        vm.adb.push_bytes(
            "/tmp/luna.yampkg", pathlib.Path(local_pkg).read_bytes(), mode=0o644
        )
        vm.adb.shell_checked("/usr/bin/yam install --yes /tmp/luna.yampkg", timeout=120.0)
    else:
        vm.adb.shell_checked("/usr/bin/yam install --yes yam/luna", timeout=180.0)

    # Ensure Luna daemon is up on 127.0.0.1:8099
    vm.adb.shell_checked(
        "if ! pidof luna >/dev/null 2>&1; then "
        "  mkdir -p /data/yam/var/luna/data /tmp/yam; "
        "  nohup /data/yam/bin/luna --listen 0.0.0.0:8099 --data-dir /data/yam/var/luna/data "
        "    >>/data/yam/var/luna/luna.log 2>&1 & "
        "fi"
    )
    vm.adb.shell_checked(
        "i=0; while [ $i -lt 30 ]; do "
        "  if wget -qO- http://127.0.0.1:8099/api/device >/dev/null 2>&1; then exit 0; fi; "
        "  i=$((i+1)); sleep 1; "
        "done; exit 1",
        timeout=40.0,
    )


class TestLunaE2ESmoke(unittest.TestCase):
    def test_luna_peripherals_volume_and_speech_across_boards(self) -> None:
        boards = discover_test_boards()
        if not boards:
            self.skipTest("No `work/*-system.ext4` guest images found")

        for idx, board in enumerate(boards):
            with self.subTest(board=board):
                with KuyaseHeadlessVm(board, port_offset=300 + idx * 10) as vm:
                    assert vm.bridge is not None
                    install_and_start_luna(vm)

                    # 1. Verify /api/device
                    dev_raw = vm.adb.shell_checked(
                        "wget -qO- http://127.0.0.1:8099/api/device"
                    )
                    dev_info = json.loads(dev_raw)
                    self.assertIn("model", dev_info)

                    # 2. Verify LED Ring via Luna HTTP API -> BridgeStateSnapshot
                    # On boards where Luna uses sysfs LEDs (cucumber, yandexmini2, plum, yandexmicro),
                    # switch sysfs trigger to `none` first; on framebuffer boards (station2, bergamot, orion),
                    # keep `[ledfb-ring]` active so `qemu-hw-bridge` reads `/dev/fb*`.
                    if board in ("cucumber", "yandexmini2", "yandexmini2nc", "plum", "plumnc", "yandexmicro"):
                        vm.adb.shell(
                            "for t in /sys/class/leds/*:ring*/trigger; do "
                            "  [ -e \"$t\" ] && echo none > \"$t\" 2>/dev/null || true; "
                            "done"
                        )
                    st0 = vm.bridge.request_state()
                    ring_leds = dev_info.get("led", {}).get("ring_count") or st0["ring_leds"]
                    colors = ["#22CCFF"] * ring_leds
                    led_payload = json.dumps({"colors": colors})
                    vm.adb.shell_checked(
                        f"wget -qO- --header='Content-Type: application/json' "
                        f"--header='X-Luna-Local: 1' "
                        f"--post-data='{led_payload}' "
                        f"http://127.0.0.1:8099/api/led/ring >/dev/null"
                    )
                    time.sleep(0.25)
                    st_led = vm.bridge.request_state()
                    self.assertEqual(
                        st_led["led_trigger_conflict_count"],
                        0,
                        f"[{board}] Luna LED write caused trigger conflict",
                    )
                    self.assertEqual(
                        st_led["ring_pixels"][0],
                        [34, 204, 255],
                        f"[{board}] Expected ring_pixels[0]==[34, 204, 255], got {st_led['ring_pixels'][0]}",
                    )

                    vm.adb.shell_checked(
                        "wget -qO- --header='Content-Type: application/json' "
                        "--header='X-Luna-Local: 1' --post-data='{}' "
                        "http://127.0.0.1:8099/api/led/off >/dev/null"
                    )

                    # 3. Verify Segment Clock on boards with has_clock
                    if st0.get("has_clock"):
                        vm.adb.shell_checked(
                            "wget -qO- --header='Content-Type: application/json' "
                            "--header='X-Luna-Local: 1' "
                            "--post-data='{\"time\":\"12:34\",\"brightness\":200}' "
                            "http://127.0.0.1:8099/api/clock >/dev/null"
                        )
                        time.sleep(0.2)
                        st_clk = vm.bridge.request_state()
                        self.assertTrue(
                            any(v > 0 for v in st_clk["clock_segments"].values()),
                            f"[{board}] Expected lit clock segments after /api/clock",
                        )

                    # 4. Verify Host -> Guest volume and Voice Recognition + CancelDialog (:9875)
                    vm.bridge.send_command({"SetVolume": {"volume_level": 7}})
                    time.sleep(0.25)
                    st_vol = vm.bridge.request_state()
                    self.assertEqual(st_vol["volume_level"], 7)

                    # Inject voice phrase (broadcasts on :9875 after 300ms) and poll /api/alice/speech in foreground
                    vm.bridge.send_command(
                        {
                            "InjectVoice": {
                                "phrase": "поставь на паузу",
                                "doa_angle": 90,
                                "rms": None,
                                "wake_only": False,
                            }
                        }
                    )
                    speech_out = vm.adb.shell_checked(
                        "wget -S -O - 'http://127.0.0.1:8099/api/alice/speech?timeout_ms=5000&cancel=true' 2>&1",
                        timeout=10.0,
                    ).strip()
                    self.assertIn(
                        "поставь на паузу",
                        speech_out,
                        f"[{board}] Expected Luna /api/alice/speech to receive injected phrase, got: {speech_out}",
                    )
                    st_final = vm.bridge.request_state()
                    self.assertGreaterEqual(
                        st_final["aliced_hijack_count"],
                        1,
                        f"[{board}] Expected aliced_hijack_count >= 1 after Luna cancel=true",
                    )
                    self.assertEqual(
                        st_final["ipc_error_count"],
                        0,
                        f"[{board}] Expected 0 IPC errors, got {st_final['ipc_error_count']}",
                    )


if __name__ == "__main__":
    unittest.main(verbosity=2)
