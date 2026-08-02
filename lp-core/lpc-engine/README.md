# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: loaded project runtime state, node trees,
resolution, bindings, runtime state slot roots, and the boundary between
shader/runtime values and portable model or wire values.

**Runtime spine:** `engine::Engine` is the core runtime owner for the new
demand-driven path. It owns the `NodeTree`, engine-level `Resolver`, artifact
store, frame state, slot shape registry, runtime buffers, and demand roots.

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
| `node-texture` | 3,856 B | |
| `node-clock` | 3,488 B | |

**Savings are not additive — do not sum this table.** Everything off
measured 902,224 B against a naive sum of 966,048 B: `lp-shader` stays
linked as long as any *enabled* node uses it (fixture, fluid), so gating the
shader node alone does not drop `lp-shader` — fixture and fluid keep it
alive. A gate's worth depends on its dependency closure at the time of
measurement, not on adding rows in this table.

**A disabled node is silently inert.** The project still loads —
`engine::project_loader`'s attach loop falls back to
`CorePlaceholderNode::new_leaf(kind)` for a gated-off kind — and the node
attaches, produces nothing, and consumes nothing. Nothing on the device or
in the studio reports why. This is a deliberate, scoped-down stopgap, not an
oversight; see
[`docs/debt/firmware-capability-reporting.md`](../../docs/debt/firmware-capability-reporting.md)
for what makes it acceptable now, what makes it unacceptable once boards
genuinely differ in capability, and the real fix (general firmware
capability reporting).

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
