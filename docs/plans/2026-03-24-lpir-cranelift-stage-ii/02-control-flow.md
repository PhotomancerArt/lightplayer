# Phase 2: Structured Control Flow

## Scope

Implement `IfStart`/`Else`/`End`, `LoopStart`/`End`, `Break`, `Continue`,
`BrIfNot` in `emit/control.rs`. Populate the `CtrlFrame` enum. Add tests
for conditionals, loops, nested structures, and multi-path returns.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. `CtrlFrame` enum in `emit/mod.rs`

```rust
pub(crate) enum CtrlFrame {
    If {
        else_block: Block,
        merge_block: Block,
    },
    Else {
        merge_block: Block,
    },
    Loop {
        header_block: Block,
        exit_block: Block,
    },
}
```

Switch variants added in Phase 3.

### 2. `emit/control.rs` — block stack operations

The function signature:

```rust
use cranelift_codegen::ir::{Block, InstBuilder};
use cranelift_frontend::FunctionBuilder;

pub(crate) fn emit_control(
    op: &Op,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctrl_stack: &mut Vec<CtrlFrame>,
) -> Result<bool, CompileError>
```

#### IfStart

```rust
Op::IfStart { cond, .. } => {
    let cond_val = use_v(builder, vars, *cond);
    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();

    // Branch: if cond != 0 goto then, else goto else_block
    let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
    builder.ins().brif(pred, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    ctrl_stack.push(CtrlFrame::If { else_block, merge_block });
}
```

#### Else

```rust
Op::Else => {
    match ctrl_stack.pop() {
        Some(CtrlFrame::If { else_block, merge_block }) => {
            // End the then-arm: jump to merge
            builder.ins().jump(merge_block, &[]);
            builder.switch_to_block(else_block);
            ctrl_stack.push(CtrlFrame::Else { merge_block });
        }
        _ => return Err(CompileError::unsupported("else without matching if")),
    }
}
```

#### End (If/Else)

```rust
// End of if-without-else: else_block falls through to merge
Some(CtrlFrame::If { else_block, merge_block }) => {
    builder.ins().jump(merge_block, &[]);
    builder.switch_to_block(else_block);
    builder.ins().jump(merge_block, &[]);
    builder.switch_to_block(merge_block);
}
// End of if-with-else: else_block falls through to merge
Some(CtrlFrame::Else { merge_block }) => {
    builder.ins().jump(merge_block, &[]);
    builder.switch_to_block(merge_block);
}
```

#### LoopStart

```rust
Op::LoopStart { .. } => {
    let header_block = builder.create_block();
    let exit_block = builder.create_block();

    builder.ins().jump(header_block, &[]);
    builder.switch_to_block(header_block);

    ctrl_stack.push(CtrlFrame::Loop { header_block, exit_block });
}
```

#### Break

Walk the control stack (from top) to find the innermost `Loop`:

```rust
Op::Break => {
    let exit = find_innermost_loop_exit(ctrl_stack)?;
    builder.ins().jump(exit, &[]);
    // Create unreachable block for any dead code after break
    let dead = builder.create_block();
    builder.switch_to_block(dead);
}
```

#### Continue

```rust
Op::Continue => {
    let header = find_innermost_loop_header(ctrl_stack)?;
    builder.ins().jump(header, &[]);
    let dead = builder.create_block();
    builder.switch_to_block(dead);
}
```

#### BrIfNot

Conditional break — if condition is false, break out of innermost loop:

```rust
Op::BrIfNot { cond } => {
    let cond_val = use_v(builder, vars, *cond);
    let exit = find_innermost_loop_exit(ctrl_stack)?;
    let continue_block = builder.create_block();

    let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
    builder.ins().brif(pred, continue_block, &[], exit, &[]);
    builder.switch_to_block(continue_block);
}
```

#### End (Loop)

```rust
Some(CtrlFrame::Loop { header_block, exit_block }) => {
    // Back-edge: jump to header
    builder.ins().jump(header_block, &[]);
    builder.switch_to_block(exit_block);
}
```

### 3. Helper functions (bottom of control.rs)

```rust
fn find_innermost_loop_exit(ctrl_stack: &[CtrlFrame]) -> Result<Block, CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Loop { exit_block, .. } = frame {
            return Ok(*exit_block);
        }
    }
    Err(CompileError::unsupported("break/brifnot outside loop"))
}

fn find_innermost_loop_header(ctrl_stack: &[CtrlFrame]) -> Result<Block, CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Loop { header_block, .. } = frame {
            return Ok(*header_block);
        }
    }
    Err(CompileError::unsupported("continue outside loop"))
}
```

### 4. Tests

All tests use `parse_module` → `jit_from_ir` → transmute → call → assert.

**`test_if_else`** — simple conditional:
```
func @max(v0:f32, v1:f32) -> f32 {
  v2:i32 = fgt v0, v1
  if v2
    return v0
  else
    return v1
  end
}
```
Verify `max(3.0, 1.0) == 3.0` and `max(1.0, 5.0) == 5.0`.

**`test_if_no_else`** — if without else:
```
func @clamp_positive(v0:f32) -> f32 {
  v1:f32 = fconst 0.0
  v2:i32 = flt v0, v1
  if v2
    v0 = copy v1
  end
  return v0
}
```
Verify `clamp_positive(-3.0) == 0.0` and `clamp_positive(5.0) == 5.0`.

**`test_loop_countdown`** — simple loop with break:
```
func @countdown(v0:i32) -> i32 {
  v1:i32 = iconst 0
  loop
    v2:i32 = ieq v0, v1
    brifnot v2         // LPIR: brifnot = "break if (v2 != 0) is false" → break when v2 is true
    // wait, brifnot semantics: break if cond is false (i.e. continue while cond is true)
    // Actually: BrIfNot breaks when cond == 0. So brifnot v0 means "break if v0 == 0"
    // We want: while v0 != 0, decrement
    v0 = isub_imm v0, 1
    v1 = iadd_imm v1, 1
  end
  return v1
}
```

Actually let me re-check BrIfNot semantics. LPIR `BrIfNot { cond }` means
"break out of the loop if cond is false (zero)". Equivalent to
"continue while cond is true". So:

```
func @sum_to_n(v0:i32) -> i32 {
  v1:i32 = iconst 0
  loop
    brifnot v0
    v1 = iadd v1, v0
    v0 = isub_imm v0, 1
  end
  return v1
}
```
Verify `sum_to_n(5) == 15` (5+4+3+2+1).

**`test_loop_break`** — explicit break:
```
func @first_below(v0:f32, v1:f32) -> f32 {
  v2:f32 = fconst 1.0
  loop
    v3:i32 = flt v0, v1
    if v3
      break
    end
    v0 = fsub v0, v2
  end
  return v0
}
```
Verify `first_below(10.0, 3.0) == 2.0`.

**`test_nested_if_in_loop`** — nesting:
```
func @abs_sum(v0:i32, v1:i32) -> i32 {
  v2:i32 = iconst 0
  v3:i32 = iconst 0
  loop
    brifnot v0
    v4:i32 = isub_imm v0, 1
    v0 = copy v4
    // conditional: if v0 is even (v0 % 2 == 0) add 1, else add 2
    v5:i32 = irem_s v0, v1
    v6:i32 = ieq_imm v5, 0
    if v6
      v2 = iadd_imm v2, 1
    else
      v2 = iadd_imm v2, 2
    end
  end
  return v2
}
```

Keep tests simple and targeted — each tests one construct. More complex
nesting scenarios can be added later.

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```
