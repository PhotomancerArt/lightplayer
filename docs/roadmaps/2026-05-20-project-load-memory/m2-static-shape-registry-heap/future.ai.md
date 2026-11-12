# Future Work

## Follow-Up Smells

- `followups/01-stream-static-catalog-export.ai.md` - implemented; keep as design notes.
- `followups/02-true-paged-static-catalog.ai.md` - implemented; keep as design notes.
- `followups/03-reduce-owned-shape-conversions.ai.md`
- `followups/04-rename-static-registration-api.ai.md`
- `followups/05-separate-engine-runtime-state-shapes.ai.md`
- `followups/06-restore-fw-esp32-validation.ai.md` - implemented; keep as validation notes.

## Generated Precompiled Slot Views

- **Idea:** Generate `SlotAccessor` steps directly from slot view codegen
  instead of compiling semantic paths through shape lookup at runtime.
- **Why not now:** The static catalog migration is already large. This can be a
  follow-up once `SlotShapeLookup` is stable.
- **Useful context:** `lp-core/lpc-slot-codegen/src/render/slot_views.rs`

## Optional Catalog Compatibility Metadata

- **Idea:** Add a static catalog version or checksum to help clients diagnose
  mismatched model builds before shape export.
- **Why not now:** Normal client setup receives exported static descriptors
  from the device, so no compatibility protocol is required for this milestone.
- **Useful context:** `lpc-wire` shape read responses and static catalog export.
