# Design: GLSL Fast Math Mode

## Scope of Work

Add a "fast math" mode for the GLSL q32 (fixed-point) compiler. In fast math mode, add/sub emit inline `iadd`/`isub` instead of saturating builtin calls, trading overflow safety for performance. Any shader node can opt in via config. esp32 demo project uses it as an example.

## File Structure

```
lp-glsl/lp-glsl-compiler/src/
├── exec/executable.rs           # UPDATE: Add fast_math to GlslOptions
├── frontend/mod.rs              # UPDATE: Pass options.fast_math to Q32Transform

lp-glsl/lp-glsl-compiler/src/backend/transform/q32/
├── transform.rs                 # UPDATE: Add fast_math field, pass to pipeline
├── instructions.rs              # UPDATE: Pass fast_math to convert_fadd/convert_fsub
└── converters/
    └── arithmetic.rs            # UPDATE: convert_fadd, convert_fsub - inline iadd/isub when fast_math

lp-core/lp-model/src/nodes/shader/
└── config.rs                    # UPDATE: Add optional glsl_opts: Option<GlslOpts>

lp-core/lp-model/src/
└── glsl_opts.rs                 # NEW: GlslOpts { fast_math: bool } (or in nodes/shader/)

lp-core/lp-engine/src/nodes/shader/
└── runtime.rs                   # UPDATE: compile_shader uses config.glsl_opts for GlslOptions

lp-fw/fw-esp32/src/
└── demo_project.rs              # UPDATE: Add glsl_opts with fast_math: true to rainbow.shader node.json
```

## Conceptual Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  node.json      │────▶│  ShaderConfig    │────▶│  ShaderRuntime   │
│  glsl_opts: {   │     │  glsl_opts?      │     │  compile_shader() │
│   fast_math:true│     │                  │     │                  │
│  }              │     └──────────────────┘     └────────┬────────┘
└─────────────────┘                                        │
                                                           │ builds
                                                           ▼
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  iadd / isub    │◀────│  Q32Transform    │◀────│  GlslOptions    │
│  (inline)       │     │  fast_math: bool │     │  fast_math      │
└─────────────────┘     └──────────────────┘     └─────────────────┘
       or
┌─────────────────┐
│  __lp_q32_add   │  (saturating builtin call when fast_math=false)
│  __lp_q32_sub   │
└─────────────────┘
```

## Main Components and Interactions

1. **ShaderConfig** (lp-model): Adds optional `glsl_opts: Option<GlslOpts>`. Deserialized from node.json. When absent, defaults to `GlslOpts { fast_math: false }`.

2. **GlslOpts** (lp-model): New struct with `fast_math: bool`. Default false. Serialize/Deserialize for JSON.

3. **GlslOptions** (lp-glsl-compiler): Adds `fast_math: bool` (default false). Passed to compile functions.

4. **Q32Transform**: Adds `fast_math: bool` field, passed through to `convert_all_instructions` → `convert_fadd`/`convert_fsub`.

5. **convert_fadd / convert_fsub**: When `fast_math`, emit `builder.ins().iadd(lhs, rhs)` or `isub` instead of builtin call. When not fast_math, keep existing builtin call path.

6. **ShaderRuntime::compile_shader**: Build `GlslOptions` using `config.glsl_opts.as_ref().map(|o| o.fast_math).unwrap_or(false)` (or similar). Pass to glsl_jit.

7. **esp32 demo_project**: Add `"glsl_opts": {"fast_math": true}` to rainbow.shader node.json.

## Backward Compatibility

- GlslOptions.fast_math defaults to false
- ShaderConfig.glsl_opts is Option; when absent, use default GlslOpts
- Existing node.json files without glsl_opts continue to work (saturating behavior)
