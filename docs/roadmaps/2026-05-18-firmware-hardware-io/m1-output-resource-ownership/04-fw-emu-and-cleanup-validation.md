# Phase 4: fw-emu And Cleanup Validation

## Scope Of Phase

Bring `fw-emu` onto the same shared hardware ownership behavior, remove temporary M1 leftovers, and
run final validation for the milestone.

Out of scope: adding new emulator syscalls for LED bytes, GPIO input emulation, radio emulation, and
UI/API exposure for hardware enumeration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep helpers below public entry points and tests at the bottom.
- Remove temporary debug code and commented-out experiments.
- Leave only TODOs that point to a concrete later milestone.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update `lp-fw/fw-emu/src/output.rs`:

- Add a `HardwareRegistry` field with a virtual manifest matching the shared memory provider's
  single-RMT behavior.
- Store `HardwareLease` per handle.
- On `open(pin, ...)`, claim GPIO plus the virtual WS281x/RMT resource before logging success.
- On `close(handle)`, release the lease and log close as before.
- Preserve the current no-op write behavior.

Clean up M1:

- Remove stale comments that say pin support is hardcoded if the implementation now validates
  hardware resources separately.
- Ensure error display text includes resource addresses for conflict debugging.
- Ensure tests are at the bottom of Rust files and helper functions sit below the tests they serve
  only within test modules.
- Check for accidental `std` requirements in the shared hardware path.
- Do not feature-gate the compiler or alter shader compile/execute behavior.

Update docs only if implementation discovers a real limitation:

- Add a brief note to `docs/roadmaps/2026-05-18-firmware-hardware-io/decisions.md` only for new
  decisions.
- Add a future item only if dynamic ESP32 pin dispatch is deferred.

## Validate

```bash
cargo test -p lpc-shared
cargo test -p lpc-engine output
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

If any shader pipeline code was touched unexpectedly, also run:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
