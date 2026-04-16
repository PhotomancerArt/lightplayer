# Import Modules

## Mechanism

External functions are declared with module-qualified names:

```
import @std.math::fsin(f32) -> f32
import @lpfx::noise3(ptr, i32, i32, i32) -> (i32, i32, i32)
```

Import `param_types` and return types use the same `f32` / `i32` / `ptr` spellings as functions. Address or buffer arguments (LPFX out-pointers, result scratch) use `ptr`. WebAssembly emission maps `ptr` to linear-memory `i32`; RV32 uses 32-bit pointers.

Call sites use the same qualified name (user operands only in text; VM context is implicit when required):

```
v1:f32 = call @std.math::fsin(v1)
```

The `::` separator is structural syntax, not a user-defined operator name. The module prefix selects which emitter **provider** supplies the implementation.

The emitter holds a map from module name to provider. If the IR references a module with no configured provider, emission fails with an error. Signature mismatch between declaration and use also fails with an error.

This design replaces a closed `MathFunc`-style enum. Which functions exist in a given module is documented as reference material; new modules or entry points can be added without changing the LPIR opcode set or core IR rules.

## Well-known module: `std.math`

The `std.math` module exposes scalar math operations aligned with GLSL 4.50 core builtins. Signatures below use LPIR types `f32` and `i32` (no `ptr` in typical `std.math` entries).

### Float math (unary)

Each row: LPIR name, signature, GLSL builtin, short semantics.

| LPIR name | Signature | GLSL | Description |
|-----------|-----------|------|-------------|
| `fabs` | `(f32) -> f32` | `abs` | Absolute value. |
| `fsign` | `(f32) -> f32` | `sign` | −1, 0, or +1 by sign of operand; exact zero handling per GLSL. |
| `fround` | `(f32) -> f32` | `round` | Round to nearest integer, ties away from zero. |
| `froundeven` | `(f32) -> f32` | `roundEven` | Round to nearest integer, ties to even. |
| `ffloor` | `(f32) -> f32` | `floor` | Largest integer not greater than operand. |
| `fceil` | `(f32) -> f32` | `ceil` | Smallest integer not less than operand. |
| `ftrunc` | `(f32) -> f32` | `trunc` | Round toward zero to integral value. |
| `ffract` | `(f32) -> f32` | `fract` | `x - floor(x)`. |
| `fsqrt` | `(f32) -> f32` | `sqrt` | Non-negative square root; domain errors per GLSL. |
| `finversesqrt` | `(f32) -> f32` | `inversesqrt` | `1/sqrt(x)`; domain errors per GLSL. |
| `fsin` | `(f32) -> f32` | `sin` | Sine (radians). |
| `fcos` | `(f32) -> f32` | `cos` | Cosine (radians). |
| `ftan` | `(f32) -> f32` | `tan` | Tangent (radians). |
| `fasin` | `(f32) -> f32` | `asin` | Arc sine; range and NaN per GLSL. |
| `facos` | `(f32) -> f32` | `acos` | Arc cosine; range and NaN per GLSL. |
| `fatan` | `(f32) -> f32` | `atan` | Arc tangent; principal value. |
| `fsinh` | `(f32) -> f32` | `sinh` | Hyperbolic sine. |
| `fcosh` | `(f32) -> f32` | `cosh` | Hyperbolic cosine. |
| `ftanh` | `(f32) -> f32` | `tanh` | Hyperbolic tangent. |
| `fasinh` | `(f32) -> f32` | `asinh` | Inverse hyperbolic sine. |
| `facosh` | `(f32) -> f32` | `acosh` | Inverse hyperbolic cosine; domain per GLSL. |
| `fatanh` | `(f32) -> f32` | `atanh` | Inverse hyperbolic tangent; domain per GLSL. |
| `fexp` | `(f32) -> f32` | `exp` | e^x. |
| `fexp2` | `(f32) -> f32` | `exp2` | 2^x. |
| `flog` | `(f32) -> f32` | `log` | Natural logarithm; domain per GLSL. |
| `flog2` | `(f32) -> f32` | `log2` | Base-2 logarithm; domain per GLSL. |

### Float math (binary)

