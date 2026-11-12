# Phase 3: Interpreter Tests — Edge-Case Numerics

## Scope

Test the GPU-aligned numeric semantics specified in `docs/lpir/00-overview.md`:
integer division/remainder by zero, NaN handling in comparisons and arithmetic,
saturating float-to-int casts, and shift amount masking.

These are the most likely places for interpreter bugs. If a test fails, fix
the interpreter inline.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

All tests go in `tests/interp.rs`.

### Division by zero → 0

Per spec: "Integer division / remainder by zero: result `0`."

```rust
#[test]
fn interp_idiv_s_by_zero() {
    let r = run_i32(
        "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = idiv_s v0, v1\n  return v2\n}\n",
        "f",
        &[Value::I32(42), Value::I32(0)],
    );
    assert_eq!(r, 0);
}
```

- `interp_idiv_s_by_zero`: 42 / 0 → 0
- `interp_idiv_u_by_zero`: 42 /u 0 → 0
- `interp_irem_s_by_zero`: 42 % 0 → 0
- `interp_irem_u_by_zero`: 42 %u 0 → 0

### Integer overflow edge case

- `interp_idiv_s_min_neg1`: i32::MIN / -1 — this is the one case where
  signed division can overflow. Rust's `wrapping_div` returns i32::MIN.
  Verify the interpreter matches.

### NaN handling

Per spec: "NaN in comparisons: treated as false (0 for condition values)."
And "NaN in arithmetic: NaN propagates per IEEE rules."

- `interp_feq_nan`: NaN == NaN → 0
- `interp_feq_nan_other`: NaN == 1.0 → 0
- `interp_fne_nan`: NaN != NaN → 1
- `interp_fne_nan_other`: NaN != 1.0 → 1
- `interp_flt_nan`: NaN < 1.0 → 0; 1.0 < NaN → 0
- `interp_fle_nan`: NaN <= 1.0 → 0
- `interp_fgt_nan`: NaN > 1.0 → 0
- `interp_fge_nan`: NaN >= 1.0 → 0
- `interp_fadd_nan`: NaN + 1.0 → NaN (check with `is_nan()`)
- `interp_fmul_nan`: NaN * 1.0 → NaN

### Float division edge cases

- `interp_fdiv_by_zero_pos`: 1.0 / 0.0 → +inf
- `interp_fdiv_by_zero_neg`: -1.0 / 0.0 → -inf
- `interp_fdiv_zero_by_zero`: 0.0 / 0.0 → NaN

### Saturating casts

Per spec: "Float-to-integer conversion: overflow or NaN — saturating to the
representable integer range."

**`ftoi_sat_s` (signed):**
- `interp_ftoi_sat_s_normal`: 3.7 → 3
- `interp_ftoi_sat_s_neg`: -3.7 → -3
- `interp_ftoi_sat_s_overflow_pos`: 1e15 → i32::MAX (2147483647)
- `interp_ftoi_sat_s_overflow_neg`: -1e15 → i32::MIN (-2147483648)
- `interp_ftoi_sat_s_nan`: NaN → 0
- `interp_ftoi_sat_s_inf`: +inf → i32::MAX
- `interp_ftoi_sat_s_neg_inf`: -inf → i32::MIN

**`ftoi_sat_u` (unsigned, stored as i32 bit pattern):**
- `interp_ftoi_sat_u_normal`: 3.7 → 3
- `interp_ftoi_sat_u_negative`: -1.0 → 0 (clamped)
- `interp_ftoi_sat_u_overflow`: 1e15 → -1 (0xFFFFFFFF as i32)
- `interp_ftoi_sat_u_nan`: NaN → 0

### Shift masking

Per spec: "Shift amount ≥ 32 bits: shift amount masked to 5 bits."

- `interp_ishl_mask`: 1 << 32 → 1 (32 & 31 = 0)
- `interp_ishl_mask_33`: 1 << 33 → 2 (33 & 31 = 1)
- `interp_ishr_s_mask`: -1 >> 32 → -1 (amount masked to 0)
- `interp_ishr_u_mask`: 0x80000000u >> 32 → 0x80000000u (masked to 0)

### Wrapping arithmetic

- `interp_imul_wrapping`: large values that overflow i32 wrap correctly

## Validate

```
cargo test -p lpir
cargo check -p lpir
cargo +nightly fmt -- --check
```
