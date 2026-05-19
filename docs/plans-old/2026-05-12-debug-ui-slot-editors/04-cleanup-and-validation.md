# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up the implementation and run focused validation.

In scope:

- remove temporary debug prints or commented experiments;
- tighten names and small helper boundaries;
- run final validation commands;
- fix warnings and formatting issues.

Out of scope:

- new editor kinds;
- broader debug UI redesign;
- final app UI work.

## Code Organization Reminders

- Keep files concept-oriented.
- Do not move large UI modules unless the implementation made the current
  layout genuinely confusing.
- Tests belong at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Review:

- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-cli/src/debug_ui/slot_render.rs`
- `lp-cli/src/debug_ui/slot_edit.rs`

Check specifically:

- no clock-only mutation special cases in UI renderer;
- no panic path for editor/type mismatch;
- queued mutations are drained exactly once;
- in-flight polls do not drop edits;
- pending/error affordances are small and stable.

## Validate

```bash
cargo fmt --check
cargo check -p lp-cli
cargo check -p lpa-server
cargo test -p lpc-view
cargo test -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
