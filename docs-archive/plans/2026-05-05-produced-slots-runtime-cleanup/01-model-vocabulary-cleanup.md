# Model Vocabulary Cleanup

## Scope of phase

Introduce and settle the model-layer vocabulary that separates node references,
slot identity, and value traversal.

In scope:

- Add `SlotName`, `SlotOwner`, `SlotRef`, and `ValueRef` under `lpc-model`.
- Keep `ValuePath` as the parsed `Vec<Segment>` path inside a structured value.
- Clean up `RelativeNodeRef` / `RelativeNodeRefSrc` rustdocs, exports, and
  stale `NodeLoc` names.
- Remove `PropNamespace` from core semantic validation. If compatibility code
  still needs conventional names, keep that local and documented as convention.
- Prefer parsed semantic types over raw string wrappers in source definitions
  where that is low-risk.

Out of scope:

- Final authored binding/value-reference string syntax.
- Generic wire/view redesign.
- Resolver/binding direction renames unless needed to keep this phase compiling.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep public domain types near the top of their modules.
- Put tests at the bottom of each file.
- Do not add broad TODOs. If a temporary adapter remains, document the exact
  reason in rustdoc.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-model/src/node/relative_node_ref.rs`
- `lp-core/lpc-model/src/node/mod.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/prop/value_path.rs`
- `lp-core/lpc-model/src/prop/prop_namespace.rs`
- `lp-core/lpc-model/src/node/node_prop_spec.rs`

Expected changes:

- Add `lp-core/lpc-model/src/slot/` with:
  - `slot_name.rs`
  - `slot_owner.rs`
  - `slot_ref.rs`
  - `value_ref.rs`
  - `mod.rs`
- Export the slot types from `lpc-model`.
- `SlotName` should be a newtype over an opaque slot identifier string. It may
  accept names like `config.width` during this plan; deeper slot structure is
  future work.
- `SlotOwner` should represent node and bus owners. Use existing runtime id
  types where appropriate for this first slice.
- `SlotRef` should contain owner + slot only.
- `ValueRef` should contain `SlotRef + ValuePath`, but document it as nested
  value addressing rather than as the binding/version boundary.
- Clean stale `NodeLoc` names in docs/errors/tests.
- If `PropNamespace` remains because removing it is too large for this phase,
  stop exporting it from `lpc-model` and ensure docs call it a temporary
  convention helper, not semantic direction.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-source
```
