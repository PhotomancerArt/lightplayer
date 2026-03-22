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
  │   • scalarizes vectors: vec3 add → 3× float.add
  │   • decomposes builtins: smoothstep → scalar math
  │   • handles LPFX calls and out-pointer ABI
  │   • caches Handle<Expression> → VReg
  │   • VReg allocator: monotonic counter
  │
  ▼
IrFunction { body: Vec<Op>, vreg_count, vreg_types }   ── LPIR (float-agnostic)
  │
  ├──────────────────────────┐
  │ (Float mode: pass-through)  │ (Q32 mode: transform)
  │                          ▼
  │                    Q32 transform (LPIR → LPIR)
  │                      • float.add → i64 widen + add + saturate
  │                      • float.const 1.5 → i32.const 98304
  │                      • consuming transform (move semantics)
  │                      • VReg types Float → Sint
  │                          │
  ├──────────────────────────┘
  ▼
IrFunction (mode-specific)
  │
  ┌──────┴──────┐
  ▼             ▼
WASM emitter   CLIF emitter (future)
(lp-glsl-wasm) (lp-glsl-cranelift)
  │             │
  ▼             ▼
.wasm bytes    machine code
```

### Crate structure

```
lp-glsl/
├── lpir/                    # NEW: LPIR core library
│   └── src/
│       ├── lib.rs           #   Op enum, IrFunction, ScalarKind, VReg
│       ├── builder.rs       #   Builder API for constructing IR
│       ├── print.rs         #   IR → text format
│       ├── parse.rs         #   text format → IR
│       ├── interp.rs        #   interpreter/emulator for testing
│       └── q32.rs           #   Q32 consuming transform (LPIR → LPIR)
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
- **Scalarized**: No vector types. Vector ops decomposed during lowering.
- **Non-SSA**: VRegs can be reassigned (loop variables, mutable locals).
- **Structured control flow**: `if`/`loop`/`break`/`continue`/`return`.
  No CFG, no basic blocks.
- **Float-mode-agnostic**: `float.add` not `f32.add`. Float mode is handled
  by an optional Q32 transform pass (LPIR → LPIR), not by the lowering or
  emitter.
- **Typed**: Each VReg has a ScalarKind (Float, Sint, Uint, Bool).

### Text format

```
; LPIR text format — comments start with ;

func @smoothstep(v0:float, v1:float, v2:float) -> float {
  v3:float = float.sub v1, v0
  v4:float = float.sub v2, v0
  v5:float = float.div v4, v3
  v6:float = float.const 0.0
  v7:float = float.max v5, v6
  v8:float = float.const 1.0
  v9:float = float.min v7, v8
  v10:float = float.mul v9, v9
  v11:float = float.const 3.0
  v12:float = float.const 2.0
  v13:float = float.mul v12, v9
  v14:float = float.sub v11, v13
  v15:float = float.mul v10, v14
  return v15
}

func @conditional(v0:float) -> float {
  v1:float = float.const 0.0
  v2:bool = float.lt v0, v1
  if v2 {
    v3:float = float.neg v0
    return v3
  }
  return v0
}

func @loop_sum(v0:int) -> int {
  v1:int = i32.const 0
  v2:int = i32.const 0
  loop {
    v3:bool = i32.lt_s v2, v0
    br_if_not v3
    v1 = i32.add v1, v2
    v2 = i32.add v2, i32.const 1
    continue
  }
  return v1
}
```

Syntax summary:
- `vN:type` — VReg definition (type on first definition, bare `vN` after)
- `@name` — function/global name
- `float.*`, `i32.*`, `i64.*`, `bool.*` — type-prefixed operations
- `if`/`loop`/`break`/`continue`/`return`/`br_if_not` — control flow
- `call @name(args)` — function call
- `i32.store`/`i32.load` — memory ops (LPFX ABI)
- `;` — line comments

### Design decisions

