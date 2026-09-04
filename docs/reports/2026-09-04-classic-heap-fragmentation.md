# Classic heap fragmentation — measured, attributed, and ranked

Date: 2026-09-04. Tree: `06946a2ea` (every figure below re-measured on it).
Plan: `lp2025/2026-09-04-1358-classic-heap-fragmentation-research`.

Where the classic ESP32's free heap goes, which live blocks hold the holes
open, and what each candidate fix is worth in contiguous bytes — measured by
replaying the emulator's exact allocation trace on the classic's two-region
first-fit layout, and by replaying it again with one lever already pulled.
Nothing here implements a lever. The point is to price them before anyone
writes one.

## 1. The question

Opening a project in Studio against the desk classic fails:

```
protocol error: project read failed: read refused: heap headroom too low
(largest free block 25511 B < 32768 B); narrow the query
(include_slots:false, one probe per read, or page nodes by id) and retry
```

Three things about that sentence are worth stating precisely.

**The heap it is speaking about.** `fw-esp32v3` runs two `esp_alloc` regions:
a 110 KiB (112,640 B) arena carved out of `dram_seg` (`HEAP_SIZE`,
`lp-fw/fw-esp32v3/src/main.rs`) and a 72 KiB (73,728 B) SRAM1 tail added
after it (`add_sram1_heap_region`, reclaimed from the JIT code region).
**186,368 B total**, in two regions, and `esp_alloc::HEAP.allocate` walks
them in registration order and takes the first that serves the layout — so
the arena fills before the tail. Idle used is ≈ 11 KB (10,936 B, the figure
`lp-riscv/lp-riscv-emu-guest/memory.ld` records the guest's 63,596 B
against), leaving ~165–175 KB free on an idle board. Largest-free-block is
bounded by *a region*, never by the total.

**The gate is a proxy, calibrated once.** `PROJECT_READ_MIN_HEADROOM_BYTES`
= 32 KiB (`lp-app/lpa-server/src/server.rs`). Its own doc comment records
the calibration: one bench point, the 2026-08-29 G1 walk, where the gate at
16 KiB let a read through that then OOM-reset the board on a 480 B
allocation in the shapes limb, so the number was doubled — "one frame budget
× 2 is the honest floor". The quantity it measures is **contiguity**.

**The read it guards does not need contiguity; it needs volume.** Measured
on this tree (`--workload studio-sync`, the `project-read` window, all three
reference projects):

| | zook-dome | basic | meteor |
|---|---:|---:|---:|
| `project-read` transient | 23,723 B | 23,723 B | 23,723 B |
| `project-read` retained | 0 B | 0 B | 0 B |
| **largest single allocation** | **4,096 B** | **4,096 B** | **4,096 B** |
| allocations in the opening | 599 | 599 | 609 |
| bytes requested in the opening | 54,625 B | 54,605 B | 54,963 B |

The gate demands one contiguous **32,768 B** block before it will begin a
read whose largest single ask is **4,096 B** — eight times smaller — and
whose whole working set is 23,723 B spread across ~600 allocations, of which
it keeps nothing. The skeleton read is nearly project-independent, because
it is dominated by the static slot-shape catalogue
(`StaticSlotShapeDescriptor::to_owned_shape`, 171 allocations / 29,279 B),
which is the same for every project.

**And the remedy the message names is a dead end.** "Narrow the query" is
advice to a client that has already narrowed as far as the protocol allows.
`lp-app/lpa-studio-core/src/app/project/project_sync.rs` sends the staged
initial sync mandated by ADR `2026-08-28-project-reads-bounded-streamed-refusable`:
stage 1 `initial_skeleton_read_request` is `include_slots: false` with no
probes, stage 2 `initial_slot_page_requests` pages node detail
`INITIAL_SYNC_SLOT_PAGE_NODES = 16` ids at a time, stage 3
`initial_probe_read_requests` sends **one probe per read**. That is exactly
the three remedies the error text lists, all already in force. The
monolithic read is no longer constructible. A client that follows the advice
literally has nowhere left to go, so the refusal is terminal.

Section 5's evidence for the mismatch: on the discounted classic replay
`examples/basic` closes its compile with **25,168 B largest and 57,128 B
free**, and `examples/meteor` with **31,128 B largest and 50,684 B free** —
both refused by this gate, both with more than twice the read's whole
transient available.

## 2. Instruments

Every figure in this report comes from one of these. All are in-tree.

