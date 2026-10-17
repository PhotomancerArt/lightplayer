---
status: fixed
found: 2026-10-17      # emulator report: docs/reports/2026-10-17-c6-flash-timing-emulator.md, K1+ `panel-176` store at t3 and t4
area: lp-cli tests (emu_first_light) × lpa-server persist_startup_project
class: unenforced-test-precondition   # the boot's choice of project depended on a save the test never waited for
related:
  - ../reports/2026-10-17-c6-flash-timing-emulator.md
  - ../adr/2026-10-07-project-loads-are-tried-and-recovered.md
---
# The emulated first-light test cut the power before the startup project was saved

**Symptom** — `emu_first_light` on the K1+ sitting's `panel-176` store
(`LP_FIRST_LIGHT_LPFS`, image `split-k12n`) lit nothing at `t3` or `t4`: the
boot said `first light none by 354 ms`. CX1 lit the same bytes at 738 ms.
PR #1177's report recorded it as "a separate difference, not flash", which
read like an emulator gap.

**Cause** — the test, not the emulator or the firmware. The server makes a
loaded project the startup project (`/lightplayer.json`) only once it has
survived its first frames (`ProjectManager::frame_survived` →
`persist_startup_project`). `emu_first_light` power-cycled the board as soon
as `LoadProject` answered, before those frames, so the save never happened.
At power-up `auto_load_project` then fell back to the lexically first
project. On a fresh store that is the pushed `first-light`, so the test
passed by luck. `panel-176` holds its own `/projects/a` (whose
`project.json` is not an object) and a `/lightplayer.json` that is not JSON,
so boot picked `a`, failed to load it, and lit nothing. The emulated console
shows it: `Boot: auto-loading /projects/a` and no
`startup_project set to` line anywhere in the run. Silicon's
`board_first_light` waits 5 s after the load ("Let the board settle") and
its boots say `found configured startup project: first-light`.

**Fix** — after the push, `emu_first_light` waits for the board's own
`startup_project set to first-light` line before it cuts the power. Re-run
on `k12n` (median of 2 emulated boots, CX1 median of 10):

| store | `t3` | `t4` | CX1 | `t4` ÷ si |
|---|---:|---:|---:|---:|
| `used-176` | 628 | 630 | 689 | 0.91 |
| `panel-128in176` | 641 | 644 | 722 | 0.89 |
| `panel-176` | 658 | 662 | 738 | 0.90 |

**Lesson** — a measurement that power-cycles after a load must wait for the
board to say the load is done, not for the reply. The firmware does what it
was designed to do (a project that has not run its first frames is not yet
the startup project, so a load that resets the board is not retried), so
a cut that soon is not a firmware defect.
