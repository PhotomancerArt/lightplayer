# Phase 6: Cleanup And Validation

## Scope Of Phase

Make the new design crisp enough to review before moving on to real domain adoption.

In scope:

- remove temporary aliases if they are no longer needed
- remove stale generated-code helpers and tests
- update docs if names changed from this plan
- run focused validation
- record remaining smells

Out of scope:

- adopting SlotCodec in real disk/wire code paths
- removing serde from no_std core crates
- splitting slot into a separate crate

## Code Organization Reminders

- Keep `slot_codec` files concept-oriented.
- Avoid large `mod.rs` bodies.
- Tests belong at the bottom of files.
- Keep generated code compact by relying on helper traits.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Check for rough edges:

- any generated per-field parse/write helper that should now be `SlotCodec`
- any mockup-only constructor created only for codec use
- any in-memory tree or buffering path introduced during implementation
- any writer type that is still JSON-named in trait signatures
- any policy table that duplicates record fields
- any errors that lost path/span context

Add or update a short summary in this plan directory after implementation:

```text
summary.md
```

Include:

- what changed
- what validation passed
- any remaining design compromises
- next recommended milestone

## Validate

```bash
cargo fmt -p lpc-model -p lpc-wire -p lpc-slot-codegen -p lpc-slot-mockup
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
```
