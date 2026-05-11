# lp-render-mvp roadmap

## Motivation / rationale

The lp-domain roadmap (`docs/roadmaps/2026-04-22-lp-domain/`) landed
the typed model — `Kind`, `Constraint`, `Shape`/`Slot`, `ValueSpec`,
`Binding`, `Presentation`, all six Visual artifact kinds with TOML
serde + canonical examples — through milestones M1–M3. M4–M6 of that
roadmap (schema codegen, migration framework, CI gates) would lock
the model in for the long haul.

**That commitment is premature.** The model has only ever been
serialized and round-tripped, never *used* to drive a render. Until
we exercise it end-to-end — load a Pattern, instantiate it as a
runtime node, route values through a bus, render pixels, edit
parameters live — we don't know whether the shape is right. The
chance that nothing changes between "looks correct on paper" and
"survives first contact with rendering" is low. Locking schemas
first would force fake migrations of our own design corpus through
the migration framework's CI gates, with no users to benefit and
all the churn for ourselves.

This roadmap pivots: **defer M4–M6 of lp-domain; build a working
visual subsystem and an editor for it; then come back and lock
schemas with confidence.**

Two things ride alongside the validation goal:

1. **lpfx becomes the implementation of lp-domain's visual
   subsystem.** Today lpfx has its own parallel domain
   (`FxModule` / `FxManifest` / `FxInputDef` / `FxValue`) — a
   manifest-driven single-shader executor. That parallel domain
   dies. lpfx evolves to consume `lp-domain` types directly: it
   loads a `Pattern` / `Effect` / `Stack`, instantiates a node
   graph, talks to a bus, renders. Long-term lpfx is what
   `lp-engine` will use as the visual subsystem when the broader
   ecosystem (rigs, fixtures, outputs, scheduling — the **Show**
   layer) gets composed; near-term, an editor wraps it.

2. **lp-studio is the editor crate**, and the framework choice
   (Dioxus) is also seeding the long-term application stack.
   lp-studio in this roadmap *is* the visual-subsystem editor;
   in the future it grows to compose Show / rig / fixture editors
   on top of the same framework. Web-demo (`lp-app/web-demo`)
   was a wasm-bindgen shim with no UI framework — it gets
   superseded.

The bus/binding concept is the architectural crowbar that makes
this work: **it cleanly separates visuals from I/O.** The editor
stands in for "the rest of Lightplayer" by driving bus channels
synthetically (sliders, manual values, a clock). lpfx sees the
same bus interface whether it's driven by an editor slider today
or by an LFO / audio-analysis / MIDI signal generator from the
future Show layer. Visuals are testable in isolation.

## Concrete near-term goals

1. lpfx demolition + rewire to lp-domain. The parallel
   `FxModule` / `FxManifest` / `FxInputDef` / `FxValue` types
   disappear; lpfx consumes `Pattern` / `Effect` / `Stack` /
   `ParamsTable` / `Slot` / `Binding` directly.
2. A real runtime in lpfx: artifact loader + cache, node instances
   per Visual, multi-node graph executor, in-memory bus,
   binding resolution, and shader texture-resource binding. Backend-
   agnostic per-shader trait so wgpu can drop in next to lpvm later.
3. A Dioxus web app (`lp-studio`) with localStorage-backed
   virtual fs, a roll-our-own widget showcase, a Pattern editor,
   a Stack/Effect editor, a synthetic bus driver, a bus debugger,
   and a Slot-driven semantic editor.
4. End-to-end editing flow: open an example, tweak params via
   widgets (or via the bus), see the preview re-render live, save
   to localStorage, refresh and resume.

## Architecture / design

### Crate layout

