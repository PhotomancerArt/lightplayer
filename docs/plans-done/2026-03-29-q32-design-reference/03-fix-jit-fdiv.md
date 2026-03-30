# Phase 3: Fix JIT fdiv builtin — 0/0 → 0

## Scope

Update `lp-glsl/lp-glsl-builtins/src/builtins/lpir/fdiv_q32.rs` so that
`0 / 0` returns `0` instead of `MAX_FIXED`.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment

## Implementation Details

Current code:

```rust
if divisor == 0 {
    if dividend >= 0 {
        return MAX_FIXED;
    } else {
        return MIN_FIXED;
    }
}
```

`dividend == 0 && divisor == 0` falls into `dividend >= 0` → returns
`MAX_FIXED`. Per the design doc, `0 / 0` should return `0`.

Fix:

```rust
if divisor == 0 {
    if dividend == 0 {
        return 0;
    } else if dividend > 0 {
        return MAX_FIXED;
    } else {
        return MIN_FIXED;
    }
}
```

Update the existing `test_division_by_zero` test to add a `0 / 0` case:

```rust
// 0 / 0 should return 0
let result_zero = __lp_lpir_fdiv_q32(0, 0);
assert_eq!(result_zero, 0, "0 / 0 should return 0");
```

## Validate

```bash
cargo test -p lp-glsl-builtins -- fdiv_q32
```
