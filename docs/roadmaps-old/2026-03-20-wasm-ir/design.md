# WASM Backend: Scalarized Structured IR

## Motivation

The current WASM backend is a single-pass tree-walk emitter. It walks the typed
GLSL AST and emits WASM instructions directly into `wasm_encoder::Function`.
This has a fundamental constraint: `Function::new(locals)` requires all locals
declared upfront, before any instructions are emitted. But the codegen doesn't
know how many scratch locals it needs until emission is complete.

The first attempt used fixed-size pools (8 slots), which is too small for
three-argument vector builtins. The second attempt pre-reserved 16k locals and
bump-allocated within that pool, causing 100x slowdown from massive stack frames
on the per-pixel hot path.

The root cause is coupling three concerns into one pass:
1. Vector-to-scalar decomposition (GLSL vec3 → 3 scalar WASM ops)
2. Scratch local allocation (how many temps does this expression need?)
3. WASM byte emission (instruction encoding)

An intermediate representation separates these concerns.

## Prior art

### QBE (c9x.me/compile)

A deliberately minimal compiler backend by Quentin Carbonneaux (~12k lines of
C). Demonstrates that a real SSA-based backend doesn't need LLVM-scale
complexity. Architecture: source → SSA IR (basic blocks, virtual registers) →
cheap passes (copy elim, const fold) → register allocation → emission.

Relevant ideas: simple IR with unlimited virtual registers; trivial "allocation"
where each virtual maps to a physical location; the IR is flat within basic
blocks (linear list of operations).

QBE targets register machines (x86, arm64). We target a stack machine (WASM),
so our emission step is different, but the IR structure is similar.

### Binaryen (github.com/WebAssembly/binaryen)

The reference WASM optimizer. Internal IR is expression trees with structured
control flow (block/loop/if). Can convert between "flat" form (every
intermediate gets a local.set/get) and "stacked" form (values flow through the
WASM operand stack).

Relevant ideas: structured control flow in the IR (not a CFG — no relooper
needed); "flat" form is essentially what our IR would look like.

### SPIR-V

SSA with structured control flow, designed for shaders. GLSL → SPIR-V is a
solved problem (glslang). The structure is: basic blocks containing SSA
operations, with structured merge/continue annotations on control flow.

Relevant idea: SSA + structured control flow coexist naturally for shader
languages.

## Design

### Core idea

A flat, scalarized IR with structured control flow and virtual register IDs.

- **Scalarized**: no vector types. `vec3 a + vec3 b` is lowered to three
  `F32Add` operations on separate virtual registers. Vector decomposition
  happens during lowering, not during WASM emission.
- **Structured control flow**: `If`/`Loop`/`Break`/`Continue` nodes, mapping
  1:1 to both GLSL source and WASM output. No CFG, no relooper.
- **Virtual registers**: unlimited. Each value gets a unique ID. No scratch
  pools, no slot limits. Final local count = number of unique registers used.

### IR definition (sketch)

