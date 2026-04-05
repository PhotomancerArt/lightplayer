# LPIR: LightPlayer Intermediate Representation

## Motivation

The LightPlayer shader pipeline has two backends (WASM, Cranelift) and is
migrating from a custom GLSL frontend to Naga. The WASM backend currently
emits directly from Naga's expression arena in a single pass, coupling
vector scalarization, temporary local allocation, and WASM byte emission.
This causes scratch local aliasing bugs for nested expressions — a problem
we've now hit twice (with the old frontend and with Naga).

Separately, the two backends share no middle-end code. Each independently
handles vector scalarization, builtin decomposition, LPFX calls, and
GLSL-to-target translation.

LPIR solves both problems:

1. **Correct local allocation by construction**: Virtual registers are
   unlimited and each maps 1:1 to a target local/value. The register count
   is known before emission. No scratch pools, no aliasing.

2. **Shared middle-end**: The Naga → LPIR lowering handles scalarization,
   builtin decomposition, and LPFX — once. Each backend only implements
   mechanical IR-to-target translation.

## Architecture

### Pipeline

```
GLSL source
  │
  ▼
Naga frontend (parse + type check)            ── existing, unchanged
  │
  ▼
naga::Module (expression arena + statements)  ── existing, unchanged
  │
  ▼  lp-glsl-naga  ─── mode-UNAWARE ─────────── middle-end (shared)
  │   • walks Naga expression arena + statements
  │   • scalarizes vectors: vec3 add → 3× fadd
  │   • decomposes builtins: smoothstep → scalar math
  │   • handles LPFX calls and out-pointer ABI
  │   • caches Handle<Expression> → VReg
  │   • VReg allocator: monotonic counter
  │
  ▼
IrFunction { body: Vec<Op>, vreg_count, vreg_types }   ── LPIR (float-agnostic)
  │
  ┌──────┴──────┐
  ▼             ▼
WASM emitter   CLIF emitter (future)
(lp-glsl-wasm) (lp-glsl-cranelift)
  │ mode-AWARE  │ mode-AWARE
  │ • Float: fadd → f32.add
  │ • Q32:   fadd → i64 widen + add + saturate
  │             │ • Float: fadd → Cranelift fadd
  │             │ • Q32:   fadd → call @__lp_q32_add
  ▼             ▼
.wasm bytes    machine code
```

### Crate structure

```
lp-shader/
├── lpir/                    # NEW: LPIR core library
│   └── src/
│       ├── lib.rs           #   Op enum, IrFunction, IrType, VReg
│       ├── builder.rs       #   Builder API for constructing IR
│       ├── print.rs         #   IR → text format
│       ├── parse.rs         #   text format → IR
│       └── interp.rs        #   interpreter/emulator for testing
├── lp-glsl-naga/            # UPDATE: add Naga → LPIR lowering
│   └── src/
│       ├── lib.rs           #   existing: compile(), LPFX injection
│       └── lower.rs         #   NEW: Naga Module → Vec<IrFunction>
├── lp-glsl-wasm/            # REWRITE: LPIR → WASM emission
│   └── src/
│       ├── lib.rs           #   public API (unchanged interface)
│       ├── emit.rs          #   REWRITE: IrFunction → wasm_encoder
│       ├── lpfx.rs          #   UPDATE: LPFX import detection
│       ├── options.rs       #   unchanged
│       ├── types.rs         #   UPDATE: simplify
│       ├── locals.rs        #   DELETE
│       └── emit_vec.rs      #   DELETE
└── lp-glsl-cranelift/       # FUTURE: LPIR → CLIF emission
```

### IR classification

LPIR is a **flat, scalarized, non-SSA IR with structured control flow and
virtual registers**.

- **Flat / ANF-style**: Every intermediate value is bound to a named virtual
  register. No expression trees.
- **Scalarized** (initially): No vector types. Vector ops decomposed during
  lowering. Vector types and ops (v2f32, v4f32, v4i32, etc.) are a planned
  future extension for SIMD backends — WASM v128 and Cranelift/ESP32-P4 PIE
  (128-bit, 4×f32). The scalar-only design does not preclude this: adding
  vector types to the VReg type system and vector ops to the op set is
  additive, and the lowering can choose to scalarize or preserve vectors
  per-backend. Scalarization (vectors → scalars) is the easy direction;
  re-vectorization (scalars → vectors) is hard, so the future path is to
  stop scalarizing in the lowering, not to re-vectorize after.
- **Non-SSA**: VRegs can be reassigned (loop variables, mutable locals).
- **Structured control flow**: `if`/`loop`/`break`/`continue`/`return`.
  No CFG, no basic blocks.
- **Float-mode-agnostic**: `fadd` not `f32.add` or Q32 i64-widen. The IR
  expresses GLSL semantics. Float mode (f32 vs Q32) is handled by each
  backend's emitter, not by the lowering or by an IR transform.
