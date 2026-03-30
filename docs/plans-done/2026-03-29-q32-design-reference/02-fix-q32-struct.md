# Phase 2: Fix Q32 struct

## Scope

Update `lp-glsl/lp-glsl-builtins/src/glsl/q32/types/q32.rs` to match
`docs/design/q32.md`.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment

## Implementation Details

### 2a. Fix constant comments

The constant block has mismatched comments. Fix to match the design doc §4:

```rust
pub const ZERO: Q32 = Q32(0);
pub const HALF: Q32 = Q32(HALF);
pub const ONE: Q32 = Q32(ONE);
/// π ≈ 3.14159265
pub const PI: Q32 = Q32(205887);
/// 2π ≈ 6.28318531
pub const TAU: Q32 = Q32(411774);
/// e ≈ 2.71828183 (Euler's number)
pub const E: Q32 = Q32(178145);
/// φ ≈ 1.61803399 (golden ratio)
pub const PHI: Q32 = Q32(106039);
```

### 2b. Make Add/Sub saturating

Current: raw `i32` wrapping.

New (matches `__lp_lpir_fadd_q32` / `__lp_lpir_fsub_q32`):

```rust
impl Add for Q32 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        let wide = self.0 as i64 + rhs.0 as i64;
        Q32(wide.clamp(i32::MIN as i64, 0x7FFF_FFFF).try_into().unwrap_or(0))
    }
}
```

Same pattern for `Sub`, `AddAssign`, `SubAssign`.

### 2c. Make Mul saturating

Current: `((self.0 as i64 * rhs.0 as i64) >> 16) as i32` — truncates.

New (matches `__lp_lpir_fmul_q32`):

```rust
impl Mul for Q32 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        let wide = (self.0 as i64 * rhs.0 as i64) >> 16;
        Q32(wide.clamp(i32::MIN as i64, 0x7FFF_FFFF) as i32)
    }
}
```

Same for `MulAssign`.

### 2d. Fix Div — div-by-zero + saturating

Current: `rhs.0 != 0 → result, else Q32(0)`.

New (matches `__lp_lpir_fdiv_q32` + agreed 0/0 → 0):

```rust
impl Div for Q32 {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            if self.0 == 0 {
                return Q32(0);
            } else if self.0 > 0 {
                return Q32(0x7FFF_FFFF);
            } else {
                return Q32(i32::MIN);
            }
        }
        let wide = ((self.0 as i64) << 16) / rhs.0 as i64;
        Q32(wide.clamp(i32::MIN as i64, 0x7FFF_FFFF) as i32)
    }
}
```

Same for `DivAssign`.

### 2e. Review `mul_int`

Current: `Q32(self.0 * i)` — wrapping. Should this saturate too?

It's used in few places. For consistency, make it saturating:

```rust
pub const fn mul_int(self, i: i32) -> Q32 {
    let wide = self.0 as i64 * i as i64;
    // const fn can't use clamp, so manual:
    if wide > 0x7FFF_FFFF {
        Q32(0x7FFF_FFFF)
    } else if wide < i32::MIN as i64 {
        Q32(i32::MIN)
    } else {
        Q32(wide as i32)
    }
}
```

## Validate

```bash
cargo test -p lp-glsl-builtins -- q32
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

Existing tests that pass on wrapping behavior may need updating if they
relied on overflow wrapping — check test output carefully.
