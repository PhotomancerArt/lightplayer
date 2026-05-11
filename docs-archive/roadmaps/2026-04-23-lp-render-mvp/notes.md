# lp-render-mvp roadmap — notes

Working notes for the roadmap. Records scope, current state, open
questions, and answers as they get resolved.

## Scope (initial)

Validate the lp-domain model by **actually rendering** Visuals through
a working pipeline, with a real editor UI to tweak them. Goal: prove
the shape of `Pattern` / `Effect` / `Stack` / `ParamsTable` / `Slot` /
`Constraint` / `Binding` / bus is right *before* locking schemas
(M4–M6 of the lp-domain roadmap stay deferred).

In scope:

- A runtime engine for lp-domain artifacts: artifact loader / cache,
  per-Visual node instances, node graph wiring, shader texture-
  resource binding, a bus, and binding resolution.
- Rewire `lpfx` (and `lpfx-cpu`) to consume `lp-domain` types
  directly — **not a wrapper**. The `FxModule` / `FxManifest` /
  `FxInputDef` / `FxValue` parallel domain goes away; lpfx becomes
  the runtime crate that takes `Pattern` / `Effect` / `Stack` /
  ... and renders them.
- A web editor app with a real framework (Leptos or Dioxus —
  speculative future fit for a full lp-app, not just a demo
  scaffold). Not `lp-app/web-demo`; that gets superseded.
- Editor surfaces: file tree, TOML editor, preview pane, bus
  debugger, and a semantic param editor (Slot-driven UI, not a
  TOML textarea).
- Rendering backend: lpvm only. wgpu deferred (see Q3).

Out of scope:

- M4–M6 of the lp-domain roadmap (schema-gen, migration framework,
  CI gates) — resume after this roadmap surfaces what should change
  in v1.
- lp-engine rewire (it stays on `lp-model`, serves firmware/server).
- Mobile/desktop deployment of the editor, even if the framework
  choice keeps the door open.
- A full Live show runtime (Live + Playlist land late or get
  punted to a follow-up).

## Current state of the codebase

### lp-domain — done through M3 of the lp-domain roadmap

- `Kind`, `Constraint`, `Shape` / `Slot`, `ValueSpec`, `Binding`,
  `Presentation` all in `lp-domain/lp-domain/src/`. `no_std + alloc`.
- All six Visual kinds (`Pattern`, `Effect`, `Transition`, `Stack`,
  `Live`, `Playlist`) as typed structs with serde + `Artifact` impl.
- TOML grammar for `Slot` (custom `Deserialize`).
- Canonical examples in `lp-domain/lp-domain/examples/v1/`
  (rainbow / fbm / fluid patterns; tint / kaleidoscope effects;
  crossfade / wipe transitions; psychedelic stack; main live;
  setlist playlist).
- `LpFs`-based loader stub (`load_artifact`, std-only).
- `Node` trait exists in `node/mod.rs` but is just an object-safe
  property-access surface — no graph executor, no instance type.
- `BindingResolver` is a trait shape only — no implementation.

### lpfx — exists, wraps lpvm, parallel domain

- `lpfx/lpfx/` is `no_std + alloc`. Owns `FxModule` (manifest +
  GLSL source), `FxEngine` / `FxInstance` traits, `FxManifest` /
  `FxMeta` / `FxResolution`, `FxInputDef` / `FxValue` /
  `FxPresentation` / `FxChoice`, `FxRenderInputs`, `TextureId`.
- `lpfx/lpfx-cpu/` is the host-side / wasm impl — owns one
  `LpsEngine<LpvmBackend>`, a texture pool, compiles GLSL via
  `lp-shader`, renders into `LpsTextureBuf`.
- Shape: one shader per `FxInstance`. No graph, no bus, no
  multi-node composition. Inputs are flat `(name, FxValue)` pairs.

This is the "parallel domain" that has to die. lpfx keeps its
runtime/engine surface; it gives up its schema/manifest/input model.

### lp-app/web-demo — minimal wasm-bindgen shim

