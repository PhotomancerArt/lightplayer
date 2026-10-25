# Captures for `test_hw_power_cut.py`

C6 console captures from the over-the-air update's M7 P11 desk sitting
(planning `lp2025/_archive/2026-10-04-0757-ota-update-protocol`,
`b-firmware/data/p11-a0f26287b48c/`), committed verbatim so the power-cut
verdict can be replayed with no board. Never edit one: a verdict that changes
on a capture is the finding.

| File | What it is |
|---|---|
| `real-cut-bk.{pre,rec}.txt` | `cuts-a/cut-005.0`: a real cut while the host backs up. The recovery's heartbeats read `power-on`, boot count 1 (X was at 7) |
| `real-cut-core.{pre,rec}.txt` | `cuts-a/cut-026.0`: a real cut mid-core. No heartbeat in the recovery; X was at boot 5 |
| `real-nocut-xy.rec.txt` | `update-xy.txt`: a whole X -> Y update with **no** cut, which is what a half cut looks like to the board |
| `real-nocut-xy.pre.txt` | the first heartbeat of `x-boot.txt`: X before it, `user-reset`, boot 2 |
| `real-nocut-xy.fin.txt` | the first heartbeat of `y-run.txt`: Y after it, `software-reset`, boot 6 (X's 2 plus the update's three, plus one) |

The test builds its other "fresh heartbeat" captures from `real-nocut-xy.fin.txt`
with the boot count and reset reason edited, and says so where it does.
