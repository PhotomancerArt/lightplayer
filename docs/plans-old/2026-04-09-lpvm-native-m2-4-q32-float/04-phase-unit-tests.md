# Phase 4: Unit tests and float filetest smoke

## Scope of phase

Finish test coverage for M2.4: assert each new lowering shape, assert F32 mode errors use the agreed message, and run a representative float filetest slice on `rv32lp.q32` (existing tests already exercise Q32).

## Code Organization Reminders

- Keep tests in `lower.rs` `mod tests` at the top of the module per project rules; helpers at the bottom of the test module.
- Prefer short tests; reuse `empty_func()` / `empty_ir_abi()` patterns already in `lower.rs`.

## Implementation Details

1. **Lowering tests** (if not already added in Phases 1–2):
   - `lower_q32_fdiv_to_call`: `Call` to `__lp_lpir_fdiv_q32`.
   - Six comparison tests (or one table-driven test): each `F*` op in Q32 → expected `IcmpCond`.

2. **F32 rejection test** (if not added in Phase 3):
   - `lower_f32_float_unsupported` (extend): call `lower_op` with `FloatMode::F32` for `Fadd` or `Fdiv` and assert `Err` message contains `Q32` (or equals the chosen string from Phase 3).

3. **Optional filetest smoke** (no new `.glsl` required per plan):
   - Run a small subset of existing float tests, e.g.  
     `cargo run -p lps-filetests-app -- test --target rv32lp.q32 scalar/float/op-divide.glsl`  
     (adjust path to match repo layout under `filetests/`).

## Validate

```bash
cargo test -p lpvm-native --lib
cargo +nightly fmt -p lpvm-native
# optional:
# cargo run -p lps-filetests-app -- test --target rv32lp.q32 scalar/float/op-divide.glsl
```

## Cleanup

None in this phase beyond test code.
