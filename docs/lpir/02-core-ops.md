# Core Operations

This document lists core LPIR operations: syntax, operands, result types, and semantics. Unless stated otherwise, operand virtual registers must already hold values of the required types.

Notation: `vn:ty` on the left-hand side is a defining occurrence with type `ty`. Operand registers are written without types when the type is clear from the operation.

## Arithmetic (floating-point)

| Op    | Syntax                         | Operands     | Result | Semantics                                      |
|-------|--------------------------------|--------------|--------|------------------------------------------------|
| fadd  | `v2:f32 = fadd v0, v1`         | `f32`, `f32` | `f32`  | IEEE 754 addition                              |
| fsub  | `v2:f32 = fsub v0, v1`         | `f32`, `f32` | `f32`  | IEEE 754 subtraction                           |
| fmul  | `v2:f32 = fmul v0, v1`         | `f32`, `f32` | `f32`  | IEEE 754 multiplication                      |
| fdiv  | `v2:f32 = fdiv v0, v1`         | `f32`, `f32` | `f32`  | IEEE 754 division; division by zero yields ±Inf or NaN per IEEE 754 |
| fneg  | `v1:f32 = fneg v0`             | `f32`        | `f32`  | Negation (`-v0`)                               |

## Arithmetic (integer)

| Op     | Syntax                         | Operands     | Result | Semantics                                      |
|--------|--------------------------------|--------------|--------|------------------------------------------------|
| iadd   | `v2:i32 = iadd v0, v1`         | `i32`, `i32` | `i32`  | Wrapping 32-bit addition (mod 2³²)             |
| isub   | `v2:i32 = isub v0, v1`         | `i32`, `i32` | `i32`  | Wrapping 32-bit subtraction                    |
| imul   | `v2:i32 = imul v0, v1`         | `i32`, `i32` | `i32`  | Wrapping 32-bit multiplication                 |
| idiv_s | `v2:i32 = idiv_s v0, v1`       | `i32`, `i32` | `i32`  | Signed truncating division toward zero         |
| idiv_u | `v2:i32 = idiv_u v0, v1`       | `i32`, `i32` | `i32`  | Unsigned division                              |
| irem_s | `v2:i32 = irem_s v0, v1`       | `i32`, `i32` | `i32`  | Signed remainder; sign of result follows dividend |
| irem_u | `v2:i32 = irem_u v0, v1`       | `i32`, `i32` | `i32`  | Unsigned remainder                             |
| ineg   | `v1:i32 = ineg v0`             | `i32`        | `i32`  | Wrapping negation (`isub` of `0` and `v0`)     |

Integer arithmetic is wrapping modulo 2³² except where an operation is defined by signed or unsigned interpretation of bit patterns (`idiv_*`, `irem_*`).

For `idiv_s`, `idiv_u`, `irem_s`, and `irem_u`, if the divisor is `0`, the result is `0` and the operation does not trap.

`fmod` is not a core operation; it is provided via import (for example `@std.math::fmod`).

## Comparison (floating-point)

All float comparisons produce `i32`: `1` or `0`. Ordering relations (`flt`, `fle`, `fgt`, `fge`) yield `0` if either operand is NaN. `feq` yields `0` if either operand is NaN. `fne` follows WebAssembly-style float inequality: `1` if either operand is NaN or the values are unequal under IEEE `eq`; `0` only when neither is NaN and they are equal.

