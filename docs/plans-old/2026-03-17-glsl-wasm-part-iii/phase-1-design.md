# Phase 1 Design: Type-aware expressions + scalar fixes

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 1

## Goals

1. Introduce `WasmRValue` type returned by `emit_rvalue`
2. Fix unary minus operand order
3. Add type inference to binary op dispatch
4. Implement integer `*`, `/`, `%`
5. Implement Q32 float multiply (inline i64)
6. Implement Q32 float divide (inline i64)
7. Implement Q32 modulo
8. Implement assignment expressions
9. Implement compound assignment (`+=`, `-=`, `*=`, `/=`)

---

## 1. WasmRValue

**Current:** `emit_rvalue` returns `()`. Callers ignore the type of the value on the stack.

**Target:** Return `WasmRValue { ty, stack_count }` so binary ops can dispatch by operand types.

```rust
// codegen/rvalue.rs
pub struct WasmRValue {
    pub ty: Type,
    pub stack_count: u32,
}

impl WasmRValue {
    pub fn scalar(ty: Type) -> Self {
        Self { ty, stack_count: 1 }
    }
}
```

**Call sites to update:**

- `emit_rvalue` → returns `Result<WasmRValue, GlslDiagnostics>`
- `emit_declaration_to_sink` – uses result for init; can ignore return type (just need success)
- `emit_return_to_sink` – uses result; can ignore (return type already known from function)
- `emit_expr_stmt_to_sink` – evaluates and drops; can ignore
- `emit_binary_op` call site – will need `lhs_ty` and `rhs_ty` from recursive `emit_rvalue` calls

---

## 2. Unary minus operand order

**Bug:** WASM `i32.sub` does `lhs - rhs`. Current code emits `operand` then `0`, so stack is `[operand, 0]`. Popping: lhs=operand, rhs=0 → `operand - 0 = operand`. Wrong.

**Fix:** Emit `0` first, then `operand`. Stack becomes `[0, operand]`. Result: `0 - operand = -operand`.

```rust
// Before (wrong):
emit_rvalue(ctx, sink, operand, options)?;
sink.i32_const(0);
sink.i32_sub();

// After (correct):
sink.i32_const(0);
emit_rvalue(ctx, sink, operand, options)?;
sink.i32_sub();
```

---

## 3. Type inference for binary op dispatch

**Requirement:** Binary op must know operand types. If both are Int/UInt/Bool → integer WASM ops. If either is Float → float-mode ops (Q32 or f32).

**Approach:** Add `infer_expr_type(ctx, expr) -> Result<Type, GlslDiagnostics>` using `ctx.locals` only (no SymbolTable). Supported: literals, variables, binary, unary, assignment. Use `infer_binary_result_type` from frontend for binary result type.

**Alternative:** Have `emit_rvalue` return `WasmRValue` and pass `lhs_rvalue.ty` and `rhs_rvalue.ty` into `emit_binary_op`. This avoids a separate inference pass. Prefer this: single source of truth.

**Dispatch logic for arithmetic (Add, Sub, Mult, Div, Mod):**

- Both operands Int/UInt/Bool → integer mode (i32 ops)
- Either operand Float + Q32 → Q32 mode (i32 ops with Q32 semantics for mul/div)
- Either operand Float + Float mode → f32 ops

---

## 4. Integer `*`, `/`, `%`

Straightforward WASM ops:

```rust
Mult => sink.i32_mul(),
Div => sink.i32_div_s(),   // signed; for uint use i32_div_u if we support it
Mod => sink.i32_rem_s(),   // signed remainder
```

For `%`: GLSL modulo semantics may differ from WASM remainder. GLSL: `a - floor(a/b) * b`. For integers, `floor(a/b) = a/b` (truncation), so `a - (a/b)*b` = remainder. `i32.rem_s` gives the remainder. Need to verify: for negative numbers, C/GLSL `%` truncates toward zero; WASM `rem_s` truncates toward negative infinity. If mismatch, we need a different sequence. (Check GLSL spec vs WASM. For now, assume `rem_s` is acceptable for Phase 1.)

---

## 5. Q32 multiply

**Semantics:** Q16.16 fixed-point. Value `a` represents `a / 2^16`. So `a * b` (in fixed-point) = `(a * b) / 2^16`. With 64-bit intermediate: `(i64(a) * i64(b)) >> 16`, then wrap to i32.

**WASM sequence:**

```wasm
local.get $a
i64.extend_i32_s
local.get $b
i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s
i32.wrap_i64
```

Stack: push a, extend; push b, extend; mul → 64-bit product; shift right 16; wrap to i32.

---

## 6. Q32 divide

**Semantics:** `a / b` in Q16.16 = `(a / b) * 2^16` (keeping Q16.16 result). So `(a << 16) / b`.

**WASM sequence:**

```wasm
local.get $a
i64.extend_i32_s
i64.const 16
i64.shl
local.get $b
i64.extend_i32_s
i64.div_s
i32.wrap_i64
```

Stack: a → i64; shift left 16; b → i64; div_s; wrap.

---

## 7. Q32 modulo

**GLSL semantics:** `mod(x,y) = x - y * floor(x/y)`.

