# Combined `sampler2D` is the authored texture surface; naga compatibility is a textual bridge

- Status: accepted
- Date: 2026-08-05
- Deciders: Yona (ratified in-session during the PR #362 review discussion)
- Context: the browser-preview palette outage
  (`docs/defects/2026-08-05-generated-palette-header-dies-on-naga.md`,
  fixed on PR #362) reopened a question that had been decided only
  implicitly: what GLSL spelling do LightPlayer shaders use for
  textures — and palettes, their first shipped client? The moment was
  deliberate: palettes had not shipped to any user, so this was the
  last cheap opportunity to *not* commit to the surface. The
  alternatives were weighed explicitly rather than inherited.

## Context

Three tiers compile the same authored GLSL:

- **`lps-glsl`** (devices, native servers) parses `sampler2D` natively
  as `LpsType::Texture2D` (`lp-shader/lps-glsl/src/hir.rs`).
- **The browser CPU tier** pins `ShaderFrontend::Naga`
  (`fw-browser/src/runtime.rs`), and naga's glsl-in has **no combined
  sampler type at all** — `sampler2D` exists there only as the
  Vulkan-style constructor builtin `sampler2D(tex, samp)` that pairs a
  separate `texture2D` with a separate `sampler`.
- **The GPU tier** (`lp-gfx-wgpu`) also parses through naga glsl-in on
  its way to WGSL.

So any authored surface must be implementable three times, and naga's
gap has to be bridged somewhere for two of the three. Both naga tiers
bridge it textually — rewriting the combined declaration into split
form before parsing — and those two rewrites, developed independently,
disagreed about which declarations to rewrite. That divergence was the
defect: the engine's own generated palette header
(`layout(binding = N) uniform sampler2D <name>;`, from
`lpc-model::generate_compute_shader_header`) failed to compile on
exactly one tier.

The upstream posture matters: naga's glsl-in is community-maintained
and de-prioritized. Betting on upstream implementing combined samplers
is weak, so whatever bridge exists is ours indefinitely.

## Decision

### 1. Combined `sampler2D` stays the authored surface

`uniform sampler2D <name>;` — optionally with a
`layout(binding = N)` qualifier — is the one texture spelling authored
shaders and generated headers use, sampled with `texture(name, uv)`
and `texelFetch(name, coord, lod)`.

The deciding frame: **this is a spec-GLSL subset, not a custom
dialect**. Combined samplers are standard GLSL (ES 3.0 and desktop);
it is naga's glsl-in that implements only the Vulkan-flavored subset.
LightPlayer restricts the standard surface (two sampling builtins;
filter/wrap deliberately *not* authorable — the engine supplies them
per slot via `TextureBindingSpec`, per
`docs/design/lp-shader-texture-access.md`), and restriction is what
every GLSL environment does. Dialects — syntax that exists in no
spec — are the trap, and every rejected alternative below walks into
it. A subset also keeps the paste-ability story: classic GLSL from
anywhere lands here spelling textures the way we parse them.

### 2. The naga gap is bridged textually, over one shared recognizer

Recognition of the combined declaration lives once, in
`lps_shared::sampler2d_decl::scan_uniform_sampler2d_decls` (spans over
comment-stripped text, offsets valid in the original source), consumed
by both naga tiers. Emission stays tier-specific because the tiers
genuinely differ:

- The CPU rewrite (`lps-frontend/src/parse.rs`) synthesizes a
  companion `uniform sampler __lp_samp_X;` and rewrites `texture(X,` →
  `texture(sampler2D(X, __lp_samp_X),` — the pair naga's constructor
  builtin requires.
- The GPU rewrite (`lp-gfx-wgpu/src/texture_lowering.rs`) binds no
  sampler object at all: its `texture()` sites lower to generated
  `texelFetch` helpers that implement filter/wrap manually (WebGPU's
  hardware `MirrorRepeat` mirrors with a different period, and
  filtering `Rgba32Float` gates on an optional feature).

Every synthesized binding numbers past the source's highest explicit
`binding = N`, so it can never collide with an authored or generated
slot.

### 3. The vendored naga fork stays one-hunk-thin

No grammar extension. The fork exists for a single upstreamable bug
fix (`&&`/`||` short-circuit, `third_party/naga/README-LP.md`) and its
re-vendoring story depends on staying that small. Teaching the fork
combined samplers would be *implementing spec GLSL upstream never
finished* — upstreamable in principle — but it trades a ~300-line
tested bridge in our own tree for a grammar patch that must survive
every re-vendor, in a frontend upstream is not investing in.

### 4. The browser CPU tier keeps the Naga frontend

Routing palette shaders (or everything) to `LpsGlsl` in the browser
would have made the defect unreachable, but it is a product decision
about frontend convergence, explicitly reserved in
`fw-browser/src/runtime.rs` — not something to decide as a side effect
of a parse bug. It remains open; nothing here forecloses it, and the
bridge shrinks to the GPU tier's copy alone if it ever lands.

## Consequences

- The combined-sampler bridge is a permanent LightPlayer maintenance
  surface for as long as any tier parses authored GLSL through naga
  glsl-in. It is bounded (~one recognizer + two emitters, tested
  against the real producer's spelling), and recognition can no longer
  diverge between tiers by construction.
- The authored contract stays narrow: `texture()` and `texelFetch`
  only, filter/wrap engine-supplied. Anything wider (textureLod,
  authored samplers) is a deliberate future decision, not an accident
  of what parses.
- Generated headers and hand-authored shaders share one spelling, so
  fixtures for the rewrite can (and now do) use the producer's own
  output — the fixture-fidelity lesson from the defect entry.
- The frontend axis stays a real test dimension: contracts carried by
  both frontends need to run through both
  (`just test-browser-shader-frontend`; the palette suite is
  frontend-parameterized).

## Alternatives Considered

- **Author Vulkan-style split `texture2D` + `sampler`.** Naga-native,
  no bridge. Rejected: it forces authors to declare a sampler object —
  the exact concept LightPlayer deliberately removed from the
  authorable surface (filter/wrap are engine policy) — and it is the
  unfamiliar spelling for the target audience. It would also have to be
  added to `lps-glsl` and re-taught in every doc and example.
- **A palette-specific construct** (`uniform palette p;`, a magic
  `paletteColor(t)` builtin). Honest to the 1-D model (kills the
  `vec2(t, 0.0)` boilerplate) but a true dialect: exists in no GLSL
  spec, breaks paste-ability, and needs bespoke handling in all three
  lowering paths — more custom surface, not less. A *generated GLSL
  helper* over the standard spelling (e.g. a per-slot
  `vec4 palette_at(float t)` emitted next to the uniform in the
  generated header) gets the same ergonomics with zero new parser
  surface, and stays open as an M5-time option.
- **Extend the vendored naga grammar.** See Decision 3 — rejected to
  keep the fork thin, not because it is unprincipled.
- **Hold the surface back entirely** ("zero users — last chance not to
  ship it"). Considered seriously and rejected: the compute-shader
  header generator emits this spelling regardless, the texture
  machinery is the foundation the media/image roadmap builds on, and
  shipping a narrower *custom* surface instead would have created the
  dialect problem it was trying to avoid.

## Follow-ups

- Decide whether `just test-browser-shader-frontend` joins CI's
  path-gated Validate job (flagged on PR #362; local `test-rust` runs
  it today).
- At M5's gate (fyeah-sign palette port), decide whether the generated
  `palette_at(t)`-style helper becomes the documented palette API.
- The browser-tier `LpsGlsl` convergence question stays open where
  `fw-browser/src/runtime.rs` reserves it.
