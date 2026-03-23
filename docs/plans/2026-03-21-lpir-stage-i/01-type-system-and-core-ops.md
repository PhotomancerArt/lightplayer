# Phase 1: Type System and Core Ops

Normative numeric semantics (div-by-zero → `0`, saturating casts, etc.) are
specified in the overview chapter (`docs/lpir/00-overview.md`); this phase
duplicates op lists only — keep them consistent.

## Scope

Write three spec chapters:
- `docs/lpir/00-overview.md` — IR classification, motivation, design
  decisions, alternatives considered (absorb relevant content from the
  roadmap overview including the "why not SPIR-V" rationale).
- `docs/lpir/01-types-and-vregs.md` — Type system and VReg semantics.
- `docs/lpir/02-core-ops.md` — All core ops with full signatures and
  semantics.

## Reminders

- This is a spec-writing phase, no Rust code.
- Be precise about types, operands, and result types for every op.
- Document signed vs unsigned semantics where they differ.
- Use consistent formatting for op definitions.

## Implementation details

### 1. Overview chapter (`docs/lpir/00-overview.md`)

This is the "what is LPIR and why does it exist" chapter. It should be
self-contained enough that someone encountering the project for the first
time understands the motivation, the design decisions, and why we didn't
just use an existing IR.

Content to absorb from `docs/roadmaps/2026-03-21-lpir/overview.md`:
- Motivation (scratch aliasing bugs, shared middle-end, two backends)
- Architecture / pipeline diagram
- IR classification (flat, scalarized, non-SSA, structured CF, VRegs)
- Float-mode-agnostic: Q32 in the emitter, not the IR
- Design decisions: Q32 in emitter (with per-backend rationale),
  scalarized with future vector extension path
- Alternatives considered: why not SPIR-V (SSA, not scalarized, size)
- Crate structure overview

This chapter is the long-lived rationale document — the other chapters
are reference material, but this one tells the story.

### 2. Type System section

Define the two scalar types:

| Type  | Width   | Description                      |
|-------|---------|----------------------------------|
| `f32` | 4 bytes | IEEE 754 single-precision float  |
| `i32` | 4 bytes | 32-bit integer (signedness per op) |

Document:
- **Conditions**: `if`, `br_if_not`, and `select` use an `i32` condition: `0`
  is false, any nonzero value is true (WASM-style). Comparisons yield `i32`
  (`0` or `1`). GLSL `bool` is not a distinct LPIR type; lowering erases it
  to `i32`.
- No separate `u32` type: unsigned behavior is selected per op (`ilt_u`, etc.).
- No i64 in the IR (Q32 is backend-internal). **Future** 64-bit and vector
  types are out of scope for v1; reserve as additive extensions.
- No vector/matrix types in v1 (scalarized during lowering).
- No pointer type (addresses are i32 VRegs).
- Signedness is a property of the op, not the type (matching WASM).

### 3. VReg Semantics section

Define:
- Naming: `v0`, `v1`, `v2`, ... (monotonic per function).
- Type annotation on first definition: `v3:f32 = fadd v1, v2`.
- Subsequent uses are bare: `v3`.
- Non-SSA: VRegs can be reassigned (same type only).
- VReg count is a property of `IrFunction`, known before emission.
- No gaps required — numbering is dense from 0 to vreg_count-1.
- Function parameters are VRegs: `func @foo(v0:f32, v1:i32)` means v0 and
  v1 are pre-defined.

### 4. Core Ops section

For each op, document:
- **Syntax**: text format representation
- **Operands**: VReg types accepted
- **Result**: VReg type produced
- **Semantics**: what the op does

#### Arithmetic ops

```
v2:f32 = fadd v0, v1       ; f32 + f32 → f32
v2:f32 = fsub v0, v1       ; f32 - f32 → f32
v2:f32 = fmul v0, v1       ; f32 * f32 → f32
v2:f32 = fdiv v0, v1       ; f32 / f32 → f32
v1:f32 = fneg v0           ; -f32 → f32

v2:i32 = iadd v0, v1       ; i32 + i32 → i32 (wrapping)
v2:i32 = isub v0, v1       ; i32 - i32 → i32 (wrapping)
v2:i32 = imul v0, v1       ; i32 * i32 → i32 (wrapping)
v2:i32 = idiv_s v0, v1     ; i32 / i32 → i32 (signed, truncating)
v2:i32 = idiv_u v0, v1     ; i32 / i32 → i32 (unsigned)
v2:i32 = irem_s v0, v1     ; i32 % i32 → i32 (signed)
v2:i32 = irem_u v0, v1     ; i32 % i32 → i32 (unsigned)
v1:i32 = ineg v0           ; -i32 → i32 (wrapping, i.e. 0 - v0)
```

Semantics notes:
- Integer arithmetic is wrapping (mod 2^32).
- `fmod` is an import (`@std.math::fmod`), not a core op (WASM has no
  `f32.rem`; GLSL `mod` semantics require `x - y * floor(x/y)` which
  differs from C `fmod`).
- Float division by zero: IEEE 754 (±Inf/NaN).
- Integer `idiv_*` / `irem_*` with divisor zero: **result `0`**, non-trapping.
  Both emitters must match (WASM: guard + select `0`; Cranelift: do not rely
  on raw RISC-V div-by-zero results). See `00-design.md` numeric semantics.

#### Comparison ops

All comparisons produce `i32`: `1` if the relation holds, `0` otherwise.

