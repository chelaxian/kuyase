#!/usr/bin/env python3
"""
E2E Smoke Test 04: Stock `/system/vendor/quasar/q`, Interactive `q cli`, and Stateful `glagold` (`:9899` & `:1961`)
Executes exclusively via `kuyase run --headless --board <board> --snapshot --exec`:
1. Verifies `/system/vendor/quasar/q version`, `q device-id` (`Device ID: EMU0001` with zero
   `libasound.so.2`, `fnvram`, `sdio`, or `PcbaIds` warnings), `q full-info`, `q wifi-info`, and `q getprop`
2. Verifies `/system/vendor/quasar/q glagol-connections` on Gen 2 / Gen 3 boards (`:9899` `glagold` IPC)
3. Verifies unmodified lifecycle commands `/system/vendor/quasar/q stop`, `q start`, `q restart`,
   `q backend-dev`, and `q backend-prod` in `Stub Mode`
4. Verifies interactive piped `/system/vendor/quasar/q cli` session (`s`, `t`, `c`, `m`, `p`, `o`, `n`, `l`,
   `q` + Russian query, `x`) with zero `Not connected to PlaybackControlCapability` warnings and zero IPC errors
5. Verifies external `:1961` Glagol HTTP state and command API (`GET /state`, `POST /command`) and peer reflection
   in `q glagol-connections`
"""

from __future__ import annotations

import unittest

from tests_e2e.harness import discover_test_boards, run_headless_exec

BOARDS_WITHOUT_GLAGOL_CONNECTIONS_SUBCOMMAND = {"yandexmini"}


