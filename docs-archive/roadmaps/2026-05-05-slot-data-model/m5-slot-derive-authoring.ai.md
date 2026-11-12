# Milestone 5: Slot Derive Authoring

## Title And Goal

Add derive-assisted Rust authoring for slot shapes and slot data after the
manual model is validated.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m5-slot-derive-authoring/`

## Scope

In scope:

- Add a focused proc-macro crate, likely `lpc-model-derive`.
- Derive slot shape/data implementations for simple structs.
- Derive enum support if the prior milestones have settled enum semantics well
  enough; otherwise keep enum derive narrowly scoped or manual.
- Use one already-migrated config slice as the macro target.
- Keep generated code readable enough to debug.
- Add compile-pass and compile-fail tests where practical.

Out of scope:

- Advanced presentation annotations.
- Full fixture mapping migration if the macro is not ready for it.
- Dynamic artifact-authored shape generation.
- Server-side mutation APIs.

## Key Decisions

- The macro comes after manual source/runtime slices so it automates a validated
  shape instead of freezing an unproven design.
- The first derive should be intentionally small; broad ergonomic polish can
  follow later.
- Prior art exists in
  `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data-derive`, but the new
  macro should match the current `lpc-model` vocabulary.

## Deliverables

- New derive crate or module wired into the workspace.
- Derive for at least one simple config struct.
- Tests showing generated shape/data matches the manual version.
- Notes on unsupported Rust forms and future macro extensions.

## Dependencies

- Milestone 1 model foundation.
- Milestone 2 shape vocabulary.
- Milestone 3 Rust-authored config slice.
- Milestone 4 runtime slot-tree exposure.

## Execution Strategy

Full plan. Proc-macro APIs are sticky and should be designed against the
validated manual model.

