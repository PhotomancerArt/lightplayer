# small-dome's first-frame budget, attributed

Date: 2026-09-06. Tree: `09a3acd8f` (origin/main after #523; small-dome's profiles were
captured on this exact commit). Plan: `lp2025/2026-09-06-0246-small-dome-sample-batches`, phase
P1. This report is the "before" half; P5 fills in the "After" section once P2–P4 land.

`examples/small-dome` (5,950-lamp dome + 360-lamp doors, both patched across two outputs of 13
ports each, 6,310 lamps total) halts the 320 K RV32 emulator guest in its first frame, in both
`--mode startup` and `--mode steady-render`. This report attributes exactly what is live when it
halts, names the failing ask precisely (it is not the graphics buffer notes assumed at the start
of the session), and projects what the frame still needs before it could complete.

## Method

```bash
cargo run -p lp-cli -- profile examples/small-dome --collect alloc --mode startup
# last printed line = profile dir
python3 scripts/heap-live-by-window.py <profile dir> --min-bytes 1000
cargo run -p lp-cli -- profile examples/zook-dome --mode steady-render
```

`scripts/heap-live-by-window.py` (new, this phase) reads a profile directory's
`heap-trace.jsonl` + `meta.json` and prints: the perf-event markers with live bytes/blocks at
each; live bytes/blocks at end of trace by birth window (the perf-event window open when a live
block's allocation was recorded); every live block at or above `--min-bytes` with its birth
window and an attributed call site; and live bytes by call site for the `frame` window alone
(all sizes). It demangles with `rustfilt` when present, and says so on stderr and falls back to
mangled names otherwise. It complements `report.txt`'s own "Live Allocations" section, which has
call site but not birth window.

The small-dome profiles below are **reused, not re-run**: `profiles/2026-09-06T02-52-04--examples-small-dome--startup`
and `profiles/2026-09-06T02-52-35--examples-small-dome--steady-render`, both already captured on
tree `09a3acd8f` per the phase instructions. Only the zook-dome `steady-render` cycle baseline
was re-run fresh in this worktree (see "Zook cycle baseline" below) — its own section explains
why the fresh number differs slightly from the session's.

## The window table (`examples/small-dome --mode startup`, `profiles/2026-09-06T02-52-04--examples-small-dome--startup`)

Heap 327,680 B (the RV32 guest arena; see `docs/heap-budget-gate.md` for why this differs from
the device arena). Figures from `budget.json` / `report.txt`.

| window | transient | retained | largest alloc |
|---|---:|---:|---:|
| server-boot | 47,648 B | 47,648 B | 36,864 B (`Vec<HwResource>`, the 256-resource emulator manifest, `grow_one`) |
| project-load | 127,730 B | 119,743 B | 47,600 B (`mapping_from_map2d_doc` — the dome mapping's `points: Vec<[f32;2]>`, 8 B/lamp: the coordinates' home) |
| frame | 117,823 B (to the halt) | — (window never closes: the trace ends inside it) | 47,600 B |

`--mode steady-render` (`profiles/2026-09-06T02-52-35--examples-small-dome--steady-render`)
halts identically: `OOM (size 47600)` at `ic=23,456,405`, with **327,680 B free** (100% —
the halt lands in the warm-up frame, before the alloc collector's capture window opens, so
every window budget reads zero and the trace holds exactly one non-marker row: the `"t":"O"`
OOM event itself).

## The halt

Two stacks from `report.txt`, quoted exactly:

**The failing ask:**
```
Failed allocation: 47,600 bytes at ic=80,683,804
Free at time of OOM: 12,445 bytes (derived)
Stack: <lp_riscv_emu_guest[...]::allocator::TrackingAllocator>::trace_event_aligned
    <- <alloc[...]::raw_vec::RawVecInner>::try_allocate_in
    <- FixtureNode::render_control
    <- EngineResolveHost::render_control
    <- SessionHostResolver::render_control
    <- OutputNode::consume
```

**The peak (lowest-free) event, 840 instructions earlier:**
```
Free: 12,445 bytes at ic=80,682,964
Event: alloc 32 bytes
Stack: <lp_riscv_emu_guest[...]::allocator::TrackingAllocator>::trace_event_aligned
    <- LpvmGraphics<lpvm_native[...]::rt_jit::engine::NativeJitEngine>::create_sample_points
    <- FixtureNode::render_control
    <- EngineResolveHost::render_control
```

**Finding: the failing ask is not the graphics buffer.** Both stacks bottom out in
`FixtureNode::render_control` (the emulator build inlines `ensure_fixture_sample_points` and its
callees into it, so neither stack names them directly — confirmed against
`lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`, where `render_direct_fixture_control`
calls `ensure_fixture_sample_points`, which calls `create_sample_points(count)` and then
`fixture_sample_point_coords`). The order the two stacks establish: `create_sample_points(5,950)`
succeeds first — that is the 47,600 B `NativeHostMemory::alloc` block, confirmed **live** in the
"Live Allocations" table below at the moment of the halt — leaving only 12,445 B free.
`fixture_sample_point_coords` then asks for its own exact-capacity **transient**
`Vec<i32>` of 5,950 points × 2 words × 4 B = 47,600 B, and that ask is what fails
(`RawVecInner::try_allocate_in`). The two 47,600 B asks are the same size by coincidence (both
scale as 8 B/lamp), which is why the task's starting assumption — "the sample-points buffer, a
fallible Vec ask" — conflated them; they are two different buffers, and the second one (the
coordinates, not the sample points) is the one that halts.

