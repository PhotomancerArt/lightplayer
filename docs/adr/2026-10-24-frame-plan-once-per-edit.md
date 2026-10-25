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

<!-- P6 fills in: emulated per-lane instructions and allocations before/after
     (lp-emu commit), heap-budget figures, C6 flash and stack, FS3 per-lane
     coefficient and fps at 620 / 1,488 / 2,048, C6 silicon fps. -->

- Expected: per-lane graph work goes from ~90 k instructions to a few
  thousand; steady per-lane allocations to ~0; ~0.5 KB resident per lane;
  first frame after an edit costs what every frame costs today.
- The C6 gains on the same code; its heap pays ~0.5 KB per lane resident and
  loses the per-frame transients.

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