**The guest's own free-list walk (P1) — the truth source.** After each perf
marker, when an alloc collector is active, the RV32 guest walks its free
list exactly (take the smallest block the allocator will hand out until it
refuses; runs with no gap between them are one hole) and emits it as `"t":"H"`
rows plus a closing `"t":"F"`. Two ratchet figures come from it,
`largest_free_at_close` (MIN across openings; **shrinking** is the failure
direction) and `holes_at_close` (MAX). Cost: +35% wall time on
`--collect alloc` runs only.

```bash
cargo run -p lp-cli -- profile examples/zook-dome --collect alloc --mode startup
just heap-budget-check
```

**The `server-boot` window (P2).** `fw-emu` brackets recovery init through
server + transport construction, so pre-project residency is attributable
rather than "everything before `project-load`".

**The two-region replay + fragmentation section (P3).** `lp-cli profile`
replays the recorded trace on a model of `linked_list_allocator` 0.10.5 at
32-bit block geometry (`lp-emu-core/src/profile/frag/first_fit_heap.rs`;
oracle-tested step-for-step against the real crate at host geometry) laid
out as the classic's two regions in `esp_alloc` registration order. Per
marker: largest block per region, hole count, hole histogram, the top holes
with the live blocks bounding them, and a pinning-residents-by-call-site
table. `--frag-discount-site` drops an emulator-only call site.

```bash
# the classic's two regions (default), discounted
cargo run -p lp-cli -- profile examples/zook-dome --collect alloc --mode startup \
    --frag-discount-site VirtualWs281xDriver::endpoints --frag-discount-site HwResource
# the guest's own single region — the only layout the cross-check means anything on
cargo run -p lp-cli -- profile examples/zook-dome --collect alloc --mode startup \
    --frag-layout guest
```

**The counterfactual replay (P4).** `--cf scratch=<windows>`,
`--cf residents-first=<windows>`, `--cf tlsf`, joined with `+`, plus
`--workload studio-sync` (Studio's staged sync issued against the emulator,
so the trace carries a `project-read` window).

```bash
scripts/frag-table.sh            # all three projects, discounted, every lever
```

**The silicon bench (P5).** `bench/bench.py` in the planning directory — see section 6; parked.

### Fidelity policy, stated as numbers

The guest's walk is truth; the host replay is checked against it, and the
figures below are only trustworthy because that check passes. Run on this
tree, `--frag-layout guest`, both projects, 11 markers each:

| project | markers | worst hole Δ | worst largest Δ | verdict |
|---|---:|---:|---:|---|
| `examples/basic` | 11 | ±0 | ±0 B | ok |
| `examples/zook-dome` | 11 | ±0 | ±0 B | ok |

Exact, not within-tolerance (the stated tolerance is holes ±2, largest
±64 B). Exactness arrived with alignment: the trace records each request's
`Layout::align` (`"al":N`, omitted at 4 B), because the allocator front-pads
a hole whose start is not already aligned, and the earlier assumed-4 replay
drifted by ±8 holes / ±320 B on `basic` and ±4 / ±744 B on `zook-dome`.

The replay's own numbers also agree with the ratchet record exactly — e.g.
zook `project-load E` 224,352 B / 22 holes, `frame E` 165,768 / 18,
`shader-compile E` 146,888 / 22 — because both read the same guest walk.

**Silicon fidelity is not established.** No classic-layout prediction has
been checked against a board; see section 6.

## 3. What the heap looks like

Classic layout, discounted (the two emulator-board artifacts of section 4
removed — the raw table measures the fixture board, not firmware),
`--mode startup --workload studio-sync`. `largest` is the whole-heap largest
free block; `r0`/`r1` are the two regions. The tables below are abridged to
the markers that carry the argument — the full per-marker sequence (both `B`
and `E` for every window opening) is what `report.txt` prints and `frag.json`
records.

### `examples/basic`

| marker | largest | holes | free | live | r0 largest | r1 largest |
|---|---:|---:|---:|---:|---:|---:|
| `profile:start` | 112,640 | 2 | 186,368 | 0 | 112,640 | 73,728 |
| `server-boot E` | 99,708 | 3 | 173,812 | 12,556 | 99,708 | 73,728 |
| `project-load E` | 73,728 | 38 | 115,068 | 71,300 | 37,192 | 73,728 |
| `frame E` (first) | 72,008 | 25 | 72,512 | 113,856 | **168** | 72,008 |
| `project-read E` | 72,008 | 27 | 72,624 | 113,744 | 168 | 72,008 |
| `shader-link B` (tightest) | 16,980 | 70 | 46,872 | 139,496 | 136 | 16,980 |
| `shader-compile E` | **25,168** | 23 | **57,128** | 129,240 | 12 | 25,168 |
| `frame E` (last) | 25,168 | 30 | 57,312 | 129,056 | 100 | 25,168 |