- `lp-app/web-demo/Cargo.toml` + `src/lib.rs`. ~100 LOC.
- Calls `lps_frontend::compile` / `lower`, owns a `BrowserLpvmEngine`,
  exposes `compile_shader` / `render_frame` / `get_shader_memory`
  via wasm-bindgen.
- No framework, no UI components, no state model. Caller (HTML in
  `www/`) does everything.
- Not a basis to grow on. Roadmap supersedes it.

### lp-engine — separate, untouched

- `lp-core/lp-engine/` is the firmware/server runtime. `no_std`,
  uses `lp-model` (the old transport-conflated domain), has
  `ShaderRuntime` / `OutputProvider` / `ProjectRuntime` / etc.
- Roadmap explicitly **does not touch lp-engine**. Its rewire is a
  future effort (lp-domain M6's Q8).

### Other relevant pieces

- `lpvm-wasm` browser engine works (web-demo proves it).
- `lp-shader` compiles GLSL → LPVM (`compile_px`, render).
- Pattern example uses `lpfn_hsv2rgb` — there's a GLSL preamble
  somewhere in lpfx land that registers builtins. Worth tracing
  during M1.

## Open questions

Numbered in ask order. Each captures the question, current-state
context, and a suggested answer. Answers get appended after each
question is resolved.

### Q1: Web framework — Dioxus or Leptos?

User suggested either; leans Dioxus for mobile-friendliness later.

- **Dioxus**: React-style component model, `dx serve` dev loop,
  first-class web/desktop/mobile targets, growing component
  ecosystem (`dioxus-components`, freya for desktop). VDOM model
  (cheap to reason about). Mobile story is the strongest of the
  Rust UI options. SSR exists but is less polished than Leptos.
- **Leptos**: fine-grained reactivity (signals, no VDOM), excellent
  SSR/hydration story, `cargo-leptos` build tool. Ecosystem is
  newer but high quality. No first-class mobile — would need
  Tauri or similar wrapper.

For an editor (interactive, stateful, not SEO-sensitive), both work.
The decisive factor is whether mobile is real near-term goal.

**Suggested answer:** Dioxus — user signaled mobile interest,
mobile/desktop future is real for a light-show editor (think
on-stage tablet), and the dev experience (`dx serve` hot reload) is
better suited to fast UI iteration on novel param-editor widgets.
Accept slightly weaker SSR (we don't need it for an editor).

**Resolved:** Dioxus. Mobile is a strong use case, so the mobile-target
story decides it.

### Q2: lpfx evolution — strip in place, or new crate?

User explicitly said "lpfx to actually use the new domain models,
not a wrapper". Two ways to land that:

- **Option A — strip lpfx in place.** Delete
  `FxModule` / `FxManifest` / `FxInputDef` / `FxValue` /
  `FxPresentation` / `FxRenderInputs` / `defaults_from_manifest`.
  Keep `FxEngine` / `FxInstance` traits + `TextureId` + the
  `lpfx-cpu` impl. Make them consume `Pattern` / `Stack` /
  `ParamsTable` / `Slot`. Existing callers (web-demo, tests) get
  ported. lpfx stays the runtime crate; no new crate name to
  learn.
- **Option B — new `lp-runtime` crate, deprecate lpfx.** Cleaner
  break, but two crate names to introduce + lpfx becomes dead
  weight that has to get deleted later.

**Suggested answer:** Option A. lpfx is "brand new and not
load-bearing" (per lp-domain overview); the engine trait surface
is generic enough to keep. Less churn than the rename.

**Resolved:** Option A. lpfx becomes **the implementation of the
visual subsystem of Lightplayer** — consumes lp-domain types
directly, owns the runtime spine (engine, node instances, node
graph, bus, bindings, LFO and other procedural signal nodes).

Crucial framing the user added:

- `lp-domain` is the *model*; `lpfx` is the *implementation*. lpfx
  supersedes the parallel-domain bits in itself.
- **Long-term:** lp-engine will use lpfx directly as part of the
  larger ecosystem (with rigs, fixtures, outputs). lpfx is the
  visual subsystem; lp-engine is the system that composes it with
  I/O.
- **Near-term:** a lightweight **editor app** wraps lpfx, provides
  the runtime hosting (event loop, surface), supplies synthetic
  bus inputs (where I/O would be), and displays output in a
  preview pane.
- **Why this layering works:** the bus/binding concept cleanly
  separates visuals from I/O. The editor can drive bus channels
  synthetically without any of the rig/fixture/output machinery
  that lp-engine eventually adds. Visual subsystem can be
  developed and validated in isolation.
- **Editor design goal:** later embeddable in (or evolvable into) a
  full `lp-studio` app that talks to `lp-server`. Implication for
  the editor architecture: state model should not assume local
  ownership of the lpfx instance; the bus is the abstraction
  boundary that survives switching between "local lpfx in the
  same process" and "remote lpfx via lp-server".

### Q3: wgpu in or out of this roadmap?

Earlier chat established lpvm covers browser (via `lpvm-wasm`) and
device. Editor preview pane works on lpvm.

- wgpu adds: validation of lp-domain → "any shader backend"
  translation; potentially faster host-side preview; future-facing
  if we ever target WebGPU directly.
- wgpu costs: a real second backend implementation, GLSL → WGSL
  or naga lowering, doubles testing surface. None of it adds
  signal about whether the *domain model* shape is right.

**Suggested answer:** Out. lpvm only in this roadmap. Add wgpu as
a separate roadmap if/when motivated by editor performance or
backend portability.

**Resolved:** Out for this roadmap, but a **real next step**, not
a vague "someday."

Concrete UX motivation captured for the next roadmap:

- Real-time previews of a *library* of visuals running concurrently —
  every effect in the library rendering on top of the current stack
  to show what each would look like applied; every pattern in the
  library rendering against current bus values to show what each
  would do at this moment.
- lpvm-wasm can simulate one visual at reasonable speed, but it
  can't render a library of them in parallel. wgpu (GPU
  parallelism) makes the library-preview UX cheap; lpvm makes it
  impossible.
- This is the central reason lpfx needs a wgpu backend, and it's
  the deliverable that should anchor the wgpu roadmap when it
  lands.

Implication for this roadmap's design: keep the renderer surface
in lpfx **backend-agnostic** from the start — the per-shader
"compile + render" surface (ex-`FxEngine`/`FxInstance`) should be
trait-shaped enough that a wgpu backend can drop in next to lpvm
without restructuring the graph layer above it. Don't bake LPVM
specifics into the visual graph code.

### Q4: LFO scope — one-off node or first of a category?

User mentioned "maybe figure out how to bring in an LFO node ... to
exercise the bus and bindings." LFO = low-frequency oscillator,
produces a time-varying scalar (sine/triangle/saw at some
frequency/phase). Procedural signal source.

- **One-off**: a single hard-coded `LfoNode` that publishes one
  bus channel. Fastest to build, exercises bus/binding minimally.
- **First of a category**: a "signal-generator" Visual kind
  (alongside Pattern/Effect/...), authored as TOML, with shape +
  shader / DSP code + `[output]` declaration. Forces a new
  artifact kind, output declarations, and starts to formalise
  what a "non-rendering" Visual looks like.

**Suggested answer:** One-off in this roadmap. LFO as a
not-authored-as-TOML built-in node, configured via constructor
args (`waveform`, `frequency`, `output_channel`). The category
question (do we need a `Signal` artifact kind?) is real but is
**signal from this roadmap** — punt the answer until we've felt
the pain.

**Resolved:** **Punt LFO entirely.** It's important but it isn't
*lpfx* scope. LFO (and signal generators in general) belong in a
**Show** — the higher-level construct that lp-engine eventually
composes around lpfx (Show = visuals + rigs + fixtures + outputs +
signals + scheduling). lpfx is purely the visual subsystem.

Implication: M1 still needs to exercise bus + bindings, but we use
**editor-driven synthetic bus inputs** instead of LFO. The editor's
job is to stand in for "the rest of Lightplayer," which means
synthesising bus values (`time`, slider-driven scalars, maybe a
synthetic "audio level" the user drags around). This is already in
the editor scope and naturally validates bus + binding without
inventing in-lpfx signal generators.

**Future work captured:** Signal generator nodes (LFO, envelope
follower, audio analysis derivatives, MIDI/OSC inputs, schedulers)
land in the Show roadmap, not in lpfx. They publish to the bus,
which lpfx consumes; the bus is the boundary that keeps the visual
subsystem clean.

### Q5: First-render target — Pattern alone, or Pattern + LFO + bus?

Earlier chat suggested Pattern-only as the smallest valuable MVP
("no bus, no graph, no cache"). User's M1 explicitly bundles bus +
bindings + LFO into the runtime milestone, which means Pattern
alone wouldn't exercise them.

- **Pattern-only first**: tiny, but doesn't validate the most
  novel parts of the model (bus, bindings, graph wiring).
- **Pattern + LFO bound to a Pattern param via bus**: forces the
  full runtime spine on day one. Still much smaller than Stack
  (no texture pipeline between effects, no input chain). The
  graph is just two nodes with one channel between them.

The user's framing is closer to the second; on reflection that's
right — Pattern alone is *too* trivial and a Pattern-with-bound-LFO
demo is a much better "shape is right" smoke test.

**Suggested answer:** Pattern + LFO + bus is the M1 deliverable.
Stack arrives in M2. Effect drops out as "what Stack composes"
in M2. Live / Transition / Playlist later (likely punted past
this roadmap).

**Resolved:** **Pattern alone, no bus, in M1.** One thing at a
time. User more interested in Stack + Effect next than in bus.

Revised milestone shape:

- **M1:** lpfx rewire + Pattern runtime + minimal editor (one
  pattern loads, renders, params expose as flat sliders driven
  directly — no bus, no binding, no graph beyond a single node).
  Validates: lpfx ↔ lp-domain integration, `Pattern` /
  `ShaderRef` / `ParamsTable` / `Slot` / `Constraint`, and the
  editor framework choice.
- **M2:** Stack + Effect, **Visual-input only** (`[input] visual =
  "..."`). Forces the texture pipeline (ping-pong between
  effects), multi-node graph executor, artifact cache (Stack
  references Pattern by path). Still no bus — the
  `psychedelic.stack.toml` example uses `visual` inputs, not
  `bus` inputs, so this is achievable without bus machinery.
- **M3:** Bus + bindings. Editor synthesises bus values; Pattern
  params can be bound to bus channels; Effect / Stack `[input]
  bus = "..."` works. Bus debugger lands here. Validates:
  `Binding`, `BindingResolver`, the cascade rules from
  `quantity.md` §8, and editor-as-synthetic-environment framing.
- **M4:** Semantic editor. Rich `Slot`-driven widgets (range →
  slider, choices → dropdown, color kind → color picker,
  frequency → log slider, etc.), file tree, multi-artifact
  workspace, save/load via virtual fs.
- **M5:** Cleanup + verification. Delete `lp-app/web-demo`,
  delete the `noise.fx` example if superseded, update design
  docs, run all gates.

Live / Transition / Playlist explicitly out of this roadmap. They
land after the core Pattern / Stack / Effect / Bus story is solid
and editor UX has been felt with hands.

### Q6: Editor app crate location

`lp-app/web-demo` is the current home of browser shim code.

- Replace `lp-app/web-demo` with `lp-app/editor` (kebab in path,
  `lp-editor` as crate name). Web-demo gets deleted (or kept as
  a smoke test of bare `lpvm-wasm`).
- Keep both alive in parallel for the roadmap, delete web-demo
  in the cleanup milestone.

**Suggested answer:** New crate `lp-app/editor` (`lp-editor`).
Delete `lp-app/web-demo` in the final cleanup milestone — its
only consumer is the existing demo HTML.

**Resolved:** **`lp-app/lp-studio`, crate `lp-studio`.** Since
we're committing to a full framework (Dioxus), now is the right
time to stand up the main app crate under the name it will keep
long-term, even though early milestones only exercise the visual
subsystem. lp-studio in M1–M4 *is* the lpfx editor; later it
grows to compose Show / rigs / fixtures / scheduling on top.

Web-demo deleted in M5 cleanup.

### Q7: Artifact resolution in the browser — virtual fs vs static serve?

`lp-domain::load_artifact` takes an `LpFs` handle. Browser has no
real fs.

- **Static serve**: examples ship as static files, fetched via
  `fetch` / a `WebLpFs` impl that wraps HTTP GETs. Simple, but
  read-only and doesn't model "user editing".
- **Virtual fs (`LpFsMem`)**: editor seeds an in-memory fs from
  bundled examples, edits live in memory. Save = serialize + (a)
  download via blob, (b) push to local fs via File System Access
  API, or (c) ignore for MVP.
- Hybrid: static serve at boot, copy into LpFsMem on first edit,
  read from LpFsMem after.

**Suggested answer:** Virtual fs (`LpFsMem`) seeded from a
bundled tarball/JSON of `examples/v1/`. Save is just "download
this file" via blob URL in MVP. File System Access API integration
is a stretch goal in the cleanup milestone.

**Resolved:** **B + localStorage persistence.** UX shape:

- Boot: if localStorage has a saved project, load it; otherwise
  seed from a bundled example.
- Edits are continuously mirrored to localStorage so a refresh
  doesn't lose state.
- Single active project for now (multi-project punted).
- A visible **"reset to example"** action that clears localStorage
  and re-seeds.
- Future expansion (multi-project, FSA-backed real fs, server
  sync) is additive on top of this layering.

Implementation sketch (resolve details in M1 design phase):

- `LpFsMem` (already in `lpfs`) is the runtime fs the loader sees.
- A `LocalStorageBackedFs` wrapper (or a sync layer alongside)
  reads on boot, writes on every mutation, debounced. Could be a
  trait impl over `LpFs` that delegates reads/writes to LpFsMem
  and persists deltas, or a simpler "snapshot the whole fs to
  localStorage on mutation" since the project will be small.
- localStorage quota (~5–10 MB per origin) is plenty for TOML +
  GLSL text. If we ever want binary assets (textures, audio),
  switch to IndexedDB — out of scope for now.
- localStorage is synchronous; `LpFs` should already be sync per
  its `no_std` design (verify in M1).

### Q8: Semantic editor — scope and where it lives

User wants params shown in nicer UX (sliders for ranges, dropdowns
for choices, color pickers for color kind, etc.). The natural unit
is "render a `Slot` tree as a UI."

- A `Slot`-driven widget library is a real piece of work. It's
  also the most reusable thing in the editor — every artifact
  kind's params come down to a `Slot` tree (`ParamsTable.0` is
  always a `Slot`).
