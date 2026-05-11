# Phase 6: Cleanup, Validation, And Summary

## Scope Of Phase

Clean up the plan implementation, run final validation, write the summary, archive the standalone plan, and commit.

In scope:

- Remove obsolete mockup helpers and imports.
- Search for stray TODOs, commented-out experiments, debug prints, and warning suppressions introduced by this plan.
- Ensure docs/rustdocs explain:
  - shape builder helpers
  - `SlotRecordShape`
  - derive macro scope and explicit annotations
  - what remains manual and why
- Run final validation commands.
- Write `summary.md`.
- Move the completed standalone plan directory to `docs/plans-old/`.
- Commit the implementation.

Out of scope:

- New functionality beyond cleanup.
- Real `lpc-source` / `lpc-engine` conversion.
- Push/PR/CI watching unless explicitly requested.

## Code Organization Reminders

- Keep helpers in their final modules.
- Keep tests at file bottoms.
- Do not leave temporary debug artifacts.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Final validation commands:

```bash
cargo test -p lpc-model
cargo test -p lpc-model --features derive
cargo check -p lpc-model --no-default-features
cargo check -p lpc-model --features schema-gen,derive
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo check -p lpc-view
cargo check -p lpc-wire --features schema-gen
git diff --check
```

Summary file should include:

- What was built.
- Decisions for future reference.
- Remaining work before real `lpc-source` / `lpc-engine` conversion.

Commit message:

```text
Add slot record derive support
```

Commit body should include:

- Shape builder promotion.
- `SlotRecordShape` and derive macro.
- Mockup conversion.
- Plan path.
