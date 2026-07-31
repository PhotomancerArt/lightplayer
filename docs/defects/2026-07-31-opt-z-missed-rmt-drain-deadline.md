---
status: fixed
found: 2026-07-31      # how: hardware-walk (P4 loopback bring-up)
fixed: this change
area: workspace release profile / fw-esp32s3
class: config-masked-defect
related:
  - docs/defects/2026-07-30-xtensa-integer-div-by-zero-trap.md   # same class
---
# opt-level="z" codegen missed the RMT RX drain deadline on the S3

**Symptom** — First run of the ported `test_loopback` harness on the desk
ESP32-S3: channel 0's known-answer capture deterministically truncated at
exactly half its RX window —

```
E4: MEASURE capture ch=0 items=24 bits=24 leading_low_ticks=0 trailing_low_ticks=36 ended_high=0
E4: FAIL loopback_decode ch=0 bytes=3 got=3CA50F want=3CA50F8001FF
```

— while channels 1–3 passed, the 100-frame soak passed on **all four**
channels including ch0, and the byte-identical reference firmware
(`2026-esp32s3-experiment` led-lab) passed everything on the same board
minutes earlier.

**Root cause** — The workspace `[profile.release]` sets `opt-level = "z"`,
chosen for the C6's 3 MB flash budget. The led-lab reference always built at
`opt-level = "s"`, and the driver's timing was validated there. At `"z"`,
the RX wrap drain (`RxTransaction::poll` through the harness spin callback)
is slow enough on the *cold first frame* that channel 0 — the first
transaction polled — misses the 24-item / 30 µs half-window deadline; its
ring is overrun and the capture ends with only the first half. Warm paths
(every later frame, and channels polled after ch0) fit the window, which is
why only one channel's first capture failed and why the failure was
perfectly deterministic.

Falsified along the way (both re-tested on silicon): the E1 RAM probe
running before the captures, and cold-cache warm-up polls before TX start.
Only the opt-level change moved the result.

**Fix** — A dedicated `[profile.release-esp32s3]` (`inherits = "release"`,
`opt-level = "s"`) and the justfile's S3 recipes build with it. The S3 has a
6 MB app partition with ~4.6 MB headroom; size-optimized codegen buys
nothing this chip needs. The C6 keeps `"z"` — its flash budget is the
constraint there, and its single-channel driver has 4× the per-half
deadline.

App image at `"s"`: 1,710,672 B of 6 MB (was 1,606,832 B at `"z"` —
~104 KB for the whole firmware).

**Regression coverage** — `just fwtest-loopback-esp32s3 <port>`: the
known-answer captures run on the cold first frame by construction, which is
exactly the case that fails at `"z"`. No host test can cover this; it is a
codegen-speed-vs-silicon-deadline interaction.

**Lesson** — A hard real-time deadline can be missed by *build
configuration* alone, with every line of driver code correct — the second
config-masked-defect this port has surfaced (the first: rv32's register
layout hiding allocator bugs). Profiles chosen for one chip's constraint
(C6 flash) silently follow every other chip in the workspace unless each
chip pins its own. When firmware has µs-scale deadlines, the build profile
is part of the timing contract and belongs next to the code it constrains —
and the first-frame/cold path is the case a soak test structurally cannot
catch, which is why the known-answer capture runs first.