```
v2:i32 = feq v0, v1       ; f32 == f32 → i32
v2:i32 = fne v0, v1       ; f32 != f32 → i32
v2:i32 = flt v0, v1       ; f32 < f32 → i32
v2:i32 = fle v0, v1       ; f32 <= f32 → i32
v2:i32 = fgt v0, v1       ; f32 > f32 → i32
v2:i32 = fge v0, v1       ; f32 >= f32 → i32

v2:i32 = ieq v0, v1       ; i32 == i32 → i32
v2:i32 = ine v0, v1       ; i32 != i32 → i32
v2:i32 = ilt_s v0, v1     ; i32 < i32 (signed) → i32
v2:i32 = ile_s v0, v1     ; i32 <= i32 (signed) → i32
v2:i32 = igt_s v0, v1     ; i32 > i32 (signed) → i32
v2:i32 = ige_s v0, v1     ; i32 >= i32 (signed) → i32
v2:i32 = ilt_u v0, v1     ; i32 < i32 (unsigned) → i32
v2:i32 = ile_u v0, v1     ; i32 <= i32 (unsigned) → i32
v2:i32 = igt_u v0, v1     ; i32 > i32 (unsigned) → i32
v2:i32 = ige_u v0, v1     ; i32 >= i32 (unsigned) → i32
```

#### Logic and bitwise ops

Bitwise ops are standard. GLSL logical `&&`, `||`, and `!` on boolean
values lower to these ops on `i32` operands that are already `0` or `1`
(from comparisons): use `iand`, `ior`, and `ieq` against `iconst.i32 0`
for `!` (since `ieq(x, 0)` is `1` iff `x` is `0`).

```
v2:i32 = iand v0, v1       ; bitwise AND (i32, i32 → i32)
v2:i32 = ior v0, v1        ; bitwise OR  (i32, i32 → i32)
v2:i32 = ixor v0, v1       ; bitwise XOR (i32, i32 → i32)
v1:i32 = ibnot v0          ; bitwise NOT (i32 → i32, i.e. xor with -1)
v2:i32 = ishl v0, v1       ; shift left  (i32, i32 → i32)
v2:i32 = ishr_s v0, v1     ; shift right (i32, i32 → i32, signed/arithmetic)
v2:i32 = ishr_u v0, v1     ; shift right (i32, i32 → i32, unsigned/logical)
```

Semantics notes:
- GLSL `&&` / `||` are short-circuiting, but LPIR is flat — both operands
  are computed before `iand` / `ior`. Lowering may use control flow when
  required to preserve side effects; pure boolean cases use bitwise ops.
- Shift amount is masked to 5 bits (0-31), matching WASM.

#### Constants

```
v0:f32 = fconst.f32 1.5        ; f32 literal
v0:f32 = fconst.f32 -0.0       ; negative zero
v0:f32 = fconst.f32 inf        ; positive infinity
v0:f32 = fconst.f32 nan        ; NaN

v0:i32 = iconst.i32 42         ; i32 literal (decimal)
v0:i32 = iconst.i32 -1         ; negative (signed interpretation)
v0:i32 = iconst.i32 0xFFFFFFFF ; hex literal (same bit pattern as -1)
; boolean true/false: iconst.i32 1 / iconst.i32 0
```

Common ops have `_imm` variants that take an inline immediate instead of a VReg second operand (e.g. `iadd_imm v1, 42`). The `_imm` variants are: `iadd_imm`, `isub_imm`, `imul_imm`, `ishl_imm`, `ishr_s_imm`, `ishr_u_imm`, `ieq_imm`.

#### Casts

```
v1:i32 = ftoi_sat_s v0     ; f32 → i32 (truncate toward zero, signed, saturating)
v1:i32 = ftoi_sat_u v0    ; f32 → i32 (truncate toward zero, unsigned, saturating)
v1:f32 = itof_s v0         ; i32 → f32 (signed interpretation)
v1:f32 = itof_u v0         ; i32 → f32 (unsigned interpretation)
```

Semantics notes:
- `ftoi_sat_s`/`ftoi_sat_u`: truncation toward zero with saturation. Out-of-range
  values clamp to `i32::MIN`/`i32::MAX` (or `0`/`u32::MAX` for unsigned). NaN
  maps to `0`. Matches WASM's `i32.trunc_sat_f32_s` / `i32.trunc_sat_f32_u` —
  non-trapping, safer for shader code where GLSL says "undefined" for overflow.
- `itof_s`/`itof_u`: may lose precision for large values (i32 has 32 bits,
  f32 has 24 bits of mantissa).
- GLSL casts involving `bool` use comparisons / `itof_s` as in the GLSL
  mapping chapter (no separate cast ops).

#### Select and copy

```
v3:f32 = select v0, v1, v2   ; if v0 (i32, nonzero) then v1 else v2
                               ; v1, v2 must be same type; result same type
v1:f32 = copy v0              ; copy value (used for VReg reassignment clarity)
```

`select` is a ternary conditional. Both branches are already evaluated (flat
IR). The condition is an `i32` VReg (`0` = false, nonzero = true). The two
value operands and the result must have the same type.

## Validate

Review the section for:
- Every op has syntax, operands, result type, and semantics.
- Signed/unsigned variants are complete and consistent.
- Type constraints are explicit (what types each operand accepts).
- No ambiguity in the semantics descriptions.
- Cross-reference with the current WASM emitter's `emit_binary`,
  `emit_unary`, `emit_cast` to ensure nothing is missing.
