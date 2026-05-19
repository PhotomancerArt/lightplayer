# Phase 5: Cleanup And Final Validation

## Scope Of Phase

In scope:

- Clean up temporary code and stale comments from earlier phases.
- Ensure docs and roadmap notes reflect the final implementation.
- Run final targeted validation.
- Prepare a short implementation summary.

Out of scope:

- Starting the radio node plan.
- Broad CI or workspace-wide host commands that are disallowed for this repo.

## Code Organization Reminders

- Remove temporary debugging artifacts.
- Remove commented-out experiments.
- Do not leave vague TODOs. Keep only concrete TODOs tied to future node/event work.
- Keep tests at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- All files touched in phases 1-4.
- `docs/roadmaps/2026-05-18-firmware-hardware-io/m3-basic-radio-messages/summary.md`
- `docs/roadmaps/2026-05-18-firmware-hardware-io/decisions.md` if implementation changes a documented decision.

Expected changes:

- Add `summary.md` with:
  - implemented files and concepts,
  - validation commands and results,
  - known remaining gaps,
  - follow-up for radio nodes.
- Check that default firmware still includes the on-device GLSL JIT compiler.
- Check that no radio work introduced host-only `std` requirements into shared/firmware paths.
- Confirm `test_espnow` no longer duplicates packet codec logic.

## Final Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware
cargo test -p lpc-shared output
cargo check -p lpc-shared --no-default-features
cargo test -p lpc-engine engine_services
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_espnow
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```
