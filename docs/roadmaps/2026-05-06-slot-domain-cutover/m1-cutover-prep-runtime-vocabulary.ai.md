# Milestone 1: Cutover Prep And Runtime Vocabulary

## Title And Goal

Prepare the production runtime vocabulary so slot roots can replace legacy detail concepts cleanly.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m1-cutover-prep-runtime-vocabulary/`

## Scope

In scope:

- Convert produced/consumed runtime slot identity from `ValuePath` toward `SlotPath`.
- Clarify `RuntimeProduct` as the produced payload shape for non-plain values.
- Define root naming conventions for `source`, `state`, `params`, and `output`.
- Add or refine wire/view types for watching slot roots by node.
- Add minimal metadata needed for generic debug rendering: label, help/description, read-only/writable, and simple visibility/category if needed.
- Audit old `Kind`, prop, and legacy detail exports that will affect the cutover.

Out of scope:

- Full node def conversion.
- Runtime node state exposure.
- Client mutation.
- Removal of legacy project detail responses.

## Key Decisions

- Slot identity is `SlotPath`; nested value projection remains `ValuePath` only inside a leaf value.
- Watching is a slot-root concept, not a node-detail concept.
- The initial root vocabulary is conventional, not a final language specification.

## Deliverables

- Updated runtime/query/binding types or explicit compatibility shims using `SlotPath`.
- Documented root naming conventions in code and roadmap notes.
- Wire/view request types for slot-root watch interest.
- Focused tests proving produced/consumed slot identity no longer depends on `ValuePath`.

## Dependencies

- Existing slot model foundation from `2026-05-05-slot-data-model`.

## Execution Strategy

Full plan. This milestone crosses resolver, runtime products, wire request vocabulary, and old prop naming.

