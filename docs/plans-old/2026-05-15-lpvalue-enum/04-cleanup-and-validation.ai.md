# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up the enum value implementation and run focused validation.

In scope:

- remove temporary TODOs or debug helpers
- ensure docs and plan notes match implementation decisions
- format code
- run final validation commands

Out of scope:

- broad serde removal
- full workspace validation
- unrelated constructor/model cleanup

## Code Organization Reminders

- Keep value model changes in `value/`.
- Keep syntax helpers in `slot_codec/`.
- Keep semantic endpoint conversion in `binding/`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search for temporary notes around enum value support:

```bash
rg -n "TODO|FIXME|LpValue::Enum|ModelEnumVariant|BindingEndpoint" \
  lp-core/lpc-model/src docs/design/slots docs/plans/2026-05-15-lpvalue-enum
```

Make sure any unresolved binding literal issue is recorded in
`docs/plans/2026-05-15-lpvalue-enum/summary.md`.

## Validate

```bash
cargo fmt -p lpc-model
cargo test -p lpc-model value::lp_value value::lp_type
cargo test -p lpc-model slot_codec::slot_value_codec
cargo test -p lpc-model slot_codec::dynamic_slot_writer
cargo test -p lpc-model binding::binding_endpoint
cargo test -p lpc-model
```