| Op  | Syntax                   | Operands     | Result | Semantics |
|-----|--------------------------|--------------|--------|-----------|
| feq | `v2:i32 = feq v0, v1`    | `f32`, `f32` | `i32`  | `1` iff neither operand is NaN and `v0` and `v1` compare equal under IEEE 754 |
| fne | `v2:i32 = fne v0, v1`    | `f32`, `f32` | `i32`  | `1` iff either operand is NaN or `feq` would be `0` |
| flt | `v2:i32 = flt v0, v1`    | `f32`, `f32` | `i32`  | `1` iff `v0 < v1` and neither operand is NaN |
| fle | `v2:i32 = fle v0, v1`    | `f32`, `f32` | `i32`  | `1` iff `v0 ≤ v1` and neither operand is NaN |
| fgt | `v2:i32 = fgt v0, v1`    | `f32`, `f32` | `i32`  | `1` iff `v0 > v1` and neither operand is NaN |
| fge | `v2:i32 = fge v0, v1`    | `f32`, `f32` | `i32`  | `1` iff `v0 ≥ v1` and neither operand is NaN |

## Comparison (integer, signed)

Operands are `i32` interpreted as two’s-complement signed integers. Result is `i32`: `1` or `0`.

| Op    | Syntax                     | Result | Semantics        |
|-------|----------------------------|--------|------------------|
| ieq   | `v2:i32 = ieq v0, v1`      | `i32`  | `1` iff equal    |
| ine   | `v2:i32 = ine v0, v1`      | `i32`  | `1` iff not equal |
| ilt_s | `v2:i32 = ilt_s v0, v1`    | `i32`  | `1` iff signed `<` |
| ile_s | `v2:i32 = ile_s v0, v1`    | `i32`  | `1` iff signed `≤` |
| igt_s | `v2:i32 = igt_s v0, v1`    | `i32`  | `1` iff signed `>` |
| ige_s | `v2:i32 = ige_s v0, v1`    | `i32`  | `1` iff signed `≥` |

## Comparison (integer, unsigned)

Operands are `i32` interpreted as unsigned 32-bit integers. Result is `i32`: `1` or `0`.

| Op    | Syntax                     | Result | Semantics          |
|-------|----------------------------|--------|--------------------|
| ilt_u | `v2:i32 = ilt_u v0, v1`    | `i32`  | `1` iff unsigned `<` |
| ile_u | `v2:i32 = ile_u v0, v1`    | `i32`  | `1` iff unsigned `≤` |
| igt_u | `v2:i32 = igt_u v0, v1`    | `i32`  | `1` iff unsigned `>` |
| ige_u | `v2:i32 = ige_u v0, v1`    | `i32`  | `1` iff unsigned `≥` |

## Logic and bitwise

All operands and results are `i32`. Bitwise operations use two’s-complement bit patterns.

| Op     | Syntax                       | Operands     | Result | Semantics |
|--------|------------------------------|--------------|--------|-----------|
| iand   | `v2:i32 = iand v0, v1`       | `i32`, `i32` | `i32`  | Bitwise AND |
| ior    | `v2:i32 = ior v0, v1`        | `i32`, `i32` | `i32`  | Bitwise OR  |
| ixor   | `v2:i32 = ixor v0, v1`       | `i32`, `i32` | `i32`  | Bitwise XOR |
| ibnot  | `v1:i32 = ibnot v0`          | `i32`        | `i32`  | Bitwise NOT |
| ishl   | `v2:i32 = ishl v0, v1`       | `i32`, `i32` | `i32`  | Shift left; shift amount is `v1 & 31` (5-bit mask, WebAssembly rule) |
| ishr_s | `v2:i32 = ishr_s v0, v1`     | `i32`, `i32` | `i32`  | Arithmetic right shift; amount `v1 & 31` |
| ishr_u | `v2:i32 = ishr_u v0, v1`     | `i32`, `i32` | `i32`  | Logical right shift; amount `v1 & 31` |

GLSL `&&` and `||` short-circuit in the source language; LPIR evaluates both operands before `iand` / `ior`. Lowering may introduce control flow when side effects or short-circuit behavior must be preserved.

GLSL logical NOT on a boolean represented as `i32` can be lowered as `ieq` against integer constant `0`.

## Constants

The constant is encoded in the instruction; it is not a separate virtual register.

