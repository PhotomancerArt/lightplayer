# Naga WASM POC — Design

## Scope

Minimal spike crate proving the path: GLSL → Naga IR → WASM bytes → wasmtime
execution. Scalar `float` only (no vectors, no builtins). Two numeric modes:
native f32 and Q32 (i32 16.16 fixed-point).

Secondary goal: validate that Naga's `glsl-in` frontend compiles under
`#![no_std]`.

## File structure

```
spikes/naga-wasm-poc/
├── Cargo.toml              # cfg_attr(not(test), no_std); naga (crates.io glsl-in), wasm-encoder; wasmtime dev-dep
├── README.md
├── src/
│   └── lib.rs              # parse + emit + Q32; param LocalVariable → WASM local mapping
└── tests/
    └── smoke.rs            # wasmtime execution tests
```

`naga` is taken from **crates.io** (v29) so CI clones do not require `../oss/wgpu`. Optional path override for local Naga hacking.

## Conceptual architecture

```
                    ┌──────────────┐
  GLSL source ────▶│ Naga Frontend │────▶ naga::Module
                    │  (glsl-in)   │       ├── functions[0].expressions: Arena<Expression>
                    └──────────────┘       ├── functions[0].arguments: Vec<FunctionArgument>
                                           └── functions[0].body: Block (Vec<Statement>)
                                                      │
                                    ┌─────────────────┤
                                    │                  │
                              [f32 mode]         [Q32 mode]
                                    │                  │
                                    ▼                  ▼
                            Walk IR, emit       Walk IR, rewrite
                            f32.add etc         i32.add + saturation
                                    │                  │
                                    └────────┬─────────┘
                                             ▼
                                     ┌──────────────┐
                                     │ wasm-encoder  │
                                     │  Function     │
                                     │  Module       │
                                     └──────┬───────┘
                                            ▼
                                      WASM bytes
                                            │
                                     [tests/smoke.rs]
                                            ▼
                                     wasmtime::Engine
                                     → validate + run
```

## Main components

### `compile(source, mode) -> Vec<u8>`

Public entry point. Parses GLSL via `naga::front::glsl::Frontend::parse()`,
then hands the `naga::Module` to the emitter.

### WASM emitter

Walks `Function.body` (a `Block` = `Vec<Statement>`) and evaluates expressions
by handle from `Function.expressions` (an `Arena<Expression>`).

Key Naga IR nodes used in the spike:

- `Expression::FunctionArgument(u32)` — maps to `local.get` of param index
- `Expression::Binary { op, left, right }` — maps to `f32.add` (f32) or
  `i32.add` + saturation (Q32)
- `Statement::Emit(range)` — marks expressions as "evaluated" (controls
  evaluation order)
- `Statement::Return { value }` — maps to the function's return

Local allocation: expression arena length gives an upper bound on needed locals.
Each expression handle maps 1:1 to a WASM local. Params get local indices
0..n_params, expression-locals get n_params..n_params+arena_len.

### Q32 mode

During emission (not as a separate IR pass), `float` binary ops are emitted as
`i32` operations:

- `f32.add` → `i32.add` (with i32 saturation clamp)
- params and return type become `i32` instead of `f32`

This mirrors the existing `lp-glsl-wasm` approach where Q32 is a 16.16
fixed-point representation stored in i32.

## Phases

1. Scaffold crate + validate no_std compilation
2. Float path: GLSL → Naga IR → f32 WASM → wasmtime test
3. Q32 transform: IR rewrite to i32 fixed-point → wasmtime test
4. Cleanup & validation