**WASM:** `i32.rem_s` implements `a - trunc(a/b)*b` (truncation toward zero). For positive operands, trunc and floor match. For negative operands they can differ (e.g. `mod(-7, 2)` floor → 1, trunc → -1).

**Options:**

1. **Use `i32.rem_s`** – matches positive case; negative case differs from GLSL spec. Simple.
2. **Inline floor division** – requires branching on signs to implement floor from trunc; non-trivial.
3. **Defer to builtin** – Phase 6 builtins can implement correct `mod`.

**Design decision:** Use `i32.rem_s` for Q32 mod in Phase 1. Covers common positive case. Document: "Q32 mod uses truncation; full floor semantics available via `mod()` builtin in Phase 6."

---

## 8. Assignment expressions

**Syntax:** `Expr::Assignment(lhs, op, rhs, span)`. For simple `=`, `op` is `AssignmentOp::Equal`.

**Semantics:** Evaluate rhs, store in lhs, result is the stored value.

**Simple variable LHS:** `Expr::Variable(ident, _)`. Look up local index, emit rhs, then `local.set(idx)`.

**Result value:** Assignment produces the value. So we need to leave the value on the stack after set. Use `local.tee(idx)`: stores and leaves value on stack. Perfect for `x = expr` when we need the result.

```rust
// x = 5;
emit_rvalue(ctx, sink, rhs, options)?;  // stack: [5]
let idx = ctx.lookup_local(lhs_var_name)?.index;
sink.local_tee(idx);                     // store 5 in x, stack: [5]
```

If we don't need the result (expression statement), we still emit the same; the caller will drop. No change.

**Caveat:** `local.tee` requires the value to match the local's type. For scalar i32, we're good.

---

## 9. Compound assignment

**Ops:** `+=`, `-=`, `*=`, `/=`. Emit: `local.get(idx)`, emit rhs, emit binary op, `local.set(idx)`. Result is the new value, so we could `local.tee` instead of `local.set` if we need to leave it on the stack. For compound assignment the result _is_ the stored value, so: emit get, rhs, op, then tee (stores and leaves value). Same as simple assignment pattern.

```rust
// x += 10;
let idx = ...;
sink.local_get(idx);
emit_rvalue(ctx, sink, rhs, options)?;
emit_binary_op(sink, &corresponding_binary_op, lhs_ty, rhs_ty, numeric)?;
sink.local_tee(idx);
```

Map: `Add` -> `Add`, `Sub` -> `Sub`, `Mult` -> `Mult`, `Div` -> `Div`.

---

## 10. Binary op signature change

**Current:** `emit_binary_op(sink, op, numeric) -> Result<(), GlslDiagnostics>`

**New:** `emit_binary_op(sink, op, lhs_ty, rhs_ty, numeric) -> Result<(), GlslDiagnostics>`

So we can dispatch:

- For Mult: if both integer-like → `i32.mul`; if either float + Q32 → Q32 mul sequence
- For Div: same
- For Mod: integer-like → `i32.rem_s`; Q32 → (use rem_s per above) or defer

---

## Call graph changes

```
emit_rvalue -> Result<WasmRValue>
  ├─ literal: emit_literal; Ok(WasmRValue::scalar(inferred_ty))
  ├─ variable: emit_variable; Ok(WasmRValue::scalar(info.ty))
  ├─ binary: lhs=emit_rvalue(lhs); rhs=emit_rvalue(rhs); emit_binary_op(..., lhs.ty, rhs.ty); Ok(WasmRValue::scalar(result_ty))
  ├─ unary: operand=emit_rvalue(op); emit unary; Ok(WasmRValue::scalar(result_ty))
  └─ assignment: emit_assign; Ok(WasmRValue::scalar(lhs_ty))
```

`emit_literal` and `emit_variable` don't currently return type. They'd need to return it, or we infer at the emit_rvalue site from the expr.

Simpler: `emit_literal` returns `Type` based on expr variant. `emit_variable` returns `Type` from ctx. Or have helpers `literal_type(expr)` and pass through.

---

## File change summary

| File                          | Changes                                                                                         |
| ----------------------------- | ----------------------------------------------------------------------------------------------- |
| `codegen/rvalue.rs`           | Add `WasmRValue` (already exists)                                                               |
| `codegen/expr/mod.rs`         | emit_rvalue returns WasmRValue; handle Assignment; fix unary minus                              |
| `codegen/expr/binary.rs`      | Add lhs_ty, rhs_ty params; implement Mult, Div, Mod; Q32 mul/div sequences; type-based dispatch |
| `codegen/expr/literal.rs`     | Return Type (or add type helper)                                                                |
| `codegen/expr/variable.rs`    | Return Type from lookup                                                                         |
| `codegen/expr/type_infer.rs`  | Already exists; may use for binary result type                                                  |
| `codegen/expr/assignment.rs`  | New: emit_assignment_rvalue                                                                     |
| `codegen/stmt/declaration.rs` | emit_rvalue now returns WasmRValue; drop return                                                 |
| `codegen/stmt/return_.rs`     | Same                                                                                            |
| `codegen/stmt/expr_stmt.rs`   | Same                                                                                            |

---

## Validation

After implementation:

- `cargo test -p lps-wasm`
- `scripts/filetests.sh` or equivalent with `--target wasm.q32`
- Expect: scalar int/uint/float tests passing (those without unimplemented features in other functions)