```
lp-domain/lp-domain/             # UNCHANGED — typed model already landed (M1–M3 of lp-domain roadmap)
  src/
    {kind,constraint,shape,value_spec,binding,presentation}.rs
    visual/{pattern,effect,stack,...}.rs
    artifact/load.rs             # std-only LpFs loader
    node/mod.rs                  # Node trait (object-safe)
    binding.rs                   # Binding + BindingResolver trait
  + new in M5: bus.rs            # Bus trait paired with BindingResolver

lpfx/lpfx/                       # REWIRE (M1) — the visual subsystem implementation
  src/
    lib.rs                       # was: re-exports FxModule/Fx*; becomes: re-exports runtime
    runtime/                     # NEW — engine, instances, graph executor
      engine.rs                  # ex-FxEngine evolved: takes Pattern/Effect/Stack
      instance.rs                # ex-FxInstance evolved: per-Visual node instance
      graph.rs                   # NEW — multi-node executor (M4)
      cache.rs                   # NEW — artifact cache
      bus.rs                     # NEW (M5) — MemBus impl of lp-domain Bus trait
      lfo.rs                     # NOT in this roadmap; lives in Show layer (lp-engine future)
    nodes/                       # NEW — per-Visual-kind node impls
      pattern.rs                 # M1
      effect.rs                  # M4
      stack.rs                   # M4
    texture/                     # NEW — TextureId stays; output pool + resource textures
    backend.rs                   # NEW — backend-agnostic per-shader trait
  REMOVED:
    manifest.rs, input.rs, parse.rs, defaults.rs, render_inputs.rs, module.rs
    (the parallel-domain types; lp-domain takes over)

lpfx/lpfx-cpu/                   # REWIRE (M1) — implements backend.rs trait via lpvm
  src/
    backend.rs                   # ex-CpuFxEngine adapted to new trait
    compile.rs                   # GLSL → LPIR → lpvm wiring (kept)

lp-app/lp-studio/                # NEW (M2) — Dioxus web app
  Cargo.toml
  src/
    main.rs                      # dx serve entrypoint
    app.rs                       # root component, router, layout
    fs/                          # localStorage-backed virtual fs
      mod.rs
      local_storage_fs.rs
      seed.rs                    # bundled examples → LpFsMem
    pages/
      pattern_editor.rs          # M3
      stack_editor.rs            # M4
      bus_debugger.rs            # M5
    runtime_bridge.rs            # M3 — wasm interop with lpfx + canvas
  assets/
    examples/                    # bundled snapshot of examples/v1/

lp-app/lp-studio-widgets/        # NEW (M2) — reusable Slot-driven widget library
  Cargo.toml
  src/
    lib.rs
    slider.rs                    # range/step/no-constraint variants
    dropdown.rs                  # choices
    number_input.rs              # no constraint
    checkbox.rs                  # bool
    color_picker.rs              # M6
    log_slider.rs                # M6
    dial.rs                      # M6 (Phase, etc.)
    composite.rs                 # M6 (struct/array Slot UI)
  examples/
    showcase.rs                  # roll-our-own widget gallery (M2)

lp-app/web-demo/                 # DELETE (M7) — superseded
```

### Data flow on render

```
Browser tab loads
    │
    ▼  lp-studio boot
LpFsLocalStorage   ◄─seed─  bundled examples/v1/   (or restore from localStorage)
    │
    ▼  user picks rainbow.pattern.toml
lp_domain::load_artifact::<Pattern>(&fs, path)   ──► typed Pattern
    │
    ▼  lpfx::Engine::instantiate_pattern(pattern)
PatternInstance (compiled GLSL via lpfx-cpu / lpvm)
    │
    ▼  per frame:
      ┌── Bus reads (M5+: editor synthesises values; M1–M4: direct param values)
      │
      ▼  PatternInstance.render(&bus) ──► LpsTextureBuf
      │
      ▼  blit to HTMLCanvasElement
display
```

### The bus as the architectural seam

