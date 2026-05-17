# Milestone 5: Explicit Escape Hatches

## Title And Goal

Define the small set of sanctioned ways to handle complex slot behavior.

## Suggested Plan Location

`docs/roadmaps/2026-05-14-slot-model-simplification/m5-explicit-escape-hatches/`

## Scope

In scope:

- Document and test delegation to a slot-data field.
- Document and test fully custom slot/codec implementations.
- Keep enum/discriminator special handling explicit.

Out of scope:

- Making the generator infer complex private models.
- Adding broad Serde-like customization.

## Key Decisions

- The derive path stays simple.
- Complexity is explicit, local, and discoverable.

## Deliverables

- Examples/tests for delegation and custom impl patterns.
- Updated design docs.

## Dependencies

Milestones 2-4.

## Execution Strategy

Small plan. This is mostly examples, docs, and a few focused tests.