**Non-SSA**: Both target runtimes (WASM JIT engines, Cranelift) perform their
own SSA construction and optimization. Making LPIR SSA would complicate the
lowering (explicit merge nodes) and the WASM emitter (de-SSA) for optimization
capability we'd rarely use. Simple non-SSA optimizations (constant folding,
dead VReg elimination, liveness-based local reuse) remain possible.

**Q32 as a transform pass (not in emitter or lowering)**: The lowering is
mode-unaware. The Q32 transform is a shared LPIR → LPIR pass that rewrites
float ops to i32/i64 sequences. This gives us:
- Clean separation: lowering = semantics, transform = mode, emitter = target
- Shared Q32 logic: written once, both backends benefit
- Simple emitters: they just map IR ops to target instructions
- Memory-efficient: the transform consumes the input via Rust move semantics,
  so only one block level is "doubled" at any time

This mirrors the original Cranelift backend's approach (f32 CLIF → Q32
transform), but LPIR is designed for mutation (plain enums, separate type
array, no SSA constraints), unlike CLIF which made type-changing transforms
painful.

**Structured control flow**: WASM requires it. Structured → CFG (for Cranelift)
is the easy direction. Avoids the relooper problem.

**Scalarized**: Scalarization is a middle-end concern, not a backend concern.
Doing it once in the lowering means backends never think about vectors.

**Interpreter**: The lpir crate includes an interpreter for testing. This
enables testing the lowering, Q32 transform, and hand-written LPIR without
any backend. Essential for isolating bugs and keeping tests fast.

## Alternatives considered

### TempStack (stack-discipline allocator in existing emitter)

Correct but doesn't address structural coupling. Would be the third patch on
the same root cause. Keeps scalarization interleaved with emission.

### Q32 in the emitter (mode-agnostic IR, mode-aware emitter)

Each backend independently implements Q32 expansion — duplicated logic. The
emitter needs "hidden" locals outside the VReg model. And Q32 expansion is
different per target (WASM: inline i64 sequences; CLIF: iadd or builtin calls),
so the sharing benefit is limited.

### Q32 in the lowering (mode-aware lowering)

The lowering becomes more complex. Same GLSL produces different LPIR per mode.
Can't test the lowering independently of mode concerns.

### Full SSA IR with register allocation

Overkill. WASM engines and Cranelift both optimize. The complexity of SSA
construction, phi nodes, and register allocation isn't justified for our
use case.

## Risks

- **VReg-per-value creates many WASM locals**: WASM engines handle this fine.
  A liveness pass for local reuse can be added later if needed.

- **Rewrite scope**: ~3100 lines replaced in lp-glsl-wasm. But the lowering
  is structurally similar to existing code and net lines likely decrease.

- **Q32 transform memory**: The consuming transform creates a new Vec<Op> per
  block while consuming the old one. Peak memory is one block's worth of
  duplication per recursion level. Acceptable for our shader sizes.

- **Two-backend maintenance**: The CLIF emitter is future work. If LPIR
  design decisions don't work for CLIF, we'll discover it then. Mitigation:
  we've verified the mapping is natural (VReg → Variable, structured → blocks).

- **IR ownership cost**: A text format, parser, interpreter, and Q32 transform
  are significant to maintain. Justified by the testing and debugging benefits,
  and by sharing across backends.

## Scope estimate

| Component | Est. lines | Location |
|---|---|---|
| IR types + builder | ~200 | lpir/src/ |
| Text printer | ~150 | lpir/src/print.rs |
| Text parser | ~300 | lpir/src/parse.rs |
| Interpreter | ~250 | lpir/src/interp.rs |
| Q32 transform | ~300 | lpir/src/q32.rs |
| Naga → LPIR lowering (scalar) | ~800 | lp-glsl-naga/src/lower.rs |
| LPIR → WASM emission | ~300 | lp-glsl-wasm/src/emit.rs |
| Tests | ~400 | across crates |
| **Total new** | **~2700** | |
| **Total deleted** | **~3100** | emit.rs + emit_vec.rs + locals.rs |
