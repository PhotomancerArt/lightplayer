# Phase 05: Cleanup And Validation

## Scope Of Phase

Clean up plan fallout, document decisions, and run final validation.

In scope:

- Remove stale `ShaderParamDef` exports/files if replaced.
- Search for old names and update docs/tests as appropriate.
- Add `future.md` entries for merge, non-leaf bindings, receiver policies,
  resolver explain output, runtime ABI conversion, and `LpValue` naming.
- Write `summary.md`.
- Run final validation commands.

Out of scope:

- Implementing runtime compute shaders.
- Implementing merge/non-leaf binding resolution.

## Code Organization Reminders

- No stray TODOs unless they point to explicit future work.
- No commented-out experiments.
- Keep new docs terse.

## Sub-Agent Reminders

- Do not commit unless explicitly told by the main agent.
- Report any validation failures with exact command output.

## Implementation Details

Searches to run:

```bash
rg -n "ShaderParamDef|ScalarHint|lp::FluidEmitter|sentinel_array|ShaderSlotGlsl"
rg -n "TODO|dbg!|println!" lp-core/lpc-model/src/nodes/shader lp-core/lpc-model/src/nodes/fluid
```

Final validation:

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo test -p lpc-wire source_slot_sync
cargo check -p lpc-model --features schema-gen
```

