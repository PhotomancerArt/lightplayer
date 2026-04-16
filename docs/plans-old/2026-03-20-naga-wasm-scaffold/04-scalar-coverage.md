# Phase 4: Scalar coverage — pass all scalar filetests

## Scope

Expand `emit_expr` to handle the full set of scalar expressions and statements
that appear in the `scalar/` filetest directory. This includes:

- All binary operators (add, sub, mul, div, comparisons, modulo)
- Unary operators (negate)
- Type conversions (`As` expressions: float↔int, int↔uint, bool↔int, etc.)
- Boolean operations (and, or, not)
- Ternary / select
- Local variable declaration with initializers
- Multiple statements in sequence
- Nested expressions

The goal is: all `scalar/float/`, `scalar/int/`, `scalar/uint/`, and
`scalar/bool/` filetests pass on `wasm.q32`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Binary operators in emit_expr

The Phase 2 implementation covers `Add`, `Sub`, `Mul`, `Div`. Add remaining:

```
Modulo:
  Float/Float → f32.div + f32.trunc + f32.mul + f32.sub (a - trunc(a/b)*b)
  Float/Q32   → import or inline sequence
  Sint        → i32.rem_s
  Uint        → i32.rem_u

Equal:    f32.eq / i32.eq
NotEqual: f32.ne / i32.ne
Less:     f32.lt / i32.lt_s / i32.lt_u
LessEqual:    f32.le / i32.le_s / i32.le_u
Greater:      f32.gt / i32.gt_s / i32.gt_u
GreaterEqual: f32.ge / i32.ge_s / i32.ge_u

And: i32.and (boolean)
Or:  i32.or  (boolean)
```

### 2. Unary operators

Naga `Expression::Unary { op, expr }`:

```
Negate:
  Float/Float → emit expr, f32.neg
  Float/Q32   → i32.const 0, emit expr, i32.sub
  Sint        → i32.const 0, emit expr, i32.sub

BitNot:
  Sint/Uint   → i32.const -1, emit expr, i32.xor

Not (logical):
  Bool → emit expr, i32.eqz
```

### 3. Type conversions (As expressions)

Naga `Expression::As { expr, kind, convert }`:

When `convert` is `Some(width)` (explicit cast):

```
Float → Sint (Float mode): i32.trunc_f32_s
Float → Sint (Q32 mode):   i32.const 16, i32.shr_s (shift right to get integer part)
Float → Uint (Float mode): i32.trunc_f32_u
Float → Uint (Q32 mode):   i32.const 16, i32.shr_u

Sint → Float (Float mode): f32.convert_i32_s
Sint → Float (Q32 mode):   i32.const 16, i32.shl

Uint → Float (Float mode): f32.convert_i32_u
Uint → Float (Q32 mode):   i32.const 16, i32.shl

Sint → Uint:  no-op (same bit pattern in WASM i32)
Uint → Sint:  no-op

Bool → Sint/Uint: already i32 (0 or 1), no-op
Sint/Uint → Bool: i32.const 0, i32.ne
Float → Bool: emit != 0.0 comparison
```

### 4. Select / ternary

Naga `Expression::Select { condition, accept, reject }`:

```
emit accept
emit reject
emit condition
select
```

WASM `select` pops (condition, val2, val1) and pushes val1 if condition != 0,
else val2.

Note: `accept` and `reject` are evaluated before `condition` for the WASM
stack order. Make sure evaluation order doesn't cause issues with side effects
(it shouldn't for Phase I — all scalar, no function calls).

### 5. Emit statement improvements

`Statement::Store` needs to handle re-assignment to existing locals:

```rust
Statement::Store { pointer, value } => {
    emit_expr(value);
    match func.expressions[pointer] {
        Expression::LocalVariable(lv) => {
            let idx = alloc.resolve_local_variable(lv).unwrap();
            wasm_fn.instruction(&Instruction::LocalSet(idx));
        }
        _ => return Err(...)
    }
}
```

### 6. Q32 multiply and divide

Q32 multiply (16.16 × 16.16):

```
emit left    → i32 (Q16.16)
i64.extend_i32_s
emit right   → i32 (Q16.16)
i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s
i32.wrap_i64
```

Q32 divide (16.16 / 16.16):

```
emit left    → i32 (Q16.16)
i64.extend_i32_s
i64.const 16
i64.shl
emit right   → i32 (Q16.16)
i64.extend_i32_s
i64.div_s
i32.wrap_i64
```

Note: These match the existing lps-wasm Q32 sequences.

### 7. Handling Naga's Emit statement

Naga's `Statement::Emit(range)` tells the backend which expression handles
in the range need to be evaluated. For our stack-based WASM emission, we need
to handle this carefully:

The key insight is that expressions within an Emit range are "made available"
but in WASM we emit them on-demand when they're referenced. However, if an
expression is used multiple times, it needs to be stored in a local.

For Phase I (scalars, no shared subexpressions expected in simple tests),
we can use a simple approach: track which expressions have been emitted to
the stack. If an expression has already been emitted and stored, `local.get`
it instead of re-emitting.

Approach: maintain a `BTreeMap<Handle<Expression>, u32>` mapping emitted
expressions to their WASM local index. When emitting for `Emit(range)`,
store each result to a temp local. When a later expression references it
via the handle, `local.get` from that temp.

This naturally solves the local allocation problem — we know upfront how many
expression temporaries we need (the arena length gives the upper bound, but
we only allocate for expressions that appear in `Emit` ranges).

### 8. Additional tests

Add unit tests for:

- Comparison operators returning bool (0 or 1)
- Int arithmetic (add, sub, mul, div, mod)
- Type conversions (float→int, int→float in both modes)
- Unary negate
- Boolean operations
- Ternary select
- Q32 multiply and divide precision

## Validate

```bash
cargo test -p lps-filetests -- scalar
```

All `scalar/float/`, `scalar/int/`, `scalar/uint/`, `scalar/bool/` tests
should pass on `wasm.q32`. Any tests that reference features outside Phase I
scope (e.g. function calls to user-defined functions, control flow) should
be annotated with `// unimplemented: wasm`.

```bash
cargo check -p lps-wasm
```

No warnings.
