# Phase 04: Cleanup And Validation

## Scope Of Phase

Clean up the dynamic read implementation and run final focused validation.

In scope:

- Remove debug prints, commented-out experiments, and accidental TODOs.
- Check exports and docs for naming consistency.
- Ensure the plan notes/design still match the implementation.
- Run final validation.

Out of scope:

- Full workspace validation.
- Dynamic writing.
- Recursive validation.
- Adopting the dynamic reader in production loading paths.

## Code Organization Reminders

- Keep `dynamic_slot_reader.rs` focused on reading.
- Keep registry wrappers thin.
- Keep tests focused and search-friendly.

## Sub-Agent Reminders

- Do not commit.
- Do not broaden scope to production adoption.
- If final validation exposes a real design problem, stop and report the
  smallest fix instead of papering it over.

## Implementation Details

Search for shortcuts:

```bash
rg -n "TODO|todo|stub|unimplemented!|dbg!|println!" \
  lp-core/lpc-model/src/slot_codec \
  lp-core/lpc-model/src/slot \
  lp-core/lpc-slot-mockup/src/tests
```

Review `git diff --stat` and staged/untracked files before completion.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-codegen -p lpc-slot-mockup --check
cargo test -p lpc-model
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
git diff --check
```
