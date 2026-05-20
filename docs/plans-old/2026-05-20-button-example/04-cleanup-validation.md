# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up temporary scaffolding, tighten documentation, and run final validation for the button
example slice.

In scope:

- Remove temporary test-only helpers or mark intentionally test-only helpers clearly.
- Ensure docs and examples consistently use D9/GPIO20.
- Ensure no unrelated roadmap or AGENTS instructions were violated.
- Run focused host, emu, and ESP32 checks.

Out of scope:

- Implementing radio send/receive nodes.
- Implementing playlist / trigger-to-visual switching.
- Expanding ESP32 GPIO dispatch beyond D9.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep test modules at the bottom of files.
- Avoid leftover TODOs unless they point to a deliberate future item and are unavoidable.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Review touched Rust files for:

   - stale imports;
   - old D0/GPIO0 references;
   - accidental GPIO4-only assumptions in the new normal button path;
   - copied tests with misleading names;
   - temporary debug logging.

2. Update docs where useful:

   - Mention `examples/button` in a suitable examples list if one exists nearby.
   - Update `docs/use-cases/2025-05-08-fyeah-sign.md` only if the implementation has enough
     concrete button assumptions to document D9/D10 wiring.

3. Confirm validation command set.

   At minimum, run:

   ```bash
   cargo fmt --check
   cargo test -p lpc-model button
   cargo test -p lpc-engine button
   cargo test -p lpc-engine button_example
   cargo test -p lp-cli --test examples_valid
   cargo check -p lpc-model --no-default-features
   cargo check -p lpa-server
   cargo test -p lpa-server --no-run
   cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
   cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
   ```

4. If the implementation touches shader pipeline or output graph behavior more broadly than
   expected, also run:

   ```bash
   cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
   ```

   These tests may remain ignored for existing canonical-project-sync markers; record that if so.

5. Write a short `summary.md` in the plan directory after implementation:

   - What changed.
   - Decisions made.
   - Validation run and results.
   - Known remaining gaps.

## Validate

Run the final validation commands listed above and fix issues before considering the slice complete.