- Smallest thing: scalar widgets driven by `Constraint`
  (`range` → slider, `choices` → dropdown, no constraint →
  number input). `Presentation` hints layer on top
  (`color` kind → color picker, `frequency` → log-scale slider).

**Suggested answer:** Build the Slot widget library incrementally.
M1 needs only flat `[params]` tables of scalars (rainbow.pattern
is `speed: frequency`, `saturation: amplitude`); rich widgets land
in the dedicated semantic-editor milestone. Library lives in
`lp-app/editor` as a `widgets/` module first; refactor to its own
crate if/when we need it elsewhere.

### Q9: Bus debugger scope

User listed bus debugger as a deliverable. Useful for "does the LFO
binding actually drive the param?"

- **Trivial**: list of `(channel, current value, kind)` tuples,
  refreshed each frame. No history, no plotting.
- **Richer**: time-series plot per channel, kind-aware formatting
  (color swatches for color channels, audio waveform for audio
  channels).

**Suggested answer:** Trivial in M1 (text grid). Plotting layered
on later if it's actually useful — likely deferred indefinitely.

**Resolved:** Trivial bus debugger lands in **M3** (when bus
itself lands), not M1. Text grid: `(channel, current value, kind)`.

### Q10: Widget showcase — adopt or roll-our-own?

