# Scope of work

Update the lp-render MVP roadmap so lpfx targets a structured shader params
ABI now that nested texture fields in uniform structs are supported by
`lp-shader`.

This is a planning/docs alignment pass. It does not implement lpfx runtime code.

In scope:

- Add a roadmap decision for one authored `params` struct uniform.
- Clarify that texture-valued authored params, such as palettes and gradients,
  live inside `params` and use dotted texture binding keys.
- Keep graph-fed texture inputs distinct from authored params.
- Update M1/M3/M4/M5/M6/M7 roadmap docs and examples so later milestone plans
  start from the same ABI.
- Update canonical `lp-domain` example shaders that are used by the lp-render
  MVP tests so they demonstrate `params.*` instead of flat `param_*` uniforms.
- Clarify `lp-domain` comments where they could be read as saying
  palette/gradient shader params are still passed as fixed-size structs.

Out of scope:

- Implementing lpfx helper code.
- Implementing nested texture fields in `lp-shader`; this plan depends on
  `/Users/yona/dev/photomancer/feature/lightplayer-textures/docs/plans/2026-04-28-texture-struct-uniforms.md`
  being complete.
- Finalizing the Effect input sampler name. `inputColor` is likely stale; the
  M4 Stack/Effect design should choose between names like `input`, `inputImage`,
  or `inputTex`.

# File structure

```text
docs/
├── plans/
│   └── 2026-04-28-lpfx-params-struct-abi/
│       ├── 00-notes.md                  # NEW: scope, assumptions, resolved answers
│       ├── 00-design.md                 # NEW: design overview
│       ├── 01-roadmap-decision.md       # NEW: phase prompt
│       ├── 02-milestone-runtime-docs.md # NEW: phase prompt
│       └── 03-editor-and-validation.md  # NEW: phase prompt
└── roadmaps/
    └── 2026-04-23-lp-render-mvp/
        ├── overview.md                  # UPDATE: ABI overview
        ├── decisions.md                 # UPDATE: new params ABI decision
        ├── m1-lpfx-runtime.md           # UPDATE: runtime helper direction
        ├── m3-pattern-editor.md         # UPDATE: editor/runtime bridge wording
        ├── m4-stack-and-effect.md       # UPDATE: graph input vs params boundary
        ├── m5-bus-and-bindings.md       # UPDATE: bound params populate params struct
        ├── m6-semantic-editor.md        # UPDATE: composite params + texture recipes
        └── m7-cleanup-verification.md   # UPDATE: audit and example migration checks
lp-domain/
└── lp-domain/
    ├── examples/v1/
    │   ├── patterns/rainbow.pattern.toml      # UPDATE: params struct shader
    │   ├── patterns/fbm/main.glsl             # UPDATE: params struct shader
    │   ├── effects/tint.effect.toml           # UPDATE: params struct shader
    │   └── effects/kaleidoscope.effect.toml   # UPDATE: params struct shader
    └── src/
        ├── kind.rs                            # UPDATE: authoring vs shader ABI comments
        └── visual/effect.rs                   # UPDATE: input sampler naming note, if settled
```

# Conceptual architecture

```text
Visual artifact
  Pattern.params / Effect.params
        │
        ▼
lpfx params ABI builder
  - walks ParamsTable
  - reads defaults / overrides / bus-bound values
  - bakes param-owned textures (palette, gradient)
        │
        ├─ uniforms value:
        │    params: LpsValueF32::Struct
        │      speed: Float
        │      gradient: Texture2D
        │
        └─ texture specs:
             params.gradient -> TextureBindingSpec::HeightOne

Graph / Stack wiring
        │
        ▼
resource uniforms outside authored params
  input or inputImage or inputTex -> upstream frame texture
```

Shader surface:

```glsl
struct Params {
    float speed;
    sampler2D gradient;
};

uniform Params params;

// Name pending M4 design; shown with current roadmap convention.
uniform sampler2D inputColor;
```

# Main components

## Authored `params` struct

The shader-visible representation of a Visual's editable parameter surface is a
single `params` uniform. This mirrors `ParamsTable`, which is already an
implicit `Shape::Struct`.

Examples should move away from flat names like `param_speed` and toward
`params.speed`.

The canonical `lp-domain/examples/v1` corpus should be updated because M1/M4
lpfx tests load those artifacts directly. If the examples remain flat while the
roadmap says `params`, the first implementation phase will have to choose
between following the tests or following the docs.

## Texture-valued params

Palette and gradient values remain authoring recipes in domain/editor state.
At runtime, lpfx bakes them into height-one textures and writes the resulting
texture value into a field such as `params.gradient`.

The texture binding spec key is the canonical dotted shader path:

```text
params.gradient
params.palette
```

This uses the texture-struct-uniforms work, where dotted texture paths became a
supported `lp-shader` contract.

## Graph-fed texture inputs

Textures supplied by graph composition are not authored params. They should
remain outside `params` even though they are still shader uniforms. This keeps
the parameter surface clean and avoids presenting Stack wiring as a tweakable
Effect parameter.

The current roadmap name `inputColor` is probably too narrow. This params ABI
plan should only mark it as a pending M4 naming decision.

## Bus and overrides

Effect/Stack overrides and bus bindings should resolve into the same final
`params` struct value. The source of a value can vary:

- default from `ParamsTable`;
- in-memory editor tweak;
- Stack/Effect override;
- bus channel read;
- baked texture resource from a recipe.

The shader ABI should not change based on that source.

# Phase shape

This plan has three documentation phases:

1. Record the decision in `overview.md` / `decisions.md`.
2. Update runtime-facing milestones M1/M4/M5 and canonical `lp-domain`
   examples.
3. Update editor/validation-facing milestones M3/M6/M7 and any `lp-domain`
   comments that would otherwise contradict the new ABI.