```
M5+:

   lp-studio (editor)              lpfx (visual subsystem)
   ┌────────────────────┐          ┌─────────────────────┐
   │ slider: speed=2.0  │ ───┐     │ PatternInstance     │
   │ slider: hue=0.4    │ ───┼──►  │   reads "speed"     │
   │ time clock         │ ───┘     │   reads "hue"       │
   └────────────────────┘          │   reads "time"      │
                                   └─────────────────────┘
                                            ▲
                                            │
                                       MemBus (impl of lp-domain::Bus)

Future (Show layer in lp-engine):

   Show signal generators           lpfx (same code as above)
   ┌────────────────────┐          ┌─────────────────────┐
   │ LFO              ──┼─►        │ PatternInstance     │
   │ AudioAnalysis    ──┼─►   ──►  │   reads "speed"     │
   │ MidiInput        ──┼─►        │ ...                 │
   │ Scheduler        ──┼─►        └─────────────────────┘
   └────────────────────┘                   ▲
                                            │
                                  Show's bus impl (richer than MemBus)
```

The Bus trait is the contract; lpfx works against either implementation
without knowing which is hosting it.

### Backend-agnostic per-shader surface

The current `FxEngine` / `FxInstance` traits are 38 lines and shaped
right: "compile a thing into a runnable handle, render with per-frame
inputs." They survive the rewrite — renamed and re-typed to take
`ShaderRef` + uniform sets + texture binding specs instead of
`FxModule`. A wgpu impl can drop in next to the lpvm impl whenever
the wgpu roadmap launches. The graph layer above them is backend-
agnostic.

### Texture resources and palette/gradient strips

The texture-access roadmap landed the shader-side contract: GLSL declares
`sampler2D` uniforms, and the caller supplies a `TextureBindingSpec` per
sampler at compile time plus `LpsTexture2DValue` uniforms at render time.

With nested texture struct support, authored shader parameters now flow through
a single `params` struct:

```glsl
struct Params {
    float speed;
    sampler2D gradient;
};

uniform Params params;
```

The texture binding spec key is the canonical dotted path: `params.gradient`.
lpfx bakes palette/gradient recipes into height-one textures and binds them
as `params.gradient` values.

Graph-fed inputs (Effect upstream textures, bus textures) remain as top-level
resource uniforms outside `params`. The naming convention for these is
determined in M4 Stack/Effect design.

The runtime therefore has two texture lifetimes:

- **Frame/output textures:** render targets and Stack ping-pong
  buffers, reused frame-to-frame.
- **Resource textures:** generated or loaded inputs such as
  palette/gradient strips, synthetic bus textures, and future image
  assets. These need cache keys, invalidation when the authoring value
  changes, and an eventual free/reuse policy.

For this roadmap, generated palette/gradient strips stay text-backed:
TOML stores the recipe, not baked pixels. Binary image asset management
remains out of scope unless the storage layer moves beyond
localStorage.

### Discipline — what's enforced vs not

This roadmap intentionally does **not** add the schema/migration CI
gates from lp-domain M4–M6. Those land after this roadmap, when the
shape has been validated in use. What this roadmap does enforce:

- `cargo test -p lpfx -p lpfx-cpu` proves rendering works headless
  (M1 deliverable).
- Texture-access MVP coverage includes a height-one
  palette/gradient Pattern test, an Effect input sampler test, and
  a resource rebake/invalidation test before cleanup.
- `cargo build -p lp-studio` + `dx build` proves the editor compiles
  to wasm (M2+).
- `dx serve` boots lp-studio, shows widget showcase, persists fs
  through refresh (M2 acceptance).
- Each milestone's "Deliverables" section lists its acceptance
  smoke tests.

## Alternatives considered

- **Lock lp-domain schemas first (continue with lp-domain M4–M6).**
  Rejected: locks an unvalidated shape; forces fake migrations of
  our own design corpus through CI gates with no user benefit.
  Better to validate via use, then lock.
- **Build a new `lp-runtime` crate, deprecate lpfx.** Rejected:
  lpfx is "brand new and not load-bearing" (per lp-domain
  roadmap); the engine trait surface is generic enough to keep;
  evolving in place is less churn than the rename. lpfx becomes
  the visual subsystem implementation in the long-term ecosystem.
- **Bundle wgpu into this roadmap.** Rejected: wgpu doesn't add
  signal about whether the *domain model* is right; lpvm covers
  device + browser + Dioxus mobile-via-WebView. wgpu becomes its
  own roadmap, anchored on the **library-of-previews UX**
  (rendering many visuals concurrently, e.g. an entire effects
  library showing what each would look like applied to the
  current stack) — impossible on lpvm, easy on the GPU.
