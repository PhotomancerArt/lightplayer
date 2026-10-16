---
status: open
found: 2026-10-10      # how: report — RAM research E6, the first-fit control run of a classic TLSF comparison
area: fw-esp32v3 main stack (37,056 B, the residual above the statics) × a project load
class: budget-exhaustion
related:
  - 2026-10-09-the-c6-main-stack-margin-is-unguarded.md   # the same gate gap, the classic's row
  - 2026-09-04-tlsf-build-hits-stack-guard-at-project-load.md
  - lp2025/2026-10-09-1203-ram-research   # E6 report and evidence
---
# Loading meteor overflows the classic's main stack (first fit, emulated)

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

**Fix** — open. Measure the classic's load high-water per catalog project on
the emulator, then either give the stack the bytes (the classic's
`HEAP_SIZE` and `.stack` are zero-sum) or gate project loads on it.

**Regression coverage** — none.

**Lesson** — the classic has less stack than its own project loads use on
the catalog's compute-shader pattern, and nothing measured it until a control
run for an unrelated experiment did. A stack is a budget the *workload* sets.
