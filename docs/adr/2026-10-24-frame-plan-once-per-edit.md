# ADR: The engine plans a frame once per edit, and replays the plan every frame

- **Status:** Accepted (planning `lp2025/2026-10-24-0208-engine-frame-plan`)
- **Date:** 2026-10-24
- **Deciders:** Photomancer
- **Extends:** `docs/adr/2026-07-31-resolver-persistent-resolution.md` — its
  invalidation contract, and the alternative it rejected ("a compiled
  resolution plan"), which this answers rather than adopts.
- **Related:** `2026-09-06-direct-sampling-bounded-batches.md` (the
  coordinates' one home, kept), `2026-07-06-sans-io-core.md`,
  `2026-08-01-scoped-bus-engine-architecture.md`,
  `docs/debt/s3-frame-cost-scales-per-fixture.md` (the burden this pays down),
  `docs/debt/per-frame-optimisations-are-unpriced-in-ram.md`.

## Context

The S3 fps spike (2026-10-23, FS3, `fps-spike/report.md` in the S3
radio/OTA planning directory) fitted the shipped engine's non-shader frame
time as **rest ≈ 5.7 ms + 4.7 ms × lanes + 1.9 µs × LEDs**, where a lane is
a fixture and the output it feeds. Each lane costs ~90 k instructions of
graph walk per frame (emulated), fetched from flash at ~12.5 cycles each on
silicon; 40 KB of hot code in IRAM brings that to ~1.85 ms per lane, and two
cores take 2,048 LEDs (8 × 256) to 23.1 fps. The ~10 ms between that and
30 fps is the walk.

The walk re-derives, every frame, things only an edit can change: the
output's `"input"` parsed into a `SlotPath` twice; the default-fragments
topology (`is_default_fragments_consumer`) rebuilt from the binding index,
uncounted; placements, fragments, coverage and span `Vec`s rebuilt (the
fixture's path spans three times); a `String` recovery name per node
dispatch, about five dispatches per lane; def-view reads that clone a
`SlotAccessor` to find a value the resolver already cached; and every shader
uniform re-bound by string path, because time is in the bind key and
`produce` clears it anyway. 54–78 allocations per lane per frame; ~45 % of
the per-lane instructions are the allocator.

2026-07-31 made *resolution* persistent and rejected a compiled plan:
nodes issue queries that are not knowable ahead of the tick (a fixture reads
`power.some` only if its def has it, a shader one query per declared input,
a playlist whichever child is current), so a global schedule would need a
discovery pass per frame anyway. That objection still holds against a global
schedule. It does not hold against plans built by the nodes themselves, on
the one frame after an edit when they run their discovery anyway.

## Decision

**Each node that does per-frame graph work builds a frame plan the first
time it runs after its plan key moves, and replays it on every later frame.
The engine does the same for its own per-dispatch and per-lane work.** No
global schedule exists; the walk stays demand-driven from the demand roots,
and each step of it becomes a replay.

### What a plan holds

| Owner | Plan | Key |
|---|---|---|
| Engine, per node | dispatch metadata: the recovery-frame name (`Rc<str>`, the path rendered once), the produce call key (the resolver's interned path handle, not a copy) | the node's path / the epoch's intern table |
| Engine | lane topology (`LaneTopology`): per node, whether it is the default fragments consumer of its bus channel | structure epoch |
| Engine | output sink configs (`refresh_output_sink_configs` runs only when this moves; checked like a plan) | structure epoch |
| Output | input query id, control products, placements, fragments plan, coverage, span layout, validated scatter runs, the published layout (compared once per build, not per frame), def snapshot (name, highlight, test pattern) | structure epoch, `output_identity_revision`, each producer's layout revision |
| Fixture | settings snapshot (`render_size`, `color_order`, brightness, gamma, power, selection, diagnostic mode, consume policy, flags) for every read whose resolved source is structural; an interned query id for every read that is not; extent, path spans, control spans, lamp channel count, request space (the shader's `visual_product_space`), patch resolution, the Direct lane job's static half | structure epoch, `mapping_version`, `patch_version`, `output_identity_revision`, the producer shader's layout revision |
| Shader | a uniform write list: per member a backend-resolved slot, its source (`Literal`, `Intrinsic`, `Seconds`, `Phasor`, `PaletteTexture`, `Bound{query, revision}`), and whether it is time-varying | structure epoch, the program (compile generation), request size |

A read is planned by its **resolved source**, not by its name: a
`Production` whose source is `Literal` or `Default` (structural, already
kept by the resolver across frames) is snapshotted into the plan; one that
comes through a producer or a bus writer keeps its interned query id and is
resolved per frame, without rebuilding a key.

### What stays per frame

Producer values (clocks, bus-bound slots, panel-writer values), merges of
per-frame products, time-varying uniforms (written **by slot**, not by
path, and only those whose source moved), the playlist's
`PlaylistFramePlan`, shader sampling, coordinate generation, the per-lamp
write (gamma, brightness, power, selection, order), the output's
reverse/rotate/fault paint and publish, sink flush, the timebase sweep, the
project-fault fingerprint.

Dynamic demand is not a plan input. A playlist pick, a selector switch, a
residency hook changes which child is asked; each child holds its own plan,
and a child not asked replays nothing. This is the 2026-07-31 rule ("a
playlist entry switch is deliberately not structural") carried to plans.

### When a plan is rebuilt

| Edit | How it reaches the plans |
|---|---|
| Project load / reload | new `Engine`; every plan is built on the first frame |
| Slot value edit, def edit, node add/remove/replace, asset refresh (all `apply_project_changes`) | structure epoch |
| Binding add / clear / rewire | structure epoch |
| Panel write / clear / TTL despawn / unpark | structure epoch (today's behaviour: a knob drag rebuilds per write step, costing what a frame costs today) |
| Residency load/unload | structure epoch via subtree removal / rewire |
| `attach_runtime_node`, `add_demand_root` | **now** structure epoch (they bypassed it) |
| Mapping change (`sample_diameter`, asset refresh) | the fixture's `mapping_version` |
| Patch edit / output rename | `patch_version`, `output_identity_revision` |
| Shader recompile | the shader's compile generation; consumers see its layout revision |
| Memory pressure | plans are rebuildable state: dropped on `High`, rebuilt on the next frame |
| Time, bus values, playlist pick, node commands | never — per-frame inputs |

### The invalidation contract, and how a miss is made loud

The 2026-07-31 contract extends: **any engine mutation that can change what
a plan holds must move a key the plan reads.** It fails silently in the same
way, so it is checked three ways:

1. **Every plan lives in one `PlanSlot<T>`.** `T: PartialEq + Debug`. In
   check mode, a replay rebuilds the plan from scratch and compares; a
   difference panics in tests (and logs once, counted, in other debug
   builds) naming the node path, the plan type and the first differing
   field. Check mode runs every frame under `cfg(test)` and every 32nd frame
   in other `debug_assertions` builds; release firmware compiles it out. A
   plan that bypasses `PlanSlot` is a review-time defect, and the checker is
   what makes the cache provably equal to the walk *without someone first
   imagining how it goes stale*.
2. **The shape fingerprint grows.** The debug guard in `Engine::tick_nodes`
   adds the demand-root count and every node's attach revision, and the
   bypasses (`attach_runtime_node`, `add_demand_root`, `tree_mut()` edits)
   either bump the epoch or become crate-private.
3. **A byte differential.** The same project corpus runs plans-on against
   `PlanMode::RebuildEveryFrame` through an edit script that exercises every
   row of the table above, and the output bytes must agree frame for frame;
   a load-vs-apply variant must agree with a fresh load after each edit.

Counters make the steady state a unit-test question, as `ResolveFrameCounters`
did: `FramePlanCounters { builds, replays, checks }`; a steady frame has
`builds == 0`.

### Lanes become work items

A Direct fixture's per-frame render becomes **prepare** (with `ctx`: plan
replay, power budget, the shader's per-frame uniform writes) and **run** (a
`DirectLaneJob`: the coordinate source, the window, the lamp-writer
parameters and the destination runs — plain data, no `Rc`, no resolver, no
allocation, no `ctx`). The job runs where it runs today, inside the shader's
sample stream. A later change can collect the jobs of a frame and hand them
to a second core: lanes write disjoint destination runs, and the only shared
state is the shader instance, which needs one vmctx per core (as the
two-core render already requires). Collecting jobs across nodes needs the
engine's steal-and-restore dispatch to lend the shader instance for the
render phase; that is the follow-up's design, not this one's.

### What does not change

Nothing persisted, nothing on the wire, no `PROJECT_FORMAT_VERSION`, no
`WIRE_PROTO_VERSION`. Output bytes are identical frame for frame. The
bounded-batches trade stands: **coordinates are still regenerated from the
mapping every frame**; a per-lamp coordinate cache costs 8 B/lamp (+47,600 B
on small-dome) and is not taken here.

### As built (P2, 2026-10-24)

- The plan state (mode, frame index, counters) lives on the `Resolver`,
  which every node context already reaches; a `TickContext` is a
  `PlanHost`, so a plan's build closure can resolve through the same context
  it was handed.
- The lane topology holds only the default-consumer answer, built for every
  node at the top of the tick (the epoch cannot move mid-walk). The patched
  runs per (producer, output) are not an engine plan: the output's own plan
  holds its placements, which is the only place they are asked.
- A check's rebuild is invisible to the counters (only `checks` and
  `check_mismatches` move), so "a steady frame builds nothing" holds in
  checked builds too.
- **The resolver caches a missing bus provider** (`ResolvedRoute::NoBusProvider`).
  The 2026-07-31 rule was that failures are never cached, so a broken
  project keeps reporting itself. A bare bus read with no provider at all is
  not a broken project: it is how a clock reads `clock.rate`, a fixture its
  `brightness` and a shader its parameters when nobody writes them, and it
  re-walked the providers every frame — the last per-frame structural work
  on a steady frame of every corpus project. Only a binding or a panel edit
  can add a provider, and both move the epoch; the cached route answers the
  same error every frame. Ambiguity and cycles stay uncached.
- The tick keeps its producer set and resolve trace across frames (cleared,
  not regrown).

## Consequences

Measured 2026-10-24 (planning `lp2025/2026-10-24-0208-engine-frame-plan`,
P6). A lane is a fixture and the output it feeds.

| | before (main 9e9c20d3f) | after (PRs A + B) |
|---|---:|---:|
| **FS3 per lane** (frame time, `lanes-8x128` − `lanes-2x512`, ÷ 6) | 1.393 ms | **0.286 ms** |
| FS3 fps 2×512 / 8×128 / 6×248 / 8×256 / skirt-620 | 41.1 / 30.6 / 28.3 / 21.9 / 53.6 | 44.6 / 41.4 / 34.5 / 27.3 / 59.3 |
| C6 fps (CX1) basic / basic-4n | 40.6 / 16.0 | 42.3 / 16.9 |
| emulated per lane (`scripts/perf/per-lane-cost.sh`, lp-emu esp32-c6 model, lp-emu 2352e4c4e) | 69,326 cycles, 46 allocs | 38,643 cycles, 6 allocs |
| host per lane, real nodes (`steady_frame_allocations_do_not_scale_with_lanes`) | 45 allocs | 5 allocs |
| C6 image: core / engine (local `fw-esp32c6-size-check`) | 1,448,144 / 1,852,000 B | 1,448,224 / 1,857,110 B (+80 / +5,110) |
| C6 main stack, meteor / choker load / idle (`emu_stack_peak`, t1) | 35,800 / 33,868 / 13,440 B | 32,632 / 33,868 / 13,440 B |
| C6 first light (t4) | 363 ms | 362 ms |

Silicon: FS3 (XIAO ESP32-S3 Plus, D8:3B:DA:47:29:70), plain `fw-esp32s3`
images, two-core default; CX1 (ESP32-C6, 14:C1:9F:E6:54:90), split images.
On FS3 the after-image runs with the IRAM hot set **re-profiled on the new
engine** (`lp-fw/fw-esp32s3/hot_iram/`): with the hot set profiled on the
old code the same build measured 1.141 ms per lane, because the new engine
functions ran from flash. A restructured engine path needs a re-profile, as
the hot-set README says.

- **The emulator underprices this walk.** Emulated per lane fell 44 %;
  silicon fell 79 %. The per-lane walk on the S3 was fetch- and
  PSRAM-bound, and allocation and pointer chasing cost more there than the
  cycle model says.
- **RAM.** Steady frames allocate less on every engine record (basic
  132 → 77, meteor 553 → 498 per frame; steady `frame.retained` unchanged).
  The kept plans and read slots cost ~0.5 KB per lane, resident, at load.
- **AC4 (−85 % emulated) is not met: −44 %.** Recovery frames are ~13 k of
  the remaining 38.6 k cycles, but even without them the rest fell ~55 %:
  the emulated cycle model prices this walk differently from silicon, where
  the bar was met.
- **What remains per lane** (emulated): the crash-recovery frames each
  dispatch enters (~13 k cycles of the 38.6 k; `lp_recovery`, in the core
  image), the output input's fragment merge, and per-dispatch overhead.
- **Found by the checker along the way:** every output swept its own
  identity as dead mid-consume, so `output_identity_revision` moved every
  frame and every patched fixture re-resolved its patch per frame; the
  revision was a frame stamp (two outputs in one frame collided) — a change
  counter now; a bus read with no provider re-walked the providers every
  frame — a cached decision now.
- **The stack.** A first cut put plan state where the C6 main stack paid for
  it several times (state in every nested context, fields of the
  by-value-moved `Engine` and `Resolver`); it broke the stack gate's 12 KiB
  margin and was fixed by boxing the engine's plan state, borrowing the
  resolver in place and keeping the per-frame reads out of the deep frames,
  which left the deepest project 3 KB *below* main.
- Not taken here: the shader's uniforms by slot (the plan's P5; fixed per
  shader, not per lane, and its engine bytes would pass the C6 budget's bar
  on top of this) — a follow-up.

## Alternatives considered

- **A global compiled schedule** (the 2026-07-31 alternative): the engine
  derives fixture → shader → output stages from the graph and executes a
  flat list. Highest ceiling, but it needs the engine to know node
  semantics, breaks on conditional queries and dynamic demand, and replaces
  the pull model wholesale. Node-held plans get the same per-frame result
  and keep the pull model.
- **More point caches** (the fps spike's two, `Rc<str>` names and a cached
  `is_default_fragments_consumer`): −32 % per-lane instructions emulated,
  −0.2 ms per lane on silicon. Each is a separately invalidated cache with no
  checker; this decision subsumes them under one mechanism with one checker.
- **Recording plans by dependency** (each plan records every revision it
  read and re-checks them per frame): exact, but costs a compare per
  dependency per frame and a recording context on every read path. The
  explicit keys plus the rebuild-and-compare checker give the same safety
  for less.
- **Caching coordinates** per lamp: rejected again, on the bounded-batches
  ADR's numbers; a follow-up may opt in on boards with PSRAM if FS3 shows
  coordinate generation above 1 ms at 2,048.
