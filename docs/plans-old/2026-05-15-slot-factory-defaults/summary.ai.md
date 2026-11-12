### What was built

- Added explicit slot factories to `SlotShapeRegistry`, with `Static`, `Dynamic`, and `Unsupported` creation behavior.
- Added `SlotShapeRegistry::create_default(shape_id) -> Box<dyn SlotMutAccess>` as the public dynamic creation path.
- Added `DynamicSlotObject` plus dynamic `SlotData` default construction for records, values, maps, options, enums, refs, and unit shapes.
- Taught generated static shape registration to install typed default factories through `T::default()`.
- Added map, option, and enum default-creation mutation hooks so generic mutation/deserialization can create missing containers explicitly.
- Added real-model `Default` impls needed by static slot factories.
- Added mockup tests that create typed and dynamic slot objects from shape metadata, then mutate map entries and enum payloads through the generic mutation layer.
- Documented the factory model in the slot serialization design notes.

### Decisions for future reference

#### Factory Behavior Is Explicit

- **Decision:** Every registered shape has an explicit creation behavior: static, dynamic, or unsupported.
- **Why:** Deserialization and generic mutation need a crisp boundary for which shapes can be materialized.
- **Rejected alternatives:** Optional factories with implicit fallback behavior.
- **Revisit when:** We have enough usage to know whether compatibility defaults should be removed from the older registration helpers.

#### Static Defaults Use Rust Defaults

- **Decision:** Static slot shapes create default instances through `T::default()`.
- **Why:** The model layer is defaultable by design; validation belongs to the logic layer. Defaults are empty sentinel data, not proof that the object is meaningful to run.
- **Rejected alternatives:** Required fields or shape-specific hydrators for basic construction.

#### Dynamic Defaults Build SlotData

- **Decision:** Dynamic factories build `SlotData` from the registered shape and wrap it in `DynamicSlotObject`.
- **Why:** Dynamic records still need to participate in `SlotAccess` and `SlotMutAccess` without static Rust types.
- **Rejected alternatives:** Returning raw `SlotData` from the registry.

#### Snapshots Restore Unsupported Factories

- **Decision:** `apply_snapshot` restores shape metadata with `Unsupported` factories.
- **Why:** Serialized registry data should not silently gain creation behavior. Local code must install static or dynamic factories intentionally.
- **Rejected alternatives:** Treating all restored snapshot shapes as dynamically creatable.

#### Creation Is Separate From Plain Set

- **Decision:** `set_slot_value` remains conservative; missing map keys and absent option payloads require explicit creation helpers.
- **Why:** Accidental structure creation during simple value assignment would make mutation semantics harder to reason about.
- **Rejected alternatives:** Auto-creating every missing container during path traversal.
