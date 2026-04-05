# Phase I: Scaffold lps-frontend + rewrite lps-wasm foundation

## Goal

Create the new Naga-based frontend crate and rewrite the WASM backend to
consume `naga::Module`. At the end of this phase, the simplest filetests pass
end-to-end through the new pipeline (scalars, basic binary ops, function calls).

## Scope

In scope:

- New crate `lps-frontend` that wraps `naga::front::glsl`
- Rewrite `lps-wasm` to walk `naga::Module` instead of `TypedShader`
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

- `lps-frontend` depends on `naga` from crates.io (v29, `glsl-in` feature)
- `lps-wasm` switches dependency from `lps-frontend` to `lps-frontend`
- Both crates are `#![no_std]` compatible
- Stack-based emission (no extra locals for simple expressions)

## Deliverables

- `lp-shader/lps-frontend/` crate with `compile()` entry point
- Rewritten `lp-shader/lps-wasm/` consuming `naga::Module`
- Filetests passing for scalar arithmetic on `wasm.q32` target

## Dependencies

- Spike validation complete (`spikes/naga-wasm-poc` — done)
- No changes to `lps-frontend` or `lps-cranelift`
