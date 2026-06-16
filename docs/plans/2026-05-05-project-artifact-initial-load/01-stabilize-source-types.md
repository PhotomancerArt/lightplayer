# Phase 1: Stabilize Source Type Semantics

## Scope of phase

Stabilize the already-started rename/move state before changing loader behavior. This phase is about terminology, source type shapes, rustdocs, and narrow parser semantics.

In scope:

- Update rustdocs/comments for `ArtifactSpecifier`, engine-side `ArtifactLocation`, `NodeInvocation`, `NodeDef`, `NodeLoc`, and concrete `*Def` types.
- Add/finish `ProjectDef` in `lpc-source/src/node/project/`.
- Keep concrete node bodies named `TextureDef`, `ShaderDef`, `OutputDef`, `FixtureDef`, and `ProjectDef`.
- Keep `NodeLoc` as a source string wrapper for now, but add parsing/validation helpers for the relative dot syntax described in `00-design.md`.
- Stabilize `NodeInvocation` as the artifact-only invocation shape needed by this plan, with docs clearly saying inline invocation is future work.
- Fix stale `Config`, `SrcNodeConfig`, `SrcArtifactSpec`, and `NodeSpec` wording where touched by this type work.

Out of scope:

- Do not rewrite project loading yet.
- Do not migrate examples yet.
- Do not implement absolute node paths.
- Do not implement artifact-plus-local-field merge semantics.
- Do not unify runtime prop/output access.

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

- `lp-core/lpc-source/src/artifact/artifact_specifier.rs`
- `lp-core/lpc-engine/src/artifact/artifact_location.rs`
- `lp-core/lpc-source/src/node/node_invocation.rs`
- `lp-core/lpc-source/src/node/node_def.rs`
- `lp-core/lpc-source/src/node/project/mod.rs`
- `lp-core/lpc-source/src/node/{shader,texture,output,fixture}/*_def.rs`
- `lp-core/lpc-model/src/node/node_loc.rs`
- `lp-core/lpc-model/src/node/mod.rs`
- `lp-core/lpc-source/src/lib.rs` and `lp-core/lpc-source/src/node/mod.rs`

`NodeLoc` syntax for this plan:

```text
.                  current node
.child             child of current node
.child.grandchild  descendant of current node
..                 parent
..sibling          sibling through parent
..sibling.child    sibling's child
```

Add a small parsed representation if helpful, but keep the serialized TOML form as a string. For this phase, it is acceptable for parsed helpers to validate and expose enough structure for Phase 4.

`ProjectDef` should deserialize a project artifact like:

```toml
kind = "project"
name = "basic"

[nodes.texture]
artifact = "./texture.toml"
```

Use `BTreeMap<NodeName, NodeInvocation>` or the closest existing no_std-friendly map pattern in this crate. If preserving TOML author order would require new infrastructure, do not do that now.

Update unit tests for:

- `ArtifactSpecifier` path/lib round trips still pass.
- `NodeInvocation` TOML form for `[nodes.foo] artifact = "./foo.toml"`.
- `NodeLoc` accepts valid relative dot examples and rejects slash paths, empty strings, and absolute-looking node paths.
- `ProjectDef` deserializes a minimal `kind = "project"` TOML with a named `nodes` table.

## Validate

Run focused checks first:

```bash
cargo test -p lpc-model node_loc
cargo test -p lpc-source node
```

If those target names do not match exact test filters, run:

```bash
cargo test -p lpc-model
cargo test -p lpc-source
```
