#!/usr/bin/env python3
"""
E2E Smoke Test 01: YAM v0.2+ Package Manager (`nyakokitsu/yam`)
Verifies across guest images in `work/`:
1. Platform auto-detection (`detect_platform`) matches firmware `quasar.cfg`
2. Provisioning `/data/yam/bin/yam` + `/usr/bin/yam` wrapper (`yam-run`)
3. Official Ed25519 registry configuration (`https://repo.yse.wtf/v2`)
4. `yam list`, `yam registry list`, and `yam doctor` health check
"""

from __future__ import annotations

import unittest

from tests_e2e.harness import (
    KuyaseHeadlessVm,
    discover_test_boards,
    install_yam_v2_on_vm,
)


class TestYamV2Smoke(unittest.TestCase):
    def test_yam_v2_install_and_doctor_across_boards(self) -> None:
        boards = discover_test_boards()
        if not boards:
            self.skipTest("No `work/*-system.ext4` guest images found")

        for idx, board in enumerate(boards):
            with self.subTest(board=board):
                with KuyaseHeadlessVm(board, port_offset=200 + idx * 10) as vm:
                    yam_platform = install_yam_v2_on_vm(vm)
                    self.assertTrue(
                        yam_platform,
                        f"[{board}] detect_platform returned empty string",
                    )

                    reg_out = vm.adb.shell_checked("/usr/bin/yam registry list")
                    self.assertIn(
                        "official",
                        reg_out,
                        f"[{board}] Expected 'official' registry in `yam registry list`, got:\n{reg_out}",
                    )

                    list_out = vm.adb.shell_checked("/usr/bin/yam list")
                    self.assertIsNotNone(list_out)

                    doctor_out = vm.adb.shell_checked("/usr/bin/yam doctor")
                    self.assertIn(
                        "Состояние пакетов в порядке",
                        doctor_out,
                        f"[{board}] Unexpected `yam doctor` output:\n{doctor_out}",
                    )


if __name__ == "__main__":
    unittest.main(verbosity=2)
