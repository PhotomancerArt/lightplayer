# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up the writer implementation and validate the targeted crates.

In scope:

- remove temporary code and debug output
- remove unused imports created during writer work
- ensure public exports are coherent
- update the writer plan summary
- run final validation commands

Out of scope:

- deleting old codec systems
- moving engine/wire callers
- committing

## Code Organization Reminders

- Prefer clear names over compatibility aliases for new APIs.
- Do not remove old aliases in this phase unless all current callers have moved.
- Tests belong at the bottom of files.
- Keep helper functions low in each file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review these files:

- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-slot-mockup/src/tests/dynamic_slot_codec.rs`

Check for:

- TODOs that should not remain
- panic paths in non-test code
- unclear error messages
- duplicated JSON/TOML shape-walk logic that can be factored without becoming
  abstract soup
- accidentally expanded generated-code surfaces
- use of old `SlotCodec` where new writer tests should use registry APIs

Update:

- `docs/plans/2026-05-15-slot-codec-writers/summary.md`

The summary should include:

- APIs added
- tests added
- old writer surfaces that are now ready for removal
- any remaining blocker for deleting old codec code

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-mockup --check
cargo test -p lpc-model dynamic_slot_writer
cargo test -p lpc-model slot_value_codec
cargo test -p lpc-slot-mockup dynamic_slot_codec
cargo check -p lpc-model --no-default-features
git diff --check
```
