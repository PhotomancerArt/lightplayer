# Phase 3: Switch Statement

## Scope

Implement `SwitchStart`/`CaseStart`/`DefaultStart`/`End` in
`emit/control.rs`. Add `CtrlFrame` variants for switch. Add tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. New `CtrlFrame` variants in `emit/mod.rs`

```rust
pub(crate) enum CtrlFrame {
    If { ... },
    Else { ... },
    Loop { ... },
    Switch {
        selector: Value,
        merge_block: Block,
    },
    Case {
        merge_block: Block,
        next_case_block: Block,
    },
    Default {
        merge_block: Block,
    },
}
```

### 2. Switch translation in `emit/control.rs`

#### SwitchStart

```rust
Op::SwitchStart { selector, .. } => {
    let selector_val = use_v(builder, vars, *selector);
    let merge_block = builder.create_block();
    let first_case_block = builder.create_block();

    builder.ins().jump(first_case_block, &[]);
    builder.switch_to_block(first_case_block);

    ctrl_stack.push(CtrlFrame::Switch {
        selector: selector_val,
        merge_block,
    });
}
```

#### CaseStart

Each case: compare selector to value, branch to body or next case.

```rust
Op::CaseStart { value, .. } => {
    let (selector, merge_block) = find_innermost_switch(ctrl_stack)?;
    let body_block = builder.create_block();
    let next_case_block = builder.create_block();

    let cmp = builder.ins().icmp_imm(IntCC::Equal, selector, i64::from(*value));
    builder.ins().brif(cmp, body_block, &[], next_case_block, &[]);

    builder.switch_to_block(body_block);
    ctrl_stack.push(CtrlFrame::Case { merge_block, next_case_block });
}
```

#### DefaultStart

```rust
Op::DefaultStart { .. } => {
    let (_, merge_block) = find_innermost_switch(ctrl_stack)?;
    ctrl_stack.push(CtrlFrame::Default { merge_block });
}
```

Default body is just the fallthrough — whatever block we're currently in
(which is the `next_case_block` from the last case, or the first case block
if there were no cases before default).

#### End on Case

```rust
Some(CtrlFrame::Case { merge_block, next_case_block }) => {
    builder.ins().jump(merge_block, &[]);
    builder.switch_to_block(next_case_block);
}
```

#### End on Default

```rust
Some(CtrlFrame::Default { merge_block }) => {
    builder.ins().jump(merge_block, &[]);
}
```

#### End on Switch

```rust
Some(CtrlFrame::Switch { merge_block, .. }) => {
    // If we reach here without default, current block falls through to merge
    builder.ins().jump(merge_block, &[]);
    builder.switch_to_block(merge_block);
}
```

### 3. Helper function

```rust
fn find_innermost_switch(ctrl_stack: &[CtrlFrame]) -> Result<(Value, Block), CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Switch { selector, merge_block } = frame {
            return Ok((*selector, *merge_block));
        }
    }
    Err(CompileError::unsupported("case/default outside switch"))
}
```

### 4. Tests

**`test_switch_basic`** — three cases + default:

```
func @classify(v0:i32) -> i32 {
  v1:i32 = iconst 0
  switch v0
    case 1
      v1 = iconst 10
    end
    case 2
      v1 = iconst 20
    end
    default
      v1 = iconst -1
    end
  end
  return v1
}
```

Verify: `classify(1) == 10`, `classify(2) == 20`, `classify(99) == -1`.

**`test_switch_no_default`** — cases only:

```
func @map_value(v0:i32) -> i32 {
  v1:i32 = iconst 0
  switch v0
    case 0
      v1 = iconst 100
    end
    case 1
      v1 = iconst 200
    end
  end
  return v1
}
```

Verify: `map_value(0) == 100`, `map_value(1) == 200`, `map_value(5) == 0`.

## Validate

```
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift
```
