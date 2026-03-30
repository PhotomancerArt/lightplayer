# GLSL to LPIR Mapping

The source language is **GLSL 4.50 core** (`#version 450 core`), parsed and typed by Naga. This chapter records how Naga’s `Module` (expression arena and statement tree) maps to LPIR for the supported subset.

## Naga expressions

| Naga `Expression` | LPIR | Notes |
|-------------------|------|-------|
| `Literal { value: F32(v) }` | `fconst.f32` / assignment to a VReg | |
| `Literal { value: I32(v) }` | `iconst.i32` | |
| `Literal { value: U32(v) }` | `iconst.i32` | Reinterpret bits as `i32`. |
| `Literal { value: Bool(v) }` | `iconst.i32 1` or `iconst.i32 0` | |
| `Constant { handle }` | `fconst.f32` or `iconst.i32` | Resolve from module constants. |
| `FunctionArgument { index }` | Parameter VReg `v`*i* | Same order as the function’s `param_list`. |
| `LocalVariable { handle }` | One VReg per scalar local | Vectors and matrices are scalarized across multiple VRegs. |
| `Load { pointer }` | Use of the VReg holding the value, or `load` | If the pointer is a scalarized local, the load is represented by the corresponding VReg; otherwise `load` from an `i32` address VReg. |
| `Binary { op, left, right }` | Core binary op or `call` | See [Binary operators](#binary-operators). Float modulo uses `call @std.math::fmod`. |
| `Unary { op, expr }` | Core unary op or `ieq` with `0` | See [Unary operators](#unary-operators). |
| `Select { condition, accept, reject }` | `select` | Condition VReg is `i32` (`0` false, nonzero true). |
| `As { expr, kind, convert }` | Cast ops | See [Scalar `As` conversions](#scalar-as-conversions). |
| `ZeroValue(ty)` | `fconst.f32 0.0` or `iconst.i32 0` | According to scalar type after decomposition. |
| `Math { fun, args }` | `call @std.math::…` | Names and signatures are listed in `06-import-modules.md`. |
| `CallResult { handle }` | VReg bound at the `call` site | |
| `Compose { … }` | Multiple VRegs | Scalarized aggregate construction. |
| `Splat { … }` | `copy` (or implicit use) into multiple VRegs | One VReg per component. |
| `Swizzle { … }` | Selection of existing VRegs | No swizzle opcode; components are separate VRegs. |
| `AccessIndex { base, index }` | Selection of the VReg for that component | After scalarization, indices map to fixed VRegs. |

## Binary operators

Each cell is the LPIR opcode (or `call`) used for the Naga `BinaryOperator` when both operands have the column’s scalar kind. The boolean column assumes GLSL `bool` lowered to `i32` with values `0` or `1`.

| Naga `BinaryOperator` | Float | Signed int | Unsigned int | Bool |
|----------------------|-------|------------|--------------|------|
| Add | `fadd` | `iadd` | `iadd` | — |
| Subtract | `fsub` | `isub` | `isub` | — |
| Multiply | `fmul` | `imul` | `imul` | — |
| Divide | `fdiv` | `idiv_s` | `idiv_u` | — |
| Modulo | `call @std.math::fmod` | `irem_s` | `irem_u` | — |
| Equal | `feq` | `ieq` | `ieq` | `ieq` |
| NotEqual | `fne` | `ine` | `ine` | `ine` |
| Less | `flt` | `ilt_s` | `ilt_u` | — |
| LessEqual | `fle` | `ile_s` | `ile_u` | — |
| Greater | `fgt` | `igt_s` | `igt_u` | — |
| GreaterEqual | `fge` | `ige_s` | `ige_u` | — |
| LogicalAnd | — | — | — | `iand` |
| LogicalOr | — | — | — | `ior` |
| And | — | `iand` | `iand` | `iand` |
| InclusiveOr | — | `ior` | `ior` | `ior` |
| ExclusiveOr | — | `ixor` | `ixor` | `ixor` |
| ShiftLeft | — | `ishl` | `ishl` | — |
| ShiftRight | — | `ishr_s` | `ishr_u` | — |

A cell marked `—` is not used for that combination in the supported GLSL subset (or is lowered by other means before it reaches this table).

Short-circuiting `&&` and `||` on `bool` are preserved when side effects require it by lowering to control flow; pure boolean cases may use `iand` / `ior` on `0`/`1` values as in the table.

## Unary operators

| Naga `UnaryOperator` | Operand scalar kind | LPIR |
|----------------------|---------------------|------|
| Negate | float | `fneg` |
| Negate | signed integer | `ineg` |
| LogicalNot | bool (`i32`) | `ieq` with `iconst.i32 0` |
| BitwiseNot | integer | `ibnot` |

## Scalar `As` conversions

| Source → destination (scalar) | LPIR |
|---------------------------------|------|
| `f32` → `i32` (signed) | `ftoi_sat_s` |
| `f32` → `u32` bits in `i32` | `ftoi_sat_u` |
| `i32` → `f32` (signed) | `itof_s` |
| `u32` bits in `i32` → `f32` | `itof_u` |
| `bool` / `i32` ↔ `f32` | Compare, `select`, or `itof_s` as required by the GLSL cast |

Exact rules follow `02-core-ops.md` for saturation and NaN.

## Naga statements

| Naga `Statement` | LPIR |
|------------------|------|
| `Emit { range }` | No standalone opcode; expressions are emitted as their using statements require. |
| `Block(body)` | Emit `body` statements in order. |
| `If { condition, accept, reject }` | `if` *vcond* `{` … `}` optional `else` `{` … `}` |
| `Switch { selector, cases }` | `switch` *v* `{` `case` *n* `{` … `}` … [ `default` `{` … `}` ] `}` |
| `Loop { body, continuing, break_if }` | `loop` `{` *body* *continuing* [ `br_if_not` *v* ] `continue` `}` |
| GLSL `for` / `while` / `do-while` | Lowered through Naga’s `Loop` form before this mapping applies. |
| `Break` | `break` |
| `Continue` | `continue` |
| `Return { value }` | `return` *v* or `return` |
| `Store { pointer, value }` | VReg assignment for scalarized locals, or `store` to an `i32` address |
| `Call { function, arguments, result }` | `call @name(args)` or `call @module::name(args)` with result VRegs |

## Vector and matrix scalarization

| GLSL / Naga shape | LPIR (scalarized) |
|-------------------|-------------------|
| `vec3` + `vec3` | Three `fadd` (or type-appropriate op) on component VRegs |
| `vec3(1.0, 2.0, 3.0)` | Three `fconst.f32` (or `iconst.i32` where applicable) and three VRegs |
| `a.xy` (swizzle) | Reuse the VRegs that hold components `x` and `y` |
| `dot(a, b)` on `vec3` | Three `fmul`, then two `fadd` |
| `cross(a, b)` | Six `fmul`, three `fsub` (component formulas per GLSL) |
| `length(a)` on `vec3` | Three `fmul`, two `fadd`, then `call @std.math::fsqrt` |

Matrices follow the same principle: each element is a separate `f32` VReg; operations decompose to scalar arithmetic and loads/stores where needed.
