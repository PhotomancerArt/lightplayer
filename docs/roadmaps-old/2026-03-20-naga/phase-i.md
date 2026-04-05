# Phase I: Scaffold lp-glsl-naga + rewrite lp-glsl-wasm foundation

## Goal

Create the new Naga-based frontend crate and rewrite the WASM backend to
consume `naga::Module`. At the end of this phase, the simplest filetests pass
end-to-end through the new pipeline (scalars, basic binary ops, function calls).

## Scope

In scope:

- New crate `lp-glsl-naga` that wraps `naga::front::glsl`
- Rewrite `lp-glsl-wasm` to walk `naga::Module` instead of `TypedShader`
- Scalar types (float/int), basic binary operators, function arguments/return
- Wire up `wasm.q32` filetest target to use the new stack
- Q32 mode (i32 fixed-point emission)

Out of scope:

- Vectors (vec2/vec3/vec4), swizzles, constructors
- Standard builtins (smoothstep, sin, cos, etc.)
- LPFX builtins
- Control flow (if/for/while)
- User-defined functions (beyond main)
- Cranelift backend changes

## Key decisions

- `lp-glsl-naga` depends on `naga` from crates.io (v29, `glsl-in` feature)
- `lp-glsl-wasm` switches dependency from `lp-glsl-frontend` to `lp-glsl-naga`
- Both crates are `#![no_std]` compatible
- Stack-based emission (no extra locals for simple expressions)

## Deliverables

- `lp-shader/lp-glsl-naga/` crate with `compile()` entry point
- Rewritten `lp-shader/lp-glsl-wasm/` consuming `naga::Module`
- Filetests passing for scalar arithmetic on `wasm.q32` target

## Dependencies

- Spike validation complete (`spikes/naga-wasm-poc` — done)
- No changes to `lp-glsl-frontend` or `lp-glsl-cranelift`
