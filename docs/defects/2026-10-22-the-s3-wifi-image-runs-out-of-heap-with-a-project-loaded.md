---
status: open
found: 2026-10-22      # how: emulator walk (`just walk-wifi-emu lan --s3`, S3 radio M5's second half, PR #1273)
area: lp-fw/fw-esp32s3 `wifi` image's heap (lp-fw/fw-esp32s3/src/c_heap.rs, dram2_heap.rs) × a loaded project × the LAN link
class: untested-path   # the S3's wifi image had never held a project and a LAN link at once: on silicon it does not join yet (2026-10-22-the-s3-wifi-image-never-authenticates.md), and the emulator could not run its station until #1273
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (m4-s3-wifi.md, m5-emu-s3-wifi.md)
  - docs/adr/2026-10-21-s3-radio-memory.md
  - docs/defects/2026-10-22-the-s3-wifi-image-never-authenticates.md
  - docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md
---
# The S3's Wi‑Fi image runs out of heap with a project loaded

**Symptom** — two emulated S3s (`fw-esp32s3 --features wifi`, 427f91ab4 +
#1273, `lp-emu:esp32s3:t1+net=lan`) on one virtual LAN, Studio reaching both
over `?lan=` (`just walk-wifi-emu lan --s3`). Joining, the secure LAN links,
DNS-SD and a reset onto a new lease all work (W1, W2, W5, W9). Then:

- **W3, Studio pushes Peach (1D) over the LAN.** The board loads it, then
  panics and resets:

  ```text
  [INFO] lpa_server::project_manager: Project loaded: studio
  [INFO] lpa_server::handlers: [mem] load_project after: 43532 B free / 218628 B used (42k / 213k)
  panicked at …/alloc/src/alloc.rs:553:9:
  memory allocation of 68 bytes failed
  [PANIC] recursive panic in the panic path; resetting now
  ```

  It boots again with `the last run's load did not finish: studio was
  interrupted while it loaded` and no project. The load took 57 KB
  (100,728 B free before, 43,532 B after).
- **W8, `lp-cli upload projects/test/button-sign lan:…`.** The deploy is
  acked, then every read is refused: `board memory busy (free 34924 B,
  largest block 26276 B; needs 40960 B free and a 16384 B block)`.
- **W10, `lp-cli link rtt lan:…`.** The link comes up (idle 106.6 fps), then
  a request gets no reply within 30 s.

With no project the same image idles at 104,284 B free (largest block
73,744 B) with Studio's LAN link open, and its radio heap at 42,976 B of
49,152 B high-water, 0 B overflowed to main.

**Suspected cause (not yet proved)** — the failing 68-byte request came
from a region the main heap's figure does not count. The heartbeat still
said 43 KB free (largest block 32 KB) when it failed. `c_heap.rs` gives
requests that carry `MemoryCapability::Internal` only the main arena's
`Internal` piece: esp-radio's `InternalMemory` allocations, the C heap's
fallback once the radio region is full, and thread stacks. Rust's global
allocator fills that same piece first, before `dram2_seg`. A project
load can therefore leave free bytes in `dram2_seg` and none that an
`Internal` request may take. The panic's frames end at the allocation
error handler and do not name the caller. The next step is a per-region
free figure (`Internal` arena, `dram2_seg`, radio) logged at the load and
at the failure.

Separately from the panic, a board left with ~35–43 KB free after a load
is below the LAN read floor (40,960 B free and a 16 KiB block,
`lpa_server::read_frame_budget`), so Studio cannot read the project back
over the LAN even when nothing resets.

**A smaller project fits** — the same walk with Fiber Headband (28 KB, the
smallest gallery project) loads with 73,984 B free. Studio's edit over the
LAN then lands (`--skip` the other steps; the report's run 2).

**Trust** — heap figures are the class the S3 emulator measured closest to
silicon: 63,060 B free against FS3's 62,504 B on the 620-px project
(docs/reports/2026-10-22-s3-620px-ble-load.md). Silicon has already shown
the pressure once: FS3's 620-pixel startup project reset the `wifi` image
while it loaded. The join defect noted that and called it M10's (PSRAM).
This entry is the same pressure on an ordinary gallery project, with the
board's own numbers. So the emulator's numbers are very likely silicon's
too. Silicon cannot show the LAN half until the `wifi` image joins a
network.

**Expected fix** — this is the S3's heap pressure before PSRAM: the S3 radio
roadmap's M10 maps the octal PSRAM and moves the project's heap there.
Leave this open until M10 lands, then re-run the walk below with Peach (1D)
to close it.

**Seen again (2026-10-22, the relay, S3 radio M6)** — the same image on the
relay cell (`lp-cli/tests/emu_relay_link.rs`'s S3 arm,
`lp-emu:esp32s3:t1+net=lan`): joined, registered, Alice's relay session
open, `lp-cli upload projects/test/basic relay:…`. The load took 38 KB
(95,484 B free before, 57,096 B after, largest block 46,672 B). The board
then reset during the GLSL compile, and the next boot said:

```text
[RECOVERY] previous run crashed (panic): at node:/Basic.show/…/shader-compile:glsl:
  memory allocation of 13092 bytes failed
Boot: the last run's load did not finish: Basic stopped the board while it loaded
```

The recovery worked: the board came back on the relay by itself. With no
project the relay costs about 6.5 KB registered (117,296 → 110,956 B free)
and about 13 KB with a session open (97,660 B). Until M10 the cell's S3 arm
uploads no project (`BoardImage::runs_basic`) and runs every relay step.
When this closes, delete that exception, so the arm runs `basic` like the
C6's.

**Repro** — `just test-emu-esp32s3-wifi` (builds and merges the image), then
`just walk-wifi-emu lan --s3` (needs `just studio-web-story-build`, `cargo
build -p lp-cli`). W3's board console is `target/walk-wifi-emu/lan-s3/console/s3-a.link.log`.