### `examples/meteor`

| marker | largest | holes | free | live | r0 largest | r1 largest |
|---|---:|---:|---:|---:|---:|---:|
| `server-boot E` | 99,708 | 3 | 173,812 | 12,556 | 99,708 | 73,728 |
| `project-load E` | 73,728 | 39 | 115,416 | 70,952 | 37,072 | 73,728 |
| `frame E` (first) | 70,104 | 53 | 70,788 | 115,580 | **36** | 70,104 |
| `project-read E` | 70,104 | 55 | 70,900 | 115,468 | 92 | 70,104 |
| `shader-link B` (2nd) | 34,320 | 51 | 48,484 | 137,884 | 396 | 34,320 |
| `shader-link E` (2nd, tightest) | **31,128** | 48 | 50,176 | 136,192 | 396 | 31,128 |
| `shader-compile E` (2nd) | 31,128 | 44 | **50,684** | 135,684 | 28 | 31,128 |
| `frame E` (last) | 31,128 | 46 | 50,900 | 135,468 | 40 | 31,128 |

### `examples/zook-dome`

| marker | largest | holes | free | live | r0 largest | r1 largest |
|---|---:|---:|---:|---:|---:|---:|
| `server-boot E` | 99,708 | 3 | 173,812 | 12,556 | 99,708 | 73,728 |
| `project-load E` | 73,728 | 30 | 122,652 | 63,716 | 46,176 | 73,728 |
| `frame E` (first) | 49,728 | 32 | 61,100 | 125,268 | 10,648 | 49,728 |
| `project-read E` | 49,728 | 33 | 61,368 | 125,000 | 10,648 | 49,728 |
| `shader-link B` | 44,308 | 31 | 52,968 | 133,400 | 2,044 | 44,308 |
| `shader-link E` (tightest) | **41,528** | 29 | 55,296 | 131,072 | 2,752 | 41,528 |
| `shader-compile E` | 41,528 | 21 | 55,960 | 130,408 | 7,632 | 41,528 |
| `frame E` (last) | 41,528 | 27 | 56,144 | 130,224 | 7,632 | 41,528 |

### Region 0 dies in the first frame, and that sets the ceiling

`esp_alloc` fills regions in registration order, so every allocation lands
in the 110 KiB arena until it cannot. By the close of the first frame the
arena's largest block is **168 B** (basic), **36 B** (meteor) and 10,648 B
falling to **7,632 B** (zook). From that marker on, the whole-heap largest
free block is *region 1's number* at every single marker — the 110 KiB arena
still serves small requests out of its confetti, and nothing else.

Two consequences:

- The largest block the classic can hand out after the first frame is
  bounded by **73,728 B** minus whatever region 1 has taken, not by the
  186,368 B total. Total-free stays comfortable (50–61 KB across the three
  projects) long after largest-free has fallen under the read gate.
- Every counterfactual in section 5 hits that ceiling. On zook all three
  winning rows land on exactly **49,728 B** — region 1's largest at the
  first `frame E`, the tail above its last resident. No amount of packing
  *inside* the transient windows recovers a byte past it; the residents that
  region 1 acquired at first-frame open are what set it.

### Hole shape at the worst marker (`basic`, `shader-link B`, 70 holes)

| bucket (lower bound) | 8 | 16 | 32 | 64 | 128 | 256 | 512 | 1K | 2K | 4K | 8K | 16K |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| count | 20 | 12 | 6 | 9 | 6 | 3 | 7 | 4 | 1 | · | 1 | 1 |

46,872 B free in 70 pieces, of which **one** is the 16,980 B largest and
**38 are under 64 B**. This is the JIT compile's shape: thousands of
short-lived small allocations interleaved with a handful of residents.

### Pre-project residency (`server-boot`)

`retained 47,648 B`, 928 allocations, 86,507 B requested, largest single ask
36,864 B. **36,864 B of that 47,648 B is the emulator's 256-resource
`Vec<HwResource>`** — the classic's manifest has 34 resources. Discounted,
`server-boot E` leaves 12,556 B live and 99,708 B still contiguous in region
0. The device's real pre-project residency is that ~12.5 KB plus its own
34-resource manifest, which is the right order for the ~11 KB idle figure.

## 4. Who pins the holes

