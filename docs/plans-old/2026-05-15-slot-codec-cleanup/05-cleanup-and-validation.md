# Phase 5: Cleanup And Validation

## Scope Of Phase

Remove compatibility names and run final validation.

Out of scope: broad real-domain adoption.

## Code Organization Reminders

- Prefer neutral writer names.
- Remove stale exports and unused imports.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report.

## Implementation Details

Remove `SlotJson*` aliases if no callers remain:

- `SlotJsonWriter`
- `SlotJsonValue`
- `SlotJsonWrite`
- `SlotJsonWriterError`
- `SlotJsonObject`
- `SlotJsonArray`

Update `summary.md` and archive the plan.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-codegen -p lpc-slot-mockup -p lpc-wire --check
cargo test -p lpc-model
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup dynamic_slot_codec
cargo test -p lpc-slot-mockup storage_codec
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
git diff --check
```