| LPIR name | Signature | GLSL | Description |
|-----------|-----------|------|-------------|
| `fmin` | `(f32, f32) -> f32` | `min` | Minimum of two scalars. |
| `fmax` | `(f32, f32) -> f32` | `max` | Maximum of two scalars. |
| `fmod` | `(f32, f32) -> f32` | `mod` | `x - y * floor(x / y)` (GLSL `mod` semantics). |
| `fpow` | `(f32, f32) -> f32` | `pow` | `x^y`; domain and edge cases per GLSL. |
| `fatan2` | `(f32, f32) -> f32` | `atan` (2-arg) | Arc tangent with `(y, x)` parameter order as in GLSL. |
| `fstep` | `(f32, f32) -> f32` | `step` | 0.0 if `x < edge`, else 1.0 (first arg edge, second `x` per GLSL `step`). |
| `fldexp` | `(f32, i32) -> f32` | `ldexp` | `x * 2^exp` using integer exponent. |

GLSL `atan` is overloaded; the binary form corresponds to `fatan2`.

### Float math (ternary)

| LPIR name | Signature | GLSL | Description |
|-----------|-----------|------|-------------|
| `fmix` | `(f32, f32, f32) -> f32` | `mix` | Linear blend: `x * (1 - a) + y * a`. |
| `fclamp` | `(f32, f32, f32) -> f32` | `clamp` | Clamp `x` to `[minVal, maxVal]` (GLSL ordering). |
| `fsmoothstep` | `(f32, f32, f32) -> f32` | `smoothstep` | Hermite edge smoothstep between `edge0` and `edge1`. |
| `ffma` | `(f32, f32, f32) -> f32` | `fma` | Fused multiply-add `a * b + c` per GLSL. |

### Integer math

| LPIR name | Signature | GLSL | Description |
|-----------|-----------|------|-------------|
| `iabs_s` | `(i32) -> i32` | `abs` (int) | Absolute value of signed 32-bit value; `INT_MIN` behavior per GLSL. |
| `imin_s` | `(i32, i32) -> i32` | `min` (int) | Signed minimum. |
| `imax_s` | `(i32, i32) -> i32` | `max` (int) | Signed maximum. |
| `imin_u` | `(i32, i32) -> i32` | `min` (uint) | Unsigned minimum on 32-bit bit pattern. |
| `imax_u` | `(i32, i32) -> i32` | `max` (uint) | Unsigned maximum on 32-bit bit pattern. |
| `iclamp_s` | `(i32, i32, i32) -> i32` | `clamp` (int) | Signed clamp. |
| `iclamp_u` | `(i32, i32, i32) -> i32` | `clamp` (uint) | Unsigned clamp on bit pattern. |

### Semantic precision (`std.math`)

- Transcendentals and library-style operations (`fsin`, `fcos`, `fpow`, `flog`, and others in the tables above) are **relaxed**: exact bits are not specified across backends or vs. a particular libm. WASM (browser) and native code may differ slightly. Conformance tests should use tolerances where appropriate.
- Core IEEE-style arithmetic expressed as dedicated LPIR ops (`fadd`, `fsub`, `fmul`, `fdiv`, etc.) is not lumped into this relaxation; those ops follow normal single-precision rules for the active numeric mode.
- `fmin` / `fmax` NaN propagation: IEEE 754-2019 minNum/maxNum behavior where the target provides it; otherwise implementation-defined. Version 1 does not pin identical NaN behavior across all backends.
- There is no strict-math or fast-math IR mode in version 1.

## Well-known module: `lp.q32`

The `lp.q32` module supplies Q32 fixed-point counterparts to core arithmetic and to the operations cataloged for `std.math`, as implemented by the Q32 provider. The emitter exposes this module only in **Q32 mode**. Using `lp.q32` imports in float-only emission is an error unless the project defines otherwise.

Exact per-function names and signatures are defined by the Q32 provider and reference documentation; they mirror the scalar `std.math` surface where a fixed-point analogue exists.

## Well-known module: `lpfx`

`lpfx` is the module for LPFX (Lygia-family) builtins. It is available only when the emitter is configured with an LPFX provider.

Functions take `i32` parameters representing Q32-encoded values where that is the LPFX ABI. Some operations use **out-parameter** or slot-based conventions (multiple results via memory or caller-allocated slots); those details are specified in the LPFX provider and ABI documentation, not duplicated here.
