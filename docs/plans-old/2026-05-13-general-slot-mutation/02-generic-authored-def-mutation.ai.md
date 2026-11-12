# Phase 2: Generic Authored Def Mutation

## Scope of phase

In scope:

- Replace the clock-specific mutation logic with a generic mutation path for authored `node.<id>.def` roots.
- Validate mutability from shape policy instead of node-kind/path special cases.
- Keep mutation operation scope to `SetValue` on value leaves only.
- Reject unsupported roots and targets clearly.

Out of scope:

- Container mutation operations.
- Runtime state mutation.
- Artifact persistence/writeback to disk.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files and symbols:

- `lp-core/lpc-engine/src/engine/slot_mutation.rs`
- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-view/src/slot/apply.rs`
- `lp-core/lpc-wire/src/slot/mutation.rs`

Expected changes:

- Remove the hard-coded `clock_def_data_version(...)` and `mutate_clock_def_value(...)` shape.
- Introduce generic authored-def target resolution for `node.<id>.def`:
  - resolve the loaded `NodeDef`,
  - resolve the requested `SlotPath`,
  - fetch target shape and current revision,
  - fetch effective policy for the target field/leaf path.
- Reject mutation when:
  - the root is unknown,
  - the root is not an authored def root,
  - the path does not resolve,
  - the target is not a value leaf,
  - the target policy is not writable,
  - the `LpValue` type does not match,
  - the optimistic shape/data revisions are stale.
- Add a generic typed writeback path for validated value leaves on `NodeDef`.
  - Prefer a reusable model-layer mutation helper over reintroducing per-node path switches.
  - Preserve typed storage; do not introduce a separate mutable dynamic slot-data shadow.

Tests to add or update:

- Accepted mutation on a non-clock authored def, such as output brightness or output options.
- Accepted mutation on the existing clock controls through the same generic path.
- Wrong-type rejection on a writable def field.
- Read-only rejection for an explicit opt-out field, if phase 1 added a test fixture for one.
- Unknown-root, unknown-path, unsupported-root, stale-shape, and stale-data rejection coverage.

Constraints and edge cases:

- Keep scope hardcoded to `node.<id>.def` for this phase.
- Preserve the current optimistic revision contract used by `lpc-view`.
- Avoid duplicating slot-path traversal logic that already exists in model/view layers unless mutation truly needs a mutable counterpart.

## Validate

```bash
cargo test -p lpc-engine mutation
cargo test -p lpc-view mutation
```

