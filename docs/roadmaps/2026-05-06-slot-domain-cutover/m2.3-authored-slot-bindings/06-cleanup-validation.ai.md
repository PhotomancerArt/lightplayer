# Phase 6: Cleanup And Validation

## Scope Of Phase

Perform final cleanup, documentation updates, and validation for M2.3.

In scope:

- Remove temporary compatibility helpers that are no longer needed.
- Update rustdocs for the new binding types.
- Update roadmap notes if implementation pressure changed any decisions.
- Ensure M2.4 notes still accurately describe the next runtime work.
- Run final focused validation.

Out of scope:

- Starting M2.4 implementation.
- Building canonical project sync.
- UI/view work.

## Code Organization Reminders

- Keep tests at the bottom of files.
- Keep `mod.rs` files as module maps and re-export surfaces.
- Avoid new TODOs unless they point to a specific future milestone.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Cleanup checklist:

- `BindingDefs`, `BindingDef`, `BindingEndpoint`, `NodeSlotRef`, and
  `BusSlotRef` have clear rustdocs explaining their role.
- `lpc-source/src/prop` has no accidental new binding vocabulary.
- `examples/basic` reflects the bus-first binding idiom.
- M2.4 notes/design explicitly depend on M2.3 and do not duplicate authored
  binding design work.
- Old tests referring to `texture_loc` or `SrcBinding` are updated or removed.
- `rg` audits are clean enough to show the old vocabulary is not growing:

  ```bash
  rg -n "SrcBinding|texture_loc|NodePropSpec|SrcShape|SrcSlot" lp-core examples docs/roadmaps/2026-05-06-slot-domain-cutover
  ```

Final validation:

```bash
cargo fmt --check --package lpc-model --package lpc-source --package lpc-engine
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-engine project_loader
cargo check -p lpc-engine
cargo check -p lpc-model --features schema-gen
cargo check -p lpc-source --features schema-gen
```

If these pass and no broad runtime/API work leaked into the milestone, M2.3 is
ready for implementation review and M2.4 can begin from a clearer authored model.
