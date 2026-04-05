# Phase 4: Add comprehensive Q32 struct tests

## Scope

Fill test gaps in `q32.rs` so that every public API and every edge case from
`docs/design/q32.md` is locked by a test.

## Code Organization Reminders

- Tests should be in `mod tests` at the top of the module
- Tests should be short and concise, use utility functions to avoid duplication
- Utility functions should be at the bottom of the test module
- Each test should test one thing clearly
- Prefer clear test names over inline comments

## Implementation Details

### Tests to add (in `q32.rs` `mod tests`)

**Conversion edge cases:**

```rust
#[test]
fn test_from_fixed_roundtrip() {
    assert_eq!(Q32::from_fixed(65536).to_f32(), 1.0);
    assert_eq!(Q32::from_fixed(-65536).to_f32(), -1.0);
    assert_eq!(Q32::from_fixed(0).to_f32(), 0.0);
}

#[test]
fn test_to_i32_truncation() {
    assert_eq!(Q32::from_f32(1.9).to_i32(), 1);
    assert_eq!(Q32::from_f32(-1.9).to_i32(), -1);
    assert_eq!(Q32::from_f32(0.5).to_i32(), 0);
    assert_eq!(Q32::from_f32(-0.5).to_i32(), 0);
}
```

**Arithmetic saturation:**

```rust
#[test]
fn test_add_saturates_positive() {
    let big = Q32::from_f32(30000.0);
    let result = big + big;
    assert_eq!(result.0, 0x7FFF_FFFF);
}

#[test]
fn test_add_saturates_negative() {
    let big_neg = Q32::from_f32(-30000.0);
    let result = big_neg + big_neg;
    assert_eq!(result.0, i32::MIN);
}

#[test]
fn test_sub_saturates() {
    let big = Q32::from_f32(30000.0);
    let big_neg = Q32::from_f32(-30000.0);
    let result = big - big_neg;
    assert_eq!(result.0, 0x7FFF_FFFF);
}

#[test]
fn test_mul_saturates_positive() {
    let big = Q32::from_f32(1000.0);
    let result = big * big;
    assert_eq!(result.0, 0x7FFF_FFFF);
}

#[test]
fn test_mul_saturates_negative() {
    let big = Q32::from_f32(1000.0);
    let big_neg = Q32::from_f32(-1000.0);
    let result = big * big_neg;
    assert_eq!(result.0, i32::MIN);
}
```

**Division by zero:**

```rust
#[test]
fn test_div_zero_by_zero() {
    assert_eq!((Q32::ZERO / Q32::ZERO).0, 0);
}

#[test]
fn test_div_positive_by_zero() {
    assert_eq!((Q32::ONE / Q32::ZERO).0, 0x7FFF_FFFF);
}

#[test]
fn test_div_negative_by_zero() {
    assert_eq!((-Q32::ONE / Q32::ZERO).0, i32::MIN);
}

#[test]
fn test_div_saturates_overflow() {
    let big = Q32::from_f32(30000.0);
    let small = Q32::from_f32(0.001);
    let result = big / small;
    assert_eq!(result.0, 0x7FFF_FFFF);
}
```

**Remainder by zero:**

```rust
#[test]
fn test_rem_by_zero() {
    assert_eq!((Q32::ONE % Q32::ZERO).0, 0);
}

#[test]
fn test_rem_basic() {
    let result = Q32::from_f32(7.0) % Q32::from_f32(3.0);
    assert!((result.to_f32() - 1.0).abs() < 0.01);
}
```

**Other gaps:**

```rust
#[test]
fn test_abs() {
    assert_eq!(Q32::from_f32(5.0).abs().to_f32(), 5.0);
    assert_eq!(Q32::from_f32(-5.0).abs().to_f32(), 5.0);
    assert_eq!(Q32::ZERO.abs().to_f32(), 0.0);
}

#[test]
fn test_is_zero() {
    assert!(Q32::ZERO.is_zero());
    assert!(!Q32::ONE.is_zero());
}

#[test]
fn test_frac() {
    assert!((Q32::from_f32(1.75).frac().to_f32() - 0.75).abs() < 0.001);
    assert_eq!(Q32::from_f32(2.0).frac().to_f32(), 0.0);
}

#[test]
fn test_to_u16_clamped() {
    assert_eq!(Q32::from_f32(0.0).to_u16_clamped(), 0);
    assert_eq!(Q32::from_f32(1.0).to_u16_clamped(), 65535);
    assert_eq!(Q32::from_f32(0.5).to_u16_clamped(), 32767);
    assert_eq!(Q32::from_f32(-1.0).to_u16_clamped(), 0);
}

#[test]
fn test_mul_int_saturates() {
    let big = Q32::from_f32(20000.0);
    let result = big.mul_int(3);
    assert_eq!(result.0, 0x7FFF_FFFF);
}
```

**Named constants verification:**

```rust
#[test]
fn test_constant_pi() {
    assert!((Q32::PI.to_f32() - core::f32::consts::PI).abs() < 0.001);
}

#[test]
fn test_constant_tau() {
    assert!((Q32::TAU.to_f32() - core::f32::consts::TAU).abs() < 0.001);
}

#[test]
fn test_constant_e() {
    assert!((Q32::E.to_f32() - core::f32::consts::E).abs() < 0.001);
}

#[test]
fn test_constant_phi() {
    // φ = (1 + √5) / 2 ≈ 1.618034
    assert!((Q32::PHI.to_f32() - 1.618034).abs() < 0.001);
}
```

## Validate

```bash
cargo test -p lps-builtins -- q32
```

All new tests should pass. No existing tests should break (if they do,
it means the old wrapping behavior was being relied on — update the test
to match the new saturating spec).
