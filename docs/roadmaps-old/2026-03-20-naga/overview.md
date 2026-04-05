# Naga Migration — Overview

## Motivation / rationale

The current WASM backend (`lps-wasm`) hit a fundamental architectural
limitation: WASM requires all local variables declared upfront in
`Function::new()`, but the single-pass tree-walk emitter doesn't know how many
it needs until after emission. This manifested as a hard-coded 8-slot scratch
pool that overflows for three-argument builtins (`smoothstep`, `mix`, `clamp`)
on vec3+. A naive bump allocator attempt caused a 100x slowdown (16k
pre-declared locals per function).

Naga, a pure-Rust `#![no_std]` shader compiler from the wgpu project, provides
a clean GLSL frontend and an expression-arena-based IR where the arena length
gives the local count upfront. A spike (`spikes/naga-wasm-poc`) confirmed: Naga
`glsl-in` compiles under `no_std`, the IR is straightforward to lower to WASM,
and both f32 and Q32 modes work.

Replacing the custom frontend with Naga eliminates ~8k lines of custom
parser/semantic analysis code, solves the local allocation problem by design,
and gives us a maintained GLSL frontend with potential future SPIR-V/WGSL
support.

## Architecture / design

```
lp-shader/
├── lps-naga/                # NEW: Naga-based frontend
│   └── src/
│       ├── lib.rs               # compile() entry point, wraps naga::front::glsl
│       └── builtins.rs          # LPFX prototype injection, #line reset
├── lps-wasm/                # REWRITE: Naga IR → WASM emission
│   └── src/
│       ├── lib.rs               # glsl_wasm() entry point
│       ├── emit.rs              # Walk naga::Module, emit wasm-encoder instructions
│       └── builtins.rs          # MathFunction → inline/import, lpfx → import
├── lps-frontend/            # UNCHANGED during migration (Cranelift uses it)
├── lps-cranelift/           # UNCHANGED during Phase I-II
├── lps-builtin-ids/         # UNCHANGED (shared by old and new stacks)
└── lps-filetests/           # UPDATE: wasm.q32 target uses new stack
```

Data flow (new stack):

```
GLSL source
    │
    ▼
lps-naga: prepend lpfx prototypes + #line 1
    │
    ▼
naga::front::glsl::Frontend::parse()
    │
    ▼
naga::Module
  ├── types: Arena<Type>
  ├── functions: Arena<Function>
  │     ├── expressions: Arena<Expression>  ← local count known
  │     ├── body: Block (Vec<Statement>)
  │     └── arguments, result, local_variables
  └── entry_points
    │
    ▼
lps-wasm: emit_module()
  ├── Expression::Math → inline WASM or BuiltinId import
  ├── Statement::Call (lpfx_*) → BuiltinId import
  ├── Expression::Binary → f32.add / i32.add (Q32)
  ├── vectors → scalarized emission
  └── wasm-encoder → WASM bytes
```

## Alternatives considered

- **Custom QBE-style IR** (`docs/roadmaps/2026-03-20-wasm-ir/design.md`):
  Would solve the local allocation problem but doesn't reduce frontend code.
  Still requires maintaining the custom parser and semantic analysis. More work
  for less benefit.
- **Adapter layer** (naga::Module → TypedShader): Would let existing backends
  work unchanged, but adds a fragile compatibility shim mapping between two IR
  representations. Rejected in favor of rewriting backends directly.
- **SPIR-V as intermediate**: Convert Naga IR to SPIR-V, then lower to WASM.
  Adds unnecessary indirection for the WASM target.

## Risks

- **ESP32 ROM size**: Naga is larger than the custom frontend (~13M rlib vs
  ~5M). With LTO and dead-code elimination the real delta is unknown. Must be
  measured empirically after Cranelift integration (Phase III).
- **Naga GLSL coverage**: Naga targets GLSL 450 / ES 320 but may not handle
  every construct identically to the custom parser. Filetests will surface
  discrepancies.
- **LPFX prototype injection**: The `#line 1` reset after prototypes depends
  on `pp-rs` handling `#line` correctly. Must verify early.
- **`out` parameter pattern**: `rainbow.glsl` uses `out` parameters
  (`lpfx_psrdnoise` writes to `gradient`). Naga models these differently from
  the custom frontend. Must handle during WASM emission.

## Phases

```
Phase I:   Scaffold lps-naga + rewrite lps-wasm foundation
Phase II:  Feature completeness — rainbow.glsl renders in web demo
Phase III: Cranelift backend port + lp-engine integration
Phase IV:  Cleanup, old frontend removal, validation
```