- **Width-aware VReg types**: Each VReg has a concrete type (`f32`, `i32`).
  Boolean conditions and comparison results use `i32` (`0` / nonzero), matching
  WASM.
  Op names use short CLIF-style prefixes (`fadd`, `isub`, `fconst`) without
  embedded widths; the VReg type resolves any ambiguity.

### Text format

```
; LPIR text format — comments start with ;

func @smoothstep(v0:f32, v1:f32, v2:f32) -> f32 {
  v3:f32 = fsub v1, v0
  v4:f32 = fsub v2, v0
  v5:f32 = fdiv v4, v3
  v6:f32 = fconst.f32 0.0
  v7:f32 = fmax v5, v6
  v8:f32 = fconst.f32 1.0
  v9:f32 = fmin v7, v8
  v10:f32 = fmul v9, v9
  v11:f32 = fconst.f32 3.0
  v12:f32 = fconst.f32 2.0
  v13:f32 = fmul v12, v9
  v14:f32 = fsub v11, v13
  v15:f32 = fmul v10, v14
  return v15
}

func @conditional(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v3:f32 = fneg v0
    return v3
  }
  return v0
}

func @loop_sum(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 0
  loop {
    v3:i32 = ilt_s v2, v0
    br_if_not v3
    v1 = iadd v1, v2
    v2 = iadd_imm v2, 1
    continue
  }
  return v1
}
```

Syntax summary:

- `vN:type` — VReg definition (type on first definition, bare `vN` after)
- `@name` — function/global name
- `f*` — float ops (`fadd`, `fmul`, `fconst.f32`, `flt`, `fneg`, etc.)
- `i*` — integer ops (`iadd`, `iadd_imm`, `iconst.i32`, `ilt_s`, etc.)
- `if`/`loop`/`break`/`continue`/`return`/`br_if_not` — control flow
- `call @name(args)` — function call
- `store`/`load` — memory ops (slots, out/inout, LPFX scratch)
- `;` — line comments

### Design decisions

**Non-SSA**: Both target runtimes (WASM JIT engines, Cranelift) perform their
own SSA construction and optimization. Making LPIR SSA would complicate the
lowering (explicit merge nodes) and the WASM emitter (de-SSA) for optimization
capability we'd rarely use. Simple non-SSA optimizations (constant folding,
dead VReg elimination, liveness-based local reuse) remain possible.

**Q32 in the emitter (not as an IR transform)**: The lowering is mode-unaware
and emits abstract float ops (`fadd`, `fmul`). Each backend's emitter handles
Q32 expansion internally based on `FloatMode`. This is the right design
because Q32 strategies are fundamentally backend-specific:

- WASM: inline i64 sequences (extend, add, saturate, wrap) — native i64
- Cranelift saturating: builtin calls (`__lp_q32_add`) — riscv32 lacks i64 div
- Cranelift wrapping: all-i32 using `imul`+`smulhi`+shifts — no i64 at all

A shared LPIR→LPIR transform would have to pick one representation, forcing
at least one backend to undo and redo the work. Keeping Q32 in the emitter
means each backend uses its optimal strategy while the IR stays clean:
`f32` and `i32` only — no `bool` type (conditions are `i32`), no i64 in the IR.

**Structured control flow**: WASM requires it. Structured → CFG (for Cranelift)
is the easy direction. Avoids the relooper problem.

**Scalarized** (initially): Scalarization is a middle-end concern, not a
backend concern. Doing it once in the lowering means backends never think
about vectors. Future SIMD support (WASM v128, ESP32-P4 PIE 4×f32) will
add vector types and ops to LPIR as an extension. The lowering will then
preserve vectors for SIMD-capable backends and scalarize only for scalar
backends. This is additive — the scalar op set remains valid and unchanged.

**Interpreter**: The lpir crate includes an interpreter for testing. This
enables testing the lowering and hand-written LPIR without any backend.
Essential for isolating bugs and keeping tests fast.

**GPU-aligned numeric semantics**: LPIR's edge-case behavior is modeled on
GPU shader execution, not WASM or general-purpose CPU semantics. The core
rule: no LPIR op traps. Integer **division and remainder by zero** are
defined to produce **`0`** on all backends (WASM uses a guard; Cranelift
matches, not raw RISC-V div-by-zero hardware results). IEEE 754 float,
wrapping integer arithmetic, shift amounts masked to 5 bits, saturating
float-to-int casts (`ftoi_sat_*`). Q32 emitters preserve the same saturating
intent in fixed-point space.

**Memory**: Well-formed LPIR assumes **in-bounds** access; lowering inserts
bounds checks for dynamic indexing. OOB is not specified — a pipeline bug.

**Entry**: At most **one** `entry func` per module — the runtime entry point.
All functions remain visible and callable by the host (emitter concern, not
IR annotation). Unresolved **import module** on a target → **emitter error**.

A future diagnostic "safe mode" may warn on edge cases without changing
results.

