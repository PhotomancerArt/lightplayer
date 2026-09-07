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

Full tables, diffs, the replay scripts and the host output:
`bench/per-node/` in the planning dir (`table.md` is the write-up this
section condenses). Two instruments on the same four project shapes,
generated from `examples/basic` (1 clock, 1 shader, one 241-lamp Direct
fixture on a 10×10 canvas, one output with interpolation and LUT on) so that
nothing but the node count differs:

| project | pairs k | lamps/pair | total lamps |
|---|---:|---:|---:|
| `examples/basic` | 1 | 241 | 241 |
| `projects/test/basic-2n` | 2 | 241 | 482 |
| `projects/test/basic-4n` | 4 | 241 | 964 |
| `projects/test/basic-2n-half` | 2 | 121 | 242 |

`projects/test/basic-2n/generate.py` writes all three (rules in its README:
pair *i* = the parent's fixture/map2d/output with its own `bus:control.out/chI`
and the next pin on the XIAO `D10/D9/D8/D7` ladder). `basic-2n-half` has
`basic-2n`'s node count at half the lamps, so `2n − 2n-half` is this tree's
per-lamp figure on the same instrument and subtracting it from the per-pair
slope leaves the node.

**Headline: one fixture+output pair costs ≈ 19.5 KB of node-level heap plus
40 B per lamp on the device, all resident after startup.** A 241-lamp pair is
≈ 29 KB, 12 % of the classic's 241,552 B heap. Of the 19.5 KB, **72 % is
shared bookkeeping** (loader/runtime spine, registry inventory, resolver
cache and interning, bindings) and 26 % the two node structs' own slot shapes
and buffers.

### Device width — emulator alloc-diff by callsite

`lp-cli profile --collect alloc --mode startup` on each project (four-region
replay, the two manifest discounts); the full live set at end of trace
replayed from `heap-trace.jsonl` (the report's own table is capped at 20
rows) and diffed `4n − 2n` ÷ 2. `2n − 1n` gives the same shape one step
lower (fixture 3,990 / output 1,152 / shared 13,585) but `examples/basic`
compiles in frame 2 where the siblings compile in frame 1 — with ≥ 2
fixtures the shader's second render of frame 1 already clears the compile
deferral flag — so `4n − 2n` is the headline.

| figure (per pair) | all-in | per-lamp part (40 B × 241) | per-node remainder |
|---|---:|---:|---:|
| `project-load` retained | 12,279 | 1,838 | 10,441 |
| `frame` retained | 17,921 | 7,712 | 10,210 |
| `shader-compile` | 0 | 0 | 0 — byte-identical on all four; the compile is a property of the shader |
| **live at end of startup** | 27,236 (30,133 engine-only) | 10,670 | **19,463** |

| side | scale | owner | B/pair |
|---|---|---|---:|
| shared | node | project loader / runtime spine | 4,699 |
| shared | node | registry inventory (`NodeDefEntry`, `ProjectNode`, def clones) | 4,369 |
| shared | node | resolver: cache, interning, productions | 3,773 |
| shared | node | dataflow bindings + node binding index | 1,175 |
| fixture | node | `FixtureDef` slot shape (compiled accessors) | 2,682 |
| fixture | node | `FixtureNode` resolve/produce (query keys, def reads) | 1,308 |
| output | node | `OutputDef` slot shape | 1,152 |
| output | port | per-port fixed (provider + driver frame headers) | 240 |
| fixture | lamps | sample points 1,992 + sample target 1,960 + mapping 1,952 + `direct_channels` 964 | 6,868 |
| output | lamps | `OutputNode` u16 runtime buffer 2,356 + 8-bit port frame 963 + emulator-only provider buffer 723 | 4,042 |
| emulator | artifact | RAM filesystem holding the project text (excluded) | −2,945 |

Nothing in `4n − 2n` is unattributed. Rolled up per pair: fixture node
3,990, output node 1,392, **shared 14,016**, lamp-scaled 10,670.

**Per lamp on the same instrument** (`2n − 2n-half` ÷ 240): mapping 8,
sample points 8, sample target 8, `direct_channels` 4, output u16 6, 8-bit
port frame 3 = **37 B/lamp**, owner for owner the device figure of
`docs/reports/2026-09-02-per-lamp-memory-table.md`, on an unrelated project
shape; +3 for the emulator's own per-port write buffer. On the classic add
`DisplayPipeline` (6, +12 interpolation, +3 dither) → ~55 B/lamp.

### Host — tracking-allocator probe

`cargo test -p lpc-engine --test per_node_memory_table -- --nocapture`
(`-p lpc-engine` only; the test regenerates its own temp copies of the four
shapes from `examples/basic` by the same rules as the committed projects,
after a discarded warm-up run):

| figure (per pair) | all-in resident | B/lamp | per-node resident |
|---|---:|---:|---:|
| load project (`2n → 4n`) | 17,159 | 8.00 | **15,231** |
| install graphics + provider | 0 | 0 | 0 (a flat 87,211 B per process) |
| steady frames | 0 | 0 | 0 |

The host's load leg is 1.4× the emulator's `project-load` retained (std
collection growth, same 4-byte pointers on both). Its per-lamp load figure,
8.00 B/lamp resident, is the mapping — the same 8 the emulator and the
per-lamp report see. **The host cannot resolve the frame leg**: its shader
compile is wasmtime's (0.55–1.02 MB resident, ±200 KB run to run) and for
k ≥ 2 it lands in tick 1 with the frame's buffers; a ~10 KB/pair signal is
not readable under that. The frame leg is the emulator column.

### Against the debt entry's estimate

`docs/debt/per-lamp-data-stored-three-times.md` put "roughly 14 KB on each
fixture+output pair" from a cross-image comparison of `examples/basic` vs
`quad60-v3` on 2026-08-02. Measured on one image: **19.5 KB per pair at end
of startup (10.4 KB of it at load, ~10 KB more in the first frame)**, so the
estimate was right in magnitude and low by a third — and its conclusion holds
harder than it said: on a four-channel show the node cost (4 × 19.5 = 78 KB)
exceeds the per-lamp cost of a 1,500-lamp project (60 KB) before the first
lamp, and three quarters of it is not in either node but in the shared
bookkeeping that every node pays into. `basic-4n` (4 pairs, 964 lamps) ends
startup with a 14,088 B largest free block on the classic's four-region
layout — under both gates. Cross-check with §3: zook (1 pair, 1,500 lamps,
5 ports) is lamp-dominated; the classic's next constraint at zook scale is
the output path (§3.4), at quad-channel scale it is this table.

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