The pinning table counts, per call site, the live blocks that bounded a
top-10 hole at any marker: `holes` is how many hole-borders it formed and
`hole bytes` is the total size of the holes it helped bound. A site high on
this list is not necessarily large — it is *in the way*.

`examples/zook-dome`, classic layout, discounted, all markers:

| bytes live | blocks | holes | hole bytes | site | born in (window of each bounding block) |
|---:|---:|---:|---:|---|---|
| 12,000 | 1 | 10 | 271,472 | `NativeHostMemory::alloc` | `frame#1` (10) |
| 18,160 | 22 | 55 | 188,980 | `RawVecInner::finish_grow` | `project-load#1` (14), `frame#1` (13), `shader-compile#1` (10), `frame#2` (7), `server-boot#1` (1), none (10) |
| 2,780 | 1 | 8 | 187,792 | `rt_jit::compiler::link_compiled_module_jit` | `shader-link#1` (8) |
| 3,336 | 1 | 2 | 92,352 | `HashMap<WireProjectHandle, Project>::insert` | `project-load#1` (2) |
| 532 | 1 | 1 | 90,212 | `HashMap<LpPathBuf, (FsVersion, FsEventKind)>::insert` | none (1) |
| 5,392 | 4 | 6 | 51,352 | `NativeCompileJob::step` | `shader-compile#1` (6) |
| 56 | 1 | 3 | 22,896 | `__rust_alloc` | `shader-compile#1` (3) |
| 40 | 2 | 4 | 6,744 | `EmitContext::emit_vinst` | `shader-compile#1` (4) |
| 292 | 17 | 39 | 6,564 | `String::clone` | `project-load#1` (13), `shader-compile#1` (12), `frame#1` (10), `shader-link#1` (1), none (3) |
| 992 | 6 | 12 | 6,480 | `NativeJitEngine::compile_shader` | `shader-compile#1` (12) |
| 160 | 1 | 9 | 3,640 | `build_function_sigs` shunt | `shader-compile#1` (9) |
| 64 | 2 | 7 | 1,884 | `OutputNode::consume` | `frame#1` (5), `frame#2` (2) |
| 2,128 | 5 | 7 | 1,020 | `LpFsMemory::write_file` | none (7) |
| 32 | 2 | 3 | 776 | `_lp_main` | `server-boot#1` (3) |
| 252 | 3 | 14 | 712 | `EngineSession::resolve_interned` | `frame#1` (14) |

The report prints the top 10 rows and the marker-by-marker bounding blocks;
rows 11–15 and the born-window column are aggregated from `frag.json` of the
same run (`pinning`, and `markers[].top_holes[].below/above.born_window` —
"none" is a block born outside any named window, counts are hole-borders,
not blocks).

Read it as three groups.