- **Build LFO into lpfx as a "first signal generator."** Rejected:
  signal generators (LFO, audio analysis, MIDI/OSC, schedulers)
  belong to the **Show** layer that lp-engine eventually adds
  around lpfx. Putting them in lpfx muddles the visual-subsystem
  boundary. Editor synthesises bus values instead.
- **Pattern + LFO + bus all in M1.** Rejected (was the initial
  suggestion): one thing at a time. M1 is just Pattern; bus is
  M5. The LFO removal (Show-scope) makes the smaller M1 scope
  honest — bus is no longer needed to exercise procedural
  signals because there *are* no procedural signals in lpfx.
- **Stack and Effect in separate milestones.** Rejected: they're
  conceptually paired and the example corpus
  (`psychedelic.stack.toml`) ships them together.
- **Leptos instead of Dioxus.** Rejected: mobile is a strong
  use case (on-stage tablets, console-side phones); Dioxus has
  first-class web/desktop/mobile targets, Leptos doesn't.
- **Static-serve examples in lp-studio.** Rejected: read-only
  breaks down by M2 ("edit the stack to point at a different
  pattern"). Virtual fs with localStorage persistence chosen
  instead — survives refresh, supports edits, easy "reset to
  example" affordance, easy to extend later (FSA API,
  multi-project, server sync).
- **Adopt Lookbook (or `dioxus-showcase`) as widget gallery.**
  Rejected: neither is blessed/active (Lookbook last update Sept
  2024; PR to integrate into Dioxus core was closed). Roll our
  own — one page per widget showing useful states, one overview
  page. Small surface; full control.
- **TOML editor in M1.** Rejected: M1 has the auto-generated
  widget panel; TOML side-by-side editor lands in M6's semantic
  editor where it belongs. For M1–M5 if you need raw TOML,
  export → edit in a real editor → re-import.
- **Don't extract a `lp-studio-widgets` crate.** Rejected:
  widgets need to be importable by both the app and the
  showcase (and any future testing harness). Separate crate
  from day one.

## Risks

- **Dioxus mobile reality vs marketing.** Dioxus advertises
  mobile support, but it's WebView-based on iOS/Android by
  default. The fast iteration is web; mobile is a real future
  target but probably needs more polish than Dioxus delivers
  today. Mitigation: this roadmap targets web-only; mobile
  becomes its own roadmap when actually needed. Framework
  choice is reversible if Dioxus mobile turns out unworkable.
- **Lookbook-style widget showcase rotting in our hands.**
  We're rolling our own — same risk applies but on us. Mitigation:
  keep it minimal (one page per widget, one overview), don't
  build framework abstractions over Dioxus; just call
  components directly with hard-coded variants.
- **localStorage quota** (~5–10 MB per origin) is fine for TOML
  + GLSL text, including palette/gradient recipes that bake into
  runtime textures. It breaks if we store binary assets (textures,
  audio) or baked texture bytes. Mitigation: scope this roadmap to
  text-only artifacts; switch to IndexedDB when binary assets arrive.
- **Bus design lands in M5 without the Show-layer constraint
  being concrete.** Risk that the in-memory `MemBus` impl
  doesn't match what the future Show bus needs. Mitigation:
  define the `Bus` trait in `lp-domain` (small, focused), keep
  the `MemBus` impl behind it, accept that Show may want
  topology features (multi-source merge, history, plotting)
  that motivate trait extension later.
- **Texture-resource lifetime during editing.** Generated
  palette/gradient strips and synthetic bus textures are not Stack
  ping-pong outputs. Repeated editor tweaks should rebake and replace
  cached resources rather than leaking a new LPVM allocation forever.
  Mitigation: M1 designs resource texture handles separately from
  frame/output textures; M3/M6 tests cover rebake/invalidation.
