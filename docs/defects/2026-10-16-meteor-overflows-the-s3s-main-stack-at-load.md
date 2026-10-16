---
status: fixed
found: 2026-10-16      # how: hardware walk — RAM research V5, the control runs of the S3 held PRs on FS3
area: fw-esp32s3 main stack (32,288 B, the residual above the statics) × a project load
fixed: 2026-10-16      # PR #1113, constants out of RAM (+20,560 B stack)
class: budget-exhaustion
related:
  - 2026-10-10-meteor-overflows-the-classics-main-stack-at-load.md   # the classic's twin, same mechanism
  - 2026-10-09-the-c6-main-stack-margin-is-unguarded.md               # the gate gap
  - lp2025/2026-10-09-1203-ram-research   # V5 report and evidence
---
# Loading meteor overflows the S3's main stack (silicon)

**Symptom** — on `FS3 fixture-s3` (ESP32-S3, XIAO ESP32-S3 Plus,
D8:3B:DA:47:29:70), main at `479fc0460` (`just build-fw-esp32s3`, default
features; main stack 32,288 B), uploading `catalog/patterns/meteor` over the
USB lp-link loads it (`Project loaded: Meteor`, 155,708 B free) and panics at
its first frames:

```
[INFO] fw_esp32s3::stack_probe: [stack] heartbeat: high-water 29856 B of 32288 B (2432 B headroom)
…
Detected a write to the stack guard value on ProCpu
```

Two of two uploads on main, and two of two on #1086's image (a probe fix that
does not touch the stack). The board resets and recovery boots the previous
project. A `Shader oracle` project left on the board's flash by an earlier
session had already been declined at startup ("was interrupted while it
loaded at startup"), and when recovery fell back to it, it overflowed too
(high-water 30,448 B of 32,288 B before the guard). The PLAYFUL choker
(without its PowerButton node) runs on the same image at a 29,968 B
high-water: 2,320 B to spare.

**Root cause** — the same as the classic's: the stack is what is left of
`dram_seg` after the statics, and a catalog compute-shader pattern's load and
first render use more than that. Nothing gates a load on the stack (the S3's
heap record ratchets only the idle high-water, 13 KB).

**Fix** — #1113 moved esp-hal's merged constant pools and the lookup tables
no RAM function reads out of `.data` (24,156 B → 3,596 B), so the main stack
grew from 32,288 B to 52,848 B and the heap arena stayed 245,760 B. On the
same board meteor then ran 5 min twice at a 37,408 B high-water (15,440 B to
spare), 32 fps; the choker kept its 29,968 B peak and 38 fps. The gate gap
stays open under
[2026-10-09-the-c6-main-stack-margin-is-unguarded](2026-10-09-the-c6-main-stack-margin-is-unguarded.md).

**Regression coverage** — none in CI: `main_stack_bytes` in
`lp-emu/esp/figures/esp32s3.json` pins the size, not a load's peak.

**Lesson** — the classic's lesson again, one chip over: a stack is a budget
the workload sets, and the first board to run the catalog's compute-shader
pattern found it. Check the other chips' margins with that pattern, not with
an idle image.
