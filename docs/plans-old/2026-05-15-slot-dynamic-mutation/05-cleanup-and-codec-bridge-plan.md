# Phase 5: Cleanup And Codec Bridge Notes

## Scope Of Phase

Clean up the mutation implementation and document how it should feed the next codec simplification step.

In scope:

- Remove temporary helpers and dead hard-coded mutation paths.
- Tighten names and module exports.
- Add a short note to the existing slot serialization docs about default-and-mutate deserialization.
- Run final validation.

Out of scope:

- Replacing generated `SlotCodec` record bodies.
- Implementing enum variant switching.
- Implementing map insertion/removal.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep mutation code in `lpc-model/src/slot`, not in mockup-specific modules.
- Do not leave TODOs unless they point to a concrete future item.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant docs:

- `docs/design/slots/serialization.md`
- `docs/design/slots/overview.md`
- `docs/plans/2026-05-15-slot-dynamic-mutation/00-design.md`

Write a concise note explaining:

- Generated mutable slot access is the Rust reflection bridge.
- Generic mutation is the shared primitive for runtime edits and default-and-mutate deserialization.
- The next codec step should remove generated per-record read bodies and replace them with a generic object-to-mutation reader.

Final validation commands:

```bash
cargo fmt -p lpc-model -p lpc-slot-macros -p lpc-slot-mockup
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-codegen
```

Do not run `cargo test --workspace`.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-macros -p lpc-slot-mockup
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-codegen
```
