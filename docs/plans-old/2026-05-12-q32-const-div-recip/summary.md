# Q32 Const-Div Reciprocal Fast Path Summary

Implemented.

## What Changed

- Added `LpirOp::FdivConstF32 { dst, lhs, rhs }` so the frontend can preserve a compile-time divisor without requiring the backend to rediscover the pattern.
- Added frontend emission for scalar and vector float divides where the RHS is a literal, `const`, splat, or composed constant expression.
- Added native Q32 lowering for nonzero const divisors as:

```text
lhs * q32(1.0 / rhs)
```

- Lowered division by literal zero inline with the existing sign-based saturation behavior: `0 / 0 -> 0`, positive `/ 0 -> i32::MAX`, negative `/ 0 -> i32::MIN`.
- Added Wasm and Cranelift handling so all compile paths accept the new LPIR operation.
- Added focused `rv32n.q32` filetests for literal, named const, vector/scalar, vector/vector, zero saturation, and dynamic fallback cases.

## Perf Results

Steady-render profile command:

```sh
cargo run -p lp-cli -- profile examples/basic --mode steady-render --note q32-const-div-fast
```

Result path:

```text
profiles/2026-05-12T12-28-29--examples-basic--steady-render--q32-const-div-fast/report.txt
```

Compared with the supplied baseline:

- Baseline total attributed cycles: `4,896,607`
- New total attributed cycles: `3,668,062`
- Baseline `__lp_lpir_fdiv_recip_q32` self cycles: `694,080`
- New `__lp_lpir_fdiv_recip_q32` self cycles: `173,520`

Focused filetests:

```sh
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-const.glsl \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/op-divide.glsl
```

Result:

- `19/19` tests passed
- `scalar/float/op-divide.glsl`: `207` estimated ESP32-C6 cycles
- `scalar/float/q32fast-div-const.glsl`: `260` estimated ESP32-C6 cycles
- `scalar/float/q32fast-div-recip.glsl`: `75` estimated ESP32-C6 cycles

## Validation

- `cargo fmt --all`
- `cargo test -p lpir`
- `cargo test -p lps-frontend`
- `cargo test -p lpvm-native`
- `cargo test -p lpvm-wasm`
- `cargo test -p lpvm-cranelift --no-default-features`
- `scripts/filetests.sh --target rv32n.q32 --detail scalar/float/q32fast-div-const.glsl scalar/float/q32fast-div-recip.glsl scalar/float/op-divide.glsl`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
- `cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu`
- `cargo check -p lpa-server`
- `cargo test -p lpa-server --no-run`

Notes:

- `fw-emu` still reports the existing `host_debug` unexpected-cfg warning.
- The two `fw-tests` tests are ignored by their current test definitions.

## Follow-Up

- Measure `rocaille` specifically to confirm the same removal of helper-call pressure in trig-heavy shader code.
- Decide whether dynamic divide should also gain a compiler-side reciprocal precompute when the divisor is loop-invariant or otherwise cheaply hoistable.
- Keep Cranelift Q32 `FdivConstF32` conservative unless that path becomes production-hot again; native is the target JIT path.
