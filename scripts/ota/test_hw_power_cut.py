#!/usr/bin/env python3
"""Tests for the power-cut verdict in scripts/ota/hw-power-cut.py.

    python3 scripts/ota/test_hw_power_cut.py        # or: just lint-power-cut-verdict

The verdict (`judge`) is a pure function of four captured texts, so these run
on logs and open no port. The logs under `testdata/power-cut/` are C6 captures
from the over-the-air update's M7 P11 desk sitting (see the README there):
real cuts that must PASS, and a real update with NO cut, which is exactly
what a half cut (one USB twin switched) looks like to the board and must not.
`docs/defects/2026-10-23-board-power-cut-one-usb-twin-and-every-check-passed.md`
is why: the C6's verdict could not fail.
"""

import importlib.util
import json
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "testdata", "power-cut")

_spec = importlib.util.spec_from_file_location("hw_power_cut", os.path.join(HERE, "hw-power-cut.py"))
hw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(hw)  # type: ignore[union-attr]

Y = "b1b1b1b1+0172481a0929"


def data(name):
    with open(os.path.join(DATA, name), errors="replace") as f:
        return f.read()


def heartbeat(boots, reset, uptime=5000):
    """One heartbeat line in the firmware's shape (the committed fin capture's, edited)."""
    msg = json.loads(data("real-nocut-xy.fin.txt").strip()[2:])
    hb = msg["msg"]["heartbeat"]
    hb["uptime_ms"] = uptime
    hb["recovery"]["bootCount"] = boots
    hb["recovery"]["resetReason"] = reset
    return "M!" + json.dumps(msg, separators=(",", ":")) + "\n"


class C6Verdict(unittest.TestCase):
    def test_real_cut_while_backing_up_passes_on_a_power_on_heartbeat(self):
        # The recovery's own heartbeats read power-on, boot count 1; X was at 7.
        ok, final, extra = hw.judge("esp32c6", Y, data("real-cut-bk.pre.txt"),
                                    data("real-cut-bk.rec.txt"), "")
        self.assertIs(ok, True, final)
        self.assertIn("build " + Y, final)
        self.assertTrue(any("power-on" in p for p in extra["proof"]), extra)

    def test_real_cut_mid_core_passes_on_the_boot_count_restarting(self):
        # X was at boot 5; after a cut the three boots of the update read 3.
        fin = heartbeat(3, "software-reset")
        ok, final, extra = hw.judge("esp32c6", Y, data("real-cut-core.pre.txt"),
                                    data("real-cut-core.rec.txt"), fin)
        self.assertIs(ok, True, final)
        self.assertTrue(any("bootCount 5 before -> 3" in p for p in extra["proof"]), extra)

    def test_converged_update_with_no_fresh_heartbeat_is_no_cut_proof(self):
        # The recovery converged, but the board said nothing about its power.
        ok, _, extra = hw.judge("esp32c6", Y, data("real-cut-core.pre.txt"),
                                data("real-cut-core.rec.txt"), "")
        self.assertIsNone(ok)
        self.assertEqual(extra["proof"], [])

    def test_a_real_update_with_no_cut_is_not_a_pass(self):
        # A whole X -> Y that never lost power, as a half cut would leave it:
        # X's boot 2, Y's boot 6, every reset a software or user reset.
        ok, final, extra = hw.judge("esp32c6", Y, data("real-nocut-xy.pre.txt"),
                                    data("real-nocut-xy.rec.txt"), data("real-nocut-xy.fin.txt"))
        self.assertIsNone(ok, "a board that never lost power must not PASS")
        self.assertIn("build " + Y, final)  # it converged; only the proof is missing
        self.assertEqual(extra["proof"], [])

    def test_synthetic_half_cut_is_not_a_pass(self):
        # A half cut mid-core: the recovery looks like the real mid-core cut's,
        # but the board kept its RTC memory, so Y reads X's count plus the
        # update's three boots, and nothing reads power-on.
        pre = data("real-cut-core.pre.txt")
        b0 = hw.heartbeats(pre)[-1]["boots"]
        rec = data("real-cut-core.rec.txt") + heartbeat(b0 + 1, "software-reset")
        fin = heartbeat(b0 + 3, "software-reset")
        ok, _, extra = hw.judge("esp32c6", Y, pre, rec, fin)
        self.assertIsNone(ok)
        self.assertEqual(extra["proof"], [])

    def test_an_update_that_did_not_converge_fails_even_with_proof(self):
        rec = data("real-cut-core.rec.txt").replace("done: UpToDate", "gave up")
        ok, _, extra = hw.judge("esp32c6", Y, data("real-cut-core.pre.txt"), rec,
                                heartbeat(3, "software-reset"))
        self.assertIs(ok, False)
        self.assertTrue(extra["proof"])

    def test_the_wrong_build_fails(self):
        ok, _, _ = hw.judge("esp32c6", "c3c3c3c3+0172481a0929", data("real-cut-core.pre.txt"),
                            data("real-cut-core.rec.txt"), heartbeat(3, "software-reset"))
        self.assertIs(ok, False)


class CutProof(unittest.TestCase):
    def hb(self, boots, reset="software-reset"):
        return [{"uptime_ms": 1, "reset": reset, "boots": boots, "projects": []}]

    def test_boot_count_threshold(self):
        # An update with no cut adds at least three boots.
        self.assertEqual(hw.cut_proof(self.hb(4), [], self.hb(7)), [])
        self.assertTrue(hw.cut_proof(self.hb(4), [], self.hb(6)))

    def test_power_on_heartbeat_is_proof_by_itself(self):
        self.assertTrue(hw.cut_proof([], self.hb(1, "power-on"), []))

    def test_other_reset_reasons_are_not_proof(self):
        for reason in ("user-reset", "software-reset", "watchdog", None):
            self.assertEqual(hw.cut_proof([], self.hb(9, reason), []), [])

    def test_a_missing_heartbeat_is_not_proof(self):
        self.assertEqual(hw.cut_proof([], [], []), [])
        self.assertEqual(hw.cut_proof(self.hb(4), [], []), [])


class S3Verdict(unittest.TestCase):
    HELLO = ('M!{"id":0,"msg":{"hello":{"buildId":"%s","state":"running"}}}\n' % Y)

    def test_power_on_proof_passes_and_none_does_not(self):
        rec = "[host-ota] done: UpToDate\n"
        ok, _, _ = hw.judge("esp32s3", Y, "", rec + heartbeat(1, "power-on"), self.HELLO)
        self.assertIs(ok, True)
        ok, _, extra = hw.judge("esp32s3", Y, heartbeat(2, "user-reset"), rec,
                                self.HELLO + heartbeat(5, "software-reset"))
        self.assertIsNone(ok)
        self.assertEqual(extra["proof"], [])


if __name__ == "__main__":
    unittest.main()
