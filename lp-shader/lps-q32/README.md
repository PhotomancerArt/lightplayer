# lps-q32

Fixed-point Q16.16 arithmetic library for **LightPlayer** — `no_std` + alloc, designed for
embedded GLSL shader execution without an FPU.

Q32 replaces IEEE `f32` with saturating fixed-point math (16 integer bits, 16 fractional bits
stored in `i32`). This crate provides the reference implementation: `Q32` scalar type, vector
and matrix types (`Vec2Q32`–`Vec4Q32`, `Mat2Q32`–`Mat4Q32`), helper functions, and compiler
encode/decode utilities.

**Normative Q32 semantics** are defined in `[docs/design/q32.md](../../docs/design/q32.md)` — all
implementations (this crate, JIT builtins, Cranelift emitter, WASM emitter, LPIR interpreter)
must conform.

## Overview

```rust
use lps_q32::{Q32, Vec3Q32, Vec2Q32};

// Create from float (truncates toward zero)
let a = Q32::from_f32_wrapping(1.5);
let b = Q32::from_i32(2);           // 2.0 exactly

// Saturating arithmetic
let c = a + b;                      // 3.5
let d = a * b;                      // 3.0

// Vectors
let v = Vec3Q32::from_f32(1.0, 2.0, 3.0);
let len_sq = v.length_squared();
let normalized = v.normalize();
```

## Features

- `**no_std` + alloc** — runs on bare-metal RISC-V (ESP32-C6 target)
- **Saturating arithmetic** — all ops clamp to `[i32::MIN, 0x7FFF_FFFF]`; no overflow surprises
- **GLSL-aligned** — `floor`, `fract`, `mod`, `mix`, `clamp`, `sin`, `cos`, `sqrt`, `step`, etc.
- **Vector/Matrix types** — full `vec2`/`vec3`/`vec4`/`mat2`/`mat3`/`mat4` coverage with swizzling
- **Trait helpers** — `ToQ32` for `i32`/`i16`/`i8`/`u16`/`u8`, `ToQ32Clamped` for `u32`

## API Structure


| Module                       | Contents                                                                       |
| ---------------------------- | ------------------------------------------------------------------------------ |
| `types::q32`                 | `Q32` scalar, constants (`PI`, `TAU`, `E`, `PHI`), arithmetic, conversions     |
| `types::vec2_q32`–`vec4_q32` | Vector types with component-wise ops, dot, cross, length, normalize, swizzles  |
| `types::mat2_q32`–`mat4_q32` | Matrix types with mul, transpose, inverse, determinant                         |
| `fns`                        | Component-wise helpers: `sin_vec2`, `cos_vec3`, `mix_vec4`, `floor_vec2`, etc. |
| `q32_encode`                 | `q32_encode(f32)`, `q32_encode_f64(f64)`, `q32_to_f64`, `Q32_SHIFT`/`Q32_FRAC` |
| `q32_options`                | `Q32Options`, `AddSubMode`, `MulMode`, `DivMode` — compiler mode selection     |


**Convenience re-exports at crate root:**

```rust
use lps_q32::{Q32, Vec2Q32, Vec3Q32, Vec4Q32, Mat2Q32, Mat3Q32, Mat4Q32};
use lps_q32::{ToQ32, ToQ32Clamped, q32_encode, q32_encode_f64, q32_to_f64};
use lps_q32::{Q32Options, AddSubMode, MulMode, DivMode, Q32_SHIFT, Q32_FRAC};
```

## Semantics Summary

**Q16.16 format:**

- 16 signed integer bits (range −32768 to 32767)
- 16 fractional bits (resolution 1/65536 ≈ 0.0000153)
- Raw storage: `floor(value × 65536)` as `i32`

**Arithmetic (all saturating):**

- Add/sub: widen to `i64`, operate, clamp to `[i32::MIN, 0x7FFF_FFFF]`
- Mul: widen to `i64`, mul, shift right 16, clamp
- Div: widen dividend to `i64`, shift left 16, divide, clamp
- Div by zero: `0/0 → 0`, `positive/0 → MAX`, `negative/0 → MIN`
- Remainder: raw `i32 % i32`; `x % 0 → 0`

See `[docs/design/q32.md](../../docs/design/q32.md)` for full specification.

## Usage

### Basic arithmetic

```rust
use lps_q32::Q32;

let one = Q32::ONE;
let half = Q32::HALF;
let pi = Q32::PI;

let sum = one + half;           // 1.5
let prod = one * Q32::from_i32(3); // 3.0
let div = Q32::from_i32(7) / Q32::from_i32(2); // 3.5 (3.4999...)
```

