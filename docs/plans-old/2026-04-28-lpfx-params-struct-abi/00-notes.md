# Scope of work

Plan the lpfx / lp-render MVP follow-up that adopts a structured shader
parameter ABI now that aggregate uniforms exist in `lp-shader` and the
`texture-struct-uniforms` work has been merged.

This plan is about the lpfx-facing contract and roadmap updates, not the
`lp-shader` nested texture implementation itself. The following dependency is
done:

`/Users/yona/dev/photomancer/feature/lightplayer-textures/docs/plans/2026-04-28-texture-struct-uniforms.md`

Key target outcome:

- Value params and param-owned texture resources, such as palette/gradient
  strips, are assembled into one shader-visible `params` struct.
- `ParamsTable` remains the authoring/domain shape.
- lpfx derives shader ABI values from `ParamsTable`: `LpsType::Struct`,
  `LpsValueF32::Struct`, and texture binding specs keyed by canonical dotted
  paths like `params.gradient`.
- Pipeline-owned frame/input resources, especially Effect input textures such
  as `inputColor`, remain distinct from authored params. The distinction is
  source/ownership, not Rust/GLSL type: texture params like gradients can live
  inside `params`, while graph-fed input textures can remain top-level uniforms.
- Roadmap docs are updated so M1/M3/M4/M5/M6/M7 stop describing flat scalar
  uniform mapping or top-level palette/gradient sampler params where that is no
  longer the intended ABI.

# Current state

- `lp-domain/lp-domain/src/visual/params_table.rs` documents `ParamsTable` as
  the implicit `Shape::Struct` form of a Visual's top-level `[params]` block.
  This maps naturally to a shader `Params` struct.
- `lp-shader/lp-shader/src/px_shader.rs` already models the public runtime
  uniform input as `LpsValueF32::Struct` matching `meta.uniforms_type`.
- Before the dependency plan lands, `LpsPxShader::apply_uniforms` only walks
  top-level members. After the dependency plan, nested struct fields and nested
  `Texture2D` leaves should be applied via dotted paths.
- Before the dependency plan lands,
  `lp-shader/lps-shared/src/texture_binding_validate.rs` only discovers
  top-level `Texture2D` uniforms. After the dependency plan, validation should
  discover nested texture leaves using canonical dotted paths.
- The lp-render MVP docs currently say `PatternInstance` maps `Pattern.params`
  to GLSL uniforms and derives sampler specs for texture-backed
  params/resources. They do not yet distinguish the new `params` struct ABI
  from flat top-level param uniforms.
- The docs also recently moved palette/gradient shader visibility to
  height-one textures. With nested texture struct support assumed complete,
  palette/gradient params can be represented as fields inside `params`, e.g.
  `params.gradient`.
- M4 Effect inputs are pipeline resources rather than authored params. The docs
  currently use a conventional top-level `inputColor` sampler for these.
- M5 bus texture values are routed resources. When a bus channel drives a
  param field, it can populate `params.someField`; when it drives a Visual
  input, it can still bind a top-level input sampler such as `inputColor`.
- `lp-domain` examples are part of the contract surface. Current examples still
  include flat shader param uniforms:
  - `lp-domain/lp-domain/examples/v1/patterns/rainbow.pattern.toml`
  - `lp-domain/lp-domain/examples/v1/patterns/fbm/main.glsl`
  - `lp-domain/lp-domain/examples/v1/effects/tint.effect.toml`
  - `lp-domain/lp-domain/examples/v1/effects/kaleidoscope.effect.toml`
  - transition examples also use `param_*`, but transition runtime is outside
    the lp-render MVP scope and should be updated only if the decision is meant
    to cover all Visual shader examples now.
- `lp-domain/lp-domain/src/visual/effect.rs` still documents the conventional
  input sampler as `inputColor`. That naming is stale enough to note, but the
  final replacement belongs to the M4 Stack/Effect design.
- `lp-domain/lp-domain/src/kind.rs` still describes `ColorPalette` and
  `Gradient` as fixed-max struct storage. That remains true for authoring /
  TOML value storage, but comments should be clarified so readers do not infer
  that shader-visible palette/gradient params are passed as those structs.

# Questions

## Confirmation-style questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Should palette/gradient params live inside the shader `params` struct as `Texture2D` fields once nested texture structs are available? | The dependency plan explicitly supports `params.gradient` with dotted texture specs. | Yes. |
| Q2 | Should graph-fed Effect input textures like `inputColor` remain outside authored `params`? | These are texture inputs from upstream visuals or bus channels, not user-authored knobs. This is about source/ownership, not type: param-owned textures can still live inside `params`. | Yes; likely as top-level resource uniforms. |
| Q3 | Should lpfx use canonical dotted texture spec keys such as `params.palette` / `params.gradient` for param-owned textures? | This matches the dependency plan's public key contract. | Yes. |
| Q4 | Is this plan limited to roadmap/design docs and phase prompts, leaving actual lpfx implementation for the relevant milestone plans? | The current work is aligning the MVP roadmap before moving into lpfx execution. | Yes. |
| Q5 | Should shader examples migrate from flat names like `param_speed` to `params.speed` and `params.gradient`? | This makes the ABI visible in examples and tests. | Yes. |

## Discussion-style questions

No discussion-style questions are currently blocking the design.

## Resolved answers

- Q1: Yes. Palette/gradient params should live inside the shader `params`
  struct as `Texture2D` fields once nested texture structs are available.
- Q2: Yes, with clarified language. Graph-fed texture inputs should remain
  outside authored `params`; param-owned textures should live inside `params`.
  The distinction is source/ownership, not texture-vs-non-texture.
- Q2 naming note: `inputColor` is probably the wrong long-term name because the
  input is a texture/image, not only a color. Candidate names include `input`,
  `inputImage`, and `inputTex`. This is not central to the params ABI plan; the
  Stack/Effect milestone design should settle the convention.
- Q3: Yes. Use canonical dotted texture spec keys such as `params.palette` and
  `params.gradient` for param-owned texture resources.
- Q4: Yes. Scope this plan to roadmap/design docs and phase prompts, leaving
  actual lpfx implementation for the relevant milestone plans.
- Q5: Yes. Shader examples should migrate from flat names like `param_speed` to
  `params.speed` / `params.gradient`.
- Q6: Yes. `lp-domain` examples should be updated as part of this roadmap
  alignment because they are the canonical artifact corpus later lpfx tests will
  load.

# Notes

- If Q4 changes to "no", the plan should become an implementation plan for M1
  lpfx runtime work instead of a roadmap-alignment plan. In that case the plan
  directory should probably move under
  `docs/roadmaps/2026-04-23-lp-render-mvp/m1-lpfx-runtime/`.
