# Summary

## What Was Built

- Added `#[derive(SlotValue)]` for simple public tuple newtypes and public named-field structs.
- Converted the main semantic leaf slots to `ValueSlot<T>` aliases:
  - `RatioSlot = ValueSlot<Ratio>`
  - `PositiveF32Slot = ValueSlot<PositiveF32>`
  - `RenderOrderSlot = ValueSlot<RenderOrder>`
  - `SourcePathSlot = ValueSlot<SourcePath>`
  - `ArtifactPathSlot = ValueSlot<ArtifactPath>`
  - `XySlot = ValueSlot<Xy>`
  - `Dim2uSlot = ValueSlot<Dim2u>`
  - `Affine2dSlot = ValueSlot<Affine2d>`
- Removed `#[slot(skip)]` support from `SlotRecord` derive.
- Made derived `SlotRecord` fields public-only.
- Moved mockup `kind` fields out of slot data; discriminators remain codec-level.
- Made fixture sampling real slot data instead of a skipped field.
- Added codegen duplicate detection for derived `SlotValue`/`SlotRecord` type names within a discovered crate.
- Updated the mockup to compile and pass on the new model.

## Decisions For Future Reference

#### SlotValue Is The Leaf Contract

- **Decision:** `ValueSlot<T>` owns revisioned storage; `T: SlotValue` owns shape, editor metadata, and `LpValue` conversion.
- **Why:** This removes duplicated `FooSlot { inner: WithRevision<T> }` code and keeps semantic meaning on the payload type.
- **Rejected alternatives:** custom per-leaf slot containers for normal semantic leaves.
- **Revisit when:** a semantic leaf needs custom storage behavior that `ValueSlot<T>` cannot express.

#### No Slot Skip

- **Decision:** `#[derive(SlotRecord)]` no longer supports `#[slot(skip)]`.
- **Why:** Skipping fields makes the slot model stop being the source of truth.
- **Rejected alternatives:** keeping skipped fields as a serde-like convenience.
- **Revisit when:** we design an explicit `transient` projection.

#### Public Fields Only

- **Decision:** generated `SlotRecord` and `SlotValue` paths require public fields.
- **Why:** Slot-modeled data should be simple, inspectable data. Complex private runtime state should live outside the slot data struct or use a custom impl.
- **Rejected alternatives:** generating access through private fields by virtue of macro expansion.

#### Value Ids Use Rust Names

- **Decision:** derived `SlotValue` ids use the Rust type name by default.
- **Why:** It keeps semantic leaf ids search-friendly and avoids handwritten ids during active model shaping.
- **Rejected alternatives:** requiring explicit `slot.leaf.*` ids.
- **Revisit when:** schema migration needs stable legacy ids.

#### Record Ids Still Use Module Paths

- **Decision:** `SlotRecord` ids still include `module_path!()` for now.
- **Why:** The mockup intentionally defines records with the same names as real model records, and both can live in the same process during tests.
- **Rejected alternatives:** type-name-only record ids immediately.
- **Revisit when:** mock and real domain records no longer coexist or we introduce explicit record namespaces.
