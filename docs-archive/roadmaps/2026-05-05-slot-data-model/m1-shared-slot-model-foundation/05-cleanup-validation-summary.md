# Cleanup, Validation, And Summary

## Scope Of Phase

Finish M1 by cleaning docs/exports, running validation, and archiving the plan
results.

In scope:

- Update `lpc-model` README/rustdocs if stale.
- Ensure new slot modules are exported consistently.
- Remove temporary TODOs, debug prints, and commented-out experiments.
- Run final validation.
- Write `summary.md` in this plan directory.
- Commit if validation passes and the user asked implementation to proceed.

Out of scope:

- Starting Milestone 2.
- Applying slot data to real nodes.
- Renaming `ModelValue`.
- Artifact mutation.

## Code Organization Reminders

- Tests stay at the bottom of Rust files.
- Keep public rustdocs semantic and durable.
- Do not stage unrelated user changes.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Searches to run:

```bash
rg "TODO|dbg!|println!|eprintln!" lp-core/lpc-model/src
rg "slot: SlotName|SlotRef \\{|ModelValue::Resource|ModelType::Resource" lp-core/lpc-model/src
```

Expected cleanup:

- `lp-core/lpc-model/src/slot/mod.rs` exports every new public type.
- `lp-core/lpc-model/src/lib.rs` re-exports key public types.
- `ModelValue::Resource` has tests.
- Slot registry/data/tree invariants have tests.
- `summary.md` records decisions, implemented types, and validation commands.

Final validation:

```bash
cargo fmt
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-view
```

If model changes create additional compile fallout, fix it in the smallest
affected crate and rerun the relevant command.

## Validate

```bash
cargo fmt
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-view
```