### Vectors

```rust
use lps_q32::{Vec3Q32, Q32};

let a = Vec3Q32::from_f32(1.0, 0.0, 0.0);
let b = Vec3Q32::from_f32(0.0, 1.0, 0.0);

let dot = a.dot(b);             // 0.0
let cross = a.cross(b);         // (0, 0, 1)
let mix = a.mix(b, Q32::HALF);  // (0.5, 0.5, 0.0)
```

### Matrices

```rust
use lps_q32::{Mat3Q32, Vec3Q32};

let m = Mat3Q32::identity();
let v = Vec3Q32::from_f32(1.0, 2.0, 3.0);
let transformed = m * v;        // (1, 2, 3)
```

### Encoding: raw bits for compiler vs. typed values for runtime

The crate provides **two different** float→Q32 paths with different semantics and return types:

| Function | Returns | Use Case | Rounding | Out-of-range | Typical Caller |
|----------|---------|----------|----------|--------------|----------------|
| `q32_encode(f32)` → `i32` | **Raw `i32` bits** | Compiler constant emission | `libm::round` (nearest) | **Saturate** to max/min | `lpvm-cranelift` generating `iconst.i32` |
| `Q32::from_f32_wrapping(f32)` → `Q32` | **Typed `Q32` value** | Runtime conversion | Truncate toward zero | **Wrap** (Rust `as` semantics) | Builtins, engine mapping, tests |

**Why two paths?**

- **Compiler constants** (`q32_encode`): Codegen emits raw `i32` constants into the instruction stream (e.g., `const float x = 50000.0;` → `iconst.i32 0x7FFF_FFFF`). Saturation prevents wrapping to negative, and rounding gives slightly better accuracy than truncation for constants. The function returns `i32` directly to avoid the conceptual indirection of "create Q32, then extract bits."

- **Runtime conversions** (`from_f32_wrapping`): In running code, we want fast conversion (no rounding overhead, no clamp checks) that matches the semantics of a cast in generated code. Returns a proper `Q32` value for arithmetic.

```rust
use lps_q32::{q32_encode, q32_encode_f64, Q32, Q32_SHIFT};

// Compiler emission: returns raw i32 bits (round + saturate)
let encoded: i32 = q32_encode(1.25);        // 0x0001_4000
let encoded = q32_encode_f64(-0.5);         // 0xFFFF_8000
let max = q32_encode(50000.0);              // 0x7FFF_FFFF (saturated)

// Runtime conversion: returns Q32 value (truncate + wrap)
let q = Q32::from_f32_wrapping(1.25);        // truncates, no saturate
let scale = 1i64 << Q32_SHIFT;              // 65536
```

## Internal `lpir` Module

`src/lpir.rs` contains internal Q16.16 operations matching LPIR builtin semantics
(`fmul_q32`, `fdiv_q32`, `fsqrt_q32`, `sin_q32`, `cos_q32`, `mod_q32`). These are
`pub(crate)` without `#[no_mangle]` to avoid duplicating symbols from `lps-builtins`
(`__lp_lpir_*` / `__lps_*`), which are the `extern "C"` versions linked into JIT code.

## Testing

```bash
# Run all lps-q32 tests (210+ tests)
cargo test -p lps-q32
```

Tests cover:

- Scalar arithmetic (saturating add/sub/mul/div)
- Vector component-wise operations
- Matrix mul/inverse/determinant
- Math functions (sin, cos, sqrt, mod, floor, fract, mix, step)
- Conversion roundtrips

## Relationship to Other Crates


| Crate            | Role                                                                                                    |
| ---------------- | ------------------------------------------------------------------------------------------------------- |
| `lps-q32`        | **This crate**: reference types and helpers (no `#[no_mangle]`)                                         |
| `lps-builtins`   | `extern "C"` builtins with `#[no_mangle]` for JIT linking (`__lp_lpir_fmul_q32`, `__lps_sin_q32`, etc.) |
| `lps-shared`     | `Q32ShaderValue` marshalling, `CallError`, `GlslReturn` for runtime shader I/O                          |
| `lpvm-cranelift` | Uses `lps-q32` for constant encoding and `Q32Options` for codegen mode selection                        |
| `lp-engine`      | Uses `lps_q32::types::q32::{Q32, ToQ32}` for fixture pixel mapping                                      |


## Dependency

```toml
[dependencies]
lps-q32 = { path = "../lps-q32", default-features = false }
```

Path is relative to your crate; from another top-level crate use
`path = "lp-shader/lps-q32"`.

## License

Licensed under the same terms as the LightPlayer workspace (see workspace `LICENSE`).