- **Pattern collapse during rewire surfaces lpfx assumptions
  that don't fit lp-domain.** The current `FxModule` is one
  shader + one manifest + one set of inputs. `Pattern` is one
  shader + a `ParamsTable` (a `Slot` tree, not a flat map).
  Risk: shape mismatch surfaces edge cases (defaults handling,
  `bind = { bus = "..." }` resolution, `kind = "instant"` →
  `time` channel default-binding). Mitigation: M1 design phase
  walks the rainbow → fbm → fluid examples through the new
  runtime explicitly; surfaces issues before code lands.
- **Renderer ↔ Dioxus integration surface (canvas, render
  loop pacing, wasm interop) is novel** for this codebase and
  has its own design surface. Mitigation: M3's small plan
  spends its first phase on this integration question; web-demo
  proves the wasm + canvas + lpvm path works in isolation, but
  fitting it into Dioxus's render loop is new.
- **GLSL preamble path (`lpfn_hsv2rgb`).** Pattern examples
  reference helper functions that aren't in inline GLSL. lpfx
  prepends a preamble somewhere today; surface it during M1
  design and decide if the preamble lives in lpfx or moves
  upstream into lp-shader. Could affect shader-source handling
  in `Pattern` / `Effect`.
- **Schema churn during validation.** This roadmap may surface
  changes to lp-domain types (Slot grammar tweaks, missing
  Presentation hints, Binding extensions). Mitigation: those
  changes land back in `lp-domain` directly during this
  roadmap, since M4–M6 of lp-domain (the immutability gates)
  are explicitly deferred until after this work. Edits to
  `examples/v1/` are free game.

## Scope estimate

Seven milestones; M1+M2 parallelizable (independent code
surfaces), M3 onwards serial.

| #  | Milestone               | Strategy        | Depends on  |
|----|-------------------------|-----------------|-------------|
| M1 | lpfx runtime            | C — full plan   | —           |
| M2 | lp-studio app core      | C — full plan   | —           |
| M3 | Pattern editor          | B — small plan  | M1 + M2     |
| M4 | Stack + Effect          | C — full plan   | M3          |
| M5 | Bus + bindings          | C — full plan   | M4          |
| M6 | Semantic editor         | C — full plan   | M5          |
| M7 | Cleanup + verification  | B — small plan  | M6          |

Strategy reasoning:

- **M1 (C):** lpfx parallel-domain demolition + lp-domain
  integration + new runtime spine + per-shader backend trait +
  artifact loader/cache + Pattern node + headless tests.
  Multiple architectural calls (where the bus trait lives, how
  the per-shader trait shape goes, how the cache keys, how the
  preamble path resolves). Phaseable.
- **M2 (C):** Dioxus skeleton + localStorage-backed virtual fs +
  widget crate (4–5 scalar widgets) + roll-our-own showcase +
  routing + base layout. Independent of M1. Lots of small
  decisions — page structure, widget API shape, fs sync
  strategy, build pipeline (`dx serve`, `dx bundle`).
- **M3 (B):** integration of M1 and M2. Wasm interop, canvas,
  render loop, "open pattern" UI, auto-generated widget panel
  bound to `ParamsTable`. Architectural calls already made;
  this is wiring + one or two integration questions. Small
  plan suffices.
- **M4 (C):** multi-node graph executor, ping-pong texture
  pipeline, Effect node, Stack node, artifact-cache cross-file
  resolution, stack.toml editor in lp-studio. Real graph
  topology and texture lifetime questions.
- **M5 (C):** `Bus` trait in lp-domain, `MemBus` impl in lpfx,
  binding resolution wiring through node instances, editor
  synthetic-input UI, "bind to channel" widget affordance, bus
  debugger panel. Touches both lpfx and lp-studio
  meaningfully.
- **M6 (C):** TOML side-by-side editor, rich Slot widgets
  (color picker is a real component, log slider, dial),
  composition widgets (struct/array Slot UI), file tree
  refinements. Largely lp-studio + widget crate work.
- **M7 (B):** delete web-demo + superseded fixtures, update
  design docs, run all gates. Stretch: File System Access API
  impl. Small plan.

After M7: resume the deferred lp-domain M4–M6 (schema gen +
migration framework + CI gates), now with shapes that have
actually been used.
