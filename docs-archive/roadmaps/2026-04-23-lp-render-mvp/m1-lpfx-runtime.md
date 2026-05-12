# Milestone 1: lpfx runtime — strip parallel domain, integrate lp-domain

## Goal

Demolish lpfx's parallel-domain types (`FxModule`, `FxManifest`,
`FxInputDef`, `FxValue`, etc.), rewire lpfx to consume `lp-domain`
types directly (`Pattern`, `ParamsTable`, `Slot`, `ShaderRef`),
and stand up the runtime spine: artifact loader + cache, per-Visual
node instance, single-node graph executor, backend-agnostic
per-shader trait, shader texture-resource binding, lpvm-backed
implementation that renders a `Pattern` end-to-end.

**Headless milestone.** No editor, no UI. Acceptance is `cargo test`
green: load `rainbow.pattern.toml` from `lp-domain/lp-domain/examples/v1/`,
instantiate it through the new runtime, render to an in-memory texture,
assert pixels are non-black and respond to param changes.

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m1-lpfx-runtime/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files
(`01-…md`, `02-…md`, …).

## Scope

**In scope:**

- **Demolish lpfx parallel domain.** Delete:
  - `lpfx/lpfx/src/manifest.rs` (`FxManifest`, `FxMeta`, `FxResolution`)
  - `lpfx/lpfx/src/input.rs` (`FxInputDef`, `FxInputType`, `FxValue`,
    `FxPresentation`, `FxChoice`)
  - `lpfx/lpfx/src/parse.rs` (`parse_manifest`)
  - `lpfx/lpfx/src/defaults.rs` (`defaults_from_manifest`)
  - `lpfx/lpfx/src/render_inputs.rs` (`FxRenderInputs`)
  - `lpfx/lpfx/src/module.rs` (`FxModule`)
  - `lpfx/lpfx/src/error.rs` types specific to manifest parsing
- **Update `lpfx::lib.rs`** to re-export the new runtime surface
  instead of the deleted types.
- **New `lpfx/lpfx/src/backend.rs`**: backend-agnostic per-shader
  trait, evolved from the current `FxEngine` / `FxInstance`.
  Designed so a future wgpu impl drops in alongside the lpvm impl
  without restructuring the graph layer above.
  - `trait ShaderBackend { type Shader; fn compile(...) -> ...;
    fn alloc_texture(...) -> ...; fn texture_uniform(...) -> ...; }`
  - `trait Shader { fn render(uniforms: ..., texture_uniforms: ..., output: ...) -> ...; }`
  - Exact shape resolved in the design phase; the constraint is
    "graph layer doesn't import lpvm or wgpu directly."
  - Compile accepts sampler binding specs derived from shader
    resource requirements, using `CompilePxDesc` for lpvm.
  - Runtime supports `LpsTextureBuf`-backed texture uniforms, not
    only scalar params and an output target.
- **New `lpfx/lpfx/src/runtime/`** module:
  - `engine.rs`: top-level `Engine<B: ShaderBackend>` that owns
    the cache, the backend, and instantiates Visuals into
    `NodeInstance`s.
  - `instance.rs`: `NodeInstance` trait (object-safe — reads bus,
    writes texture handle, reports outputs). M1 has only one
    impl.
  - `cache.rs`: `ArtifactCache` keyed by canonical path. Stores
    parsed `Pattern` / `Effect` / `Stack` (Effect/Stack land in
    M4 but the cache shape covers them). LRU is overkill; a
    simple `BTreeMap` is fine for v0.
  - `graph.rs`: minimal executor that walks a single-node graph
    and calls `render`. Real topology lands in M4.
- **New texture-resource model** in `lpfx/lpfx/src/texture/`:
  - Separate frame/output textures from resource textures.
  - Frame/output textures back Pattern outputs and, later, Stack
    ping-pong buffers.
  - Resource textures back generated or loaded shader inputs such
    as palette/gradient strips and, later, synthetic bus textures.
  - M1 does not need eviction, but it should expose cache keys and
    replacement semantics so editor rebakes do not imply unbounded
    live resources.
- **New `lpfx/lpfx/src/nodes/pattern.rs`**: `PatternInstance`
  implementing `NodeInstance`. Compiles the `Pattern.shader`
  (handling `ShaderRef::Glsl` / `ShaderRef::File` /
  `ShaderRef::Builtin`), derives `params` struct uniform from `Pattern.params`,
  derives texture binding specs using dotted paths like `params.gradient` for texture-valued params,
  evaluates each param at render time. M1 reads param values
  *directly* (no bus); the bus integration arrives in M5 and
  swaps "direct value lookup" for "bus channel read."
- **Palette/gradient resource spike.** Add at least one
  texture-backed Pattern fixture in the design/test phase:
  - TOML stores an authoring recipe, not baked pixels.
  - lpfx bakes the recipe into a width-by-one `Rgba16Unorm`
    resource texture.
  - Compile uses `texture_binding::height_one(...)`.
  - Render passes the baked texture via a named `sampler2D`
    uniform.
- **Artifact loader integration.** lpfx exposes a `load_pattern`
  / `load_artifact` that wraps `lp_domain::artifact::load`
  through the cache. lpfx is `no_std + alloc`; the loader
  surface accepts an `&dyn LpFs` so both `LpFsStd` and
  `LpFsMem` work.
