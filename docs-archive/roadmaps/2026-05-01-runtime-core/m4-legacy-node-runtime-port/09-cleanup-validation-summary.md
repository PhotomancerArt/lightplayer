# Phase 9: Cleanup, Validation, And Summary

## Scope of Phase

Clean up the M4 implementation, run final validation, and write the milestone
summary. This phase should not add new features.

In scope:

- Run formatting.
- Search the M4 diff for temporary code, debug prints, stubs, TODOs, disabled
  tests, and warning suppressions.
- Ensure any necessary temporary/hacky decisions are recorded in `future.md`.
- Run final validation commands.
- Write `summary.md`.

Out of scope:

- New runtime feature work.
- M4.1 buffer sync.
- M5 legacy runtime removal.
- Manual desktop/device validation; the user runs those after automated tests.

## Code Organization Reminders

- Prefer fixing warnings over suppressing them.
- Keep cleanup edits small.
- Keep `summary.md` terse and grep-friendly.
- Do not rewrite plan docs except to correct factual drift.

## Sub-agent Reminders

- Do not commit.
- Stay within cleanup scope.
- Do not suppress warnings or weaken tests.
- If validation fails with a non-trivial runtime bug, stop and report instead of
  debugging deeply.
- Report changed files, validation results, cleanup findings, and deviations.

## Implementation Details

Plan directory:

- `docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/`

Cleanup checks:

- Search the diff for:
  - `TODO`;
  - `todo!`;
  - `unimplemented!`;
  - `dbg!`;
  - `println!`;
  - `#[ignore]`;
  - new `#[allow(...)]`;
  - commented-out code.
- Existing TODOs outside touched lines are not automatically in scope.
- Any intentional temporary shortcut should be documented in `future.md` with:

```markdown
## <short title>

- **Idea:** <cleanup/follow-up needed>
- **Why not now:** <why M4 accepted it>
- **Useful context:** <files/symbols>
```

Write `summary.md` with:

```markdown
### What was built

- ...

### Decisions for future reference

#### <short title>

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Likely decisions to capture if they match the implementation:

- M4 cut over demo/server to `CoreProjectRuntime`; M5 is cleanup/removal.
- Shader/pattern output is render product, not runtime buffer.
- Fixtures are demand roots; outputs are pushed sinks.
- Compatibility snapshots remain until M4.1.

## Validate

Run:

```bash
cargo +nightly fmt
cargo test -p lpc-engine --test scene_render --test scene_update --test partial_state_updates
cargo test -p lpc-engine
cargo test -p lpa-server
```

If these pass and the touched area affects firmware/shader pipeline behavior,
also run or report the need to run:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
