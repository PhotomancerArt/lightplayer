# 04 - Static Factory Codegen

## Scope of phase

Install typed default factories for generated static slot shapes.

In scope:

- Update `lpc-slot-codegen` static shape registration output.
- Add or use a helper that creates `Box<dyn SlotMutAccess>` from `T::default()`.
- Ensure generated static shapes compile only when the type is `Default`.
- Update tests around generated `slot_shapes.rs`.

Out of scope:

- Full generated codec changes.
- Downcasting created boxed objects to concrete types.

## Code organization reminders

- Keep generated code small; binary size is a leading motivation.
- Prefer shared helper functions in `lpc-model` over large repeated generated
  bodies.
- Keep codegen renderer files focused by domain.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-slot-codegen/src/render/slot_shapes.rs`
- `lp-core/lpc-slot-codegen/src/tests` or existing codegen tests in `src/lib.rs`
- `lp-core/lpc-model/src/slot/slot_factory.rs`
- generated include target: `lpc-model::slot_shapes`

Preferred helper:

```rust
impl SlotFactory {
    pub const fn for_default<T>() -> Self
    where
        T: SlotMutAccess + Default + 'static;
}
```

If generic const helpers with bounds are not workable, emit small per-type
factory functions from codegen and register those function pointers.

Generated registration should install static factories:

```rust
<T as StaticSlotShape>::ensure_registered_with_factory(
    registry,
    SlotFactory::for_default::<T>(),
)
```

or call a registry method directly:

```rust
registry.ensure_shape_named_with_factory(
    <T as StaticSlotShape>::SHAPE_ID,
    <T as StaticSlotShape>::shape_name().unwrap_or(...),
    <T as StaticSlotShape>::slot_shape(),
    SlotFactory::for_default::<T>(),
)?;
```

If adding `ensure_registered_with_factory` to `StaticSlotShape` is cleaner, do
that in `slot_access.rs`.

Tests:

- generated registration creates a static object for `ProjectDef::SHAPE_ID`.
- created object exposes `ProjectDef::SHAPE_ID`.
- static factory produces typed defaults observable through `SlotAccess`; for
  example `ShaderDef` default `glsl_path` should be `"main.glsl"`, while the
  dynamic factory for the same shape would not know that semantic default.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-codegen --check
cargo test -p lpc-slot-codegen
cargo test -p lpc-model
```
