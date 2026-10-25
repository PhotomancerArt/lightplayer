---
status: fixed          # board fixed; hw-power-cut.py still cannot tell (below)
found: 2026-10-23      # live-debugging: the S3 radio/OTA director, FS3 kept its uptime through five cuts
fixed: lp-board-bench 11eaf25   # PhotomancerArt/lp-board-bench#10, outside this repo
area: desk tooling (lp-board-bench `board power-*`), scripts/ota/hw-power-cut.py
class: oracle-could-not-fail
related:
  - docs/reports/2026-10-21-s3-bluetooth-walk.md
  - scripts/ota/README.md
  - lp2025/2026-10-20-2335-s3-radio-and-ota/power-cut-audit.md
---
# For four days `board power-*` cut only one USB twin, and every check that watched the bus passed

**Symptom** — From 2026-10-20 to 2026-10-23 08:57 PDT, `board power-cycle`
and `board power-off` exited 0, the board left USB and came back, and the
board kept running. FS3 kept its uptime through five such cuts (5–15 s off).
`board list` showed no `(+twin)` for any hub board (FS3, FC6, CX1, CX2, FV3).

**Root cause** — A VIA hub is a USB 2 hub and a USB 3 hub on one chip, and
VBUS drops only when both have the port off. `board` found a hub's USB 3
twin by path string (`1-1.X.Y` ↔ `1-2.X.Y`). On 2026-10-20 the desk's VIA
chain was moved under a ganged USB 2-only CH hub (`1a86:8095`) on `1-1` port
3. That hub has no USB 3 half, so it made the USB 2 path one level deeper:
`1-1.3.1`'s twin is `1-2.3`, and `1-1.3.1.3`'s is `1-2.3.3`. The path rule
found no twin, and `board` switched only the USB 2 half. Dropping the USB 2
half takes the board off the bus, so it looks like a cut. VBUS stayed up
through the USB 3 half, so the board never lost power.

Before 10-20 the chain sat where the path rule pairs it (`1-1.2` ↔ `1-2.2`,
`1-1.2.3` ↔ `1-2.2.3`, briefly `1-1.3` ↔ `1-2.3`), and `board list` showed
`(+twin)` for every board on every day a cut was made. Cuts then were real.

The tool's own checks could not see it: the hub said the port was off, and
the board left the bus and came back. Both happen on a half cut too.

**Fix** — lp-board-bench#10 (`11eaf25`): twins are paired by where they sit
in the tree, stepping over another vendor's hub. `board` refuses a hub whose
twin it cannot name, or a port that carries a hub, and fails a cycle if the
port reads power during the off time. After it, FS3 and CX1 came back with
uptime ~5 s and `resetReason: power-on`. `hub-twins.toml` names a twin by
hand for a tree no rule fits.

**What it touched** — the audit
(`lp2025/2026-10-20-2335-s3-radio-and-ota/power-cut-audit.md`) checked every
silicon claim that rests on a cut. **No merged claim is wrong.** The only
silicon cuts made in the window were on FS3: the S3 Bluetooth walk (whose
report already said the cut did nothing) and two attempts to wake a wedged
FS3 (so "a power cycle does not clear the wedge" is unproven). No C6 was cut
in the window.

**Still open: `scripts/ota/hw-power-cut.py` cannot tell either.** A C6 PASS
is "`done: UpToDate`, Y's core, an engine". A board that never lost power
resumes the update over the re-enumerated link and passes the same way, and
the script stops only if the board does not come back on the bus. Every run
of it so far switched both real twins, so no result is wrong, but its next
run on a changed desk could be. The silicon tests that cut through `board`
(`board_first_light`, `board_tree_store_*`, `board_flash_qio`) and
`scripts/emu/flash-tears-cuts.py` each demand a power-on boot and stop
without one, and `scripts/ota/s3-desk-sitting.py cuts` demands POWERON
(`rst:0x1`) after its first cut.

**Regression coverage** — lp-board-bench's `desk_parsers.test.ts` (the CH
hub tree fixture `uhubctl-ch-hub-between-twins.txt`: twins paired, ambiguous
trees refused, `uhubctl -e`). In this repo: none for `hw-power-cut.py`.

**Lesson** — A power cut's proof is the board saying it lost power (a
power-on reset reason, an uptime that restarted, a boot count), never the
board leaving the bus or the hub saying the port is off: a half cut does
both. Any check built on a side effect of the thing it means to prove
should be asked "what else produces this?" A desk change (moving a hub)
silently broke a tool that had worked for two weeks, and only the checks
that read the board's own reset reason could have caught it.
