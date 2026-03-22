# LPIR Stage I: Language Specification — Design

## Scope

Define the complete LPIR language specification. The deliverable is a set of
chapters in `docs/lpir/` covering: type system, core op set, math builtins,
memory model, call conventions, control flow, text format grammar, operation
semantics, and GLSL → LPIR mapping. No Rust code.

## File structure

```
docs/plans/2026-03-21-lpir-stage-i/
├── 00-notes.md                 # Questions and answers
├── 00-design.md                # This file
└── 01-*.md … 0N-*.md           # Implementation phases

docs/lpir/                      # THE DELIVERABLE: LPIR specification
├── 00-overview.md              # IR classification, motivation, design decisions
├── 01-types-and-vregs.md       # Type system (f32, i32), VReg semantics
├── 02-core-ops.md              # Arithmetic, comparison, logic, casts, constants, _imm
├── 03-memory.md                # Pointer model, slots, load/store/memcpy/slot_addr
├── 04-control-flow.md          # if/else, loop, break, continue, br_if_not, return
├── 05-calls.md                 # Function declarations, call op, multi-return
├── 06-mathcall.md              # MathCall mechanism, MathFunc enumeration
├── 07-text-format.md           # Grammar, lexical rules, _imm syntax
├── 08-glsl-mapping.md          # Naga expression/statement/vector mapping tables
└── 09-future.md                # Reserved ops, vector types, planned extensions
```

The `docs/lpir/` directory is the long-lived specification. It includes
the what, the why, and the design rationale (including why not SPIR-V,
why not SSA, why Q32 is in the emitter, etc.) — absorbing content from
`docs/roadmaps/2026-03-21-lpir/overview.md` into the overview chapter.

## Conceptual architecture

### Type system

Two scalar types, width-aware:

| Type  | Width   | Description                     |
|-------|---------|---------------------------------|
| `f32` | 4 bytes | IEEE 754 single-precision float |
| `i32` | 4 bytes | 32-bit integer (signedness per op) |

**Boolean conditions** use `i32`: `0` is false, any nonzero value is true
(same as WASM `i32` conditions). Comparisons produce `i32` (`0` or `1`).
GLSL `bool` lowers away in Naga → LPIR; there is no separate `bool` type.

No i64 in the IR (Q32 widens in the emitter). No vector types in v1
(scalarized during lowering). **Future extensions** may add 64-bit scalars,
vectors (e.g. v128 / PIE), etc., as additive chapters.

No pointer type (pointers are i32 VRegs holding addresses).

Signed vs unsigned is determined by the op, not the type (e.g. `ilt_s` vs
`ilt_u`), matching WASM's approach. There is no separate `u32` type.

### VRegs

- Named `v0`, `v1`, `v2`, ... (monotonic allocation per function).
- Each VReg has a concrete type annotation on first definition: `v0:f32`.
- Subsequent uses are bare: `v0`.
- Non-SSA: VRegs can be reassigned (loop variables, mutable locals).
- VReg count is known before emission (stored in `IrFunction`).

### Slots

- Function-level metadata declaring addressable memory: `slot ss0, 64`.
- Not runtime ops — sizes are static, known at compile time.
- `slot_addr ssN` is a runtime op returning the i32 base address.
- Named `ss0`, `ss1`, ... (CLIF-style). Map to Cranelift `StackSlot` and
  WASM shadow stack frames.

### Op categories