- **Big residents that split a region** — `NativeHostMemory::alloc`
  (12,000 B: the JIT's host-memory block), the two `HashMap` inserts
  (3,336 B and 532 B), and the 2,780 B JIT link block. **Four blocks,
  18,648 B between them, bound holes worth a cumulative 641,828 B** across
  the run. The link block is the single worst
  placement in the trace: at zook's tightest marker it sits **immediately
  below the region top**, so the 41,528 B tail above it is the whole
  remaining heap, and everything below it is confetti.
- **`RawVecInner::finish_grow`** — 22 blocks, 18,160 B live, bounding 55
  holes. This is *every* `Vec` doubling in the program, which is why it
  cannot be attributed further one frame up (see the discount caveat below),
  and why "reserve exactly" is the lever that addresses it.
- **Compile-time confetti** — `String::clone` (17 blocks, 292 B live, 39
  hole borders), `emit_vinst`, `compile_shader`, the `build_function_sigs`
  shunt. Tiny blocks, enormous border count. These are what turn one
  compile's transient into 20–30 permanent holes.

On `examples/basic` the same shape appears with `SyscallOutputProvider::open`
(272 B live, bounding 288,080 B of holes) at the top and
`RawVecInner::finish_grow` at 25,752 B live / 45 hole borders; on
`examples/meteor`, `finish_grow` leads at 11,024 B live / 81 hole borders
followed by `SyscallOutputProvider::open` and `LpFsMemory::write_file`.

### The 20,480 B first-frame allocation, named

The ratchet has carried an unattributed `frame.largest_alloc = 20,480` for
every project since it was baselined. It is
**`VirtualWs281xDriver::endpoints()`**, rebuilding its whole
`Vec<HwEndpoint>` on every output-port open:

```
LpServer::advance_frame → Engine::tick → SharedOutputProvider::open
  → SyscallOutputProvider::open → VirtualWs281xDriver::endpoints
  → RawVec<HwEndpoint>::grow_one → RawVecInner::finish_grow
```

The emulator's board manifest (`HwManifest::virtual_quad_rmt_gpio_board`)
declares GPIO pins `0..=255`, all `GpioOutput`-capable; `endpoints()` walks
every one and pushes an `HwEndpoint` (80 B on rv32 plus two heap `String`s)
into a fresh `Vec` with no `with_capacity`, so it doubles
320 → 640 → … → 10,240 → **20,480 B**. The guest's `TrackingAllocator`
allocates the new block before freeing the old, so 30,720 B of one vector is
live at that instant. Once per port opened in the first frame: ×1 for
`basic` and `meteor`, ×5 for `zook-dome`.

**It is an emulator-board artifact.** The classic's manifest has 34
resources, not 256. But it is also the *first* allocation the classic's
layout cannot serve in all three replays, so it poisons everything after it.
Raw (undiscounted) classic-layout replay on this tree:

| project | would-OOM allocations | final largest free |
|---|---:|---:|
| `examples/basic` | 61 | 13,364 B |
| `examples/meteor` | 1 | 32,900 B |
| `examples/zook-dome` | 2,452 | 7,328 B |

Discounting the two fixture-board sites —
`VirtualWs281xDriver::endpoints` (8,975 blocks, 438,675 B requested,
55,784 B peak live on zook) and the manifest's `Vec<HwResource>` (8 blocks,
73,440 B requested, 55,296 B peak live, 36,864 B resident for the whole
run) — takes **all three projects to zero would-OOM**, and zook's final
largest free block from 7,328 B to **42,428 B** (default workload) /
41,528 B (studio-sync). Every table in sections 3 and 5 is the discounted
one; the report header names its discounts, and says "discounts: none" when
there are none.

⚠️ The discount matches any frame of the symbolized stack, not only the
innermost site, because all `Vec` growth reports the same innermost site.
`--frag-discount-site HwResource` therefore also removes the real
34-resource manifest a device would allocate — a few hundred bytes against
the 36,864 B the fixture board costs.

## 5. The ranked lever table

Same recorded trace, replayed with one lever already pulled, against a
baseline replay of the untransformed trace. Classic layout, discounted,
`--mode startup --workload studio-sync`. **Δ largest** is the row's largest
free block at the last `frame E` minus the baseline's; **holes** is that
marker's hole count, baseline → lever.

Reproduce the whole table with `scripts/frag-table.sh`.

