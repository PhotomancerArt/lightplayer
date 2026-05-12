# Phase 4: Resolve NodeLoc Dependencies And Attach Order

## Scope of phase

Make current core nodes attach correctly from artifact-loaded definitions by resolving `NodeLoc` values in context.

In scope:

- Resolve relative `NodeLoc` values against a parent/invocation context.
- Support the dot syntax from `00-design.md`, especially sibling refs like `..texture` and `..output`.
- Use resolved `NodeId`s when constructing `ShaderNode` and `FixtureNode`.
- Attach current core nodes in dependency-safe order for `examples/basic`: texture/output before shader/fixture; shader before fixture where fixture samples shader output.
- Add tests for successful resolution and useful errors for missing refs.

Out of scope:

- Do not add absolute node paths.
- Do not add property refs after `#`; only node-location refs before `#` are needed.
- Do not implement general graph cycle handling beyond what current load needs.

## Code Organization Reminders

- Follow the repo rule: top to bottom is most important to least important, with tests at the bottom of each Rust file.
- Prefer one concept per file and keep related functionality grouped together.
- Keep helper functions below the public/primary API they support.
- Any temporary code must have a searchable TODO comment and should be removed by the cleanup phase.
- Preserve no_std compatibility in `lpc-model`, `lpc-source`, `lpc-engine`, and shader/runtime paths. Do not add std gates to compile/execute paths.

## Codex / Worker Reminders

- Do not commit. The plan commits at the end as a single unit unless the user explicitly says otherwise.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to make the build pass. Fix the issue.
- Do not disable, skip, or weaken existing tests.
- If blocked by ambiguity or an unexpected design issue, stop and report back rather than improvising.
- Report back with: what changed, what was validated, and any deviations from this phase.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/node/node_loc.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/tree/node_tree.rs`
- `lp-core/lpc-engine/src/nodes/core/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/core/fixture_node.rs`

Resolution rules for this plan:

```text
.                  current node
.child             child of current node
..                 parent
..sibling          sibling through parent
..sibling.child    sibling's child
```

For `examples/basic`, `ShaderDef.texture` should resolve `..texture` from the shader node context to the project sibling named `texture`. `FixtureDef.texture` and `FixtureDef.output` should resolve similarly.

Implement enough attach ordering for current node kinds. A simple current-kind dependency plan is acceptable if clear and tested:

1. create all child tree entries
2. attach texture nodes
3. attach output nodes and register output sinks
4. attach shader nodes after texture refs resolve
5. attach fixture nodes after texture/output/shader refs resolve

Add tests for:

- `..texture` resolves to sibling texture.
- `.child` resolves to child.
- slash-containing values are rejected by `NodeLoc` parsing or loader resolution.
- missing node refs produce a clear load error naming the missing ref.

## Validate

```bash
cargo test -p lpc-model node_loc
cargo test -p lpc-engine project_loader
```
