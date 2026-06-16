## Wire protocol VariantSet

- **Idea:** Expose `VariantSet` on client wire alongside `SetSlot`.
- **Why not now:** Registry/model only; `lpc-wire` unchanged in M9.
- **Useful context:** Same serde shape as `EditOp` in `edit_op.rs`.

## Slotted derive for NodeInvocation enum

- **Idea:** Full `#[derive(Slotted)]` on `Ref|Def` if manual codec proves brittle.
- **Why not now:** Phase 01 tries slotted enum first; revisit only if TOML edge cases block.
- **Useful context:** Compare `NodeDef` enum in `nodes/node_def.rs`.
