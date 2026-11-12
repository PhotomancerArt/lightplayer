# Phase 3: Expression Lowering

## Scope

Implement `lower_expr.rs` — the core expression lowering from Naga
`Expression` to LPIR ops. Covers all scalar expression types except
`Math` builtins (Phase 5) and `CallResult` (Phase 5).

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### `lower_expr.rs` — main function

```rust
pub(crate) fn lower_expr(
    ctx: &mut LowerCtx,
    expr: Handle<Expression>,
) -> Result<VReg, LowerError>
```

Check expression cache first. If hit, return cached VReg. If miss, lower
and cache.

### Expression types to handle

**`Literal`**

- `Literal::F32(v)` → `Op::FconstF32 { dst, value: v }`
- `Literal::I32(v)` → `Op::IconstI32 { dst, value: v }`
- `Literal::U32(v)` → `Op::IconstI32 { dst, value: v as i32 }`
- `Literal::Bool(b)` → `Op::IconstI32 { dst, value: b as i32 }`
- Other literals → `LowerError::UnsupportedExpression`

**`Constant`**

- Resolve via `module.constants[h].init` → lower the global expression.
- Global expressions: `Literal` (same as above), `Compose` (vector — error
  for scalar stage), `Splat` (error for scalar stage).

**`FunctionArgument(idx)`**

- VReg is `VReg(idx)` (params are the first VRegs).

**`LocalVariable(handle)`**

- Error: "LocalVariable must be used through Load" (same as WASM emitter).

**`Load { pointer }`**

- If pointer is `LocalVariable(lv)` → return `ctx.resolve_local(lv)`.
- Otherwise → `LowerError::UnsupportedExpression`.

**`Binary { op, left, right }`**

- Lower both operands via `ensure_expr`.
- Resolve scalar kind of `left` to pick float vs int vs unsigned op.
- Map Naga `BinaryOperator` to LPIR op:

| Naga op      | Float                  | Sint  | Uint  | Bool |
|--------------|------------------------|-------|-------|------|
| Add          | Fadd                   | Iadd  | Iadd  | Iadd |
| Subtract     | Fsub                   | Isub  | Isub  | —    |
| Multiply     | Fmul                   | Imul  | Imul  | —    |
| Divide       | Fdiv                   | IdivS | IdivU | —    |
| Modulo       | (decompose via ffloor) | IremS | IremU | —    |
| Equal        | Feq                    | Ieq   | Ieq   | Ieq  |
| NotEqual     | Fne                    | Ine   | Ine   | Ine  |
| Less         | Flt                    | IltS  | IltU  | —    |
| LessEqual    | Fle                    | IleS  | IleU  | —    |
| Greater      | Fgt                    | IgtS  | IgtU  | —    |
| GreaterEqual | Fge                    | IgeS  | IgeU  | —    |
| LogicalAnd   | —                      | Iand  | —     | Iand |
| LogicalOr    | —                      | Ior   | —     | Ior  |
| And          | —                      | Iand  | Iand  | —    |
| InclusiveOr  | —                      | Ior   | Ior   | —    |
| ExclusiveOr  | —                      | Ixor  | Ixor  | —    |
| ShiftLeft    | —                      | Ishl  | Ishl  | —    |
| ShiftRight   | —                      | IshrS | IshrU | —    |

For float `Modulo`: decompose inline as
`x - y * ffloor(x / y)`:

```
v_div = fdiv(x, y)
v_fl  = ffloor(v_div)
v_mul = fmul(v_fl, y)
v_mod = fsub(x, v_mul)
```

**`Unary { op, expr }`**

- Lower operand via `ensure_expr`.
- Resolve scalar kind.
- `Negate` + Float → `Fneg`
- `Negate` + Sint → `Ineg`
- `LogicalNot` → `IeqImm { imm: 0 }` (produces 1 when src is 0)
- `BitwiseNot` → `Ibnot`

**`Select { condition, accept, reject }`**

- Lower all three operands.
- Emit `Op::Select { dst, cond, if_true: accept, if_false: reject }`.

**`As { expr, kind, convert }`** (casts)

- Resolve source scalar kind.
- Same-type cast (e.g. float→float, sint→sint): `Op::Copy` or no-op (reuse VReg).
- Float→Sint: `Op::FtoiSatS`
- Float→Uint: `Op::FtoiSatU`
- Sint→Float: `Op::ItofS`
- Uint→Float: `Op::ItofU`
- Sint↔Uint: no-op (same i32 bit pattern)
- Bool→Sint/Uint: no-op (already i32)
- Sint/Uint→Bool: `Ine { dst, lhs: src, rhs: zero_const }`
- Float→Bool: `Fne { dst, lhs: src, rhs: zero_const }`
- Bool→Float: `ItofS` (0→0.0, 1→1.0)

**`ZeroValue(type_handle)`**

- Scalar float → `FconstF32 { value: 0.0 }`
- Scalar int/uint/bool → `IconstI32 { value: 0 }`
- Non-scalar → error

**`CallResult(_)`**

- Stubbed with TODO for Phase 5 (needs call lowering first).

**`Math { .. }`**

- Stubbed with TODO for Phase 5.

### Helper: `expr_scalar_kind`

Resolve `Handle<Expression>` → `ScalarKind`. Follow the same logic as
`lps-wasm/src/emit.rs::expr_scalar_kind`:

- `Literal` → kind from literal variant
- `FunctionArgument(i)` → from argument type
- `LocalVariable(lv)` → from local variable type
- `Load { pointer: LocalVariable(lv) }` → from local variable type
- `Binary { op, left, .. }` → comparison ops → Bool, others → kind of left
- `Unary { expr, .. }` → kind of inner
- `Select { accept, .. }` → kind of accept
- `As { kind, .. }` → target kind
- `Constant(h)` → from constant type
- `ZeroValue(ty)` → from type
- `CallResult(fh)` → from callee return type

## Validate

```
cargo check -p lps-frontend
cargo +nightly fmt -p lps-frontend -- --check
```

The crate compiles. Expression lowering is exercisable but not yet
reachable from `lower()` (statement lowering is still stubbed).