def build_quasar_q_exec_script(board: str) -> str:
    has_glagol_subcmd = board not in BOARDS_WITHOUT_GLAGOL_CONNECTIONS_SUBCOMMAND
    glagol_check_initial = (
        'echo "=== 6. q glagol-connections ==="\n'
        "GLAGOL_CONN=$(/system/vendor/quasar/q glagol-connections 2>&1)\n"
        'printf "%s\\n" "$GLAGOL_CONN"\n'
        'printf "%s\\n" "$GLAGOL_CONN" | grep -Fq \'"device_id" : "EMU0001"\'\n'
        if has_glagol_subcmd
        else ""
    )
    glagol_check_peer = (
        "GLAGOL_CONN2=$(/system/vendor/quasar/q glagol-connections 2>&1)\n"
        'printf "%s\\n" "$GLAGOL_CONN2" | grep -Fq "127.0.0.1:"\n'
        if has_glagol_subcmd
        else ""
    )

    return (
        "set -eu\n"
        'echo "=== 1. q version ==="\n'
        "VER_OUT=$(/system/vendor/quasar/q version 2>&1)\n"
        'printf "%s\\n" "$VER_OUT"\n'
        'printf "%s\\n" "$VER_OUT" | grep -Fq "Quasar Software Version:"\n'
        'echo "=== 2. q device-id ==="\n'
        "DEV_OUT=$(/system/vendor/quasar/q device-id 2>&1)\n"
        'printf "%s\\n" "$DEV_OUT"\n'
        'printf "%s\\n" "$DEV_OUT" | grep -Fq "Device ID: EMU0001"\n'
        'if printf "%s\\n" "$DEV_OUT" | grep -Eiq "fnvram status file not found|wm_db_fnvram_get.*failed|no version information|sdio|PcbaIds"; then\n'
        '  echo "UNEXPECTED STDERR IN q device-id" >&2\n'
        "  exit 1\n"
        "fi\n"
        'echo "=== 3. q full-info ==="\n'
        "FULL_OUT=$(/system/vendor/quasar/q full-info 2>&1)\n"
        'printf "%s\\n" "$FULL_OUT"\n'
        'printf "%s\\n" "$FULL_OUT" | grep -Fq "Device ID: EMU0001"\n'
        'echo "=== 4. q wifi-info ==="\n'
        "WIFI_OUT=$(/system/vendor/quasar/q wifi-info 2>&1)\n"
        'printf "%s\\n" "$WIFI_OUT"\n'
        'printf "%s\\n" "$WIFI_OUT" | grep -Fq "wlan0"\n'
        'echo "=== 5. q getprop ==="\n'
        "PROP_OUT=$(/system/vendor/quasar/q getprop 2>&1)\n"
        'printf "%s\\n" "$PROP_OUT"\n'
        'printf "%s\\n" "$PROP_OUT" | grep -Fq \'"ro.board.platform":"Linux"\'\n'
        f"{glagol_check_initial}"
        'echo "=== 7. q stop / start / restart / backend-dev / backend-prod ==="\n'
        "/system/vendor/quasar/q stop 2>&1\n"
        "/system/vendor/quasar/q start 2>&1\n"
        "/system/vendor/quasar/q restart 2>&1\n"
        "BDEV_OUT=$(/system/vendor/quasar/q backend-dev 2>&1)\n"
        'printf "%s\\n" "$BDEV_OUT"\n'
        'if printf "%s\\n" "$BDEV_OUT" | grep -Fq "Dev Flag is not supported"; then\n'
        '  echo "UNEXPECTED Dev Flag error in q backend-dev" >&2\n'
        "  exit 1\n"
        "fi\n"
        "test -f /data/quasar/devFlag\n"
        "BPROD_OUT=$(/system/vendor/quasar/q backend-prod 2>&1)\n"
        'printf "%s\\n" "$BPROD_OUT"\n'
        "test ! -f /data/quasar/devFlag\n"
        'echo "=== 8. q cli interactive session ==="\n'
        "CLI_OUT=$( (sleep 1; printf 's\\nt\\nc\\nm\\np\\no\\nn\\nl\\nq\\nпривет\\nx\\n'; sleep 1) | /system/vendor/quasar/q cli 2>&1 )\n"
        'printf "%s\\n" "$CLI_OUT"\n'
        'if printf "%s\\n" "$CLI_OUT" | grep -Fq "Not connected to PlaybackControlCapability"; then\n'
        '  echo "UNEXPECTED PlaybackControlCapability warning in q cli" >&2\n'
        "  exit 1\n"
        "fi\n"
        'printf "%s\\n" "$CLI_OUT" | grep -Fq "UserInput: Start Conversation"\n'
        'printf "%s\\n" "$CLI_OUT" | grep -Fq "UserInput: Text Input Query"\n'
        'echo "=== 9. Glagol :1961 state & command check ==="\n'
        "GLAGOL_STATE=$(command wget -qO- http://127.0.0.1:1961/state)\n"
        'printf "%s\\n" "$GLAGOL_STATE"\n'
        'printf "%s\\n" "$GLAGOL_STATE" | grep -Fq \'"id":"EMU0001"\'\n'
        "GLAGOL_PLAY=$(command wget -qO- --header='Content-Type: application/json' --post-data='{\"command\":\"play\"}' http://127.0.0.1:1961/command)\n"
        'printf "%s\\n" "$GLAGOL_PLAY"\n'
        'printf "%s\\n" "$GLAGOL_PLAY" | grep -Fq \'"playing":true\'\n'
        f"{glagol_check_peer}"
        'echo "=== 10. Unified maind.log verification ==="\n'
        "grep -Fq \"recognizedPhrase='привет'\" /data/quasar/daemons_logs/maind.log\n"
        "grep -Fq \"recognizedPhrase='привет'\" /tmp/daemons_logs/maind.log\n"
        'echo "__QUASAR_Q_E2E_OK=1"\n'
    )


class TestQuasarVendorQE2E(unittest.TestCase):
    def test_quasar_q_cli_and_glagol_across_boards(self) -> None:
        boards = discover_test_boards()
        if not boards:
            self.skipTest("No `work/*-system.ext4` guest images found")

        for idx, board in enumerate(boards):
            with self.subTest(board=board):
                script = build_quasar_q_exec_script(board)
                out = run_headless_exec(board, script, port_offset=260 + idx * 10)
                self.assertIn("Device ID: EMU0001", out)
                self.assertIn("__QUASAR_Q_E2E_OK=1", out)


if __name__ == "__main__":
    unittest.main(verbosity=2)
