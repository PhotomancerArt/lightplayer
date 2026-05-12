# Phase 2: Update Runtime Milestone Docs and Examples

## Scope of phase

Update the runtime-facing milestone documents (M1, M4, M5) and canonical `lp-domain` example shaders to reflect the `params` struct ABI.

In scope:

- Update `m1-lpfx-runtime.md`:
  - Change "maps `Pattern.params` to GLSL uniforms" to "derives `params` struct uniform from `Pattern.params`"
  - Update texture-backed param description to use dotted paths like `params.gradient`
  - Update key decisions and deliverables
  
- Update `m4-stack-and-effect.md`:
  - Clarify that Effect params go in `params` struct
  - Clarify that graph-fed inputs (`inputColor` or pending rename) remain outside `params`
  - Update key decisions and test descriptions
  
- Update `m5-bus-and-bindings.md`:
  - Bus-bound params populate `params.*` fields
  - Texture bus channels can feed `params.*` texture fields or graph inputs
  
- Update `lp-domain` example shaders:
  - `rainbow.pattern.toml` - migrate `param_time`, `param_speed`, `param_saturation` to `params` struct
  - `fbm/main.glsl` and `fbm.pattern.toml` - migrate `param_*` to `params`
  - `tint.effect.toml` - migrate `param_color`, `param_amount` to `params`
  - `kaleidoscope.effect.toml` - migrate `param_slices`, `param_rotation` to `params`

Out of scope:

- Transition examples (crossfade, wipe) - these are outside lp-render MVP scope
- Core domain code changes - examples only
- Editor milestone docs (M3, M6) - those are Phase 3

## Code Organization Reminders

- Keep example shader changes minimal - just the uniform structure and access patterns.
- Preserve all existing behavior; only change naming/structure.
- Place related doc updates together.
- Mark any uncertainties with TODO comments.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

### Target files

Docs:
- `docs/roadmaps/2026-04-23-lp-render-mvp/m1-lpfx-runtime.md`
- `docs/roadmaps/2026-04-23-lp-render-mvp/m4-stack-and-effect.md`
- `docs/roadmaps/2026-04-23-lp-render-mvp/m5-bus-and-bindings.md`

Examples:
- `lp-domain/lp-domain/examples/v1/patterns/rainbow.pattern.toml`
- `lp-domain/lp-domain/examples/v1/patterns/fbm.pattern.toml`
- `lp-domain/lp-domain/examples/v1/patterns/fbm/main.glsl`
- `lp-domain/lp-domain/examples/v1/effects/tint.effect.toml`
- `lp-domain/lp-domain/examples/v1/effects/kaleidoscope.effect.toml`

### Doc updates

#### m1-lpfx-runtime.md

Find and update:

1. "maps `Pattern.params` to GLSL uniforms" → "derives `params` struct uniform from `Pattern.params`"

2. "derives sampler specs for any texture-backed params/resources" → "derives texture binding specs using dotted paths like `params.gradient` for texture-valued params"

3. Key decisions section - add or update:
   - `PatternInstance` builds one `LpsValueF32::Struct` for the `params` uniform
   - Texture-valued params use dotted spec keys inside `params`

4. Deliverables - update bullet:
   - "Texture-aware lpvm compile/render path using `CompilePxDesc`, `TextureBindingSpec`, and `LpsTextureBuf` texture uniforms" → add "with dotted path specs for nested texture fields"

#### m4-stack-and-effect.md

Find and update:

1. "applies params" → "applies `params` struct (scalar and texture-valued fields)"

2. "`inputColor` sampler" - add note that naming is pending M4 design (input vs inputImage vs inputTex)

3. Key decisions - add:
   - Effect authored params live in `params` struct
   - Graph-fed inputs remain outside `params` as top-level resource uniforms

4. Clarify ping-pong buffers vs resource textures in texture pipeline section

#### m5-bus-and-bindings.md

Find and update:

1. "At render: for each bound param, read from bus instead of in-memory value" → "At render: for each bound param, read from bus and populate the corresponding `params.*` field"

2. "Texture bus values are routed resources" → clarify they can populate `params.*` texture fields or graph inputs

### Example shader migrations

#### rainbow.pattern.toml

Change GLSL from:
```glsl
uniform float param_time;
uniform float param_speed;
uniform float param_saturation;
```

To:
```glsl
struct Params {
    float time;
    float speed;
    float saturation;
};
uniform Params params;
```

Change access from `param_time` to `params.time`, etc.

#### fbm/main.glsl

Change from flat `param_time`, `param_scale`, `param_octaves` to `params.time`, `params.scale`, `params.octaves`.

#### tint.effect.toml

Change from:
```glsl
uniform vec3  param_color;
uniform float param_amount;
```

To:
```glsl
struct Params {
    vec3 color;
    float amount;
};
uniform Params params;
```

Change access from `param_color` to `params.color`, `param_amount` to `params.amount`.

Note: Keep `inputColor` as top-level for now (pending M4 naming decision), but document that it's outside the authored `params`.

#### kaleidoscope.effect.toml

Same pattern as tint.

### Validate

```bash
# Verify TOML examples still parse
cargo test -p lp-domain --lib artifact 2>&1 | head -20

# Check no syntax errors in GLSL (basic validation)
# (Full GLSL validation requires lp-shader; just check file structure)
head -30 lp-domain/lp-domain/examples/v1/patterns/rainbow.pattern.toml
head -30 lp-domain/lp-domain/examples/v1/effects/tint.effect.toml
```

Ensure all examples still load successfully after changes.
