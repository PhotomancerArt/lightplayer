# Naga WASM Scaffold (Roadmap Phase I) — Notes

## Scope

Create `lp-glsl-naga` frontend crate and rewrite `lp-glsl-wasm` to consume
`naga::Module`. Wire up filetests. Pass scalar arithmetic filetests end-to-end.

Part of: `docs/roadmaps/2026-03-20-naga/`

## Current state

- Spike (`spikes/naga-wasm-poc`) validated: Naga `glsl-in` compiles `no_std`,
  GLSL → naga IR → WASM → wasmtime works for f32 and Q32.
- `lp-glsl-wasm` currently depends on `lp-glsl-frontend` and consumes
  `TypedShader`. Entry point: `glsl_wasm(&str, WasmOptions) → WasmModule`.
- `lp-glsl-filetests` wasm_runner uses `FunctionSignature` and `Type` from
  `lp-glsl-cranelift` (re-exported from `lp-glsl-frontend`) for call dispatch.
- `WasmModule` contains `bytes: Vec<u8>` and `exports: Vec<WasmExport>`.
  `WasmExport` has `name`, `params`, `results`, and `signature: FunctionSignature`.

## Key decisions

- `lp-glsl-naga` defines its own `FloatMode`, `GlslType`, `FunctionInfo` types
  (no dependency on `lp-glsl-frontend`).
- `lp-glsl-wasm` switches dependency from `lp-glsl-frontend` to `lp-glsl-naga`.
- `lp-glsl-filetests` wasm_runner updated to use new types from `lp-glsl-wasm`.
- Phase I scope limited to scalars (float, int, uint, bool), basic binary ops,
  literals, local variables, assignment. No vectors, builtins, control flow.

## Naga IR patterns for scalar GLSL

For `float test() { float a = 10.5; float b = 20.3; return a + b; }`:

```
expressions:
  [0] FunctionArgument(0)     — (if params exist)
  [1] LocalVariable([0])      — pointer to local `a`
  [2] Literal(Float32(10.5))
  [3] LocalVariable([1])      — pointer to local `b`
  [4] Literal(Float32(20.3))
  [5] Load { pointer: [1] }   — read `a`
  [6] Load { pointer: [3] }   — read `b`
  [7] Binary { op: Add, left: [5], right: [6] }

body:
  Store { pointer: [1], value: [2] }   — a = 10.5
  Store { pointer: [3], value: [4] }   — b = 20.3
  Emit([5..8])                          — evaluate [5], [6], [7]
  Return { value: Some([7]) }
```

Key: `in` params are lowered to LocalVariable + Store from FunctionArgument.
Non-param locals need actual WASM locals. Expression arena gives local count
upper bound.
