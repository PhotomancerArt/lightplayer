# Phase 6: Cleanup And Validation

## Scope Of Phase

Clean up temporary scaffolding and validate the M4 mockup codegen path.

In scope:

- remove dead structs/functions from the old static codec table
- update comments and docs that mention the temporary M2 codec generator shape
- ensure generated code is not obviously bloated or repetitive
- record remaining follow-up work

Out of scope:

- production loader adoption
- embedded binary size measurements
- moving `authored_toml.rs` between crates

## Code Organization Reminders

- Remove temporary TODOs unless they point to a real follow-up.
- Do not leave commented-out old generated snippets.
- Keep generated helpers grouped by concept.
- Prefer crisp helper names over generic `do_field_thing` names.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/build.rs`
- `lp-core/lpc-slot-mockup/src/generated_slot_codec.rs`
- `docs/roadmaps/2026-05-14-slot-model-simplification/*`
- `docs/design/slots/*.md`

Expected cleanup:

- Remove or significantly shrink:
  - `mockup_source_codec_module`
  - `SlotCodecType`
  - `SlotCodecField`
  - hard-coded record construction expressions
- Update comments around `generate_mockup_slot_codec` so it no longer says it
  is the narrow M2 static-table experiment.
- If useful, add a short summary file in this plan directory after execution.
- Preserve unrelated dirty files and staged docs unless the user explicitly
  asks to include them.

Final validation commands:

```bash
cargo fmt -p lpc-slot-codegen -p lpc-model -p lpc-wire -p lpc-slot-mockup
cargo test -p lpc-slot-codegen
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
```

Search checks:

```bash
rg "from_codec|_from_codec" lp-core/lpc-slot-mockup/src lp-core/lpc-slot-codegen/src
rg "mockup_source_codec_module|SlotCodecType|SlotCodecField" lp-core/lpc-slot-codegen/src
```
