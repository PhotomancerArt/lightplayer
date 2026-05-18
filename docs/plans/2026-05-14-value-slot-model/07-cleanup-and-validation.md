# Phase 7: Cleanup And Validation

## Scope Of Phase

Make the reshaped model feel intentional and leave the codebase ready for the next custom codec milestone.

In scope:

- Remove temporary TODOs and compatibility shims.
- Remove stale docs that mention `#[slot(skip)]` as normal practice.
- Update design docs to describe the new leaf model.
- Run focused validation.
- Capture remaining rough edges.

Out of scope:

- Full firmware CI.
- Full removal of serde from no-std core parts.
- Crate extraction.

## Code Organization Reminders

- Prefer small files with one concept per file.
- Keep slot leaf semantics in `lpc-model/src/slots`.
- Keep generic slot machinery in `lpc-model/src/slot`.
- Keep proc-macro implementation in `lpc-slot-macros`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update docs:

- `docs/design/slots/overview.md`
- `docs/design/slots/value.md`
- `docs/design/slots/serialization.md` if it refers to the old model
- this plan's final notes if needed

Search for stale concepts:

```bash
rg "slot\\(skip\\)|slot.leaf.|WithRevision<.*Slot|struct .*Slot" lp-core/lpc-model lp-core/lpc-slot-mockup docs/design/slots
```

Not all `struct .*Slot` hits are wrong, but semantic leaf slot containers should be gone unless they truly need custom storage behavior.

Check that examples use:

```rust
pub struct Ratio(pub f32);
pub type RatioSlot = ValueSlot<Ratio>;
```

not:

```rust
pub struct RatioSlot {
    inner: WithRevision<f32>,
}
```

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-macros
cargo test -p lpc-slot-codegen
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
```
