# Milestone 1: Cleanup Enum Slots

## Title And Goal

Remove the old raw structured enum field model so `EnumSlot<T>` is the only
supported structured slot enum field pattern.

## Suggested Plan Location

`docs/roadmaps/2026-05-16-slot-codec-serde-removal/m1-cleanup-enum-slots/`

## Scope

In scope:

- convert mockup structured enum fields to `EnumSlot<T>`
- remove direct variant revision fields from mockup raw enum variants
- replace mockup `SlotEnumAccess` / `SlotEnumMutAccess` impls with
  `SlottedEnum` / `SlottedEnumMut`
- remove `#[slot(enum)]` support from the derive attribute parser
- remove `#[slot(enum)]` documentation and codegen emission
- update tests that expected raw structured enum fields

Out of scope:

- switching real message or definition paths away from serde
- removing serde derives from real model types
- changing dynamic `SlotData::Enum`

## Key Decisions

- `EnumSlot<T>` owns active-variant revision.
- Raw structured enum types expose domain payload through `SlottedEnum`.
- Record fields should infer enum slot behavior from `EnumSlot<T>`, not from a
  field attribute.

## Deliverables

- No `#[slot(enum)]` usages in Rust source.
- No macro/codegen support for `#[slot(enum)]`.
- Mockup tests pass with structured enums wrapped in `EnumSlot<T>`.
- `lpc-model` tests pass after the `EnumSlot<T>` unit-payload revision
  adjustment.

## Dependencies

- Current `EnumSlot<T>` implementation.
- Existing mockup dynamic slot codec tests.

## Execution Strategy

Direct execution. The scope is narrow, search-driven, and already validated by
the mockup test suite.
