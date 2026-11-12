# SlotShape And Registry

## Scope Of Phase

Add schema/metadata types and a registry for complete slot shape trees.

In scope:

- Add `slot_meta.rs`.
- Add `slot_shape.rs`.
- Add `slot_registry.rs`.
- Export `SlotMeta`, `SlotShape`, `SlotFieldShape`, `SlotVariantShape`,
  `SlotShapeId`, and `SlotRegistry`.
- Add focused tests for shape construction and registry lookup.

Out of scope:

- Slot data instances.
- Recursive shape/data validation.
- Static/dynamic authoring traits.
- Derive macros.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep support structs close to `SlotShape` unless they become large enough to
  justify their own files.
- Tests stay at the bottom.
- Do not add internal `SlotShapeId` references inside a registered shape tree.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/slot/slot_meta.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_registry.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/Cargo.toml` only if collection choices require dependency
  adjustments; prefer existing dependencies.

Suggested types:

```rust
pub struct SlotMeta {
    pub label: Option<String>,
    pub description: Option<String>,
}

pub struct SlotShapeId(String);

pub enum SlotShape {
    Value { meta: SlotMeta, ty: ModelType },
    Record { meta: SlotMeta, fields: Vec<SlotFieldShape> },
    Map { meta: SlotMeta, key: SlotMapKeyShape, value: Box<SlotShape> },
    Enum { meta: SlotMeta, variants: Vec<SlotVariantShape> },
    Option { meta: SlotMeta, some: Box<SlotShape> },
}

pub enum SlotMapKeyShape {
    String,
    I32,
    U32,
}

pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShape,
}

pub struct SlotVariantShape {
    pub name: SlotName,
    pub shape: SlotShape,
}
```

`SlotRegistry` should store complete shape trees:

```rust
pub struct SlotRegistry {
    shapes: BTreeMap<SlotShapeId, SlotShape>,
}
```

If `BTreeMap` is not appropriate under current `no_std + alloc`, choose a
deterministic alternative and document the reason in code comments.

Registration policy:

- Do not silently ignore duplicate ids.
- Either return an error on duplicate or return the replaced shape explicitly.
- Prefer an error if that fits local style cleanly.

Tests:

- `SlotShapeId` parse/display/serde.
- Registry register/lookup.
- Duplicate id behavior.
- Registered shape can contain nested record/map/enum/option shapes directly.

## Validate

```bash
cargo test -p lpc-model
```
