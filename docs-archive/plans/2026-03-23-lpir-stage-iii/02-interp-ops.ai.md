# Phase 2: Interpreter Tests — Arithmetic, Comparisons, Logic, Constants, Immediates, Casts, Select/Copy

## Scope

Add interpreter tests exercising every non-control-flow, non-memory Op variant
with representative inputs. These tests use text-format IR parsed at runtime
and the `run` / `run_f32` / `run_i32` helpers from Phase 1.

Each test should be short: build a tiny function with one or two ops, run it,
check the result. Group related ops in blocks but keep individual `#[test]`
functions for clear failure isolation.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

All tests go in `tests/interp.rs`, appended after the existing tests from
Phase 1.

### IR pattern

Most tests follow this pattern (single op + return):

```rust
#[test]
fn interp_fsub() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fsub v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(5.0), Value::F32(2.0)],
    );
    assert!((r - 3.0).abs() < 1e-6);
}
```

### Float arithmetic

- `interp_fsub`: 5.0 - 2.0 = 3.0
- `interp_fmul`: 3.0 * 4.0 = 12.0
- `interp_fdiv`: 10.0 / 4.0 = 2.5
- `interp_fneg`: fneg 3.0 = -3.0

### Integer arithmetic

- `interp_iadd`: 3 + 7 = 10
- `interp_isub`: 10 - 3 = 7
- `interp_imul`: 6 * 7 = 42
- `interp_idiv_s`: 7 / 2 = 3 (signed)
- `interp_idiv_u`: 0xFFFFFFFF / 2 = 0x7FFFFFFF (unsigned; pass -1 as i32)
- `interp_irem_s`: 7 % 3 = 1
- `interp_irem_u`: 7u % 3u = 1 (unsigned)
- `interp_ineg`: ineg 5 = -5
- `interp_iadd_wrapping`: i32::MAX + 1 wraps to i32::MIN

### Float comparisons

- `interp_feq_true`: 1.0 == 1.0 → 1
- `interp_feq_false`: 1.0 == 2.0 → 0
- `interp_fne_true`: 1.0 != 2.0 → 1
- `interp_flt`: 1.0 < 2.0 → 1; 2.0 < 1.0 → 0
- `interp_fle`: 1.0 <= 1.0 → 1; 2.0 <= 1.0 → 0
- `interp_fgt`: 2.0 > 1.0 → 1
- `interp_fge`: 2.0 >= 2.0 → 1

### Integer comparisons (signed)

- `interp_ieq`: 5 == 5 → 1; 5 == 6 → 0
- `interp_ine`: 5 != 6 → 1
- `interp_ilt_s`: -1 < 1 → 1; 1 < -1 → 0
- `interp_ile_s`: -1 <= -1 → 1
- `interp_igt_s`: 1 > -1 → 1
- `interp_ige_s`: 0 >= 0 → 1

### Integer comparisons (unsigned)

- `interp_ilt_u`: 1 <u 2 → 1; -1 (0xFFFFFFFF) <u 1 → 0
- `interp_ile_u`: 2 <=u 2 → 1
- `interp_igt_u`: -1 (0xFFFFFFFF) >u 1 → 1
- `interp_ige_u`: 0 >=u 0 → 1

### Logic / bitwise

- `interp_iand`: 0xFF & 0x0F = 0x0F
- `interp_ior`: 0xF0 | 0x0F = 0xFF
- `interp_ixor`: 0xFF ^ 0x0F = 0xF0
- `interp_ibnot`: !0 = -1
- `interp_ishl`: 1 << 4 = 16
- `interp_ishr_s`: -16 >>s 2 = -4 (arithmetic)
- `interp_ishr_u`: -1 >>u 28 = 0xF (logical)

### Constants

- `interp_fconst`: fconst.f32 3.14 → 3.14
- `interp_iconst`: iconst.i32 42 → 42
- `interp_iconst_neg`: iconst.i32 -7 → -7

### Immediate variants

- `interp_iadd_imm`: iadd_imm v0, 10 with v0=5 → 15
- `interp_isub_imm`: isub_imm v0, 3 with v0=10 → 7
- `interp_imul_imm`: imul_imm v0, 4 with v0=3 → 12
- `interp_ishl_imm`: ishl_imm v0, 2 with v0=3 → 12
- `interp_ishr_s_imm`: ishr_s_imm v0, 1 with v0=-4 → -2
- `interp_ishr_u_imm`: ishr_u_imm v0, 1 with v0=-2 → 0x7FFFFFFF
- `interp_ieq_imm`: ieq_imm v0, 42 with v0=42 → 1; v0=0 → 0

### Casts

- `interp_ftoi_sat_s`: 3.7 → 3
- `interp_ftoi_sat_u`: 3.7 → 3
- `interp_itof_s`: -1 → -1.0
- `interp_itof_u`: -1 (0xFFFFFFFF) → 4294967296.0 (as u32)

### Select / Copy

- `interp_select_true`: select cond=1, 10.0, 20.0 → 10.0
- `interp_select_false`: select cond=0, 10.0, 20.0 → 20.0
- `interp_copy`: copy v0 → same value

## Validate

```
cargo test -p lpir
cargo check -p lpir
cargo +nightly fmt -- --check
```
