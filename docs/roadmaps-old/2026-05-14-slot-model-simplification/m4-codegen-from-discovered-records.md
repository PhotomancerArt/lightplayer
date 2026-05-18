# Milestone 4: Codegen From Discovered Records

## Title And Goal

Replace the hand-authored mockup codec table with codegen driven from discovered
`SlotRecord` structs.

## Suggested Plan Location

`docs/roadmaps/2026-05-14-slot-model-simplification/m4-codegen-from-discovered-records/`

## Scope

In scope:

- Build a shared discovered record model in `lpc-slot-codegen`.
- Feed shape, view, and codec generation from that model.
- Generate mockup record readers/writers from discovered fields.
- Delete or shrink `mockup_source_codec_module()`.

Out of scope:

- Production loader adoption.
- General Serde-like behavior.

## Key Decisions

- No hidden record/field lists.
- Field behavior comes from field type, slot metadata, and generic helpers.

## Deliverables

- Generated mockup codecs for source records.
- Existing generated mockup round-trip tests still passing.

## Dependencies

Milestones 2 and 3.

## Execution Strategy

Full plan. This is the central generator rewrite.
