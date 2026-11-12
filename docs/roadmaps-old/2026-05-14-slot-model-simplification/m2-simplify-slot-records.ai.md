# Milestone 2: Simplify Slot Records

## Title And Goal

Make generated `SlotRecord` targets plain public data records.

## Suggested Plan Location

`docs/roadmaps/2026-05-14-slot-model-simplification/m2-simplify-slot-records/`

## Scope

In scope:

- Remove `#[slot(skip)]` from the generated-record path.
- Make mockup slot fields public.
- Remove discriminator fields such as `kind` from record data.
- Document delegation/manual-impl escape hatches.

Out of scope:

- Full production domain migration.
- SlotCodec codegen rewrite.

## Key Decisions

- If a field is in a slot record, it is slot data.
- Complex objects delegate to simple slot-data structs or implement custom
  machinery.

## Deliverables

- Mockup source records shaped as simple slot data.
- Derive/codegen diagnostics that reject unsupported record shapes where useful.

## Dependencies

Milestone 1 is helpful but not strictly required.

## Execution Strategy

Small plan. This is mostly model cleanup plus guardrails.
