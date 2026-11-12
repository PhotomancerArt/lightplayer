# Phase 3: Accessor Path-Root Docs

## Scope Of Phase

Clarify `SlotAccessor` docs and diagnostics so `root` means root of a path
traversal, not top-level app object.

In scope:

- `lp-core/lpc-model/src/slot/slot_accessor.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- Generated view code in `lp-core/lpc-slot-codegen/src/lib.rs` if needed.
- Tests and diagnostics that mention "slot root shape" but mean path-root shape.

Out of scope:

- `SlotPath::root()` and `SlotPath::is_root()`.
- Runtime object naming in `MockRuntime::roots()`.
- Registry method renames already covered by Phase 2.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Suggested changes:

- Keep `SlotAccessor { root: SlotShapeId }` and `SlotAccessor::root()` if the
  existing API reads well in context.
- Rewrite docs to say the field is the root shape for path resolution.
- Rewrite diagnostics:
  - "missing slot root shape" -> "missing slot path root shape"
  - "slot accessor root ... data root ..." -> "slot accessor path root ... data shape ..."
- Rename runtime object variables from `root` to `object` or `slot_object`
  where that avoids confusing them with path-root shapes.

Searches:

```bash
rg -n "missing slot root shape|accessor root|data root|slot root" lp-core/lpc-model/src/slot lp-core/lpc-slot-codegen/src
```

## Validate

```bash
cargo test -p lpc-model slot_accessor
cargo test -p lpc-model slot_lookup
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup shape_codegen
```
