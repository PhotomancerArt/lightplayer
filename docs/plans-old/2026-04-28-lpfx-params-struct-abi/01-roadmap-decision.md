# Phase 1: Record Params ABI Decision

## Scope of phase

Add a new decision entry to `docs/roadmaps/2026-04-23-lp-render-mvp/decisions.md` and update `overview.md` to reflect the structured shader params ABI.

In scope:

- Add D* decision entry to `decisions.md` documenting:
  - One authored `params` struct uniform for value parameters
  - Texture-valued params (palette/gradient) live inside `params` using dotted paths
  - Graph-fed texture inputs remain outside `params`
- Update `overview.md` ABI section to describe `params.*` pattern
- Cross-reference the texture-struct-uniforms work as the enabling dependency

Out of scope:

- Milestone-specific implementation details (those go in M1/M3/M4/M5/M6/M7)
- Example shader migrations (Phase 2)
- Editor widget updates (Phase 3)

## Code Organization Reminders

- Place decision entries at the end of the decisions list to preserve chronological order.
- Keep overview changes minimal: describe the contract, not implementation strategy.
- Group related functionality together.
- Any temporary notes get a TODO comment.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

### Target files

- `docs/roadmaps/2026-04-23-lp-render-mvp/decisions.md`
- `docs/roadmaps/2026-04-23-lp-render-mvp/overview.md`

### New decision entry (append to decisions.md)

Add a new decision entry (next number after existing D14). Use this shape:

```markdown
## D15: Structured `params` uniform for authored shader parameters

**Context:** With nested texture fields now supported in `lp-shader`, we can put
texture-valued authored params like palettes and gradients inside a single
`params` struct alongside scalar params. Previously we considered this but
deferred because the language didn't support it.

**Decision:** 
- Shader-visible authored parameters are passed as a single `params` struct
  uniform that mirrors `ParamsTable`.
- Scalar params become fields like `params.speed`, `params.intensity`.
- Texture-valued params (palette, gradient) become fields like
  `params.palette`, `params.gradient` with dotted texture binding specs.
- Graph-fed texture inputs (Effect upstream, bus textures) remain outside
  authored `params` as resource uniforms; the naming convention for these
  is TBD in M4 Stack/Effect design.

**Rationale:** 
- `ParamsTable` is already an implicit `Shape::Struct`; the shader ABI should
  mirror the domain model.
- One struct reduces top-level uniform sprawl and makes param access consistent.
- Palette/gradient as texture fields inside `params` keeps them in the
  authored parameter surface rather than as separate top-level resources.

**Design implication:**
- lpfx helper derives `LpsType::Struct` for `Params`, builds
  `LpsValueF32::Struct` at runtime, and uses dotted paths like
  `params.gradient` for texture binding specs.
- Example shaders migrate from `param_speed` to `params.speed`.
- Graph inputs like `inputColor` are likely stale naming; M4 design should
  pick between `input`, `inputImage`, or `inputTex`.
```

### Overview.md updates

In the "Backend-agnostic per-shader surface" section, update the texture binding description:

Current text likely describes top-level palette/gradient samplers. Update to:

```markdown
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
```

### Validate

```bash
# Just confirm the docs parse and no broken links
cargo build -p lp-domain --no-default-features 2>/dev/null || true

# Check that decisions.md and overview.md are valid markdown
head -100 docs/roadmaps/2026-04-23-lp-render-mvp/decisions.md > /dev/null
head -100 docs/roadmaps/2026-04-23-lp-render-mvp/overview.md > /dev/null
```

Report back what decision number was assigned and confirm overview.md renders correctly.
