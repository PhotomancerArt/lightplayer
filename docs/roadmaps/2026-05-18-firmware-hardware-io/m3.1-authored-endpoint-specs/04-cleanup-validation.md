# Phase 4: Cleanup And Final Validation

## Scope Of Phase

In scope:

- Remove stale numeric output authoring language.
- Update roadmap decisions and summary.
- Search for leftover output `pin` compatibility.
- Run final targeted validation.

Out of scope:

- Button/radio node implementation.
- Broad workspace commands disallowed by repo instructions.

## Code Organization Reminders

- Remove temporary debug prints.
- Remove commented-out experiments.
- Do not leave vague TODOs.
- Tests at file bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- All files touched in phases 1-3.
- `docs/roadmaps/2026-05-18-firmware-hardware-io/decisions.md`
- `docs/roadmaps/2026-05-18-firmware-hardware-io/m3.1-authored-endpoint-specs/summary.md`

Expected changes:

- Add `summary.md`.
- Record the decision that authored hardware endpoints are exact
  `cap:driver:config` strings.
- Confirm old `pin = ...` output examples are gone or intentionally part of
  negative tests.

## Final Validate

```bash
cargo fmt --check
cargo test -p lpc-model output
cargo test -p lpc-model hardware_endpoint_spec
cargo test -p lpc-shared hardware
cargo test -p lpc-shared output
cargo check -p lpc-shared --no-default-features
cargo test -p lpc-engine engine_services
cargo test -p lpc-engine output_flush
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

