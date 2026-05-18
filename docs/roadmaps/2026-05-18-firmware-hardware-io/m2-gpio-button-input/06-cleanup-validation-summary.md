# Phase 6: Cleanup, Validation, And Summary

## Scope Of Phase

In scope:

- Remove temporary scaffolding and stale provisional manifest code.
- Ensure docs reflect root-owned hardware.
- Run final validation.
- Write `summary.md` for this milestone.

Out of scope:

- M3 radio implementation.
- Dynamic arbitrary ESP32 pin dispatch.
- Hardware editing UX.

## Code Organization Reminders

- Remove dead compatibility wrappers if all callers have moved.
- Keep tests at file bottoms.
- Do not leave commented-out experiments.
- Keep TODOs only for real deferred work and make them specific.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Clean up provisional manifest paths.

   - If `lp-fw/fw-esp32/src/board/esp32c6/hardware_manifest.rs` is now just a
     stale hand-coded manifest, remove it or turn it into a thin wrapper around
     the shared default helper.
   - Update module gates accordingly.

2. Search for private registry construction in production providers.

   Run:

   ```bash
   rg -n "HardwareRegistry::new|virtual_single_rmt_gpio_board|esp32c6_devkit_hardware_manifest|with_hardware_manifest" lp-fw lp-core lp-app
   ```

   Private registry construction is still fine in tests and convenience
   constructors, but normal firmware startup should create one root registry and
   pass it into hardware services.

3. Update docs:

   - `docs/roadmaps/2026-05-18-firmware-hardware-io/notes.md` if current state
     changed materially.
   - `docs/roadmaps/2026-05-18-firmware-hardware-io/decisions.md` with the
     root-owned hardware decision if not already captured.
   - `README.md` only if user-facing hardware manifest startup behavior needs a
     short note.

4. Write
   `docs/roadmaps/2026-05-18-firmware-hardware-io/m2-gpio-button-input/summary.md`
   with:

   - What shipped
   - Important decisions
   - Validation run
   - Deferred work for M3/future

## Final Validation

Run the focused checks first:

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
```

If those pass and time allows, run the broader firmware validation from the repo
instructions:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

Do not run `cargo build --workspace` or `cargo test --workspace`.

