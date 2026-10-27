# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: loaded project runtime state, node trees,
resolution, bindings, runtime state slot roots, and the boundary between
shader/runtime values and portable model or wire values.

**Runtime spine:** `engine::Engine` is the core runtime owner for the new
demand-driven path. It owns the `NodeTree`, engine-level `Resolver`, artifact
store, frame state, slot shape registry, runtime buffers, and demand roots.

**Entry residency:** a parent node can ask for its entry-keyed children to
be loaded or unloaded (`NodeRuntime::residency_request`; only `PlaylistNode`
does). `Engine::apply_residency(fs, &mut registry)` applies the request at
the top of the tick — unload first, then load, rolled back on failure — and
tells the node through `entry_loaded` / `entry_unloaded` /
`entry_load_failed` / `residency_refused`. The registry owns which entries
are resident; the tick owner (`lpa-server` `Project::tick`) calls this step,
because `tick` itself has no filesystem. See
`docs/adr/2026-09-25-parent-owned-child-residency.md`.

**Bindings and resolution:** bindings are node-instance data stored on
`node::NodeEntry` and indexed by `node::NodeTree`. Bus names remain useful
runtime vocabulary for labeled channels, but resolved values are cached by the
engine resolver rather than by a bus object.

`resolver::Resolver` owns same-frame query cache state. `ResolveSession` is the
active per-frame/per-demand object that resolves `QueryKey`s through the
active `ResolveHost`, calls that host on cache misses, and carries a
`ResolveTrace`.
`ResolveTrace` combines cycle detection with optional structured trace events so
tests and future diagnostics can explain value provenance.

The first runnable core slice uses test-only dummy shader/fixture/output nodes
from `engine::test_support` to validate demand roots, bus binding selection,
same-frame caching, recursive resolution, cycle detection, and revised values
without depending on concrete node implementations.

Unlike `lpc-model` and `lpc-wire`, this crate may depend on `lps-shared`
because it is responsible for converting between `LpsValue` / `LpsType` and
`LpValue` / `LpType`.

**Produced values:** demand-driven resolution caches
[`resolver::production::Production`]: an `LpValue` plus revision provenance.
Nodes expose produced values through their runtime state slot roots. Shader ABI
values are converted at node/shader boundaries; lazy graph products travel as
`LpValue::Product`.

**Naming:** Prefer plain engine/runtime nouns when the crate already owns the
concept (`Engine`, `NodeTree`, `Resolver`).
Use an `Engine*` prefix only when ambiguity with another layer remains high.
Conversion helpers should name both sides of the boundary (for example functions
that mention `lp_value` / `LpType` vs `LpsValueF32` / `LpsType`).

## The frame plan

Resolution persists across frames until a structural edit
(`docs/adr/2026-07-31-resolver-persistent-resolution.md`). The frame plan
carries the same idea one step further: graph work a node (or the engine)
re-derived every frame — binding topology, parsed paths, spans, fragments,
uniform slots — is built **once per edit** and replayed every frame after
(`docs/adr/2026-10-24-frame-plan-once-per-edit.md`).

**The contract:** any engine mutation that can change what a plan holds must
move a key the plan reads. Keys are the resolver's structure epoch plus a few
node-local revisions; `attach_runtime_node` and `add_demand_root` move the
epoch too, and the debug shape guard in `Engine::tick_nodes` counts demand
roots and attaches as well as nodes and bindings.

**`frame_plan::PlanSlot<K, T>`** is the only home of a plan: `get_or_build(key,
host, build)` replays while the key holds, builds when it moves. Its checker
is the reason it is the only home: in `PlanMode::Check` a replay also
rebuilds the plan and compares, and a difference names the owner, the plan
type and the first differing line of their `Debug` forms. `clear()` drops a
plan (memory pressure: plans are rebuildable state, dropped on `High`).

**Modes** (`Engine::set_plan_mode`):

| mode | where | what |
|---|---|---|
| `Check { every: 1, strict: true }` | this crate's own tests | rebuild and compare every frame; panic on a difference |
| `Check { every: 32, strict: false }` | other debug builds (Studio dev, emulator debug) | compare every 32nd frame; log once, count, serve the fresh plan |
| `Replay` | release | replay; the comparison is not linked |
| `RebuildEveryFrame` | the differential | build every frame, as if every key moved |

Allocation and counter tests that price the product set `Replay`.

**Counters:** `Engine::frame_plan_counters()` (`builds`, `replays`, `checks`,
`check_mismatches`, `default_fragments_reads`), reset per frame like
`ResolveFrameCounters`. A steady frame has `structural_work() == 0` on both.
A check's own rebuild is not counted as work.

**The oracles:** `tests/frame_plan_differential.rs` runs every corpus project
through one edit of each kind, frame for frame in four modes (cached,
forced-invalidate, rebuild-every-frame, strict-checked) and against a fresh
load of each edit; `engine::steady_frame_alloc_tests` ratchets steady-frame
allocations per lane on the real nodes (`scripts/perf/README.md`).