Storybook-equivalent for developing widgets in isolation. Two
real Dioxus options exist (Lookbook, dioxus-showcase) but neither
is blessed/active.

**Suggested answer:** Lookbook with fallback to roll-our-own.

**Resolved:** **Roll-our-own.** Don't adopt an inactive
dependency. Core functionality is small:

- One page per widget component, showing it in all useful states
  (slider with no constraint, with range, with step, with
  range+step; dropdown with 2/5/many choices; etc.).
- One overview page showing all widgets in a normal/default state
  for a glance check.
- Lives as `lp-app/lp-studio-widgets/examples/showcase.rs` (or
  similar) — sibling Dioxus app that imports the widget crate.

### Q11: Milestone shape

User suggested 3 milestones. With the resolved scope (Pattern
alone in M1, Stack/Effect in M2, bus in M3, semantic editor in
M4, cleanup), the natural shape is **5 milestones**. To be
confirmed below.

**Resolved:** **7 milestones**, with M1+M2 parallelizable. User
flagged M1 too big (rewire + runtime + UI is three separate
concerns); split into M1 (lpfx runtime, headless) + M2 (lp-studio
app core, no rendering) + M3 (pattern editor, integration).
Artifact cache moves into M1 (the runtime needs a loader anyway,
and the cache becomes the home for future bus / runtime concerns).

