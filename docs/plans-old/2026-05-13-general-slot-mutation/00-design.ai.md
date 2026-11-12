# General Slot Mutation Design

## Scope of work

- Generalize project slot mutation support from the current clock-only implementation to authored node-definition roots.
- Make authored `node.<id>.def` value leaves mutable by default.
- Preserve explicit opt-out for exceptional fields and future domains that should remain read-only.
- Keep runtime state roots (`node.<id>.state`) out of scope and non-mutable by default for this phase.
- Keep mutation operations limited to `SetValue` on value leaves; container mutation remains out of scope.

## File structure

```text
lp-core/
├── lpc-slot-macros/
│   ├── src/attr.rs
│   └── src/record.rs
├── lpc-model/
│   ├── src/slot/
│   │   ├── slot_policy.rs
│   │   └── slot_shape.rs
│   └── tests/
│       └── slot_record_derive.rs
├── lpc-engine/
│   └── src/engine/
│       ├── slot_mutation.rs
│       └── project_read_nodes.rs
├── lpc-view/
│   └── src/slot/
│       └── mirror.rs
└── lpc-wire/
    └── src/slot/
        └── mutation.rs

lp-cli/
└── src/debug_ui/
    └── slot_render.rs
```

## Architecture summary

- Leave `SlotPolicy::default()` unchanged as read-only persisted so existing non-authored slot domains do not silently change semantics.
- Add an authored-record mutability default at the `SlotRecord` derive layer, so node definitions can opt into writable-by-default as a container-level rule rather than repeating field policy on every def field.
- Add source-level policy override attributes so the rare read-only exception stays explicit and local to the field or record declaration.
- Replace the hard-coded clock mutation logic with a generic authored-def mutation path that:
  - only accepts `node.<id>.def` roots,
  - resolves the target by `SlotPath`,
  - checks shape/data revisions,
  - checks that the target is a value leaf,
  - checks that the resolved field policy is writable,
  - applies a typed `SetValue`,
  - and relies on the normal project-read snapshot path to surface the new value.
- Keep the debug UI policy-driven. Once authored defs carry writable policy by default and the server accepts generic def mutations, editors appear naturally without UI special cases.

## Main components and interactions

### 1. Derive-layer authored policy defaults

- Extend `lpc-slot-macros` attribute parsing so a record can declare its default field policy.
- Expected shape: authored node defs use a container-level default such as writable persisted.
- Generated fields should use `field_with_semantics_and_policy(...)` instead of `field_with_semantics(...)`, with policy chosen from:
  - explicit field override, if present;
  - explicit container default, if present;
  - otherwise `SlotPolicy::default()`.

### 2. Field-level opt-out

- Add field-level policy override syntax in the derive.
- This allows future defs to mark isolated fields read-only without rewriting the whole shape by hand.
- Handwritten shapes such as `ClockControls` continue to work with `field_with_policy(...)`.

### 3. Generic authored-def mutation engine path

- Refactor `lp-core/lpc-engine/src/engine/slot_mutation.rs` away from `NodeDef::Clock` and hard-coded path matches.
- Introduce shape-aware target resolution for authored defs:
  - parse `node.<id>.def`,
  - resolve loaded `NodeDef`,
  - resolve the target shape/data at the requested path,
  - reject non-value leaves,
  - reject read-only targets,
  - reject wrong type,
  - reject stale shape/data revisions.
- After validation, mutate the concrete leaf and stamp the new revision.
- Runtime state roots are rejected explicitly in this phase.

### 4. Typed value application

- Because `NodeDef` stores typed Rust structs rather than dynamic `SlotData`, the engine needs a generic way to write a validated `LpValue` back into the underlying typed field.
- The preferred direction is to add a small mutation-oriented access layer in `lpc-model` for slot-authored data rather than open-coding per-node path matches again.
- The plan should preserve the existing typed ownership model and avoid introducing a parallel mutable `SlotData` shadow store.

### 5. Client/UI integration

- `lpc-view` already performs generic optimistic mutation preparation.
- `lp-cli` already gates editors on `policy.writable`.
- With the model and engine changes in place, the client/UI path should mostly work unchanged; tests should confirm that authored-def leaves become editable and accepted end to end.