| # | Lever | Windows | Δ largest, basic | Δ largest, meteor | Δ largest, zook | Holes at last `frame E` | Approximation the number carries | Suggested implementation shape |
|---:|---|---|---:|---:|---:|---|---|---|
| 1 | **Residents-first packing** | `project-load`, `frame` | **+31,316** (25,168 → 56,484) | **+15,972** (31,128 → 47,100) | **+8,200** (41,528 → 49,728, *ceiling*) | basic 30 → 6; meteor 46 → 16; zook 27 → 7 | Assumes every retained block's final size is knowable at window open (exact `with_capacity`); a realloc chain collapses to one allocation of its final size. Free total at the last `frame E` is **identical** to baseline on all three projects (a collapsed realloc chain shifts it by ≤ 184 B at earlier markers) — this is pure packing, not saving. | Reserve exactly at the sites the pinning table names, so a resident is born at its final size ahead of the churn instead of doubling through it: the lamp/endpoint/slot vectors behind `RawVecInner::finish_grow`, the JIT's `NativeHostMemory` block, the two `HashMap`s. 938–1,663 blocks / 94–110 KB move per project. |
| 2 | **Scratch arena for `shader-compile`** | `shader-compile` | **+19,064** (25,168 → 44,232) | **−984** (31,128 → 30,144) | **+8,200** (41,528 → 49,728, *ceiling*) | basic 30 → 11; meteor 46 → 15; zook 27 → 18 | A real arena still costs its peak, which becomes a resident for the window's life; growth strategy and alignment slack are not modeled. Meteor is negative *because* of that: its arena peaks at 51,728 B across two openings, larger than the churn it replaced. | Bump-allocate the compile's transients from one block sized at the window's measured peak (basic 44,712 B, meteor 51,728 B over 2 openings, zook 22,692 B), released at `shader-compile E`. 564–5,637 transient blocks per project stop touching the general heap. Meteor says the arena must be *sized*, not merely introduced. |
| 3 | **Scratch arena for `project-read`** | `project-read` | **+0** | **+0** | **+0** | unchanged | Same arena approximation. The zero is the finding, not a failure to measure: the window's `retained` is 0 B and its 599–609 blocks unwind completely. | **Do not build this for fragmentation.** The read is a *victim* of fragmentation, not a cause. (An arena might still be worth it for the abort-safety argument in lever 7, which is a different question.) |
| 4 | **Scratch arena for inbound decode** | inbound line → wire parse | — | — | — | — | **UNMEASURED.** No emulator window brackets it: `fw-emu` receives through a syscall, not the classic's UART path. | The classic copies each inbound line **three times** before serde sees it: `read_buffer` → `line_bytes` (`Vec<u8>` via `drain(..=newline_pos).collect()`) → `String` (`line_str.to_string()`) → `lpc_wire` JSON parse — `lp-fw/fw-esp32v3/src/serial/io_task.rs::process_read_buffer`. The 2026-09-04 manifest-write OOM (observed on the bench during the classic-stamp work; **not yet filed in `docs/defects/`**) is the same path: a 6 KB write became a ~25 KB ask with a project resident. Measuring it needs a firmware-side bracket or a bench capture; that is a session of its own. |
| 5 | **TLSF instead of first-fit** | whole heap | (−20,384) | (−31,048) | (−38,760) | — | ⚠️ **PESSIMISTIC BOUND — DO NOT RANK FROM THIS ROW.** `rlsf` derives its geometry from `size_of::<usize>()`: 16 B header / 32 B granule on this host, 8 B / 16 B on the device, and there is no 32-bit instantiation to ask for. The surcharge peaks at 24–33 KB of live set and is what produces the 314–2,687 would-OOM counts and every negative number. | The honest number needs the silicon arm (section 6) or a width-parametrized TLSF model. `esp-alloc` 0.10 already ships it: `esp_config.yml` defaults `heap_algorithm` to `"LLFF"` (`linked_list_allocator` 0.10.5) with `"TLSF"` (`rlsf::Tlsf<'static, usize, usize, 32, 32>`) as the alternative, `rlsf` 0.2.2 is already in `Cargo.lock`, and **nothing in this repo overrides it** — so all three firmware crates run first-fit today and the flip is `ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF` in the fw crate env, +5,744 B of image. |
| 6 | **Smaller read frame budget** | `project-read` | — | — | — | — | **ESTIMATE, and it moves `transient`, not Δ largest.** `PROJECT_READ_FRAME_MAX_BYTES` = 16 KiB (`lpc-wire/src/budget.rs`) bounds the sink's in-memory `pending_events` batch (`ProjectReadStreamSink`, `lpc-shared/src/transport/server.rs`), which is the read's dominant scaling component; the measured window transient is 23,723 B. Halving the budget to 8 KiB should cut up to ~8 KB from it. Nothing here replays it — the trace was recorded at 16 KiB. | Change the constant and re-run `--workload studio-sync`; the instrument that proves it is the `project-read` window's `transient` figure. Cheap to measure, and it costs wire round-trips. |
| 7 | **Fallible read path, retiring the gate** | `project-read` | no number | no number | no number | — | **No counterfactual applies** — this changes *whether* an allocation is attempted, which a trace replay cannot model. The argument is the measurement mismatch, not a Δ. | Make the read path allocate fallibly (`try_reserve` on the sink batch and the atom builders) and return a structured "out of memory, retry with fewer" instead of pre-judging with a contiguity proxy; Studio backs off page size on that error. That retires `PROJECT_READ_MIN_HEADROOM_BYTES` rather than re-tuning it. |
| 8 | **Narrow the emulator board manifest** | all — instrument fidelity | (removes 61 would-OOM) | (removes 1) | (removes 2,452) | — | Not a device lever at all: it changes what the *gate measures*, not what the firmware costs. | `HwManifest::virtual_quad_rmt_gpio_board` declares 256 GPIO pins; the classic has 34. Narrowing it retires the two standing `--frag-discount-site` flags, un-poisons the ratchet's `frame.transient` and `frame.largest_alloc` (which still carry the 20,480 B `Vec<HwEndpoint>` doubling and the 36,864 B `Vec<HwResource>`), and makes the raw table readable. It moves recorded figures, so it is a hardware-session follow-up, not a drive-by. |