What each plan holds and its key:

| owner | plan | key |
|---|---|---|
| engine | `LaneTopology`: per node, whether it is the default fragments consumer of its bus channel (where unaddressed patch runs and auto-flow land) | structure epoch |
| engine | output sink configurations, re-read from the defs | structure epoch (checked like a plan in `Check` mode) |
| output | `OutputFramePlan`: the fragments planned from its producers' placements, their coverage and its status (and the products it was built for) | structure epoch, `output_identity_revision` (a change count), the producers' summed `control_patch_generation` |
| fixture | `FixtureFramePlan`: the lamp count, the control extent and the full-write control spans | structure epoch, `mapping_version`, the colour order |
| engine | per dispatch: the recovery-frame name (`RuntimeNodeEntry::recovery_name`, the path rendered once) and the produce call key (the resolver's interned path handle) | the node's path / the epoch's intern table |

**Reads a node makes every frame** keep their query id in a
`frame_plan::PlannedRead` (tagged with the epoch it was interned in) and
resolve by id (`TickContext::resolve_planned_static`, `read_planned_accessor`,
`read_planned_option_or`): the value is still resolved per frame — a bus
writer stays live, a default is a cache hit — but no key is built and no
memo searched. The fixture's settings reads and the output's go through it.

**A Direct lane is a work item.** `nodes::fixture::direct_lane_job::DirectLaneJob`
is the per-frame render once the engine is done deciding: the coordinate
source and the lamp writer (encoding, power pass, selection, destination),
plain data with no `Rc`, `RefCell`, resolver or context, because a second
core will run lanes. Today the shader's sample stream drives its batches;
its test runs one from a hand-built job and a fake sampler, allocating
nothing.

Reads through a compiled def view (`reader.get(ctx)`) resolve by an id the
resolver interns once per epoch per `(node, accessor)`
(`Resolver::intern_consumed_accessor`) instead of cloning the accessor into
a key on every read.

The resolver's own share: a bus read nobody provides is cached as a
`ResolvedRoute::NoBusProvider` decision (it still answers the same error
every frame), so an optional channel nobody writes costs no provider walk
per frame.

## Node-kind feature gates (M2)

> **This gate set is provisional and expected to be revisited.** It was sized
> against one constraint — the ESP32-C6's 3 MB partition — and deliberately
> kept light-touch. The ESP32-S3 has 16 MB and will soon carry features the
> other builds do not, at which point the useful axis stops being "which node
> kinds fit" and becomes "which capabilities does this board have". Treat the
> granularity below as a first cut, not doctrine: adding, merging, or
> re-drawing gates is expected work, not a redesign.

Every node runtime kind except `Project` (the always-present root
placeholder) and `Output` (`LpServer` requires an output provider) is behind
its own Cargo feature, all default-on:

| Feature | Runtime(s) it gates |
|---|---|
| `node-button` | `ButtonNode` |
| `node-power-button` | `PowerButtonNode` |
| `node-radio` | `ControlRadioNode` |
| `node-fluid` | `FluidNode` |
| `node-fixture` | `FixtureNode` |
| `node-texture` | `TextureNode` |
| `node-playlist` | `PlaylistNode` |
| `node-clock` | `ClockNode` |
| `node-shader` | `ShaderNode`, `ComputeShaderNode` |

The build's resulting gate set is introspectable:
`lpc_engine::supported_features()` (`src/features.rs`) derives the enabled
engine-owned `lpc_model::LpFeature`s from the same `cfg!` facts as the gates
themselves, for the firmware manifest and `ServerHello` capability reporting.

Gating is **removal-only**: switching one off drops that node kind's runtime
code and its exclusive dependencies from the build; it never changes what a
default build (`default-features = true`, the normal host/studio/test build)
compiles or runs. They exist so a constrained firmware build — the S3 app
layer; see the M2 plan — can link only the node kinds it actually uses.
As of 2026-07-31 (the S3 node-gates plan) no in-tree firmware is gated down:
`fw-esp32c6` and `fw-esp32s3` both enable all eight, and the gates remain for
genuinely constrained future boards.

### A ninth gate that trades RAM, not flash

`resolver-payload-cache` follows the same removal-only contract but is
measured in *heap*, not image size, and it is the one gate an in-tree
firmware actually drops.

On, the resolver keeps resolved slot **payloads** across frames as well as its
route and intern decisions. Off, it keeps only the decisions. Measured on the
classic ESP32 (`fw-esp32v3`, 110 KB arena, `projects/test/quad-strips-v3`):

| | free heap | fps | `tick` |
|---|---|---|---|
| before the cache existed | 18,128 B | 13 | 69 ms |
| gate off (decisions only) | 18,144 B | 16 | 58 ms |
| gate on (decisions + payloads) | 9,776 B | 21 | 45 ms |

The decisions are 11 ms of the 24 ms for no heap at all; the payloads are the
remaining 13 ms for 8,368 B. On a part with room that is worth it and the gate
stays on. On the classic 8,368 B is roughly 90 LEDs of capacity, so
`fw-esp32v3` leaves it off and takes 16 fps.

Correctness does not depend on the setting — a miss recomputes — and
`cached_and_uncached_resolution_agree_frame_for_frame` runs the differential
across all three modes.

Do not read `node-shader` as a step toward making the GLSL JIT compiler
itself opt-in on `lpc-engine`/`lpa-server` — see the hard rule in
`AGENTS.md` ("Make the compiler an opt-in feature ... STOP. You are about to
break the product."). Disabling `node-shader` removes the *Shader* and
*ComputeShader* node runtimes only.

**What each gate is worth in flash**, measured against the C6
(`fw-esp32c6`) image at 2,864,496 B:

| Gate | Saving | Notes |
|---|---|---|
| `node-shader` | 84,816 B | Shader + ComputeShader |
| `node-fixture` | 73,600 B | the only gate that drops a dependency (`lpc-mapping`) |
| `node-fluid` | 23,824 B | |
| `node-playlist` | 14,304 B | |
| `node-radio` | 12,240 B | |
| `node-button` | 6,704 B | |
| `node-power-button` | 5,824 B | measured 2026-09-22 against a 2,434,080 B image, after the table above |
| `node-texture` | 3,856 B | |
| `node-clock` | 3,488 B | |

**Savings are not additive — do not sum this table.** Everything off
measured 902,224 B against a naive sum of 966,048 B: `lp-shader` stays
linked as long as any *enabled* node uses it (fixture, fluid), so gating the
shader node alone does not drop `lp-shader` — fixture and fluid keep it
alive. A gate's worth depends on its dependency closure at the time of
measurement, not on adding rows in this table.

**A disabled node reports itself.** The project still loads —
`engine::project_loader`'s attach loop falls back to
`CorePlaceholderNode::new_leaf(kind)` for a gated-off kind — and the node
attaches, produces nothing, and consumes nothing. Since 2026-08-01 it also
**says so**: the placeholder's `runtime_status()` returns
`NodeRuntimeStatus::Unsupported("node kind X is not included in this
firmware build")`, adopted at attach time so it rides the first tree delta,
and the resolve path's `ProduceResult::Unsupported` arm names that cause
instead of the old, misleading *"does not produce slot"*. Studio renders it
as an error-toned "Not on this device", with the node's pane body replaced
by a hazard-striped "not supported on this device" surface. Only kinds that CAN be gated report it —
the project root is a placeholder too, and `LpFeature::for_node_kind` is the
line. See
[`docs/adr/2026-08-01-capability-reporting-on-hello.md`](../../docs/adr/2026-08-01-capability-reporting-on-hello.md)
and the retired
[`docs/debt/firmware-capability-reporting.md`](../../docs/debt/firmware-capability-reporting.md).

**The trap** — the compiler will not catch this: any crate depending on
`lpc-engine` (or `lpa-server`, which forwards these same eight gates — see
`lp-app/lpa-server/Cargo.toml`) with `default-features = false` gets **no
node runtimes at all** unless it lists the gates it wants. `default =
[...]` only applies to a consumer that takes the crate's defaults; a
consumer that opts out is on its own. `lpa-server` and `fw-emu` were both
silently affected the moment these gates landed (2026-07-30) — `lpa-server`
briefly hard-coded all eight directly on its `lpc-engine` dependency line as
an emergency fix, which made them unreachable from firmware; `fw-emu` needs
the same explicit list today because it depends on `lpc-engine` directly.
Anyone adding a ninth node gate here must add it to both of those dependency
declarations (or their forwarding features) too.

**The far bigger lever is not in this crate.** `lp_gfx::NullGraphics` —
saving 743,216 B, roughly 8x the largest node gate above — because
`LpServer` requires an `Arc<dyn LpGraphics>`, and the real backend links the
whole on-device JIT compiler regardless of which node gates are on. It sits
behind `lp-gfx`'s off-by-default `null-backend` feature, so a firmware opts
in at its own dependency line. See
[`lp-gfx/README.md`](../../lp-gfx/lp-gfx/README.md#backend-doctrine).

### Validating a gate configuration

The gates are all default-on, so no ordinary build ever compiles a gate-off
configuration and one can rot unnoticed:

```bash
just check-lpc-engine-gates
```

builds every gate individually off, plus all-off, with `--all-targets -D
warnings`, and runs `disabled_node_kind_still_loads_project` — the
missing-node contract test, which exists only in a gate-off build and is run
nowhere else. `--all-targets` is load-bearing: the first version of this
matrix checked only the lib and missed test code referencing gated node
types entirely.
