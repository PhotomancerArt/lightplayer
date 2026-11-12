# Phase 07: Cleanup And Validation

## Scope Of Phase

Remove obsolete registration paths, document the new shape catalog model, and capture
before/after memory data.

## Code Organization Reminders

- Remove temporary debug logging and obsolete TODOs.
- Remove compatibility helpers rather than keeping host/device split paths.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Main-agent phase preferred.
- Do not commit unless explicitly asked.
- Do not suppress warnings or weaken tests.
- Report changed files, validation, and deviations.

## Implementation Details

Tasks:

- Search for static shape registration still happening in engine startup.
- Search for broad `SlotShapeRegistry::snapshot()` use that accidentally
  serializes only dynamic shapes without catalog metadata.
- Add/update docs explaining static catalog export behavior.
- Run memory/profile validation once M1 load-only mode is available.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model -p lpc-wire -p lpc-slot-mockup
cargo test -p lpa-server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
git diff --check
```

## Latest Validation

Passed after final cleanup:

- `cargo fmt --check`
- `git diff --check`
- `cargo check -p lpc-model -p lpc-wire -p lpc-engine -p lpa-server -p lp-cli`
- `cargo test -p lpc-model -p lpc-wire -p lpc-slot-mockup`
- `cargo test -p lpa-server`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
