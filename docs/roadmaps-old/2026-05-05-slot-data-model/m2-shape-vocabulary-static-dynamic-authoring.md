# Milestone 2: Shape Vocabulary And Static/Dynamic Authoring

## Title And Goal

Define the useful slot shape vocabulary and prove static and dynamic authored
shape/data construction.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m2-shape-vocabulary-static-dynamic-authoring/`

## Scope

In scope:

- Finalize and implement the initial `SlotShape` / `SlotData` vocabulary:
  `Value`, `Record`, `Map`, `Enum`, and `Option`.
- Define map key constraints for stable-id collections.
- Add manual static Rust-authored examples.
- Add dynamic shape/data construction examples that approximate shader params.
- Add validation helpers that ensure `SlotData` matches registered `SlotShape`.
- Borrow useful concepts from the old `lp-data` static/dynamic shape split.

Out of scope:

- Proc-macro derives.
- Arrays and tuples.
- Applying the model to all real nodes.
- Wire sync.

## Key Decisions

- Dynamic collections use maps with stable keys, not arrays.
- Enums are first-class because real node config needs discriminated unions.
- Option is first-class because optional config should not be modeled by ad hoc
  sentinel values.
- Static and dynamic authoring must share common model traits/types.

## Deliverables

- Complete initial shape/data variant implementations.
- Static and dynamic examples in tests.
- Validation tests for record fields, map entries, enum variants, option values,
  and shape mismatch errors.
- Documentation explaining why arrays/tuples are omitted initially.

## Dependencies

- Milestone 1 model foundation.

## Execution Strategy

Full plan. The shape vocabulary is central to the roadmap and should be
designed and validated carefully.

