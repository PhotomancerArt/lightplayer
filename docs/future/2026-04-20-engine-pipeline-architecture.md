# Engine pipeline architecture: point fixtures + functional effects

## Status: pending architectural decision

Captured 2026-04-20 after a perf exploration ([fixture-render-perf
plan][plan]) showed the easy wins in `FixtureRuntime::render` are
exhausted (~43k cycles saved by phase 01; phases 02 and 04 regressed
and were reverted). Further meaningful gains in the engine pipeline
require an architectural change, not micro-optimization. This doc
captures the thinking before we make that call.

Sister doc on the orthogonal compiler-side track:
[`2026-04-20-middle-end-optimization.md`](2026-04-20-middle-end-optimization.md).
That work is purely additive and stays "deferred post-release"; this
doc is the call that *might* move.

[plan]: ../plans-old/2026-04-19-fixture-render-perf/

## Context

### Performance evidence

`p0-baseline` profile (esp32c6, `examples/perf/fastmath`,
`__render_texture_rgba16` per tick):

| layer | inclusive % | what it does |
| --- | --- | --- |
| `FixtureRuntime::render` | 97.4% | top of the per-tick pipeline |
| `ensure_texture_rendered` + `get_texture` | 75.4% / 75.7% | run shader into texture buffer |
| `ShaderRuntime::render` + JIT shader body | 74.7% / 74.3% | per-texel shader execution |
| `[jit] __render_texture_rgba16` | 73.6% | per-pixel rasterization driver |
| `__lp_lpfn_psrdnoise2_q32` | 41.7% | builtin (other agent's territory) |
| `Rgba16Sampler::sample_pixel` | 3.9% | reading the texture in the fixture |

For the simple "one strip, one shader, one fixture" case we run the
shader over the **whole texture** to produce pixels we then sample at
~one point per lamp. The texture is mostly throwaway. If the shader
were called point-wise per lamp, work would scale with `lamps`, not
with `texture_w * texture_h`. That's the architectural lever — much
larger than any micro-opt available inside `FixtureRuntime::render`'s
14.8% self.

### Product framing

The lightplayer pitch is **"Shadertoy driving your LED art"** — modular
GLSL effects (lpfx modules) writing into fixtures with hardware
correction, sent to output devices. That pitch is non-negotiable.

The previous-generation app (lp2014, `~/dev/personal/lightPlayer`)
targeted the *upper-right* of the matrix below: complex stateful
effects on polygon-rich fixtures at ~30k LEDs, on a powerful CPU-only
host machine. It worked well for what it was built for, but required a
real machine.

lp2025 was designed at the *upper-right too*, with the bet that "build
what you want first, optimize later" + GLSL semantics + microcontroller
JIT would scale *down* to small installs as well as up to wgpu hosts.

The current trigger: rainbow.glsl runs at ~30 fps on esp32 with
**zero headroom** — no audio, no network, no UI. By the user's own
"build, then optimize" rule, the optimize trigger has fired.

### Targets

esp32 stretch: 16×16 @ 30 fps with these effect classes:

- noise / functional shadertoy-style
- fluid (MSAFluid)
- real-time audio
- video (ideally)

Larger machines (wgpu): 100k LEDs comfortably, lp2014-class.

## The 2×2 matrix

Two orthogonal axes drop out of the pipeline analysis:

|  | **Functional effect** (no self-feedback) | **Stateful effect** (reads its own previous frame) |
| --- | --- | --- |
| **Point fixture** (each lamp is a `(u, v)` sample) | Strip + shadertoy. Skip texture entirely. Evaluate shader once per lamp. **The 75% inclusive win.** | Rasterize a small feedback texture, sample at one point per lamp. |
| **Polygon fixture** (each lamp is a coverage region with weights) | Big-art, no feedback. Evaluate per polygon sample point, accumulate. (Or: rasterize then sample, when many lamps share a region.) | Today's path. Rasterize feedback texture, sample at polygon sample points, weight-accumulate. |

Today the engine implements **only the lower-right cell**, and forces
every project into it. That's the architectural mismatch.

The two axes are independent:

- **Point vs polygon** is a *fixture* property. It describes hardware
  geometry: a WS2811 strip is a sequence of points; a custom
  install with diffuser tiles is polygons. The user doesn't pick
  this in a settings menu — it falls out of describing the rig.
- **Functional vs stateful** is an *effect* property. It describes
  whether the shader reads from a feedback sampler. The compiler
  can detect it.

## lp2014 → lp2025 in the matrix

| era | typical project | matrix cell | host |
| --- | --- | --- | --- |
| lp2014 | 30k-LED big-art install, MSAFluid, custom fixtures | upper-right (polygon + stateful) | beefy CPU-only desktop |
| lp2025 today | every project, including 60-LED strips | forced upper-right | esp32 → 100k via wgpu |
| lp2025 target | strips on MCU, install-class on host, both with shadertoy | full matrix | esp32 + wgpu, both backends |

The product story improves with the matrix split: a strip user
describes a strip, gets the cheap path, can write functional effects
just like Shadertoy. An install user describes polygons, gets the
expressive path, can use feedback for fluid sims. Same `.shader`
syntax. Different evaluator under the hood, picked by fixture geometry
and shader analysis.

## Three options on the table

| option | what it is | per-effort win | risk |
| --- | --- | --- | --- |
| **A. Architectural split** (this doc) | Add point fixtures; later add functional/stateful detection | Big — drops 75% inclusive cost on the common case | One-time refactor; sticky; baked-in if delayed |
| **B. Middle-end opts** ([sister doc][middle]) | LICM, const-divisor, store-to-load forwarding, inliner | Modest — 15–30% across the board, every shader | Low; deferrable; pure compiler work |
| **C. Defer** | Build features, revisit perf when forced | Zero now | Features bake polygon assumptions; (A) gets harder |

[middle]: 2026-04-20-middle-end-optimization.md

These are not mutually exclusive — (A) is architectural, (B) is
additive optimization. (B)'s value is the same whenever it lands. (A)
is path-dependent: every feature that ships before (A) bakes in
polygon assumptions across mappings, scene loader, fixture state, dev
UI, and project format.

## Recommendation: scoped (A) before features bake polygon assumptions

Specifically: **point fixtures + stateless-only constraint**, defer
the full functional/stateful detection until a real use case demands it.

Phase 1 (the cheap cut, ~80% of the win for ~30% of the cost):

- Add `LampGeometry::Point { uv: (Q32, Q32) }` alongside today's
  polygon `MappingCell`.
- Point fixtures call the shader directly per lamp via a per-lamp
  uniform write (or equivalent). No texture allocation. No sampler.
  No accumulator.
- Restriction: shaders bound to point fixtures must be stateless (no
  feedback sampler binding). Compile-time error if violated. This
  defers the functional-vs-stateful detection problem.
- Polygon fixtures: completely unchanged.
- Stateful effects: completely unchanged (must use a polygon fixture).

Phase 2 (defer indefinitely, build only when justified):

- Functional-vs-stateful detection on shaders.
- Polygon fixtures evaluated point-wise when the shader allows
  (eliminates the texture stage for "polygon fixture + functional
  effect").
- Stateful effects in point fixtures via per-lamp inter-frame state
  buffer.

The Phase 1 cut deliberately matches a hardware reality (point
fixtures = strips, the WLED-killer use case) without forcing a new
user-visible knob. It opens the door to Phase 2 without committing to
it.

## Why this is preferred over (B)-only

Two arguments:

1. **Architecture is path-dependent in a way optimization isn't.**
   Every feature shipped against polygon-only assumptions makes (A)
   more expensive later. (B) doesn't have that property — middle-end
   passes can land any time and pay the same.
2. **Per-target needs split match the proposed architecture.** Of
   the four esp32 stretch targets:
   - noise / functional → big win from Phase 1
   - audio → big win from Phase 1
   - video → big win from Phase 1
   - fluid → no help from Phase 1 *or* Phase 2 (it stays on the
     polygon-or-stateful path)
   3 of 4 benefit. Fluid is the open question — see below.

## The fluid risk

MSAFluid on esp32 may not fit at any reasonable resolution regardless
of (A) *or* (B). A 16×16 fluid sim is 256 cells × N solver iterations
× per-cell math — that's intrinsically a lot of work, and it lives
entirely in the polygon-or-stateful path which the architecture split
doesn't help.

If fluid doesn't fit on esp32 even after (A) + (B), the stretch goal
list needs to be honest: fluid becomes a "wgpu only" feature. That's
fine, but it has to be said.

**Worth measuring before committing to (A):** port a stripped-down
fluid solver as a `.shader`, profile it on esp32 *without any
fixture*, see if the raw shader budget alone fits. That data point
tells us whether "fluid on esp32" is real or aspirational.

## Open questions

1. **Multi-effect compositing.** Does "stateful" need to also cover
   shared inter-effect state — effect A reads effect B's output,
   classic shadertoy multi-buffer? If yes, "stateful" is a per-graph-
   edge property, not a per-effect property. Probably defer until a
   real multi-effect use case shows up.
2. **wgpu point evaluation primitive.** On wgpu, point fixtures could
   either dispatch a fragment shader with one fragment per lamp
   (point-list draw), or a compute shader writing channel buffers
   directly. Both work; pick later when the wgpu backend matures.
3. **Hardware corrections in point fixtures.** Gamma, brightness,
   color order all stay — they're cheap and live downstream of shader
   evaluation regardless of fixture geometry.
4. **Project file format.** Adding `LampGeometry::Point` is an
   additive change, but project files written with point fixtures
   won't open in older builds. Versioning is cheap; just call it out.
5. **lp2014 architectural lessons not yet pulled in.** lp2014 lived
   in the upper-right for a decade and presumably learned things
   about polygon mapping ergonomics, accumulator weight tuning, and
   fixture authoring. Worth a focused look before phase 1 lands, not
   urgent before this doc.

## Why the answer might be wrong

- **Headroom budget doubts.** If the real esp32 budget after (A) is
  *still* not enough for audio + network + simple effect, then (A)
  bought us nothing on the critical path and we should have done (B)
  + better builtins instead. Mitigation: actually measure the
  point-fixture path on a prototype before committing to the full
  refactor.
- **(B) might be enough.** Const-divisor specialization plus LICM
  could plausibly drop 20–30% across the board. If shaders shrink
  enough that texture rasterization is no longer 75%, the case for
  (A) weakens.
- **Phase 1 is still real work.** Adding a parallel evaluation path
  through `FixtureRuntime` / scene loader / lpvm dispatch is not a
  weekend project. Cost estimate is honest about ~80% of full (A)'s
  win for ~30% of the cost, but ~30% of "large refactor" is still a
  refactor.
- **WLED-killer framing might be premature.** The "strip + shadertoy"
  pitch is compelling but unproven in the field. If actual users want
  install-class first, then (A)'s win goes to a use case that doesn't
  exist yet.

## Suggested ordering when we pick this up

1. **Measure fluid on esp32 first** (1 day): minimal MSAFluid `.shader`,
   no fixture, profile on esp32 to find the raw budget. This decides
   whether fluid stays on the esp32 stretch list.
2. **Prototype Phase 1 evaluation path** (1–2 days, throwaway): wire a
   point-fixture eval that bypasses texture in `FixtureRuntime`,
   profile the same `examples/perf/fastmath` shader against it.
   Confirms the 75% inclusive savings is real before the refactor
   lands.
3. **Decide.** If (1) and (2) come out as expected: schedule Phase 1
   as the next architecture-touching work item, revisit before any
   big feature lands that touches fixtures.
4. **Leave middle-end deferred** as documented in the sister doc.
5. **Phase 2 stays speculative** until a real use case (probably
   "polygon fixture + simple shadertoy effect on wgpu") forces it.

## Pointers for whoever picks this up

- Per-channel render loop (where the easy wins were exhausted):
  [`lp-core/lp-engine/src/nodes/fixture/runtime.rs`](../../lp-core/lp-engine/src/nodes/fixture/runtime.rs)
  (search for the "do not 'optimize' the per-channel transform"
  warning).
- Mapping / accumulation that point fixtures would skip:
  [`lp-core/lp-engine/src/nodes/fixture/mapping/`](../../lp-core/lp-engine/src/nodes/fixture/mapping/)
- Texture stage that point fixtures would skip:
  [`lp-core/lp-engine/src/nodes/texture/runtime.rs`](../../lp-core/lp-engine/src/nodes/texture/runtime.rs)
  +
  [`lp-core/lp-engine/src/nodes/shader/runtime.rs`](../../lp-core/lp-engine/src/nodes/shader/runtime.rs)
- Profile that prompted this:
  [`profiles/2026-04-20T10-43-20--examples-perf-fastmath--steady-render--p0-baseline/report.txt`](../../profiles/2026-04-20T10-43-20--examples-perf-fastmath--steady-render--p0-baseline/report.txt)
- Plan that exhausted the easy wins:
  [`docs/plans-old/2026-04-19-fixture-render-perf/summary.md`](../plans-old/2026-04-19-fixture-render-perf/summary.md)
- lp2014 source for the polygon + stateful precedent:
  `~/dev/personal/lightPlayer` (out of repo).
- WLED for the "what we're trying to be better than" reference:
  `~/dev/opensource/WLED` (out of repo).
