# M9 Plan Notes — Invocation Ref|Def + Generic Slot Edit Ops

## Scope

1. **`NodeInvocation`** becomes `Ref(ArtifactSpecifier) | Def(NodeDef)` slotted enum
2. **TOML** — `ref = "..."` vs `[....def] kind = ...` (breaking; no dual-read)
3. **Delete** `NodeDefRef`, `def_slot`, whole-invocation custom codec hack
4. **`VariantSet`** edit op; **`SetSlot`** = value leaves only
5. **Remove** registry shortcuts: `project_node_def_mutation`, `inline_body_mutation`,
   `apply_node_invocation_def`, diff `CustomDef` / `invocation_def_value`
6. **Examples + docs** after Rust/tests green

**Out of scope:** wire protocol, client undo/history, engine cutover to `SyncOp`.

## Current state

- `NodeInvocation { def: NodeDefRef, def_slot: ArtifactPathSlot }` — duplicate path state
- Slot shape lies: `def` field typed as `ArtifactPathSlot` even for inline defs
- `slot_apply.rs` string heuristics + invocation-specific routers
- `def_diff.rs` emits `SetSlot + String` for enum/kind; `CustomDef` for wiring
- Tests/examples use `def = { path = "..." }` and `[node.def]` inline form

## Agreed decisions

| # | Decision |
|---|----------|
| D1 | No backwards compat — update tests and examples |
| D2 | Variants **`Ref`** and **`Def`** (not Path/Inline/Artifact) |
| D3 | TOML: top-level `ref` key; inline body under `def` subtable |
| D4 | Edit op **`VariantSet { path, variant }`** (not SetKind) |
| D5 | **`SetSlot`** = scalar/string leaf values only |
| D6 | Phases 01–06 Rust/tests; phase 07 examples/docs |

## Open questions

*(None blocking — user confirmed naming and no compat.)*