```rust
type VReg = u32;

/// One scalar operation or control flow node.
enum Op {
    // --- Constants ---
    F32Const { dst: VReg, val: f32 },
    I32Const { dst: VReg, val: i32 },

    // --- Arithmetic (f32) ---
    F32Add { dst: VReg, lhs: VReg, rhs: VReg },
    F32Sub { dst: VReg, lhs: VReg, rhs: VReg },
    F32Mul { dst: VReg, lhs: VReg, rhs: VReg },
    F32Div { dst: VReg, lhs: VReg, rhs: VReg },
    F32Min { dst: VReg, lhs: VReg, rhs: VReg },
    F32Max { dst: VReg, lhs: VReg, rhs: VReg },
    F32Neg { dst: VReg, src: VReg },

    // --- Arithmetic (i32, used by Q32 path and int ops) ---
    I32Add { dst: VReg, lhs: VReg, rhs: VReg },
    I32Sub { dst: VReg, lhs: VReg, rhs: VReg },
    I32Mul { dst: VReg, lhs: VReg, rhs: VReg },
    I32DivS { dst: VReg, lhs: VReg, rhs: VReg },
    I32GtS { dst: VReg, lhs: VReg, rhs: VReg },
    I32LtS { dst: VReg, lhs: VReg, rhs: VReg },
    I32GeS { dst: VReg, lhs: VReg, rhs: VReg },
    I32And { dst: VReg, lhs: VReg, rhs: VReg },
    I32Eqz { dst: VReg, src: VReg },

    // --- Moves ---
    Copy { dst: VReg, src: VReg },

    // --- Q32 compound ops (lowered as instruction sequences) ---
    Q32MulSat { dst: VReg, lhs: VReg, rhs: VReg },
    Q32AddSat { dst: VReg, lhs: VReg, rhs: VReg },
    Q32SubSat { dst: VReg, lhs: VReg, rhs: VReg },
    Q32DivSat { dst: VReg, lhs: VReg, rhs: VReg },

    // --- Calls ---
    /// Call a WASM function. Args are registers, results written to dst regs.
    Call { dst: Vec<VReg>, func_idx: u32, args: Vec<VReg> },

    // --- Memory (Q32 builtin ABI) ---
    I32Store { addr: VReg, val: VReg, offset: u32 },
    I32Load { dst: VReg, addr: VReg, offset: u32 },

    // --- Control flow ---
    If { cond: VReg, then_body: Vec<Op>, else_body: Vec<Op> },
    /// If with result: like ternary. Result written to dst.
    IfValue { dst: VReg, cond: VReg, then_body: Vec<Op>, then_val: VReg,
              else_body: Vec<Op>, else_val: VReg },
    Loop { body: Vec<Op> },
    Break,
    Continue,
    Return { values: Vec<VReg> },
}

/// One function in IR form.
struct IrFunction {
    /// Parameter registers (pre-assigned).
    param_regs: Vec<(VReg, ValType)>,
    /// Return types.
    result_types: Vec<ValType>,
    /// Function body.
    body: Vec<Op>,
    /// Next free VReg (= total virtual registers allocated).
    next_vreg: u32,
}
```

### Pipeline

```
GLSL source
  → lps-frontend (parse, type-check)
  → TypedShader AST
  → IR lowering (new code, replaces current codegen/expr + codegen/stmt)
      - walks AST
      - scalarizes vectors: vec3 add → 3x F32Add on separate VRegs
      - inlines builtins: smoothstep → sequence of scalar ops
      - Q32 arithmetic → Q32MulSat/AddSat/SubSat ops
  → IrFunction (Vec<Op> + vreg count)
  → WASM emission (new code, much simpler than current emitter)
      - count vregs → declare that many locals
      - walk Vec<Op>, emit corresponding WASM instructions
      - each VReg maps 1:1 to a WASM local index
  → wasm_encoder::Function
  → .wasm bytes
```

### What each stage does

**IR lowering** (replaces `codegen/expr/` and `codegen/stmt/`):

This is structurally similar to the current emitter — it walks the AST and
pattern-matches on expression/statement types. The difference is it produces
`Op` values instead of calling `sink.f32_add()`. Vector scalarization is
explicit: when the lowering sees `vec3 + vec3`, it emits three `F32Add` ops
with separate dst/lhs/rhs VRegs for each component.

The lowering context tracks a VReg allocator (just a counter) and a map from
GLSL variable names to VRegs (one vreg per scalar component).

**WASM emission** (replaces `codegen/stmt/mod.rs` emit_function):

Walks the `Vec<Op>` and emits WASM. Each register-form op becomes a short
WASM instruction sequence:

```
F32Add { dst: v3, lhs: v1, rhs: v2 }
→  local.get v1
   local.get v2
   f32.add
   local.set v3

Q32MulSat { dst: v3, lhs: v1, rhs: v2 }
→  local.get v1
   i64.extend_i32_s
   local.get v2
   i64.extend_i32_s
   i64.mul
   i32.wrap_i64        (with shift + saturation clamp)
   local.set v3

If { cond: v0, then_body, else_body }
→  local.get v0
   if
     <emit then_body>
   else
     <emit else_body>
   end
```

The emitter is mechanical. No type inference, no vector logic, no scratch
allocation. It just maps ops to instructions. `next_vreg` tells it how many
locals to declare.

## Scope estimate

Current backend: ~4100 lines of codegen (expr/ + stmt/).

| Component | Est. lines | Notes |
|---|---|---|
| IR types (`ir.rs`) | ~150 | Op enum, IrFunction, VReg allocator |
| IR lowering (`lower/`) | ~1500 | Replaces expr/ + stmt/, similar structure |
| WASM emission (`emit.rs`) | ~300 | Mechanical: Op → WASM instructions |
| Module assembly | ~100 | Adapted from existing `codegen/mod.rs` |
| **Total new** | **~2050** | |
| **Total deleted** | **~4100** | Current expr/ + stmt/ + context.rs |

