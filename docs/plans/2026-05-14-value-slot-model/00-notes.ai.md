# ValueSlot / SlotValue Model Notes

## Scope

This plan reshapes the slot leaf model around one crisp rule:

> `ValueSlot<T>` is revision-tracked slot storage. `T: SlotValue` is the semantic leaf payload.

This is intentionally a move-fast shaping pass. Compatibility with the current `FooSlot` implementation style is not a goal. The goal is to delete duplicate leaf-slot boilerplate and make the model obvious enough that future codec/codegen work can build on it.

## Current State

Relevant files:

- `lp-core/lpc-model/src/slot/value_slot.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`
- `lp-core/lpc-model/src/slots/*.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-slot-macros/src/lib.rs`
- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/**`

The good parts already exist:

- `ValueSlot<T>` already wraps `WithRevision<T>`.
- `ValueSlot<T>` already has generic `SlotValueAccess` and `FieldSlot` impls.
- `SlotValue` already exists as `ToLpValue + FromLpValue + value_shape`.
- Primitive `SlotValue` impls already exist for `String`, `i32`, `u32`, `f32`, `bool`, `[f32; 2]`, and `[f32; 3]`.
- Several semantic payloads already implement `SlotValue`, such as `Dim2u`, `Affine2d`, `ColorOrderValue`, `ResourceRef`, and product refs.

The messy parts:

- Many files in `lp-core/lpc-model/src/slots/` define hand-written slot containers:
  - `RatioSlot`
  - `PositiveF32Slot`
  - `RenderOrderSlot`
  - `XySlot`
  - `SourcePathSlot`
  - `ArtifactPathSlot`
  - `Dim2uSlot`
  - `Affine2dSlot`
  - `ColorOrderSlot`
  - `RelativeNodeRefSlot`
  - `ResourceRefSlot`
- These duplicate the same storage/revision/serde/access pattern that `ValueSlot<T>` already provides.
- `SlotValue` ids are manually written in most impls.
- The user explicitly does not want handwritten ids by default.
- `lpc-slot-macros` currently only exposes `SlotRecord`.
- `SlotRecord` docs still advertise `#[slot(skip)]`.
- The current codebase still has `#[slot(skip)]` in real and mockup records.

## User Notes

- This is not Serde.
- Slot records are basic data objects that can be serialized, deserialized, synced, reflected, and edited.
- It is fine to force models to be simple.
- Be bold. This is rough molding and shaping of the app.
- Do not preserve confusing compatibility layers unless they buy something concrete.
- `ValueSlot<T>` should be the generic slot leaf container.
- `SlotValue` should be the public semantic concept.
- `ToLpValue` and `FromLpValue` may still be useful, but they should be lower-level plumbing.
- Semantic leaves should look like:

  ```rust
  pub struct Ratio(pub f32);
  pub type RatioSlot = ValueSlot<Ratio>;
  ```

- Auto-id generation matters a lot:
  - do not require ids everywhere
  - derive ids from Rust type names by default
  - use one namespace for now
  - conflicts are errors
  - explicit ids can wait unless needed

## Open Questions

### Q1. Should `SlotValue` absorb `ToLpValue` and `FromLpValue`?

Suggested answer: not yet. Keep `ToLpValue` and `FromLpValue` as reusable conversion traits, but make `SlotValue` the trait model authors think about.

Target shape:

```rust
pub trait SlotValue: Sized + ToLpValue + FromLpValue {
    const SHAPE_ID: SlotShapeId;

    fn value_shape() -> SlotValueShape;
}
```

The derive implements all three traits for simple cases. Manual impls can still exist for odd cases.

### Q2. What should the default generated id be?

Suggested answer: `stringify!(TypeName)`.

Examples:

- `Ratio`
- `SourcePath`
- `Dim2u`
- `BindingEndpoint`

This intentionally ignores module path for now. One global namespace is simpler and search-friendly. Duplicate type names are a build error.

### Q3. Where can conflict detection happen?

Suggested answer: in generated/discovered slot metadata, not in the proc macro alone.

A proc macro cannot reliably know every other type in the crate or workspace. The derive should emit a simple static id. `lpc-slot-codegen` should be extended to discover slot value derives and fail when two slot shapes claim the same id in the generated registry/catalog. Runtime registry insertion should also reject duplicates as a backstop.

### Q4. Do we support explicit ids now?

Suggested answer: no for the first pass. Add the parser shape only if implementation pressure demands it. If added, it should be rare:

```rust
#[slot_value(id = "LegacyStableName")]
```

### Q5. How do editor hints work?

Suggested answer: start with a small `#[slot_value(...)]` attribute surface on the semantic value type.

Examples:

```rust
#[derive(SlotValue)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
pub struct Ratio(pub f32);

#[derive(SlotValue)]
#[slot_value(editor = path)]
pub struct SourcePath(pub String);
```

Manual `SlotValue` impls remain allowed when attributes become awkward.

### Q6. How aggressive should the first conversion be?

Suggested answer: aggressive inside `lpc-model/src/slots` and `lpc-slot-mockup`; conservative about unrelated crates. Delete duplicated slot containers as soon as their semantic values work.

Validation should focus on:

- `cargo test -p lpc-slot-macros`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-model`
- `cargo test -p lpc-slot-mockup`
- targeted `cargo check` for dependent host crates if needed

## Non-Goals

- Extracting `lpc-slot` into its own crate.
- Full custom disk/wire codec completion.
- Removing every `serde` derive from the domain.
- Solving schema versioning.
- Preserving old authored ids or old hidden shape names.
- Supporting private fields in generated slot data.
