# Phase 1: Model Primitives

## Scope Of Phase

Add the reusable slot primitives in `lpc-model` needed by the pressure harness.

In scope:

- typed rust-authored slot leaves and maps,
- dynamic snapshot container naming/documentation,
- map key conversion traits,
- access trait implementations for both typed and dynamic containers,
- shape registry ownership/ref semantics.

Out of scope:

- derive macros,
- real source/engine integration,
- server mutation message APIs.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `no_std + alloc` compatibility.
- Put tests at the bottom of each file.
- Avoid temporary TODOs unless they explicitly point to a future milestone.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

- Update `lp-core/lpc-model/src/slot` so `SlotData` remains the owned dynamic snapshot representation.
- Add typed wrappers:
  - `SlotValue<T>` with `FrameId` and typed value,
  - `SlotMap<K, V>` with `keys_changed_frame` and `BTreeMap<K, V>`.
- Rename dynamic owned containers only where necessary to avoid conflict with typed wrappers.
- Add `SlotMapKeyLike` or equivalent conversion support for `String`, `u32`, and `i32`.
- Extend leaf conversion enough for mockup domain/editor values such as strings, relative node refs encoded as values, and enum-ish strings.

## Validate

```bash
cargo test -p lpc-model
```
