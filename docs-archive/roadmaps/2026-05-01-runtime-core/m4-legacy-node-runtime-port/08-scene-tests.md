# Phase 8: Port And Repair Scene Render/Update Tests

## Scope of Phase

Update automated tests so the simple scene render/update coverage validates the
new core runtime path. These tests are the basic automated proof that M4 works.

In scope:

- Update `lp-core/lpc-engine/tests/scene_render.rs`.
- Update `lp-core/lpc-engine/tests/scene_update.rs` as needed.
- Update `partial_state_updates.rs` only if compatibility projection changed.
- Add focused tests for `CoreProjectRuntime` if integration tests need smaller
  coverage.
- Ensure existing simple render tests pass on the new path.

Out of scope:

- Manual desktop/device demo validation; the user will run that after automated
  tests pass.
- Full M4.1 buffer sync tests.
- Deleting old runtime tests unless they are now actively misleading and a
  replacement exists.

## Code Organization Reminders

- Keep tests readable and short.
- Prefer builders/helpers over repeated setup.
- Do not weaken tests to hide bugs; fix the implementation or report blockers.
- Record temporary compatibility assumptions in `future.md`.

## Sub-agent Reminders

- Do not commit.
- Stay within phase scope.
- Do not suppress warnings or weaken tests.
- If a test fails due to a real runtime behavior bug, fix the bug if local and
  obvious; otherwise stop and report.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `lp-core/lpc-engine/tests/scene_render.rs`.
- `lp-core/lpc-engine/tests/scene_update.rs`.
- `lp-core/lpc-engine/tests/partial_state_updates.rs`.
- `lp-core/lpc-shared/src/project/builder.rs`.
- `lp-core/lpc-engine/src/project_runtime/`.

Expected outcome:

- Simple render tests pass through `CoreProjectRuntime`.
- Tests still validate real shader compile/execute where they did before.
- Scene update tests either pass on the new path or clearly document any
  compatibility shortcut in `future.md`.

## Validate

Run:

```bash
cargo test -p lpc-engine --test scene_render --test scene_update --test partial_state_updates
```