| Op          | Syntax                              | Result | Semantics |
|-------------|-------------------------------------|--------|-----------|
| fconst.f32  | `v0:f32 = fconst.f32 <literal>`     | `f32`  | IEEE 754 single-precision literal |
| iconst.i32  | `v0:i32 = iconst.i32 <literal>`     | `i32`  | 32-bit integer literal (decimal or `0x` hex) |

Examples:

```
v0:f32 = fconst.f32 1.5
v0:f32 = fconst.f32 -0.0
v0:f32 = fconst.f32 inf
v0:f32 = fconst.f32 nan
v0:i32 = iconst.i32 42
v0:i32 = iconst.i32 -1
v0:i32 = iconst.i32 0xFFFFFFFF
```

`fconst.f32` accepts finite values, signed zeros, `inf`, `-inf`, and `nan` as defined by the text format for literals.

## Immediate variants

The second operand is an immediate integer embedded in the instruction.

| Op         | Syntax                          | Register operand | Immediate | Result | Semantics |
|------------|---------------------------------|------------------|-----------|--------|-----------|
| iadd_imm   | `v2:i32 = iadd_imm v1, <imm>`   | `i32`            | `i32`     | `i32`  | Wrapping add |
| isub_imm   | `v2:i32 = isub_imm v1, <imm>`   | `i32`            | `i32`     | `i32`  | Wrapping `v1 - imm` (immediate encoded as 32-bit; wrapping semantics) |
| imul_imm   | `v2:i32 = imul_imm v1, <imm>`   | `i32`            | `i32`     | `i32`  | Wrapping multiply |
| ishl_imm   | `v2:i32 = ishl_imm v1, <imm>`   | `i32`            | `i32`     | `i32`  | Shift left; immediate masked to 5 bits |
| ishr_s_imm | `v2:i32 = ishr_s_imm v1, <imm>` | `i32`            | `i32`     | `i32`  | Arithmetic right shift; immediate masked to 5 bits |
| ishr_u_imm | `v2:i32 = ishr_u_imm v1, <imm>` | `i32`            | `i32`     | `i32`  | Logical right shift; immediate masked to 5 bits |
| ieq_imm    | `v2:i32 = ieq_imm v1, <imm>`    | `i32`            | `i32`     | `i32`  | Same as `ieq` with second operand the immediate (32-bit value per text grammar) |

Example:

```
v2:i32 = iadd_imm v1, 1
```

## Casts

| Op          | Syntax                       | Operand | Result | Semantics |
|-------------|------------------------------|---------|--------|-----------|
| ftoi_sat_s  | `v1:i32 = ftoi_sat_s v0`    | `f32`   | `i32`  | Truncate toward zero; signed; out-of-range clamps to least / greatest signed 32-bit integer; NaN → `0` (WebAssembly `trunc_sat` family) |
| ftoi_sat_u  | `v1:i32 = ftoi_sat_u v0`    | `f32`   | `i32`  | Truncate toward zero; unsigned; out-of-range clamps to `0` / `0xFFFFFFFF`; NaN → `0` |
| itof_s      | `v1:f32 = itof_s v0`         | `i32`   | `f32`  | Convert signed `i32` to `f32`; large magnitudes may not be exactly representable |
| itof_u      | `v1:f32 = itof_u v0`         | `i32`   | `f32`  | Convert unsigned `i32` to `f32`; precision limits apply |

## Select and copy

| Op     | Syntax                           | Operands              | Result | Semantics |
|--------|----------------------------------|-----------------------|--------|-----------|
| select | `v3:ty = select v0, v1, v2`      | `i32`, `ty`, `ty`     | `ty`   | If `v0` is nonzero (`i32` condition), result is `v1`; else `v2`. Both `v1` and `v2` are evaluated before `select` (flat IR). Types of `v1`, `v2`, and `v3` must match (`f32` or `i32`). |
| copy   | `v1:ty = copy v0`                | `ty`                  | `ty`   | Result bit pattern equals `v0`; type must match |

Example:

```
v3:f32 = select v0, v1, v2
v1:f32 = copy v0
```