- **`lpfx-cpu` rewire.** The current `CpuFxEngine` /
  `CpuFxInstance` (which take `FxModule` + `FxRenderInputs`)
  evolve to implement the new `ShaderBackend` trait. The lpvm
  + lp-shader integration (compile_px, render_frame, texture
  pool) stays, but texture-aware shaders compile through
  `compile_px_desc` and pass `LpsValueF32::Texture2D` fields built
  from `LpsTextureBuf::to_named_texture_uniform(...)`.
- **GLSL preamble investigation and resolution.** Pattern
  examples reference helpers like `lpfn_hsv2rgb` that aren't in
  inline GLSL. Trace where the preamble lives today (lpfx vs
  lp-shader vs lps-frontend), decide if it stays where it is or
  moves, and document. Affects `Pattern.shader` handling.
- **Headless integration tests** in `lpfx/lpfx/tests/render.rs`:
  - Load `rainbow.pattern.toml` via the new loader; render to
    a 4×4 texture; assert at least one non-black pixel.
  - Render twice with different `speed` values; assert outputs
    differ (proves the param actually flows through).
  - Load `fbm.pattern.toml` and `fluid.pattern.toml`; assert
    they parse + compile + render without panic.
  - Render a synthetic texture-backed Pattern that samples a
    height-one gradient/palette strip; assert the output reflects
    the baked texture.

**Out of scope:**

- Bus / `BindingResolver` impl (M5).
- Multi-node graph topology (M4).
- Effect / Stack node impls (M4).
- Editor / UI of any kind (M2/M3).
- wgpu backend (separate roadmap).
- LFO / signal generators (Show layer, future).
- Live / Transition / Playlist node impls.
- File watchers, hot reload, fs change handling (later).

## Key decisions

- **lpfx becomes the visual subsystem implementation.** Long-term,
  `lp-engine` will use lpfx as the visual subsystem when it
  composes the broader Show layer (rigs, fixtures, outputs,
  signal generators, scheduling). Near-term, lp-studio (M2)
  wraps it.
- **`lp-domain` stays the model; lpfx stays `no_std + alloc`.**
  Loader surface takes `&dyn LpFs` so it works in browser
  (`LpFsMem` + localStorage wrapper from M2) and on native
  (`LpFsStd`). No std features in lpfx itself.
- **Per-shader backend is trait-shaped from day one.** Even
  though M1 only ships the lpvm impl, the trait has to be
  designed against both lpvm-shaped and wgpu-shaped consumers.
  The graph layer above the trait must not import lpvm or
  wgpu directly. This is the **library-of-previews future
  carve-out** referenced in the overview.
- **`PatternInstance` builds one `LpsValueF32::Struct` for the `params` uniform.**
  All authored scalar and texture-valued params live inside this struct.
  Texture-valued params use dotted spec keys like `params.gradient`.
- **Shader-visible palettes/gradients are texture resources.**
  `Kind::ColorPalette` and `Kind::Gradient` may remain useful as
  authoring/editor concepts, but M1 should not assume they are
  passed to shaders as fixed-size uniform structs. The shader ABI is
  `sampler2D` + `TextureBindingSpec::HeightOne` + a runtime
  texture value.
- **`PatternInstance` reads param values directly in M1, not via
  bus.** When M5 lands the bus, the change is "swap the param
  lookup function from `params.get(name)` to `bus.read(name)`"
  — a localized refactor. Don't preemptively bake bus calls
  into M1.
- **Artifact cache is simple `BTreeMap` — no LRU, no eviction.**
  A working visual library has dozens of artifacts max in v0.
  Eviction is a later concern.
- **Headless milestone.** Acceptance is `cargo test`. UI work
  parallelizes in M2.

## Deliverables

- Deleted parallel-domain files in lpfx (per Scope list above).
- New `lpfx/lpfx/src/backend.rs` (per-shader trait).
- New `lpfx/lpfx/src/runtime/` module
  (`engine.rs` / `instance.rs` / `cache.rs` / `graph.rs`).
- Texture-resource manager separating output textures from
  generated resource textures.
- New `lpfx/lpfx/src/nodes/pattern.rs` (`PatternInstance`).
- Rewired `lpfx-cpu` (`CpuFxEngine` → `LpvmShaderBackend`).
- Texture-aware lpvm compile/render path using `CompilePxDesc`,
  `TextureBindingSpec`, and `LpsTextureBuf` texture uniforms
  with dotted path specs for nested texture fields.
- Loader integration (`lpfx::load_pattern`, etc.).
- GLSL preamble path documented and (if needed) refactored.
- Integration tests in `lpfx/lpfx/tests/render.rs`.
- Updated lpfx README explaining the new role
  (visual-subsystem implementation, lp-domain consumer).

## Acceptance smoke tests

```bash
cargo test -p lpfx
cargo test -p lpfx-cpu
cargo test -p lpfx --test render

# Verify no_std still holds:
cargo build -p lpfx --no-default-features

# Verify the parallel-domain types are actually gone:
! grep -r 'FxModule\|FxManifest\|FxInputDef\|FxValue' lpfx/
```

## Dependencies

- lp-domain M1–M3 complete (already done — `Pattern`, `Slot`,
  `ParamsTable`, `ShaderRef`, `load_artifact` all exist).
- No dependency on M2 (parallel milestone).

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: Parallel-domain demolition, new runtime spine,
per-shader backend trait, texture-resource binding, cache,
Pattern node, lpfx-cpu rewire, and preamble path investigation.
Multiple architectural
calls (where the bus trait *will* live, how the per-shader trait
should be shaped to accommodate wgpu, how the cache keys, how
GLSL preambles resolve). Phaseable: backend trait + lpfx-cpu
rewire as one phase, runtime/cache/loader/texture resources as
another, Pattern node + integration tests as the third.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