## Alternatives considered

### TempStack (stack-discipline allocator in existing emitter)

Correct but doesn't address structural coupling. Would be the third patch on
the same root cause. Keeps scalarization interleaved with emission.

### Q32 as a shared LPIR → LPIR transform

Appealing in theory: write Q32 logic once, both backends benefit. But the
backends use fundamentally different Q32 strategies (WASM: inline i64, Cranelift
saturating: builtin calls, Cranelift wrapping: all-i32). A shared transform
would have to pick one and the others would need to undo it. The "shared"
part is really just a dispatch table of which ops need Q32 treatment — not
enough to justify i64 in the IR and a consuming transform pass.

### Q32 in the lowering (mode-aware lowering)

The lowering becomes more complex. Same GLSL produces different LPIR per mode.
Can't test the lowering independently of mode concerns.

### Use SPIR-V instead of a custom IR

Naga can emit SPIR-V and it's a well-specified IR with structured control
flow. But three properties make it a poor fit:

1. **SSA**: SPIR-V is SSA — each value defined once, with phi nodes for
   control flow merges. Our targets don't benefit: WASM uses mutable locals
   (we'd need de-SSA), and Cranelift does its own SSA construction. SSA
   in our IR would complicate the lowering (explicit merge nodes for loops
   and if-branches) for no optimization payoff.

2. **Not scalarized**: SPIR-V has first-class `vec3`, `mat4`, etc. We need
   scalarization for both backends. Using SPIR-V means we still need a
   scalarization pass between SPIR-V and emission — at which point we've
   built the same middle-end layer, just consuming a more complex input.

3. **Size and complexity**: SPIR-V's type system includes structs, arrays,
   pointers, images, samplers, and more. It's a binary format requiring a
   parser library. LPIR has two types (f32, i32), zero dependencies,
   and fits in `no_std`. The surface area difference is enormous.

Beyond these, SPIR-V has no concept of LPFX, Q32, or our calling conventions.
We'd still need the full middle-end (LPFX ABI expansion, builtin
decomposition, mode-aware emission). SPIR-V is designed for GPU driver
consumption; LPIR is designed for two specific software backends.

### Full SSA IR with register allocation

Overkill. WASM engines and Cranelift both optimize. The complexity of SSA
construction, phi nodes, and register allocation isn't justified for our
use case.

## Risks

- **VReg-per-value creates many WASM locals**: WASM engines handle this fine.
  A liveness pass for local reuse can be added later if needed.

- **Rewrite scope**: ~3100 lines replaced in lp-glsl-wasm. But the lowering
  is structurally similar to existing code and net lines likely decrease.

- **Two-backend maintenance**: The CLIF emitter is future work. If LPIR
  design decisions don't work for CLIF, we'll discover it then. Mitigation:
  we've verified the mapping is natural (VReg → Variable, structured → blocks).

- **Q32 duplication**: Each backend implements Q32 emission independently. This
  is accepted because the strategies are fundamentally different (inline i64 vs
  builtin calls vs all-i32). The shared part (which ops need Q32 treatment) is
  a small dispatch table, not worth an IR transform layer.

- **IR ownership cost**: A text format, parser, and interpreter are significant
  to maintain. Justified by the testing and debugging benefits, and by sharing
  the lowering across backends.

## Future work (beyond Stage VII)

- **Cranelift backend migration**: Rewrite `lp-glsl-cranelift` to consume
  LPIR. Multi-return calling convention (StructReturn for large tuples like
  `mat4` → 16× `f32`) is a known implementation task for the Cranelift
  `GlslExecutable`.

- **SIMD / vector types**: Add `v2f32`, `v4f32`, `v4i32` to the IR type
  system and corresponding vector ops. The lowering stops scalarizing for
  SIMD-capable backends (WASM v128, ESP32-P4 PIE).

- **LPIR optimizations**: Dead VReg elimination, constant folding, liveness-
  based local reuse. Not needed for correctness but reduce output size.

- **Diagnostic safe mode**: Interpreter / validator flag that warns on
  div-by-zero, NaN inputs, out-of-range casts, OOB memory. Never changes
  results.

## Scope estimate

| Component                       | Est. lines | Location                          |
|---------------------------------|------------|-----------------------------------|
| IR types + builder              | ~200       | lpir/src/                         |
| Text printer                    | ~150       | lpir/src/print.rs                 |
| Text parser                     | ~300       | lpir/src/parse.rs                 |
| Interpreter                     | ~250       | lpir/src/interp.rs                |
| Naga → LPIR lowering (scalar)   | ~800       | lp-glsl-naga/src/lower.rs         |
| LPIR → WASM emission (incl Q32) | ~500       | lp-glsl-wasm/src/emit.rs          |
| Tests                           | ~400       | across crates                     |
| **Total new**                   | **~2600**  |                                   |
| **Total deleted**               | **~3100**  | emit.rs + emit_vec.rs + locals.rs |
