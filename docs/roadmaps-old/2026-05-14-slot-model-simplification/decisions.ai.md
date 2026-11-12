#### Keep Slot Records Simple

- **Decision:** Generated `SlotRecord` targets are plain public data records.
- **Why:** This keeps shape, view, sync, and codec generation obvious.
- **Rejected alternatives:** Private-field constructor inference; Serde-like
  customization.
- **Revisit when:** A production model has a proven need that cannot delegate
  or implement custom machinery.

#### ValueSlot Owns Storage

- **Decision:** `ValueSlot<T>` is the standard revision-tracked leaf container.
- **Why:** It avoids duplicating storage/revision/access code in every semantic
  leaf.
- **Rejected alternatives:** One custom `FooSlot` storage wrapper per leaf.

#### Semantic Values Own Shape

- **Decision:** `T: SlotValue + ToLpValue + FromLpValue` owns leaf shape,
  editor metadata, and conversion.
- **Why:** This is the information needed for generic leaf serialization and
  reflection.
- **Rejected alternatives:** Per-leaf codec tables; record-specific field
  tables.

#### No Crate Split For This Roadmap

- **Decision:** Do not extract `lpc-slot` or `lpc-domain` during this roadmap.
- **Why:** The concepts are interlinked and the split would become its own
  project.
- **Rejected alternatives:** Pre-splitting domain or slot crates before the
  mockup is smooth.
- **Revisit when:** The slot model has stabilized and import churn is worth it.

#### Mockup First

- **Decision:** Optimize validation and design pressure around the mockup.
- **Why:** The mockup is the safe place to prove the model before production
  adoption.
- **Rejected alternatives:** Workspace-wide migration during model cleanup.
