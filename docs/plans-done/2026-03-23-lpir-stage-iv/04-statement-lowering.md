# Phase 4: Statement Lowering

## Scope

Implement `lower_stmt.rs` — walks Naga `Statement` trees and emits LPIR
ops via `FunctionBuilder`. Wire it into the `lower()` entry point so
function bodies are actually populated. After this phase, programs with
arithmetic, control flow, and local variables produce correct LPIR (but
not yet calls or math builtins).

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.

## Implementation Details

### `lower_stmt.rs` — main functions

```rust
pub(crate) fn lower_block(
    ctx: &mut LowerCtx,
    block: &naga::Block,
) -> Result<(), LowerError>
```

Iterate over the block's statements and dispatch each one.

### Statement types to handle

**`Emit(range)`**
- No-op. LPIR expressions are lowered lazily when referenced as operands.

**`Block(inner)`**
- Recursively call `lower_block(ctx, inner)`.

**`If { condition, accept, reject }`**
- Lower `condition` via `ctx.ensure_expr(condition)`.
- `ctx.fb.push_if(cond_vreg)`
- `lower_block(ctx, accept)`
- If `reject` is non-empty: `ctx.fb.push_else()` then `lower_block(ctx, reject)`
- `ctx.fb.end_if()`

**`Loop { body, continuing, break_if }`**
- `ctx.fb.push_loop()`
- `lower_block(ctx, body)`
- If `continuing` is non-empty:
  - `ctx.fb.push_continuing()`
  - `lower_block(ctx, continuing)`
  - If `break_if` is `Some(expr)`:
    - Lower the condition via `ctx.ensure_expr(expr)`
    - `ctx.fb.push(Op::BrIfNot { cond: vreg })`
      Note: Naga's `break_if` breaks when true; LPIR's `BrIfNot` breaks
      when the condition is **false**. So we must negate: emit
      `IeqImm { dst: neg, src: cond, imm: 0 }` first, then
      `BrIfNot { cond: neg }`.
      Wait — re-reading the WASM emitter: it emits `BrIf(1)` which
      branches when true. And LPIR's `BrIfNot` breaks when false.
      So: Naga break_if(cond) → break when cond is true.
      LPIR BrIfNot(cond) → break when cond is false.
      Therefore: emit `BrIfNot` with the condition directly — this is
      wrong. We need to emit it as: break when cond IS true.
      Since LPIR only has `BrIfNot`, we negate: emit `BrIfNot` with
      a negated condition. Or simpler: just emit `Op::BrIfNot { cond }`
      BUT since Naga's semantics are "break if true" and LPIR's are
      "break if NOT cond", we pass the condition as-is to get
      "break if NOT cond = break if cond is false" which is WRONG.
      
      Correct approach: negate the condition.
      `neg = IeqImm(cond, 0)` → neg is 1 when cond is 0 (i.e. false).
      `BrIfNot(neg)` → breaks when neg is false → breaks when cond is true.
      This matches Naga's `break_if` semantics.

- `ctx.fb.end_loop()`

**`Break`**
- `ctx.fb.push(Op::Break)`

**`Continue`**
- `ctx.fb.push(Op::Continue)`

**`Return { value }`**
- If `value` is `Some(expr)`:
  - Lower the expression via `ctx.ensure_expr(expr)`
  - `ctx.fb.push_return(&[vreg])`
- If `value` is `None`:
  - `ctx.fb.push_return(&[])`

**`Store { pointer, value }`**
- Resolve the pointer to a local variable handle.
  ```rust
  fn store_pointer_local(func: &naga::Function, expr: Handle<Expression>) -> Handle<LocalVariable>
  ```
  Pattern-match on `Expression::LocalVariable(lv)` in the function's
  expression arena.
- Lower the value expression via `ctx.ensure_expr(value)`.
- Emit `Op::Copy { dst: local_vreg, src: value_vreg }` where
  `local_vreg = ctx.resolve_local(lv)`.

**`Call { function, arguments, result }`**
- Stubbed with `todo!()` for Phase 5.

**`Switch { selector, cases }`**
- Lower `selector` via `ctx.ensure_expr(selector)`.
- `ctx.fb.push_switch(sel_vreg)`
- For each case in `cases`:
  - If `case.value == SwitchValue::Default`:
    `ctx.fb.push_default()`
  - Else (`SwitchValue::I32(v)`):
    `ctx.fb.push_case(v)`
  - `lower_block(ctx, &case.body)`
- `ctx.fb.end_switch()`

Note: Naga represents switch cases with `fall_through` field. For now,
each case body is self-contained (Naga GLSL frontend inserts implicit
breaks). If `fall_through` is true, omit the `end_switch_arm()` call.
Actually — check if `FunctionBuilder` handles this or if we need explicit
arm endings. Looking at the builder: `push_case` patches the previous
case's `end_offset`, and `end_switch` closes the whole thing. There is
also `end_switch_arm()` which inserts `Op::End` for a single arm. We
should call `end_switch_arm()` after each case body to close the case
block, matching the text format's `}` per case.

Revised: After each case body, call `ctx.fb.end_switch_arm()` (this emits
`Op::End` for the case arm). After all cases, call `ctx.fb.end_switch()`.

### Wire into `lower.rs`

Replace the stub in the per-function loop:
1. Create `LowerCtx` for the function.
2. Call `lower_block(&mut ctx, &function.body)`.
3. If the function has no explicit `Return` at the end (e.g. void functions),
   emit `ctx.fb.push_return(&[])`.
4. Call `ctx.fb.finish()` and add to the module builder.

## Validate

```
cargo check -p lp-glsl-naga
cargo +nightly fmt -p lp-glsl-naga -- --check
```

At this point, simple GLSL programs (arithmetic, if/else, loops, local
variables) should lower to valid LPIR. Calls are still stubbed.
