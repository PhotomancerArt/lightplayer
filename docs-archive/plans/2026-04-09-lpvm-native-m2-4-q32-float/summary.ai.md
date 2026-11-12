# Summary: M2.4 Q32 float (lpvm-native)

## Completed

- **`lower_op` (`lower.rs`)**: `Op::Fdiv` in Q32 lowers to `VInst::Call` → `__lp_lpir_fdiv_q32`.
- **`lower_op`**: `Feq`, `Fne`, `Flt`, `Fle`, `Fgt`, `Fge` in Q32 lower to `VInst::Icmp32` with signed conditions matching `lpvm-cranelift` Q32 behavior (`Eq`, `Ne`, `LtS`, `LeS`, `GtS`, `GeS`).
- **F32 mode**: Single `LowerError` for `Fadd`/`Fsub`/`Fmul`/`Fdiv`/`FconstF32` and all six float compares: `float op requires Q32 mode (F32 not supported on rv32)`.
- **Tests**: `lower_q32_fdiv_to_call`, `lower_q32_float_comparisons_to_signed_icmp`, extended `lower_f32_float_unsupported`; helper `assert_q32_fcmp` at bottom of test module.

## Files touched

- `lp-shader/lpvm-native/src/lower.rs`

## Validation run

- `cargo test -p lpvm-native --lib`
- `cargo +nightly fmt -p lpvm-native`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
- `cargo run -p lps-filetests-app -- test --target rv32lp.q32 scalar/float/op-divide.glsl scalar/float/op-equal.glsl scalar/float/op-less-than.glsl`

## Not in scope (unchanged)

- Other LPIR float ops (`Fneg`, `Fsqrt`, …) still hit the generic `other` lowering error until a later milestone.
- No new filetests; existing `scalar/float/*` coverage used for smoke.
