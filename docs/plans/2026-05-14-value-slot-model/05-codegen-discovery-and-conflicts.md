# Phase 5: Codegen Discovery And Id Conflicts

## Scope Of Phase

Make auto-generated slot value ids enforceable.

In scope:

- Extend build-time discovery to notice `#[derive(SlotValue)]`.
- Detect duplicate default ids.
- Fail loudly with a useful error when two discovered shapes claim the same id.
- Keep the id rule simple: Rust type name only.

Out of scope:

- Workspace-global schema versioning.
- Stable explicit id migration.
- Fancy namespacing.

## Code Organization Reminders

- Keep discovery logic in `lp-core/lpc-slot-codegen/src/lib.rs` unless it is already splitting naturally.
- Error messages should name both conflicting types and files if possible.
- Avoid hand-maintained lists.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- generated view/catalog tests

Current discovery looks for `SlotRecord`. Add similar discovery for `SlotValue`.

The immediate purpose is conflict detection, not necessarily generating runtime views for values.

Required behavior:

- `#[derive(SlotValue)] pub struct Ratio(...)` claims id `Ratio`.
- Another `#[derive(SlotValue)] pub struct Ratio(...)` in the same discovered source set is an error.
- A `SlotRecord` and a `SlotValue` with the same id should also be an error if they share the same `SlotShapeId` namespace.

If cross-crate checking is too much in one pass, implement per-crate checking first and document the remaining gap in the final phase.

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
```