**Combining 1 and 2 adds nothing** over the better of the two on any project
(basic +31,316, meteor +15,972, zook +8,200 — identical to residents-first
alone). They compete for the same bytes: once the residents are packed at
the front, the compile's confetti has nowhere harmful left to land.

### The evidence for the proxy/volume mismatch

Section 1 claimed the gate measures contiguity while the read needs volume.
The discounted classic replay shows the failure mode directly — markers
where the heap has ample total free and would still refuse a read:

| project | marker | largest free | total free | read transient needed | verdict at the 32 KiB gate |
|---|---|---:|---:|---:|---|
| `examples/basic` | `shader-compile E` | **25,168 B** | 57,128 B | 23,723 B | **refused** (7,600 B short of the proxy; 33,405 B of slack against the real need) |
| `examples/basic` | `frame E` (last) | 25,168 B | 57,312 B | 23,723 B | **refused** |
| `examples/meteor` | `shader-compile E` | **31,128 B** | 50,684 B | 23,723 B | **refused** (1,640 B short of the proxy) |
| `examples/meteor` | `frame E` (last) | 31,128 B | 50,900 B | 23,723 B | **refused** |
| `examples/basic` | `shader-link B` | 16,980 B | 46,872 B | 23,723 B | refused — and here *correctly*, on volume grounds too (70 holes, 38 of them under 64 B) |

The `basic` rows are the case the gate gets wrong: **57 KB free, largest ask
4 KB, whole transient 24 KB, refused.** That 25,168 B happens to land within
343 B of the 25,511 B the desk classic reported in the failure that opened
this plan — a rhyme, not a correspondence: different project, different
board, and the silicon arm that would make such a comparison meaningful has
not run (section 6).

The `shader-link B` row is the case the gate gets right, and it is why the
answer is a fallible path rather than simply lowering the constant: at 70
holes with 46,872 B free, a 23,723 B transient of ~600 allocations is
genuinely at risk, and the honest way to find out is to try and fail, not to
guess from one scalar.

## 6. Silicon arm — PARKED, not skipped

The bench arm (P5) is built and blocked on the port, not abandoned.

**Ready on disk** — in the planning directory,
`lp2025/2026-09-04-1358-classic-heap-fragmentation-research/bench/`
(copied there from the session scratchpad; rebuild from the recipe below if
the tree has moved on, since both ELFs were built at `5c870f6c1` + this
branch's lp-perf marker changes).


- `bench.py` — opens `/dev/cu.wchusbserial1320` once at 921600 (raw fd +
  `stty`), records every heartbeat's `MemoryStats` (`free_bytes`,
  `used_bytes`, `largest_free_block`, `oom_retry_saves`) with a timestamp
  and a phase label, and drives reboot → boot-idle → `LoadProject` → compile
  settle → skeleton read → 16-node slot pages → binding-graph probe → 60 s
  idle. Output CSV `t_s,phase,free,used,largest,retry_saves,note`.
- `fw-esp32v3-llff` — stock first-fit ELF, 2,964,704 B.
- `fw-esp32v3-tlsf` — `just build-fw-esp32v3` with
  `ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF` in the environment (esp-config's
  key form is `<CRATE>_CONFIG_<NAME>`), 2,970,448 B (**+5,744 B**). Verified
  by symbols (`llvm-nm`): the TLSF ELF carries `rlsf` symbols and no
  `linked_list_allocator`; the stock ELF the reverse.

**Why it is parked.** `/dev/cu.wchusbserial1320` is held exclusively by
Brave — Yona's live Studio session against the desk classic (confirmed
again at the time of writing: `lsof` shows Brave, pid 46887, fd 337u). A
serial port has one owner, and we do not take one from under a user. No
substitute was run: the emulator is not the classic, and saying otherwise
would be the kind of overstated fidelity the gate doc warns about.

**What the capture will measure when the port frees.**

1. **Does the replay predict silicon?** The classic-layout replay predicts
   largest free block at `project-load E` (73,728 B), first `frame E`
   (49,728 B) and `shader-compile E` (41,528 B) for zook. The bench reads
   `largest_free_block` at those same phases. The emulator carries ~52 KB of
   harness baseline the firmware does not, so the comparison is **deltas
   between markers**, not absolute free. If those deltas match, every
   counterfactual Δ in section 5 can be read as an absolute byte figure on
   the classic; if they do not, the whole table is relative-only and needs a
   fidelity caveat on top.
2. **What TLSF actually costs and buys.** Same script, same phases, on the
   TLSF build: the honest replacement for the pessimistic bound in lever 5.
   ⚠️ `largest_free_block` (bisection by trial allocation) still measures
   under TLSF, but `free_list_shape` (1-byte walk, 8 B step) will overcount
   runs against TLSF's larger minimum block — record what the OOM path
   prints, do not trust its hole count.
