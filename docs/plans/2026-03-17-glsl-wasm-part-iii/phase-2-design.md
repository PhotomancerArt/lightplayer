# Phase 2 Design: Type constructors, coercion, logical ops

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 2

## Goals

1. Implement scalar type constructors (`int()`, `float()`, `bool()`, `uint()`)
2. Implement implicit type coercion (int→float, bool→int)
3. Implement logical `&&` with short-circuit evaluation
4. Implement logical `||` with short-circuit evaluation
5. Implement ternary `? :`

---

## 1. Scalar type constructors

**Syntax:** `FunCall` with type name as function identifier: `int(x)`, `float(x)`, `bool(x)`, `uint(x)`.

**Detection:** In `emit_rvalue`, when we see `Expr::FunCall`, check if the identifier is a scalar type name (`int`, `float`, `bool`, `uint`). Use `lp_glsl_frontend::semantic::type_check::is_scalar_type_name`.

**Arguments:** Scalar constructors take exactly one argument (per Cranelift constructor.rs).

**Implementation:** Emit the argument, then coerce to target type.

```rust
// float(42) — int to Q32 float
emit_rvalue(ctx, sink, &args[0], options)?;  // stack: [42]
// Coerce: int → Q32 = (i32 << 16), i.e. multiply by 65536
sink.i32_const(16);
sink.i32_shl();
```

```rust
// int(3.14) — Q32 float to int
emit_rvalue(ctx, sink, &args[0], options)?;  // stack: [q32_val]
// Coerce: Q32 → int = (i32 >> 16), arithmetic shift
sink.i32_const(16);
sink.i32_shr_s();
```

```rust
// bool(x) — any scalar to bool: x != 0
emit_rvalue(ctx, sink, &args[0], options)?;
sink.i32_const(0);
sink.i32_ne();  // 1 if nonzero, 0 if zero
```

```rust
// uint(x) — int/bool to uint: bit pattern preserved
emit_rvalue(ctx, sink, &args[0], options)?;
// No op needed; i32 value is already correct bit pattern
```

---

## 2. Implicit type coercion

**When:** Binary ops, assignments, constructor args. The frontend validates; we apply the coercion at emission time when types differ.

**int → float (Q32):** `i32.shl` by 16. Value `a` becomes `a * 65536` in Q16.16.

**int → float (native):** `f32.convert_i32_s` (if we support Float mode).

**bool → int:** Identity. Bool is 0/1 in i32, already compatible.

**float → int:** `i32.shr_s` by 16 for Q32.

**Where to apply:** In `emit_binary_op` when promoting operands; or a `coerce_to_type(ctx, sink, val_on_stack, from_ty, to_ty)` helper. May need to emit extra instructions after `emit_rvalue` when types don't match expected.

---

## 3. Logical `&&` with short-circuit

**GLSL:** `a && b` — if `a` is false, `b` is not evaluated. Result is bool.

**WASM:** Use `if` block that produces a value:

```wasm
;; a && b
;; Stack at start: empty
(local.get $a)           ;; push a
(i32.eqz)                ;; a == 0 ?
(if (result i32)
  (then (i32.const 0))   ;; if a is 0: result 0, skip b
  (else
    (local.get $b)      ;; push b
    (i32.eqz)
    (if (result i32)
      (then (i32.const 0))
      (else (i32.const 1))
    )
  )
)
```

Simpler: `if (result i32) (then 0) (else (b != 0 ? 1 : 0))`. So:
- If `a == 0`: result 0.
- Else: evaluate `b`, result is `b != 0 ? 1 : 0`.

```rust
// a && b
let a = emit_rvalue(ctx, sink, lhs, options)?;
sink.i32_const(0);
sink.i32_eq();           // a == 0?
sink.if_(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32));
sink.instruction(&wasm_encoder::Instruction::I32Const(0));
sink.else_();
emit_rvalue(ctx, sink, rhs, options)?;
sink.i32_const(0);
sink.i32_ne();           // b != 0 → 1 or 0
sink.end();
```

Need to check wasm-encoder API for `if` with result type. May need `BlockType::Empty` and use different structure.

**wasm-encoder `if`:** Typically `sink.if_(block_type)` then `sink.instruction(...)` for then/else. For `(if (result i32) ...)`, block_type has a result.

---

## 4. Logical `||` with short-circuit

**GLSL:** `a || b` — if `a` is true, `b` is not evaluated.

```rust
// a || b
emit_rvalue(ctx, sink, lhs, options)?;
sink.i32_const(0);
sink.i32_ne();            // a != 0?
sink.if_(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32));
sink.instruction(&wasm_encoder::Instruction::I32Const(1));  // then: 1
sink.else_();
emit_rvalue(ctx, sink, rhs, options)?;
sink.i32_const(0);
sink.i32_ne();            // else: b != 0 ? 1 : 0
sink.end();
```

---

## 5. Ternary `? :`

**GLSL:** `cond ? then_expr : else_expr`. Condition must be bool. Result type is the common type of both branches.

**WASM:** `(if (result i32) (then ...) (else ...))` — both branches must produce a value of the same type.

```rust
// cond ? a : b
emit_rvalue(ctx, sink, cond, options)?;
sink.i32_const(0);
sink.i32_ne();            // cond != 0
// Now need: if (cond) { emit a } else { emit b }
sink.if_(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32));
emit_rvalue(ctx, sink, then_expr, options)?;
sink.else_();
emit_rvalue(ctx, sink, else_expr, options)?;
sink.end();
```

**Multi-value:** For vectors (Phase 5), ternary needs component-wise select. For Phase 2, scalars only.

---

## 6. FunCall dispatch in emit_rvalue

**Current:** `Expr::FunCall` is unimplemented.

**New flow:**
1. If scalar type constructor → emit constructor (section 1)
2. If vector constructor → Phase 5
3. If builtin → Phase 6
4. If user function → Phase 4
5. Else → error

For Phase 2, implement (1) only. Defer (2)–(4).

---

## 7. Binary op: enable And, Or

Phase 1 has `And | Or | Xor => Err(...)`. For Phase 2:
- `And` → short-circuit AND (section 3)
- `Or` → short-circuit OR (section 4)
- `Xor` → can use non-short-circuit: `(a != 0) != (b != 0)` → `a ^ b` when both are 0/1. Or `i32.xor` then normalize. For bool, `a ^ b` gives 0 or 1. Use `sink.i32_xor()` if both are bool (0/1).

---

## File change summary

| File | Changes |
|------|---------|
| `codegen/expr/mod.rs` | Add FunCall handling; dispatch to constructor, ternary, binary And/Or |
| `codegen/expr/constructor.rs` | New: emit_scalar_constructor (int, float, bool, uint) |
| `codegen/expr/ternary.rs` | New: emit_ternary_rvalue |
| `codegen/expr/binary.rs` | Implement And (short-circuit), Or (short-circuit), Xor |
| `codegen/expr/coercion.rs` | New: coerce_scalar (optional helper) |

---

## Validation

- `from-int.glsl`, `from-float.glsl`, `from-bool.glsl` type conversion tests
- `op-and.glsl`, `op-or.glsl` logical op tests
- `ternary*.glsl` control tests
