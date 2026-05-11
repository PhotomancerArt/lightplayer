#### Slot Data Is The Shared Domain Model

- **Decision:** Use `SlotData`, not `SyncData`, for the shared structured data
  model.
- **Why:** Sync is one consumer. The same data is authored, mutated, observed,
  bound, and synced.
- **Rejected alternatives:** `SyncData`, `AuthoredData`, `VersionedData`.

#### Slot Owner And Slot Tree Are Separate

- **Decision:** Keep `SlotOwner` for identity/authority and introduce
  `SlotTree` for rooted data/access.
- **Why:** A `SlotRef` needs an owner, while traversal/mutation belongs to the
  data tree owned by that entity.
- **Rejected alternatives:** Replacing `SlotOwner` with `SlotTree`.

#### Registry From The Beginning

- **Decision:** Use `SlotShapeId` and `SlotRegistry` from the first milestone.
- **Why:** Shape ownership/lifecycle should be explicit before dynamic artifact
  shapes arrive.
- **Rejected alternatives:** Recursive unregistered shapes as the only model.

#### Registered Shapes Own Complete Trees

- **Decision:** A registered `SlotShapeId` owns one complete `SlotShape` tree.
- **Why:** Avoids partial shape graphs and unclear lifecycle. Common/global
  shape references can be revisited when a real need appears.
- **Rejected alternatives:** Internal id references inside registered shapes.
- **Revisit when:** Shape duplication or cross-artifact shared shape identity
  becomes a concrete problem.

#### Initial Shape Vocabulary

- **Decision:** Start with `Value`, `Record`, `Map`, `Enum`, and `Option`.
- **Why:** This covers natural Rust config/state shapes without taking on array
  identity or tuple complexity immediately.
- **Rejected alternatives:** Only `Record`/`Value`; adding arrays and tuples at
  the start.

#### Maps Instead Of Arrays

- **Decision:** Use maps with stable ids for dynamic collections.
- **Why:** Index-based identity creates noisy sync and brittle UI editing.
- **Rejected alternatives:** Plain array/index-addressed dynamic collections.
- **Revisit when:** Ordered collections need first-class modeling beyond a
  map plus explicit ordering field.

#### ResourceRef Is A Model Value

- **Decision:** Add `ResourceRef` as a portable `ModelValue` variant.
- **Why:** Slot data should carry resource references generically; payload bytes
  are fetched separately by resource ref.
- **Rejected alternatives:** Separate `RuntimeProduct`/`WireProduct` style
  value layers for every boundary.

#### ModelValue Rename Can Wait

- **Decision:** Keep `ModelValue` / `ModelType` names during early milestones.
- **Why:** The slot model should stabilize before broad rename churn.
- **Rejected alternatives:** Rename immediately to `Value` / `ValueShape` or
  `LpValue` / `LpType`.
- **Revisit when:** The slot model is applied broadly enough that the rename
  clarifies more than it churns.

#### Shader ABI Boundary Stays Explicit

- **Decision:** Rich slot data is not automatically shader-compatible.
- **Why:** Authoring data needs enums/maps/options/resources, while shader ABI
  values are a smaller scalar/vector/matrix/resource-reference subset.
- **Rejected alternatives:** Forcing all slot data into shader-compatible
  shapes.

#### Artifact Mutation Is Follow-Up Work

- **Decision:** Keep artifact mutation through the message API out of this
  roadmap's core milestones.
- **Why:** It is important but depends on the slot data model and deserves its
  own roadmap or follow-on milestone.
- **Rejected alternatives:** Folding mutation API design into the slot model
  foundation.

#### Derive Comes After Manual Validation

- **Decision:** Add derive-assisted slot authoring after a manual model and
  runtime/source slice are validated.
- **Why:** Hand-written slices reveal whether the model is right; derive then
  removes repetitive boilerplate before broad migration.
- **Rejected alternatives:** Building the derive macro before proving the model;
  migrating every node manually.