3. Stock build reflashed and the heartbeat confirmed back to first-fit
   numbers before the board is handed back.

**Until then, lever 5 carries no ranking.** The TLSF row exists so the
question is on the table with its geometry caveat attached, and the follow-up
below is explicitly gated on this capture.

## Follow-ups

In rank order. Each line: the number it should move, and the instrument that
will show it.

1. **Residents-first packing for `project-load` and `frame`.** Move largest
   free at the last `frame E` by **+31,316 B** (basic) / **+15,972 B**
   (meteor) / **+8,200 B** (zook, at the region-1 ceiling) and hole count
   from 30/46/27 to 6/16/7. Instrument: `scripts/frag-table.sh` — the
   baseline row moves toward the `residents-first` row as sites gain exact
   `with_capacity`. Start with the sites the section 4 pinning table names.
2. **Fallible read path retiring `PROJECT_READ_MIN_HEADROOM_BYTES`, plus
   Studio back-off.** No replay number; the argument is section 5's evidence
   table — 57,128 B free, a 4,096 B largest ask, and a refusal. Instrument:
   the `project_read_refusal.rs` test flips from "refuses at 25 KB largest"
   to "serves, or fails with a retryable structured error"; the bench
   confirms no reset. This is the one that fixes the reported failure.
3. **Scratch arena for `shader-compile`.** Move the same figure by
   **+19,064 B** (basic) / **+8,200 B** (zook), and prove meteor's
   **−984 B** goes positive once the arena is sized rather than peak-sized.
   Instrument: `scripts/frag-table.sh`'s `scratch=shader-compile` row
   becoming the baseline. Updates
   `docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`.
4. **Measure the classic's inbound decode path.** No number exists today.
   Instrument: a firmware-side bracket around `process_read_buffer` (or a
   bench capture across a manifest write), then the same counterfactual
   machinery. It should also file and then close the 2026-09-04
   manifest-write OOM, which has no `docs/defects/` record yet.
5. **Studio's device card shows largest free block and hole count.** The
   heartbeat already carries `largest_free_block`; the card shows only
   "Memory free", which is exactly the number section 3 shows is *not* the
   binding one. No byte moves; the instrument is the card.
6. **Smaller `PROJECT_READ_FRAME_MAX_BYTES`.** Estimated ~8 KB off a
   23,723 B `project-read` transient at 8 KiB. Instrument: the
   `project-read` window's `transient` figure under
   `--workload studio-sync`. Do this *after* (2), which may make it moot.
7. **TLSF flip on device firmware — gated on the silicon arm.** No ranking
   until the bench runs; the emulator row is a pessimistic bound.
   Instrument: `bench.py` on both ELFs, largest free block per phase, plus
   the +5,744 B image cost against `just fw-esp32v3-size-check`.
8. **Narrow the emulator board manifest to something device-shaped.**
   Retires two standing `--frag-discount-site` flags and un-poisons the
   ratchet's `frame.transient` / `frame.largest_alloc`. It moves recorded
   figures, so it wants a deliberate re-baseline in its own PR.
9. **The gate adopts the `studio-sync` workload — Yona's call.** Recording a
   `project-read` window in the ratchet would let read residency ratchet
   like everything else, but the workload change moves *every* recorded
   figure with it. Instrument: `just heap-budget-baseline`, and the diff a
   reviewer then has to read.

## ADR

None. The ratchet gained two figures (`largest_free_at_close`,
`holes_at_close`) and a window (`server-boot`) inside an existing gate whose
conventions are already recorded in `docs/heap-budget-gate.md`; no new budget
convention is being adopted here — this report ranks levers, it does not
commit the team to a per-window contiguity floor. If a lever session later
proposes such a floor (e.g. "the classic must close every window above N
bytes contiguous"), that is the ADR, and it belongs to that session.

## Registers touched

- `docs/defects/2026-09-04-read-gate-refuses-on-largest-block-proxy.md` (new,
  open) — the failure in section 1, with section 5's evidence.
- `docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`
  — the compile window's classic-layout numbers and the scratch-arena
  counterfactual; stays open on the silicon bracket.
- `docs/debt/per-frame-optimisations-are-unpriced-in-ram.md` — the ratchet
  now prices fragmentation too.
- `docs/heap-budget-gate.md` — the instruments, flags and scripts above.
