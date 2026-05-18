# SlotCodec Domain Serialization Notes

## Scope

Move slot-authored domain persistence and slot-shaped wire payload handling onto
SlotCodec. Do not treat direct `serde` and `serde_json` dependencies as a
problem by themselves; they are acceptable for protocol shells, tests, host
tooling, and other small non-slot surfaces when firmware measurements stay
flat.

The migration strategy is "switch it and fix it":

- keep existing serde derives, attributes, helpers, and tests during the early
  phases
- switch one real application path at a time to the slot registry and slot codec
- fix behavior and tests after each switch
- remove specific serde-derived behavior only after slot paths own the behavior
  and measurement supports the cleanup

## Current State

The slot-native infrastructure is now credible enough to drive production paths:

- JSON and TOML syntax sources feed the same slot reader semantics
- slot writers can emit JSON and TOML from `SlotAccess`
- `SlotShapeRegistry` can create default objects with factories
- `SlotMutAccess` supports default-and-mutate deserialization
- boxed slot objects can be downcast back to concrete Rust types
- `ValueSlot<T>` owns revision for atomic semantic leaves
- `EnumSlot<T>` owns active-variant revision for structured slot enums

The old plan is archived at:

- `docs/plans-old/2026-05-15-remove-serde-from-lpc-model/00-notes.md`

Treat it as historical notes. This roadmap supersedes its execution order.

## Milestone 1 Cleanup Status

Milestone 1 has been executed in the working tree:

- mockup structured enum fields now use `EnumSlot<T>`
- mockup raw enum variants no longer store active-variant revision fields
- macro/codegen support for `#[slot(enum)]` has been removed

This leaves one rule:

- slotted record fields contain slot containers
- atomic choices use `ValueSlot<T>` and `LpValue::Enum`
- structured choices use `EnumSlot<T>` and `SlotShape::Enum`
- raw Rust enum values expose domain data; they do not own slot revisions

## User Notes

- This is roadmap-level work.
- Milestone 1 is cleanup and can probably be executed directly.
- The main migration should switch real call sites first and only remove
  expensive serde-derived behavior when measurements justify it.
- Existing serde annotations and helpers can stay during the switch.
- Starting with messages is acceptable and probably a good first real path.
- Defs/artifact loading can follow after message paths.

## Open Questions

### Q1. Start real migration with messages or defs?

Context: messages likely pressure JSON and wire payloads first. Defs pressure
TOML, artifact loading, `NodeDef`, and authored discriminators.

Suggested direction: start with messages in Milestone 2 because it is likely the
smaller surface and proves the wire side before changing authored project load.

### Q2. Should `#[slot(enum)]` be removed completely or rejected with a tailored error?

Context: once mockup codegen uses `EnumSlot<T>`, the attribute is no longer
needed. Keeping it invites raw enum fields with no revision boundary.

Decision: remove support and make any lingering use fail as an unknown slot
attribute. A targeted diagnostic can be added later if this comes up in normal
development.

### Q3. When do slot infrastructure snapshot types lose serde?

Context: `SlotShape`, `SlotData`, `SlotMeta`, `Revision`, `LpType`, and
`LpValue` still derive serde and keep the dependency alive even after domain
paths switch.

Decision update: defer indefinitely unless firmware bloat measurements point at
these exact paths. The current post-merge bloat check shows `serde_core` is a
modest flat cost and `lpc_model` shrank after moving authored loading to
SlotCodec.
