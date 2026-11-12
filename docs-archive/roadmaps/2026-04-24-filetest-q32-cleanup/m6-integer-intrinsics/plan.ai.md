# M6 Integer intrinsics — plan & status

Product validation targets: `wasm.q32`, `rv32c.q32`, `rv32n.q32` (jit deprecated).

## Checklist

| Item | Status | Notes |
|------|--------|--------|
| `common-intbitstofloat.glsl` (finite / non-Inf rows) | **Blocked** | Observed failure is **not** 16-bit literal parse: large decimals lower correctly. On **Q32**, `intBitsToFloat` must map IEEE `i32` bit patterns → **Q16.16 float lanes** (same as `q32_encode(f32::from_bits(...))`). Today `FfromI32Bits` in Q32 backends is a **raw mov**, so `0x3f800000` is interpreted as fixed-point and surfaces as **16256.0**, not `1.0`. Fix needs **IEEE decode + `q32_encode`** (new `@lpir` builtin or inline sequence), not `Expression::As`→`ItofS` alone. |
| `integer-bitfieldextract.glsl` / `integer-bitfieldinsert.glsl` | Blocked | `Math::ExtractBits` / insert not lowered; compile-fail. |
| `integer-imulextended.glsl` / `integer-umulextended.glsl` | Blocked | Wide multiply; compile-fail. |
| `integer-uaddcarry.glsl` / `integer-usubborrow.glsl` | Blocked | Carry/borrow; compile-fail. |
| `integer-findmsb.glsl` edge rows | Blocked | Naga const-eval `FirstLeadingBit` ≠ GLSL `findMSB` for negatives / `2147483648`; `@broken` remains. |
| `integer-bitcount.glsl` | **Green** | Literals fold via naga; no runtime `CountOneBits` lowering needed for current tests. |
| `common-roundeven.glsl` | **Out of scope** | M2; do not touch. |

## Commands

```bash
./scripts/glsl-filetests.sh -t wasm.q32,rv32c.q32,rv32n.q32 --concise \
  builtins/integer-bitfieldextract.glsl \
  builtins/integer-bitfieldinsert.glsl \
  builtins/integer-imulextended.glsl \
  builtins/integer-umulextended.glsl \
  builtins/integer-uaddcarry.glsl \
  builtins/integer-usubborrow.glsl \
  builtins/integer-findmsb.glsl \
  builtins/integer-bitcount.glsl \
  builtins/common-intbitstofloat.glsl
```

**Outcome (2026-04-24 run):** all files **pass** as baseline (expected failures only); no stale `@broken` removed for `intbitstofloat` until Q32 `FfromI32Bits` semantics are fixed.

## Blockers (implement next)

1. **Q32 `FfromI32Bits`**: implement as `q32_encode(f32::from_bits(u32::from_ne_bytes(...)))` (rounding + saturation per `lps_q32::q32_encode`), via a small **`@lpir`** import used from wasm/native/cranelift Q32 lowering, **or** widen `lps-builtins` + `lps-builtin-ids` with codegen regen.
2. **Bitfields / wide mul / carry / borrow**: frontend `MathFunction` lowering + RV32-safe sequences.
3. **findMSB**: align naga folding with GLSL or lower `FirstLeadingBit` at runtime with correct spec.

## Recommended next subtask

Add **`__lp_lpir_ffrom_ieee_i32_bits_q32`** (or equivalent) wired into Q32 `FfromI32Bits` lowering, then drop `@broken` on finite `common-intbitstofloat.glsl` rows for `wasm` / `rv32c` / `rv32n`.