The lowering is the bulk of the work. It's structurally similar to the current
emitter — same AST walk, same pattern matching, same builtin dispatch. The
difference is writing `ops.push(Op::F32Add { dst, lhs, rhs })` instead of
`sink.local_get(lhs); sink.local_get(rhs); sink.f32_add(); sink.local_set(dst)`.
The vector scalarization logic (for-k-in-0..dim loops, broadcast, etc.) moves
from being interleaved with WASM emission to being explicit in the lowering.

Net: fewer total lines because the current code has a lot of boilerplate around
scratch pool management, error-checked pool access, and duplicated
Float-vs-Q32-path plumbing that becomes cleaner with explicit ops.

## What stays unchanged

- `lib.rs` — public API (`glsl_wasm()`)
- `module.rs` — `WasmModule` / `WasmExport` types
- `options.rs` — `WasmOptions`
- `types.rs` — GLSL→WASM type mapping
- `codegen/mod.rs` — module assembly (imports, exports, type section)
- `codegen/builtin_scan.rs` — pre-scan for builtin imports
- `codegen/builtin_wasm_import_types.rs` — import type signatures
- `codegen/memory.rs` — linear memory helpers
- `codegen/numeric.rs` — WasmNumericMode
- All tests (they test the public API, not internals)

## What gets deleted

- `codegen/context.rs` — replaced by IR lowering context (no scratch pools)
- `codegen/rvalue.rs` — WasmRValue not needed (IR tracks types via VRegs)
- `codegen/expr/*` — all 14 files, replaced by IR lowering
- `codegen/stmt/*` — all 8 files, replaced by IR lowering

## What gets created

- `codegen/ir.rs` — Op enum, IrFunction, VReg allocator
- `codegen/lower.rs` (or `codegen/lower/`) — AST → IR lowering
- `codegen/emit.rs` — IR → WASM emission

## Benefits

- **No scratch limits**: VRegs are unlimited. `smoothstep(vec4, vec4, vec4)`
  just uses 14 VRegs. No pools, no caps, no error paths for overflow.
- **Correct local count**: `next_vreg` is known before WASM emission starts.
  No pre-reservation, no 16k locals, no guessing.
- **Simpler WASM emitter**: the emitter never thinks about vectors, types,
  builtins, or GLSL semantics. It just maps scalar ops to WASM instructions.
- **Separation of concerns**: vector scalarization, Q32 lowering, and WASM
  emission are distinct stages instead of interleaved in one pass.
- **Future optimization**: dead register elimination, constant folding,
  register reuse (liveness analysis) — all become possible as IR passes.
  Not needed now, but the door is open.

## Risks

- **VReg-per-value creates many locals**: every intermediate gets its own
  WASM local. A complex shader might use hundreds. This is fine for WASM
  engines (locals are a flat stack array), but if it becomes a concern, a
  trivial linear-scan pass can merge non-overlapping VRegs later.
- **Q32 compound ops**: `Q32MulSat` etc. expand to multi-instruction
  sequences in the emitter. This is still simpler than the current approach
  where Q32 arithmetic is interleaved with scratch management, but it's not
  trivial — the saturation logic has branches.

## Phasing

### Phase 1: IR types + WASM emitter

Define `Op`, `IrFunction`. Write the emitter that converts `Vec<Op>` to
`wasm_encoder::Function`. Test with hand-built IR (unit tests that construct
`IrFunction` directly and verify the WASM output via wasmtime).

This is self-contained and testable before the lowering exists.

### Phase 2: IR lowering — scalar expressions

Lower literals, variables, scalar arithmetic, unary ops, assignments, ternary.
No vectors yet. Scalar filetests should pass.

### Phase 3: IR lowering — control flow

Lower if/else, for, while, do-while, break, continue, return. The existing
loop/if tests should pass.

### Phase 4: IR lowering — vectors

Vector constructors, component access (swizzle), vector arithmetic (scalarized
binary ops). This is where the architecture pays off — the scalarization is
explicit and clean.

### Phase 5: IR lowering — builtins

Inline builtins (smoothstep, mix, clamp, abs, sign, floor, fract, mod, min,
max, step). Q32 imported builtins (sin, cos, exp, etc.). LPFX calls.

The smoothstep vec3 case that started this whole effort should work without
any special handling — it's just more VRegs.

### Phase 6: Delete old codegen

Remove `codegen/expr/`, `codegen/stmt/`, `context.rs`, `rvalue.rs`.
All filetests pass on the new pipeline.