```
┌─────────────────────────────────────────────────┐
│                    LPIR Ops                      │
│                                                  │
│  ┌───────────────────────────────────────┐       │
│  │ Core Ops  (~20 op variants)           │       │
│  │                                       │       │
│  │  Arithmetic:                          │       │
│  │    fadd, fsub, fmul, fdiv, fneg       │       │
│  │    iadd, isub, imul,                  │       │
│  │    idiv_s, idiv_u, irem_s, irem_u,    │       │
│  │    ineg                               │       │
│  │                                       │       │
│  │  Comparison:                          │       │
│  │    feq, fne, flt, fle, fgt, fge       │       │
│  │    ieq, ine,                          │       │
│  │    ilt_s, ile_s, igt_s, ige_s,        │       │
│  │    ilt_u, ile_u, igt_u, ige_u         │       │
│  │                                       │       │
│  │  Logic / Bitwise:                     │       │
│  │    iand, ior, ixor, ishl,             │       │
│  │    ishr_s, ishr_u, ibnot              │       │
│  │    (bool as i32 0/1: iand, ior, ieq)  │       │
│  │                                       │       │
│  │  Constants:                           │       │
│  │    fconst.f32, iconst.i32             │       │
│  │                                       │       │
│  │  Casts:                               │       │
│  │    ftoi_sat_s, ftoi_sat_u,            │       │
│  │    itof_s, itof_u                    │       │
│  │                                       │       │
│  │  Misc:                                │       │
│  │    select, copy                       │       │
│  └───────────────────────────────────────┘       │
│                                                  │
│  ┌───────────────────────────────────────┐       │
│  │ Memory Ops                            │       │
│  │    load, store, slot_addr, memcpy     │       │
│  └───────────────────────────────────────┘       │
│                                                  │
│  ┌───────────────────────────────────────┐       │
│  │ Control Flow                          │       │
│  │    if/else, loop, break, continue,    │       │
│  │    return, br_if_not                  │       │
│  └───────────────────────────────────────┘       │
│                                                  │
│  ┌───────────────────────────────────────┐       │
│  │ Calls                                 │       │
│  │    call  (unified op — import vs      │       │
│  │           func distinguished in       │       │
│  │           function declarations)      │       │
│  └───────────────────────────────────────┘       │
│                                                  │
│  ┌───────────────────────────────────────┐       │
│  │ MathCall  (extensible, SPIR-V style)  │       │
│  │                                       │       │
│  │  Float:                               │       │
│  │    fmod,                              │       │
│  │    fmin, fmax, fabs, fround,          │       │
│  │    ffloor, fceil, ftrunc, ffract,     │       │
│  │    fsin, fcos, ftan,                  │       │
│  │    fasin, facos, fatan, fatan2,       │       │
│  │    fsinh, fcosh, ftanh,              │       │
│  │    fpow, fexp, fexp2, flog, flog2,    │       │
│  │    fsqrt, finversesqrt,               │       │
│  │    fmix, fstep, fsmoothstep,          │       │
│  │    fclamp, ffma, fsign,               │       │
│  │    fldexp, ffrexp                     │       │
│  │                                       │       │
│  │  Integer:                             │       │
│  │    imin_s, imax_s, iabs_s,            │       │
│  │    imin_u, imax_u,                    │       │
│  │    iclamp_s, iclamp_u                 │       │
│  └───────────────────────────────────────┘       │
│                                                  │
│  ┌───────────────────────────────────────┐       │
│  │ Future Extensions (reserved)          │       │
│  │    switch, relational (any/all)       │       │
│  └───────────────────────────────────────┘       │
└─────────────────────────────────────────────────┘
```

### Op naming conventions

- **Short CLIF-style prefixes**: `f` = float, `i` = integer.
- **No width in op names** (except constants): width comes from the VReg
  type annotation. Constants use explicit type suffix: `iconst.i32`,
  `fconst.f32` (CLIF-style).
- **Immediate variants**: `_imm` suffix for ops with inline constant
  operands: `iadd_imm v1, 42` instead of `iadd v1, v2`.
- **Signed/unsigned suffix**: `_s` / `_u` where signedness matters
  (e.g. `ilt_s`, `idiv_u`).
- **MathCall ops**: prefixed by type (`fsin`, `imin_s`), called via
  `mathcall` keyword.

### Memory model

Pointers are i32 VRegs. No special pointer type.

- `load base, offset` — load scalar from `base + offset`. Result type from
  VReg annotation.
- `store base, offset, value` — store scalar to `base + offset`. Type from
  value's VReg type.
- `slot_addr ssN` — get base address of declared slot.
- `memcpy dst, src, size` — bulk copy. Maps to WASM `memory.copy`,
  Cranelift `emit_small_memory_copy` / `call_memcpy`.

Pointer arithmetic via regular `iadd` / `imul`. Alignment defaults to
natural (4 bytes for f32/i32).

Use cases:
- LPFX out-pointer ABI (scratch at known base)
- Out/inout function parameters
- Local arrays (slot + dynamic index)
- Globals via context pointer parameter

### Call conventions

Single `call` op. Function declarations distinguish linkage. Multi-return
supported for scalarized vector/matrix results.

