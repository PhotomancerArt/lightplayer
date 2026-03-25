# Phase 1: WASM Continue Depth Fix

## Scope

Fix `continue` inside nested constructs (if, nested loops) within a loop body.
Currently `innermost_loop_continue_depth` always returns `Ok(0)`, which is only
correct when there's no nesting inside the loop body.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### `lp-glsl/lp-glsl-wasm/src/emit/control.rs`

Change `innermost_loop_continue_depth` to accept `wasm_open: WasmOpenDepth`
and compute the correct branch depth:

```rust
pub(crate) fn innermost_loop_continue_depth(
    ctrl: &[CtrlEntry],
    wasm_open: WasmOpenDepth,
) -> Result<u32, String> {
    for entry in ctrl.iter().rev() {
        if let CtrlEntry::Loop {
            inner_closed,
            outer_open_depth,
            ..
        } = entry
        {
            if *inner_closed {
                return Err(String::from("continue inside loop continuing section"));
            }
            // The inner body block is at outer_open_depth + 2
            // (outer block = +0, loop = +1, inner block = +2).
            // br depth = current nesting - target nesting.
            return Ok(wasm_open.saturating_sub(*outer_open_depth + 2));
        }
    }
    Err(String::from("continue outside loop"))
}
```

### `lp-glsl/lp-glsl-wasm/src/emit/ops.rs`

Update the `Op::Continue` call site to pass `wasm_open`:

```rust
Op::Continue => {
    let d = control::innermost_loop_continue_depth(ctrl, *wasm_open)?;
    sink.br(d);
}
```

### Tests

The existing WASM smoke test `q32_while_accumulates` should still pass (it
doesn't nest inside the loop body). The real validation is the filetests:

- `control/for/continue_nested.glsl` — all 3 cases
- `control/while/continue.glsl` — all 3 cases
- `control/while/nested_for.glsl` — both cases

## Validate

```bash
cargo test -p lp-glsl-wasm -q
scripts/glsl-filetests.sh control/for/continue_nested.glsl
scripts/glsl-filetests.sh control/while/continue.glsl
scripts/glsl-filetests.sh control/while/nested_for.glsl
```

All should pass on both `cranelift.q32` and `wasm.q32`.
