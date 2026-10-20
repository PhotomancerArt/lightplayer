---
status: fixed
found: 2026-10-10      # how: report — RAM research E6, the first-fit control run of a classic TLSF comparison
area: fw-esp32v3 main stack (37,056 B, the residual above the statics) × a project load
fixed: this change     # PR #1114, constant pools and unread tables out of RAM (+8,000 B stack)
class: budget-exhaustion
related:
  - 2026-10-09-the-c6-main-stack-margin-is-unguarded.md   # the same gate gap, the classic's row
  - 2026-09-04-tlsf-build-hits-stack-guard-at-project-load.md
  - lp2025/2026-10-09-1203-ram-research   # E6 report and evidence
---
# Loading meteor overflows the classic's main stack (first fit; emulated, then silicon)

**Symptom** — the shipped-feature classic image (`just build-fw-esp32v3`,
default features, first-fit heap; research/ram @ `8733eb97b`), direct-loaded
on `lp-emu:esp32v3:t1`, with `catalog/patterns/meteor` (a copy with its
output retargeted from `ws281x:local:D10` to `ws281x:local:IO18`, as the
classic walk retargets its project) uploaded over the hosted UART0 link:

```
lp-cli emu run --chip esp32v3 --elf <fw-esp32v3> --host-link --upload <meteor copy> --timeout 12s
```

The load reaches `project new after graphics` and panics:

```
[INFO] fw_esp32v3::stack_probe: [stack] heartbeat: high-water 31600 B of 37056 B (5456 B headroom)
…
Detected a write to the stack guard value on ProCpu
```

The board resets (`cause=software-reset`), comes back without the project
(the load never finished, so it is not the startup project) and the upload's
status poll waits until the wall-clock net. The same image loads
`projects/test/basic` (its peak 30,336 B, 6,720 B to spare).

**Root cause** — not established. What is known: the stack is 37,056 B and a
project load alone already reaches 30–31.6 KB on it before the compile; the
classic's record gates the stack only against its idle high-water
(12,860 B), the gap
[2026-10-09-the-c6-main-stack-margin-is-unguarded](2026-10-09-the-c6-main-stack-margin-is-unguarded.md)
names for the C6. Not checked: whether the deep frame is meteor's (its
compute-shader node) or the load path's, and whether silicon overflows at the
same point (no desk run; this is emulated only, and the Xtensa windowed ABI's
spill depth is what the emulator models).

**On silicon (2026-10-15, RAM research V4)** — reproduced on `FV3
fixture-v3` (classic ESP32, DOM-Z-102, 30:76:F5:EC:F6:34) with main at
`e54d49731` (main stack 36,944 B): uploading the same retargeted meteor copy
over the UART link panicked with `Detected a write to the stack guard value
on ProCpu` in two of two runs (three panics: the upload, then the startup
re-load, then the recovery declined it: "Meteor was interrupted while it
loaded at startup"). The same image ran the PLAYFUL choker for 5 min at a
30,204 B high-water.

**Fix** — the stack got the bytes, without taking them from the heap: #1114
moved esp-hal's merged constant pools and the lookup tables no RAM function
reads out of `.data`, so the main stack grew from 36,944 B to 44,944 B and the
heap arena stayed 112,640 B. On the same board, meteor then loaded and ran
5 min with a 37,612 B high-water (7,332 B to spare). The gate gap stays open
under
[2026-10-09-the-c6-main-stack-margin-is-unguarded](2026-10-09-the-c6-main-stack-margin-is-unguarded.md):
nothing gates a load on the stack, and the margin is 7 KB, not a guarantee.

**Regression coverage** — none in CI: the stack figure (`main_stack_bytes`
in `lp-emu/esp/figures/esp32v3.json`) pins the size, not the load's peak.

**Lesson** — the classic has less stack than its own project loads use on
the catalog's compute-shader pattern, and nothing measured it until a control
run for an unrelated experiment did. A stack is a budget the *workload* sets.
