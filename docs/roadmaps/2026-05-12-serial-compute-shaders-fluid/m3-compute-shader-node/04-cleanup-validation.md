# Phase 4: Cleanup And Validation

## Scope

- Remove debug prints and unused helpers.
- Update module docs and roadmap summary.
- Run focused formatting and checks.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-model`
- `cargo test -p lpc-engine compute -- --nocapture`
- `cargo check -p lpc-engine`
- If compile boundary changed enough, run:
  `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