Final shape:

| #  | Milestone                 | Strategy        | Depends on  |
|----|---------------------------|-----------------|-------------|
| M1 | lpfx runtime              | C — full plan   | —           |
| M2 | lp-studio app core        | C — full plan   | —           |
| M3 | Pattern editor            | B — small plan  | M1 + M2     |
| M4 | Stack + Effect            | C — full plan   | M3          |
| M5 | Bus + bindings            | C — full plan   | M4          |
| M6 | Semantic editor           | C — full plan   | M5          |
| M7 | Cleanup + verification    | B — small plan  | M6          |

M1 + M2 can run in parallel (independent code surfaces). M3 is
the join. Everything else is serial.

## Design considerations surfaced (not questions, captured for design phases)

**Bus lives where?** Long-term, the bus connects Show-level signal
generators (LFO, audio analysis, MIDI/OSC, schedulers) to
lpfx-level visual consumers. The boundary should be a trait so
that:

- M3 lpfx provides a basic in-memory bus that the editor writes
  synthetic values into and visuals read from.
- Future Show layer (in lp-engine) can substitute its own bus
  impl that multiplexes signal-generator outputs.
- `lp-domain` already has the `BindingResolver` trait shape; we
  likely want a paired `Bus` trait (`read(channel) -> Option<...>`,
  `write(channel, value)`, `kind_of(channel) -> Option<Kind>`)
  in `lp-domain` with a basic `MemBus` impl in lpfx.

