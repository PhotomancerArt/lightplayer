# Milestone 1: Simplify Leaf Slots

## Title And Goal

Clarify the leaf model so `ValueSlot<T>` owns storage and `T` owns semantic
shape/conversion.

## Suggested Plan Location

`docs/roadmaps/2026-05-14-slot-model-simplification/m1-simplify-leaf-slots/`

## Scope

In scope:

- Prove the simplified leaf pattern on a small representative set.
- Prefer semantic newtypes plus `ValueSlot<T>` aliases.
- Keep custom code next to the semantic value in `lpc-model/src/slots`.

Out of scope:

- Converting every slot leaf.
- SlotCodec record generation.
- Crate extraction.

## Key Decisions

- Most leaf serialization is `ToLpValue` / `FromLpValue`.
- Avoid per-leaf codec impls unless a leaf truly cannot use generic `LpValue`
  syntax.

## Deliverables

- A proven pattern for leaves like ratio/positive-f32/render-order.
- Updated tests around leaf shape metadata and value conversion.

## Dependencies

None.

## Execution Strategy

Small plan. The pattern needs a little care, but should stay narrow.
