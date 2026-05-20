# Phase 5: Cleanup And Final Validation

## Scope Of Phase

Clean up the radio-node milestone and run the final validation set.

In scope:

- Remove stray TODOs, debugging logs, unused helpers, and commented-out experiments.
- Ensure docs and examples match the implemented binding syntax.
- Run final host and RV32 validation.
- Write a short `summary.md` in this plan directory.

Out of scope:

- New features.
- Ack, TTL, mesh, ownership, or state sync.
- Broad refactors outside radio-node integration.

## Code Organization Reminders

- Keep tests at the bottom of source files.
- Keep radio concepts in radio-named files.
- Avoid moving unrelated node code during cleanup.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review:

- `lp-core/lpc-model/src/nodes/radio/`
- `lp-core/lpc-engine/src/nodes/radio/`
- radio service additions in engine and server paths
- example bindings
- `docs/roadmaps/2026-05-19-events-playlists-radio-nodes/m3-radio-node/future.md`

The final `summary.md` should record:

- implemented ControlRadio node shape;
- reliability policy: fixed repeated send plus receiver dedupe;
- validation commands run;
- anything deferred to future radio sync.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model radio
cargo test -p lpc-shared radio
cargo test -p lpc-engine control_radio
cargo test -p lp-cli --test examples_valid
cargo test -p lpc-engine --test runtime_spine
cargo check -p lpc-model --no-default-features
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