```
import @__lp_q32_add(i32, i32) -> i32              ; imported (Q32 builtin)
import @__lpfx_noise3(i32, i32, i32, i32) -> (i32, i32, i32)  ; imported (LPFX, multi-return)
export func @shader_main(v0:i32) -> f32 { ... }    ; exported entry point
func @my_helper(v0:f32, v1:f32) -> f32 { ... }     ; local, single return
func @vec3_fn(v0:f32) -> (f32, f32, f32) { ... }   ; local, multi-return
```

`export` marks functions callable from the host. The emitter uses this to
generate WASM exports or Cranelift entry points. Most modules have one
export (the shader entry); the concept is cheap to support and avoids
out-of-band configuration.

Multi-return call syntax:
```
v4:f32, v5:f32, v6:f32 = call @vec3_fn(v0)
```

Multi-return target mapping: WASM native multi-value, Cranelift
multi-return or automatic StructReturn for large return counts.

The lowering expands LPFX ABI (store args → call → load results) into
explicit LPIR ops. The emitter maps `import` to WASM imports / Cranelift
`ExternalName`.

### Control flow

Structured control flow matching Naga/WASM:

```
if v2 {                ; branch on i32 condition (0 = false)
  ...
} else {
  ...
}

loop {                 ; infinite loop (exit via break/br_if_not)
  ...
  br_if_not v3         ; break if v3 is false
  ...
  continue             ; jump to loop header
}

return v15             ; return value (or bare `return` for void)
```

No CFG, no basic blocks. Maps 1:1 to WASM `if`/`loop`/`block`.
Structured → CFG conversion (for Cranelift) is the easy direction.

### Float mode handling

LPIR is float-mode-agnostic. The IR expresses GLSL semantics (`fadd`,
`fmul`) and each backend's emitter handles Q32 expansion internally:

- WASM: `fadd` → inline `i64.extend_s`, `i64.add`, saturate, `i32.wrap`
- Cranelift saturating: `fadd` → `call @__lp_q32_add`
- Cranelift wrapping: `fadd` → `iadd` (i32, no saturation)

No Q32-specific ops in the IR. No i64 type. Float mode is a parameter
to the emitter, not visible in LPIR.

### Text format example

```
; LPIR text format

import @__lp_q32_add(i32, i32) -> i32

func @smoothstep(v0:f32, v1:f32, v2:f32) -> f32 {
  v3:f32 = fsub v1, v0
  v4:f32 = fsub v2, v0
  v5:f32 = fdiv v4, v3
  v6:f32 = fconst.f32 0.0
  v7:f32 = mathcall fmax(v5, v6)
  v8:f32 = fconst.f32 1.0
  v9:f32 = mathcall fmin(v7, v8)
  v10:f32 = fmul v9, v9
  v11:f32 = fconst.f32 3.0
  v12:f32 = fconst.f32 2.0
  v13:f32 = fmul v12, v9
  v14:f32 = fsub v11, v13
  v15:f32 = fmul v10, v14
  return v15
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

func @out_pointer_example(v0:f32, v1:i32) {
  ; v1 is an out-pointer for a vec3 result
  v2:f32 = fmul v0, v0
  store v1, 0, v2
  store v1, 4, v2
  store v1, 8, v2
}

func @array_example(v0:i32) -> f32 {
  slot ss0, 16
  v1:i32 = slot_addr ss0
  v2:f32 = fconst.f32 1.0
  store v1, 0, v2
  store v1, 4, v2
  store v1, 8, v2
  store v1, 12, v2
  v3:i32 = imul_imm v0, 4
  v4:i32 = iadd v1, v3
  v5:f32 = load v4, 0
  return v5
}
```

### GLSL → LPIR mapping (summary)

