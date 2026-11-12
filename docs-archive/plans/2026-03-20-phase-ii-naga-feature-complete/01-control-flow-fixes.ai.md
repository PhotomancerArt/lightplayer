# Phase 1: Control flow + minor expression fixes

## Scope

Add `Break`, `Continue` to `emit_stmt`. Fix `LogicalNot` for non-Bool types
(Naga sometimes applies `!` to Sint comparisons). This unblocks ~22 test files
in `control/while`, `control/do_while`, `control/ternary`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Break and Continue in emit.rs

The current loop structure in `emit_stmt` for `Statement::Loop`:

```
block $exit     ;; br depth 2 from body = exit
  loop $loop    ;; br depth 1 from body = restart loop
    block $body ;; br depth 0 from body = end of body (→ continuing)
      <body>
    end $body
    <continuing>
    <break_if → br_if 1>  ;; exits $loop which exits to $exit? No...
    br 0                   ;; restart $loop
  end $loop
end $exit
```

Actually tracing the br depths more carefully from inside `<body>`:

- `br 0` → end of `$body` block → falls to `<continuing>`, then `br 0` restarts `$loop` = **continue
  **
- `br 1` → end of `$loop` → falls to end of `$exit` = **not right**, that exits the loop but not via
  `$exit` branch...

Let me re-check. From inside `<body>`:

- depth 0 = `$body` block
- depth 1 = `$loop` loop
- depth 2 = `$exit` block

`br 0` → exits `$body`, continues to `<continuing>`, then `br 0` in the
unconditional branch targets the `$loop` loop (restarts it) = **continue**
`br 2` → exits `$exit` block = **break**

For `break_if`, from inside `<continuing>`:

- depth 0 = `$loop` loop
- depth 1 = `$exit` block

So `br_if 1` from continuing = exits `$exit` = **break if true**. Correct.

Implementation:

```rust
Statement::Break => {
    wasm_fn.instruction(&Instruction::Br(2));
    Ok(())
}
Statement::Continue => {
    wasm_fn.instruction(&Instruction::Br(0));
    Ok(())
}
```

But wait — these are relative to the current nesting depth. If Break/Continue
appear inside an `if` block within the loop body, the br depths increase.

Need: a **break depth** context parameter that tracks how many WASM blocks
deep we are relative to the loop structure. Each `if/else/end` adds 1 to the
depth.

Approach: add `loop_break_depth: Option<u32>` and `loop_continue_depth: Option<u32>`
to the emit context. When entering a `Loop`, set these to their base values (2 and 0
respectively for break and continue). When entering an `If`, increment both by 1.
When entering a nested `Loop`, push new values.

Simpler approach: use a `Vec<LoopContext>` stack. Each entry records the WASM
block nesting depth at the point the loop was entered. Break =
`br (current_depth - loop_entry_depth + break_offset)`, Continue =
`br (current_depth - loop_entry_depth + continue_offset)`.

Actually, simplest: maintain a counter `wasm_block_depth` that increments for
every `block`, `loop`, or `if` instruction, and record the loop's depths at
entry time.

Recommended: pass `&mut EmitContext` instead of raw parameters:

```rust
struct EmitContext {
    break_depth: Option<u32>,
    continue_depth: Option<u32>,
    wasm_depth: u32,
}
```

On `Loop` entry: push 3 blocks (block/loop/block), record `break_depth = wasm_depth`
(for the outer `block`), `continue_depth = wasm_depth + 2` (for the inner `block`).
On `If`: push 1 block.

Then:

- `Break` → `br(wasm_depth - break_depth)`
- `Continue` → `br(wasm_depth - continue_depth)`

Where `wasm_depth` is the depth **inside** the body.

Let me think again. After entering the loop:

```
block     ;; wasm_depth was D, now D+1
  loop    ;; D+2
    block ;; D+3 -- this is where body starts
```

Inside body at depth D+3:

- break targets the outermost `block` at D+1: `br(D+3 - (D+1)) = br(2)`
- continue targets the inner `block` at D+3: `br(D+3 - D+3) = br(0)`

If there's an `if` inside the body:

```
      if    ;; D+4
```

- break: `br(D+4 - (D+1)) = br(3)`
- continue: `br(D+4 - D+3) = br(1)`

So we need `break_target_depth` and `continue_target_depth` to be the WASM
depth of the block we want to branch to, and compute `br(current - target)`.

Implementation: refactor emit functions to take an `EmitCtx`:

```rust
struct EmitCtx {
    depth: u32,
    break_target: Option<u32>,
    continue_target: Option<u32>,
}

impl EmitCtx {
    fn enter_block(&self) -> Self { Self { depth: self.depth + 1, ..*self } }
    fn enter_loop(&self) -> Self {
        Self {
            depth: self.depth + 3,
            break_target: Some(self.depth + 1),
            continue_target: Some(self.depth + 3),
        }
    }
}
```

### 2. LogicalNot for Sint

Naga sometimes emits `Unary { op: LogicalNot, expr }` where `expr` is Sint
(e.g. `!(i > 0)` where the comparison returns Sint in Naga's view). Fix
`emit_unary` to handle this:

```rust
(UnaryOperator::LogicalNot, ScalarKind::Sint | ScalarKind::Uint, _) => {
    wasm_fn.instruction(&Instruction::I32Eqz);
}
```

### 3. Tests

Run control flow filetests:

```bash
scripts/filetests.sh --target wasm.q32 "control/"
```

Expected: `while/`, `do_while/`, `for/` tests that only use scalars should pass.
Tests that use vectors or builtins will still fail (handled in later phases).

### 4. Do-while: split loop body (trailing `if (!cond) break`)

Naga’s GLSL front lowers `do { S } while (C)` to a `Loop` whose `body` block is
`[ … S … , if (!C) { break; } ]` with **empty** `continuing` (see
`naga::front::glsl::parser::functions`).

If the WASM `continue` target is the `end` of a single inner block around that
entire Naga `body`, `continue` skips the trailing condition and the loop never
terminates (fuel trap). **Fix:** when `continuing` and `break_if` are empty and
the last statement matches `if (!cond) { break; }`, emit the **user** prefix
inside the inner `block` only, `end` that block, then emit the trailing guard
**outside** that block (still inside the `loop`). `split_do_while_trailing_guard`
in `emit.rs` implements the detection.

## Validate

```bash
scripts/filetests.sh --target wasm.q32 "control/"
scripts/filetests.sh --target wasm.q32 "scalar/"
cargo check -p lps-wasm
```

Scalar tests must remain passing. Control flow tests should have reduced failures.
