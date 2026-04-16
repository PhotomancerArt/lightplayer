# Rainbow WASM Correctness: Filetest Coverage Plan

## Problem

The rainbow shader renders correctly via the Cranelift/egui path but produces
visual artifacts in the WASM web demo. Rather than guessing, we want systematic
filetest coverage of every operation the rainbow shader uses, plus an
integration test that blesses cranelift output and verifies wasm matches.

## Audit

Operations traced from `examples/basic/src/rainbow.shader/main.glsl`:

### Already covered (runs on wasm)

- `mod(float)`, `mod(vecN)` — `builtins/common-mod.glsl`
- `floor(float)` — `builtins/common-floor.glsl`
- `fract(float)` — `builtins/common-fract.glsl`
- `sin(float)`, `cos(float)` — `trig-sin.glsl`, `trig-cos.glsl`
- `exp(float)` — `exp-exp.glsl`
- `atan(float, float)` — `trig-atan.glsl`
- `vec3 * vec3`, `vec3 + vec3`, `vec2 + vec2` — `vec*/op-multiply.gen.glsl`, `op-add.gen.glsl`
- `vec2(scalar)` broadcast — `vec2/from-scalar.glsl`
- `min`/`max` scalar+vec — `vec*/fn-min.gen.glsl`, `fn-max.gen.glsl`
- `lpfx_psrdnoise` — `lpfx/lp_psrdnoise.glsl`

### Missing — no filetests at all

| Operation | Rainbow usage | Priority |
|---|---|---|
| `smoothstep(float, float, float)` | palette blend timing | high |
| `mix(float, float, float)` | pan, scale interpolation | high |
| `mix(vec3, vec3, float)` | palette lerp | high |
| `clamp(float, float, float)` | every palette function | high |
| `clamp(vec3, float, float)` | every palette function | high |
| `vec2 - vec2` subtraction | `fragCoord - center` | high |
| `float * vec3` / `vec3 * float` | palette scalar-vec math | medium |
| `vec4(vec3, float)` constructor | `vec4(..., 1.0)` return | medium |

### Exists but wasm-skipped (`@unimplemented(backend=wasm)`)

| Operation | Filetest |
|---|---|
| `cos(vec3)`, `sin(vec2)` component-wise | `edge-component-wise.glsl` |
| `abs(vec3)` | `edge-component-wise.glsl` |

## Phases

### Phase 1 — Missing builtin filetests

Fill the gaps with standard filetests. These are generic coverage that should
exist regardless of the rainbow shader.

New files:

- `builtins/common-smoothstep.glsl` — scalar, vec2, vec3 variants
- `builtins/common-mix.glsl` — `mix(float,float,float)`, `mix(vec3,vec3,float)`
- `builtins/common-clamp.glsl` — runtime `clamp(float)`, `clamp(vec3)`
- `vec/vec2/op-subtract.gen.glsl` — vec2 subtraction
- `vec/vec3/op-subtract.gen.glsl` — vec3 subtraction
- Scalar-vec multiply test (in vec3 tests or standalone)
- `vec4(vec3, float)` constructor test

Approach: write tests, bless with cranelift, run on wasm. Any failures here are
likely the actual bugs causing the visual artifacts.

### Phase 2 — Component-wise builtins on wasm

Check whether the wasm backend now supports `sin(vec3)`, `cos(vec3)`,
`abs(vec3)`. If so, remove `@unimplemented(backend=wasm)` from
`edge-component-wise.glsl` and run them. If not, create targeted wasm-enabled
tests for just the operations the rainbow shader needs (`cos(vec3)`,
`abs(vec3)`).

### Phase 3 — `debug/rainbow.glsl` blessed integration test

A filetest that mirrors the rainbow shader structure. Sub-functions called at
known inputs, blessed from cranelift, verified on wasm:

```
test_palette_heatmap(0.0)  ~= vec3(...)
test_palette_heatmap(0.5)  ~= vec3(...)
test_palette_rainbow(0.25) ~= vec3(...)
test_prsd_demo(32.0, 32.0, 64.0, 64.0, 1.0) ~= vec2(...)
test_rainbow_main(32.0, 32.0, 64.0, 64.0, 0.0)  ~= vec4(...)
test_rainbow_main(32.0, 32.0, 64.0, 64.0, 2.5)  ~= vec4(...)
test_rainbow_main(0.0, 0.0, 64.0, 64.0, 5.0)    ~= vec4(...)
```

This catches composition bugs that individual builtin tests miss.

## Execution order

1. Phase 1 first — most likely to surface the actual bug
2. Phase 3 next — provides the high-level sanity check
3. Phase 2 last — nice-to-have coverage improvement