## Live set at the halt, by birth window

315,235 B in 3,209 blocks (`report.txt`'s "Live Allocations" total). Markers, from
`scripts/heap-live-by-window.py profiles/2026-09-06T02-52-04--examples-small-dome--startup --min-bytes 1000`:

```
markers (live bytes, blocks):
  profile:start  I  ic=           0  live=        0  blocks=0
  server-boot    B  ic=           0  live=        0  blocks=0
  server-boot    E  ic=   8,576,250  live=   47,648  blocks=909
  project-load   B  ic=  18,859,759  live=   78,470  blocks=970
  project-load   E  ic=  51,026,077  live=  198,213  blocks=2318
  frame          B  ic=  65,027,447  live=  197,412  blocks=2320
  profile:end    I  ic=  80,683,804  live=  315,235  blocks=3209
```

By birth window at end of trace:

| birth window | live B | blocks | what |
|---|---:|---:|---|
| project-load | 119,743 | 1,348 | mapping points 47,600 + 2,880 (`mapping_from_map2d_doc`, 50,480 total); patch rows 3,840 (`lpc_mapping::patch::parse_rows`); inventory/shapes/slots the remaining ~65,423 B (`InventoryDerivation::walk_graph_node`, `EngineServices::reconcile_wires`, project handle table, node-def entries, …) |
| frame | 117,823 | 889 | sample points 47,664 (= 47,600 + a 64 B handle, 2 blocks — `LpvmGraphics::create_sample_points`); `OutputNode::consume` 21,518 in 4 blocks (the output-A runtime buffer + fragment bookkeeping); resolver residents ≈ 30 KB: `snapshot_slot_shape` 7,488, `QueryInternTable::intern` 7,264, `resolve_interned` 5,628, `produce_consumed_slot` 3,432, `SlotPath::parse` 3,072, `SlotAccessor::compile` 3,072, `ResolverCache::insert` 3,040, `String::clone` 2,467, plus smaller sites |
| server-boot | 47,648 | 909 | `Vec<HwResource>` 36,864 (the emulator's 256-resource manifest, emulator-only) + JIT symbol table (`VecMap::insert`, `_lp_main`) and format/init churn |
| after-server-boot | 29,919 | 58 | the profile workload's project deploy into `LpFsMemory` (`write_file`/`append_file`, three ~8 KB chunks) plus `fw_emu::server_loop::run_server_loop`'s own buffer — emulator-only; a device keeps project files in flash, not RAM |
| after-project-load | 102 | 5 | negligible |

Both the marker table and the birth-window table match `notes.md`'s "Measurements" section
exactly — expected, since this is the same profile directory, not a re-run.

## Per-lamp / fixed / emulator-only attribution at the halt

**Per-lamp residents present when it halted:** mapping 8 B/lamp (50,480 B, both fixtures),
sample points 8 B/lamp (47,600 B, dome only — doors have not been reached yet), output-A
samples 6 B/lamp (20,010 B of the 21,518 B `OutputNode::consume` block, = 3,335 lamps × 6 B —
output A's share of the patched lamps).

**Not yet allocated when it halted** (the frame's remaining asks, all per the code path in
`fixture_node.rs`): the coordinates transient that actually failed (47,600 B); the sample
*target* (`create_sample_out`, another 47,600 B, 8 B/lamp of the dome); the doors' three 2,880 B
buffers (mapping done, sample points/target/coords still to come, at 360 lamps); output B's
runtime buffer (17,850 B, 6 B/lamp of its ~2,975 lamps); the 26 ports' 8-bit frames (3 B/lamp =
18,930 B total) plus per-port bookkeeping (~400 B × 26 ≈ 10,400 B); and frame 2's shader
compiles (two shaders on this project — zook's own compile transient is 22,858 B, `examples/basic`'s
is 47,716 B, so this project's is somewhere in that range, uncounted here).

**Emulator-only fixed cost, not paid by a device:** `Vec<HwResource>` 36,864 B (the permissive
256-resource manifest, resident for the whole run); `LpFsMemory`'s deploy of the project files,
~30 KB resident (a device keeps these in flash); `Vec<HwEndpoint>` 20,480 B per port open
(transient — every port open re-enumerates all 256 manifest resources; not counted in the
"live at halt" total above since it frees within the opening, but it is the `frame` window's
largest emulator-only transient cost still to come as more ports open).

**Per-lamp residents on the device path today (Direct fixture), all five homes together:**
mapping 8 + sample points 8 + sample target 8 + output samples 6 + 8-bit frame 3 =
**33 B/lamp**, matching `docs/reports/2026-09-02-per-lamp-memory-table.md`'s "owner table after"
figure. On small-dome's 6,310 lamps that is 208,230 B before any fixed cost. Removing the two
graphics buffers (sample points + sample target, 16 B/lamp) would save 100,960 B of residents
plus the 47,600 B coordinates transient that halts today.

## Projection

**What the frame still needs after the halt:** 315,235 (live now) + 47,600 (the sample target)
+ 5,760 (the doors' two *resident* buffers — sample points and sample target, 8 B/lamp × 360
lamps each; their own transient coords ask does not stay live, so it is not part of this
residency sum, unlike the dome's, which is what halted) + 17,850 (output B) + ~29,000 (the
remaining ports) ≈ **415 KB**, against a 327,680 B heap. No
ordering trick closes that gap: allocating the sample points at load instead of at render time
moves the same bytes earlier without shrinking them, and the frame is short by roughly 90 KB
*before* counting a shader-compile transient (23–48 KB on other projects). Only removing
per-lamp residents closes it.

**Projection after the chosen lever (bounded batches, D1's L6 below):** per-lamp residents drop
to 17 B/lamp (mapping 8 + output samples 6 + 8-bit frame 3; the two graphics buffers become a
bounded per-fixture batch window instead of per-lamp homes) = 107,270 B on small-dome; batch
buffers add 3 × 1,024 B per fixture ≈ 6 KB; fixed costs stay ≈ server-boot 47,648 + emulator FS
deploy 29,919 + project-load non-lamp ~65 KB + frame non-lamp ~30 KB + ports ~10 KB ≈ 183 KB.
Steady total ≈ 296 KB, leaving ~31 KB for the compile transient and the emulator-only
per-port-open `Vec<HwEndpoint>` (20,480 B, transient). Tight — whether small-dome then completes
is a measurement for P5, not a promise this report makes.

### D1 — the lever, and why every alternative was rejected

Reproduced from `notes.md`'s decision table (this phase does not revisit the decision, only
verifies the measurements it rests on):

| ID | lever | bytes on small-dome | verdict |
|---|---|---:|---|
| L1 | Quantize the sample-points buffer (u16 per coordinate) | −23,800 | **Rejected**: pixel-space Q16 for a 128-wide target needs 24 bits; not byte-identical. Normalized Q0.16 can't hold 1.0 (65536). |
| L2 | Allocate sample points at load, before the frame's transients | 0 | **Rejected**: the halting ask is the transient coords `Vec`, and the frame is ~90 KB over regardless of order. |
| L3 | Stream the coords into the buffer (no transient `Vec`) | −47,600 transient | **Necessary but not sufficient**: the sample target's 47,600 B comes next with only 12 KB free. |
| L4 | Per-output sampling of only the runs on this output | 0 resident | **Rejected**: the buffers key on count; two outputs would reallocate every frame, and the render-whole contract (#523) stays. |
| L5 | Store Q16 in the mapping, drop the f32 points | −47,600 | **Rejected**: a format change to `ResolvedMappingCompact`; lossy for TextureArea precompute and Studio layouts — not byte-identical outside Direct. |
| **L6** | **Bounded batches**: sample through a 128-point window — resident points + out + coords scratch per fixture (3 KB), coordinates regenerated per render from the mapping (their one home) | **−100,960 resident, −47,600 transient** | **Chosen.** Byte-identical (per-point shader evaluation is batch-agnostic). Costs ~40 cycles/lamp/render with an integer coordinate conversion (≈3% of zook's frame), ~390 without (≈30%) — the integer conversion is mandatory alongside this lever. |

## Zook cycle baseline

```bash
cargo run -p lp-cli -- profile examples/zook-dome --mode steady-render
```

Re-run fresh in this worktree: `profiles/2026-09-06T03-32-37--examples-zook-dome--steady-render`.
Compared against the session's `profiles/2026-09-06T02-55-37--examples-zook-dome--steady-render`.
`steady-render` runs 2 warm-up `frame` windows then captures 4 (`STEADY_RENDER_WARMUP_FRAMES` /
`STEADY_RENDER_CAPTURE_FRAMES` in `lp-cli/src/commands/profile/mode/steady_render.rs`, matching
`docs/heap-budget-gate.md`'s "two warm-up frames, then four captured steady frames"; a mid-run
shader-compile resets the warm-up counter, which is why the trace shows 8 `frame` Begin markers
for 4 captured frames on this run).

| figure | session (02-55-37) | fresh (03-32-37) | Δ |
|---|---:|---:|---:|
| total_attributed_cycles | 9,578,848 | 9,579,232 | **+384 (+0.004%)** |
| profiled_instructions | 7,309,996 | 7,310,268 | +272 |
| cycles / captured frame (÷4) | ≈2,394,712 | ≈2,394,808 | — |
| `[jit] __render_samples_rgba16` inclusive | 6,544,488 (68.3%) | 6,544,488 (68.3%) | 0 |
| `write_direct_lamps` inclusive | 1,878,348 (19.6%) | 1,878,348 (19.6%) | 0 |

**Finding: the two runs are not quite identical, and the difference is explained, not noise.**
The emulator is deterministic on an unchanged tree, but this worktree is *not* a clean checkout
of `09a3acd8f`: at the moment this profile was captured, `git status` already showed uncommitted
edits to `lp-gfx/lp-gfx/src/shader.rs`, `lp-gfx/lp-gfx/src/graphics.rs`, and
`lp-shader/lp-shader/src/px_shader.rs` — P2's backend seam (`LpShader::bind_uniforms` +
`sample_rgba16_bound`, with `sample_rgba16` becoming a default-bodied convenience that calls
both), landing in this same shared worktree ahead of this phase per the plan's "P2–P4 may run in
parallel with this phase." (By the time this report was finished, `git status` showed a dozen
more files touched across `lp-gfx-lpvm`, `lp-gfx-wgpu`, `lp-gfx-harness` and `lpc-engine` — P2's
work continuing to land live in this worktree while P1 was written. That later state was not
re-profiled; the analysis below is against the tree as it stood for this one `cargo run`.) The
fresh report's "Top 20 by inclusive cycles" section shows the extra call frame directly:
`LpvmShader::sample_rgba16` (6,601,000) now calls `LpvmShader::sample_rgba16_bound` (6,535,052)
which calls `BackendAdapter::call_render_samples` (6,533,804) — one more hop than the session's
report, which went `sample_rgba16` (6,600,664) straight to `call_render_samples` (6,533,804,
identical figure). The **shares that matter for the lever this plan is choosing are
unaffected** — `__render_samples_rgba16`'s inclusive cycles and share, and `write_direct_lamps`'s,
match the session's numbers exactly — so this is call-boundary overhead from unrelated in-flight
work sharing the worktree, not a regression to chase down, and not something this report's own
script or report touches. It is flagged here rather than silently reproduced, per the phase's
instructions. **Practical consequence for later phases:** a worktree shared with P2/P3/P4 work in
progress is not a clean baseline for cycle-exact comparisons; a re-run for the ship-gate report
should either wait for those phases to land and commit, or run against a dedicated clean
checkout of `09a3acd8f`.

`profiles/2026-09-06T02-55-44--examples-zook-dome--startup` (also session data, not re-run —
only the `steady-render` baseline was in scope for a fresh run) additionally shows `__mulsf3`
at 845,903 cycles over 26,240 calls (≈32 cycles each) during coordinate generation at load: the
C6 has no FPU, so every f32 op there is a libcall. This is the basis for the D1 table's estimate
that per-render coordinate regeneration (this plan's lever, L6) costs ~390 cycles/lamp without
an integer `normalized_f32_to_q16`, ~40 with one.

## After

_(P5 fills this in once P2–P4 land: the new streaming seam, byte-identical goldens, the integer
coordinate conversion, and whether small-dome completes its first frame.)_