Resolve in M3 design.

**GLSL preamble (lpfn_hsv2rgb etc.).** Pattern examples reference
helper functions that aren't in the inline GLSL. lpfx today
prepends a preamble somewhere. Need to trace that during M1
design and decide if the preamble lives in lpfx or in lp-shader.

**Texture resources after shader texture access.** lp-shader now
owns `sampler2D` lowering and validation through `TextureBindingSpec`
and runtime `LpsTexture2DValue` uniforms. lpfx/domain own resource
source, routing, baking, allocation, and lifetime. For this MVP,
palettes and gradients are authoring recipes that materialize into
width-by-one `Rgba16Unorm` textures; shaders sample them via
`sampler2D` compiled with `TextureShapeHint::HeightOne`. M1 needs
the backend trait and texture manager to distinguish frame/output
textures from generated resource textures, because Patterns can use
palette/gradient resources before Stack/Effect arrives.

**Renderer ↔ Dioxus integration.** The wasm-side renderer needs to
draw into an HTML canvas. Probably an `HTMLCanvasElement` + JS
interop layer that lp-studio owns. Resolve in M1 design.

## Notes (resolved decisions, captured here as we go)

(see "Resolved" entries inline under each question)

## Notes (resolved decisions, captured here as we go)

(empty — populated as questions resolve)
