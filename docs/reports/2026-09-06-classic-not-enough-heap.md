# Classic "not enough heap": what the message was, zook bracketed on silicon, and what a node costs

Plan `~/.photomancer/planning/lp2025/2026-09-06-0051-classic-not-enough-heap`
(artifacts under its `bench/`). Tree `d6cfaa2051ae` (origin/main after
PR #522). Board: DOM-Z-102, the desk classic.

## 1. The question

Yona reported a "not enough heap" error on a classic ESP32 (`fw-esp32v3`)
board on 2026-09-02, the day PR #503 (the per-lamp memory table) merged.
Three things were owed on the back of it:

1. **Which failure was it** — the load-refusal gate, a mid-frame OOM, or the
   shader compile transient?
2. **Silicon `[mem]` brackets for zook on a classic after #503** — the line
   the defect `2026-08-29-shader-jit-compile-transient-starves-classic-heap`
   still carried as `open`.
3. **The per-node cost** — `docs/debt/per-lamp-data-stored-three-times.md`
   estimated ~14 KB per fixture+output pair from a cross-image comparison
   and said "nobody has done it".

⚠️ **The heap moved twice between the report and this measurement.** The
brief, the defect and the per-lamp report all describe a 186,368 B heap
(110 KiB `dram_seg` arena + 72 KiB SRAM1 tail). PR #521 (2026-09-05) added
the two ROM stack spans as heap regions and PR #522 moved the JIT code region
to SRAM0, growing the SRAM1 tail. The classic now boots with
`heap=15072+112640+98304+15536=241552` in four `esp_alloc` regions. Every
silicon number below is on that heap; every comparison names its basis.

## 2. Classification

Three messages on the classic could be paraphrased as "not enough heap":

| ID | Message (verbatim shape) | Emitter | Class |
|---|---|---|---|
| C1 | `load refused: heap headroom too low (largest free block N B < 65536 B); power-cycle the device or load a smaller project` | `lp-app/lpa-server/src/server.rs` `check_load_headroom`, on both load paths (wire handler after the unload; host-call path for startup loads) | **load gate** — a floor on the largest free block, not a fit check (`PROJECT_LOAD_MIN_HEADROOM_BYTES`) |
| C2 | `read refused: heap headroom too low (largest free block N B < 32768 B); narrow the query …` | `server.rs` `tick_and_send`, Studio's staged reads | read gate (`PROJECT_READ_MIN_HEADROOM_BYTES`) |
| C3 | `OOM: alloc N bytes failed (align 4) in <context>` / `[OOM] free list: holes=… largest=…`, then on the next boot the recovery ledger's `allocation failed: requested=… largest_free=… context=…` | `fw-esp32v3` panic path + recovery ledger | mid-load or mid-frame OOM: a **reset**, not a message the client sees |

Only C1 and C2 reach a client as text; only C1 talks about a *load*. What
the 2026-09-04/05 sessions measured on the same board makes C1 the answer
by mechanism (`docs/reports/2026-09-04-classic-heap-fragmentation.md` §6,
PR #516):

- With `/projects/studio` as the stamped startup project, DOM-Z-102 rested at
  ~34 KB free / ~25.5 KB largest from the first compiled frame on, so every
  Studio read was refused (C2) — "the normal state, not a race".
- After `stopAllProjects` the heap had 166 KB free but a 39.7 KB largest
  block, so the 64 KiB load gate refused **every** load, the same project
  included, until a power cycle (C1; defect
  `2026-09-04-unload-leaves-classic-unloadable-until-power-cycle`). The
  2026-09-05 hardware walk hit exactly this line on DOM-Z-102 (`largest
  free block 40 KB < 64 KB`) when its upload did stop-all then load.

Studio's ordinary flow — connect, stop, upload, load — passes through that
state, and the message reads as "your project is too big" when the truth is
"the heap has the bytes but not the contiguity". On 2026-09-02 nobody yet
knew the mechanism; the diagnosis arrived two days later.

**Verdict: C1, the load gate after an unload, unconfirmed** — Q1 (the exact
line and the project) was put to Yona at planning time and had no answer
when this shipped. If the message said `read refused`, it was C2 in the
same resting state. Either way, on the 186,368 B heap it was the board's
normal state with `studio` resident; §3 shows what the same cycle does on
today's heap.

## 3. Silicon brackets — zook on DOM-Z-102, today's heap

Full tables, the capture script and the raw log:
`bench/silicon/{brackets.md,bench.py,bench-zook.csv,bench-zook.csv.log,upload-1.log}`
in the planning dir. Firmware built from `d6cfaa2051ae` (ELF sha1
`c2e1c5c1…`), flashed in the foreground without a monitor;
`examples/zook-dome` pushed with `lp-cli upload`, which stores it as
`/projects/Zook dome` and stamps it as the startup project (DOM-Z-102's
startup project is now zook, not studio). `[MEM]` = the firmware's serial
heartbeat line; `hb` = the wire heartbeat's `memory` object.

| phase | free | used | largest free block | note |
|---|---:|---:|---:|---|
| boot idle, before auto-load | 225,500 | 16,052 | **109,446** | `[mem] boot auto_load before` |
| after load | 183,588 | 57,964 | 98,303 | `[mem] boot auto_load after` — region 2 (SRAM1 tail, 98,304 B) is now the largest block, whole |
| first frame, before compile | 91,112 | 150,440 | 74,816 | `[mem] shader compile before`; 5 ports opened (IO18/IO13/IO2/IO14/IO16) |
| **first compiled frame** | 88,952 | 152,600 | **65,030** | `[mem] shader compile after`; 112 ms, 2,144 B of code into SRAM0 |
| steady (14 hb / 65 s) | 89,276 → 89,272 | 152,280 | 65,038 → 65,022 | 19 fps, `retry_saves=0`, `level=green` |
| Studio skeleton read (~38 KB) | 88,892 | 152,560 | 65,024 | **accepted** (read gate 32 KiB) |
| after `stopAllProjects` | 221,440 | 20,112 | **72,943** | 4.8 KB of residents survive; 36.5 KB of contiguity does not |
| reload `LoadProject /projects/Zook dome` | 220,620 | 20,932 | 72,954 | **accepted** by the 64 KiB load gate |
| reload, after compile / steady | 88,524 → 88,936 | 153,028 → 152,616 | 64,641 → 64,627 | second compile 113 ms |

The push itself (`upload-1.log`) was a studio → unload → zook cycle on the
same image: studio at rest 88,764 B free; after stop-all 221,036 free; zook's
`load_project before` saw a **73,693 B** largest block and was accepted.

### Against the emulator (P1, same tree, `lp-cli profile --collect alloc`)

The guest's absolutes do not transfer (320 K single region, ~52 KB harness
baseline, permissive manifest); its deltas and the four-region replay's
shape do (`--frag-regions 15072,112640,98304,15536`; `bench/emulator/`).

| delta | silicon | emulator | reading |
|---|---:|---:|---|
| load resident | **41,912** | 41,663 | 0.6 % apart — the load is the same object on both |
| first-frame residents incl. compile (after load → after compile) | 94,636 | 61,020 | Δ ≈ 33.6 KB is the classic output path the emulator never allocates (below) |
| compile retained in heap | **2,160** | 4,990 (2026-09-02, JIT still in heap) | the 2,144 B of code now lives in SRAM0 |
| compile: largest block before → after | 74,816 → 65,030 (−9,786) | tightest 73,784 at `shader-link B`, 90,880 at end | the replay sees the mid-compile trough; the board's 1 Hz line sees the ends |
| compile transient (peak) | not observable (no peak counter on the board) | 22,858 | the one figure still owed by the *instrument* |
| steady per frame | flat (free ±4 B over 65 s) | 896 R / 1,200 T | |
| largest after unload | **72,943** | — (no unload in the profile) | vs 39,655 (studio, pre-#521), 48,048 (#521), 57,792 (#522 on the dig2go) |

What the numbers say:

1. **The compile-transient defect's silicon line is closed.** On today's
   heap zook's compile retains 2,160 B and costs 9.8 KB of contiguity; zook
   renders at 19 fps, green, and Studio's skeleton read is accepted. The
   ">100 KB" of 2026-08-29 was, as the 2026-09-02 attribution said, the
   JIT link plus the first frame's residents folded into the bracket, on a
   heap 55 KB smaller.
2. **Zook rests ~500 B above the load gate and reloads 7.4 KB above it.**
   The largest block at rest is 65,022–65,040 B against a 65,536 B floor;
   nothing asks the gate at rest, but the margin is one small resident
   wide. After unload the largest block is 72,943 B, so the reload is
   accepted — the unload defect's mechanism is intact (109,446 → 72,943 B of
   contiguity lost to 4.8 KB of leftovers) and its refusal is not, for zook
   or for studio, on this heap.
3. **Region 2 carries everything from the load on**, as P1's replay
   predicted: after load the largest block is 98,303 B ≈ the whole SRAM1
   tail; the first frame's buffers and the compile's leftovers chip it to
   65,030 B. Region 3 (ROM APP, 15,536 B) is registered after the tail and
   is never the largest block — its bytes count for `free`, never for the
   gate.
4. **The output path is the largest first-frame resident on silicon and the
   emulator does not model it**: ~33.6 KB for 1,500 lamps over five ports.
   From the code, not measured per owner here: `DisplayPipeline` 6 B/lamp
   `current` + 12 B/lamp `prev`/`next` (zook's `output.json` sets no options,
   and interpolation defaults on) + 3 B/lamp dither = 21 B/lamp = 31,500 B,
   plus the RMT driver's 900 B slot per port = 4,500 B. The "classic
   `DisplayPipeline` copies" follow-up from #503 now has a silicon number.

## 4. Per-node cost

_(P3 — filled in below when the attribution lands.)_

## 5. What this changes

- `docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`
  → `fixed` (measured on silicon 2026-09-06; the fix was #474/#475/#497/#503
  plus the heap itself).
- `docs/defects/2026-09-04-unload-leaves-classic-unloadable-until-power-cycle.md`
  stays `open` with the post-#522 figures: the refusal no longer reproduces
  for studio or zook on the four-region heap, but the contiguity loss that
  caused it is unchanged and the margin is 7–8 KB.
- `docs/debt/per-lamp-data-stored-three-times.md`: incident log gets the
  per-node attribution.
- Follow-ups (in `~/.photomancer/planning/lp2025/todo.md`): the emulator's
  `FragLayout::Classic` still models the pre-#521 two-region heap
  (`CLASSIC_REGIONS = [112640, 73728]`); the same trace replayed on the four
  regions ends at 90,880 B largest instead of 42,428 B, so any classic frag
  figure quoted from the default layout before 2026-09-05 is the wrong heap.
  The startup profile of zook ends at 198,974,122 of the 200,000,000
  `--max-cycles` cap (0.5 % headroom). The board's `[MEM]` line has no peak
  counter, so a compile transient can only be bracketed, never peaked, on
  silicon.

## 6. Reproduce

```bash
# emulator (P1)
just profile examples/zook-dome --collect alloc --mode startup \
  --frag-regions 15072,112640,98304,15536 \
  --frag-discount-site VirtualWs281xDriver::endpoints --frag-discount-site HwResource
# silicon (P2) — see bench/silicon/README.md in the planning dir
just build-fw-esp32v3
espflash flash --chip esp32 --partition-table lp-fw/fw-esp32v3/partitions.csv \
  --flash-size 4mb --after hard-reset --port /dev/cu.wchusbserialNNN \
  target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3
cargo run -q -p lp-cli -- upload examples/zook-dome serial:/dev/cu.wchusbserialNNN
python3 bench.py --port /dev/cu.wchusbserialNNN --label zook --out bench-zook.csv
```