| Naga construct | LPIR |
|---|---|
| `Expression::Binary { Add, .. }` (float) | `fadd` |
| `Expression::Binary { Add, .. }` (int) | `iadd` |
| `Expression::Binary { Less, .. }` (float) | `flt` |
| `Expression::Binary { Less, .. }` (sint) | `ilt_s` |
| `Expression::Unary { Negate }` (float) | `fneg` |
| `Expression::Unary { LogicalNot }` (bool) | `ieq` with `iconst.i32 0` |
| GLSL `bool(f)` | `fne` vs `fconst.f32 0.0` → i32 |
| GLSL `float(b)` | `itof_s` on i32 0/1 |
| `Expression::Literal(F32(v))` | `fconst.f32 v` |
| `Expression::Literal(I32(v))` | `iconst.i32 v` |
| `Expression::Literal(Bool(v))` | `iconst.i32 1` / `iconst.i32 0` |
| `Expression::Select` | `select` |
| `Expression::As` (float→int) | `ftoi_sat_s` / `ftoi_sat_u` |
| `Expression::Math { Mix }` | `mathcall fmix(...)` |
| `Expression::Math { SmoothStep }` | `mathcall fsmoothstep(...)` |
| `Expression::Math { Min }` | `mathcall fmin(...)` / `mathcall imin_s(...)` |
| `Expression::Math { Abs }` | `mathcall fabs(...)` / `mathcall iabs_s(...)` |
| `Statement::If` | `if v { ... } else { ... }` |
| `Statement::Loop` | `loop { ... }` |
| `Statement::Break` | `break` |
| `Statement::Continue` | `continue` |
| `Statement::Return` | `return v` |
| `Statement::Store` (local var) | VReg reassignment or `store` |
| `Statement::Call` (user fn) | `call @name(...)` |
| `Statement::Call` (LPFX) | `store` + `call` + `load` sequence |
| Vector expression | N× scalar ops (scalarized in lowering) |

### Numeric semantics: GPU-aligned, non-trapping

LPIR's numeric behavior is modeled on GPU shader execution, not on WASM
or Cranelift defaults. The guiding principles:

1. **Non-trapping**: No LPIR op traps. Every op produces a result, even
   for "undefined" inputs. Shader code must never crash — a shader that
   validates on the WASM backend must not crash on device.
2. **Performance over correctness**: Shaders are visual code. Garbage
   pixels from an edge case are acceptable; a halt is not.
3. **Backend consistency**: Both backends must agree on observable behavior
   for well-defined inputs. For undefined inputs (GLSL UB), both must
   produce *some* result without crashing; the exact value may differ.

| Edge case | LPIR behavior | Notes |
|---|---|---|
| Float arithmetic | IEEE 754 single-precision | Both backends agree |
| Integer arithmetic | Wrapping (mod 2^32) | Both backends agree |
| Integer div/rem by zero | Non-trapping, result unspecified | GPU: returns 0 or junk. WASM emitter adds zero-guard. RISC-V M ext returns -1/x natively. |
| Float div by zero | IEEE 754 (±Inf, NaN) | Both backends + GPUs agree |
| NaN in arithmetic | IEEE 754 (propagates) | Both backends + GPUs agree |
| NaN in comparisons | `0` (false) | Unordered comparison semantics |
| Shift by >= 32 bits | Shift amount masked to 5 bits | Both backends agree |
| Float-to-int overflow/NaN | Saturating (`ftoi_sat_s/u`) | NaN → 0, overflow → clamp |

**Interpreter**: Follows the same non-trapping rules. Integer div-by-zero
returns 0. Useful as a third reference point alongside the two backends.

**Future: diagnostic / safe mode**: A validation pass or interpreter flag
that *warns* on: division by zero, NaN inputs, out-of-range casts before
saturation, out-of-bounds memory access. Not in v1, but the spec reserves
this concept. The safe mode never changes results — it only reports.

### Key design decisions

1. **Width-aware VReg types, short op names**: `v0:f32 = fadd v1, v2`.
   Type on the VReg, not in the op name.

2. **Q32 in the emitter, not as an IR transform**: Backend-specific Q32
   strategies (inline i64, builtin calls, all-i32) make a shared transform
   impractical. LPIR stays clean: f32 and i32 only (no i64 in the IR).

3. **General pointer model via i32**: Pointers are i32 VRegs. Covers LPFX
   ABI, out/inout params, arrays, globals via context pointer. Slots for
   stack-allocated memory.

4. **MathCall for builtins**: Core Op enum stays small. Math builtins use
   a separate extensible `MathFunc` enum via `mathcall`, mirroring SPIR-V's
   extended instruction set.

5. **Single `call` op**: Import vs local is a property of the function
   declaration, not the call site. `export` marks host-callable entry points.

6. **Non-SSA**: VRegs can be reassigned. Both targets (WASM, Cranelift)
   perform their own SSA construction.

7. **Structured control flow**: Required by WASM. Structured → CFG (for
   Cranelift) is the easy direction.

8. **Scalarized**: No vector types. Lowering decomposes vectors. Backends
   never think about vectors.

9. **GPU-aligned numeric semantics**: Non-trapping, IEEE 754 floats,
   wrapping integers. Matches shader execution expectations, not
   general-purpose CPU semantics.
