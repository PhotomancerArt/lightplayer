# Types and Virtual Registers

## Type system

LPIR has two scalar types. Both are width-aware.

| Type  | Width   | Description                     |
|-------|---------|---------------------------------|
| `f32` | 4 bytes | IEEE 754 single-precision float |
| `i32` | 4 bytes | 32-bit integer (signedness per op) |

### Rules

- Boolean conditions use `i32`: `0` is false, any nonzero value is true (WebAssembly-style). Comparison operations produce `i32` (`0` or `1`). GLSL `bool` is not a distinct LPIR type.
- There is no separate `u32` type: unsigned behavior is selected per operation (for example `ilt_u`).
- There is no `i64` in the IR. Q32 widening is backend-internal, not an LPIR type.
- There are no vector or matrix types in v1; those forms are scalarized during lowering. Vectors and matrices may be added in a future extension.
- There is no pointer type. Addresses are `i32` virtual registers.
- Signedness is a property of the operation, not of the type (aligned with WebAssembly).

## Virtual register semantics

- Virtual registers are named `v0`, `v1`, `v2`, … with indices monotonic within a function.
- A type appears on the first definition of a register: `v3:f32 = fadd v1, v2`.
- Later uses omit the type: `v3`.
- The IR is not SSA: a virtual register may be reassigned; redefinitions must keep the same type.
- `vreg_count` is a property of `IrFunction` and is fixed before emission.
- Indices are dense: valid indices are `0` through `vreg_count - 1` with no gaps.
- Function parameters are virtual registers. For example, `func @foo(v0:f32, v1:i32)` defines `v0` and `v1` before the function body.
