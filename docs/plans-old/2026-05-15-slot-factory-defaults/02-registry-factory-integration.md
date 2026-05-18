# 02 - Registry Factory Integration

## Scope of phase

Teach `SlotShapeRegistry` to store runtime-only factories and expose
`create_default`.

In scope:

- Add factory storage to `SlotShapeRegistry`.
- Add factory-aware registration/ensure/replace methods.
- Make all shape registration paths install explicit creation behavior.
- Preserve snapshot wire shape without serializing factories.
- `apply_snapshot` installs documented snapshot factory behavior for all
  restored shapes.

Out of scope:

- Static codegen installation of typed factories.
- Recursive dynamic data builder details beyond calling the dynamic factory.

## Code organization reminders

- Keep the registry public API readable; do not bury behavior in large helper
  blocks.
- If custom serde is needed, keep it narrowly scoped.
- Tests stay at the bottom of `slot_shape_registry.rs`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot/slot_factory.rs`

Preferred internal shape:

```rust
pub struct SlotShapeRegistry {
    pub ids_revision: Revision,
    shapes: BTreeMap<SlotShapeId, SlotShapeEntry>,
    #[serde(skip)]
    factories: BTreeMap<SlotShapeId, SlotFactory>,
}
```

If derive constraints become awkward, implement serde/clone/debug/partial-eq
manually or move factory storage to a non-serialized wrapper in the least
invasive way.

Add:

```rust
pub fn create_default(
    &self,
    id: SlotShapeId,
) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;
```

Add factory-aware registration methods. Existing methods should call them with
`SlotFactory::dynamic()`.

Important behavior:

- Duplicate/conflict checks stay unchanged.
- Factory is installed when a shape is inserted.
- Missing factory should not be a normal registry state.
- Unsupported creation is a valid explicit factory state and should surface as
  a factory error from `create_default`.
- `ensure_shape_with_factory` should not replace an existing factory for an
  already-registered identical shape unless this is explicitly needed by static
  bootstrap. If static bootstrap needs to upgrade a dynamic factory to a static
  factory, add a focused `set_factory` or `ensure_factory` method and test it.
- `replace_shape*` replaces both shape and factory.
- `unregister_shape*` removes both shape and factory.
- `snapshot()` does not include factories.
- `apply_snapshot()` installs explicit unsupported factories for all restored
  shapes. Do not leave entries with no factory. Local code can later reinstall
  static or dynamic factories when creation is intended.

Tests:

- registered shape has a factory.
- explicitly uncreatable shape returns the expected factory error from
  `create_default`.
- unregister removes factory.
- snapshot/apply preserves shape metadata and restored registry can still
  `create_default` dynamically once phase 03 implements dynamic construction.

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model slot_shape_registry
```
