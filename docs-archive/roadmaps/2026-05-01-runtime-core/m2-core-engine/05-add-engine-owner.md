## Scope of Phase

Add the first `Engine` owner for the new runtime spine.

`Engine` should own the generic node tree, binding registry, resolver, artifact
manager, frame state, and output capability placeholders needed for M2. It
should drive demand roots inside a `ResolveSession` and mediate node-backed
production for `QueryKey::NodeOutput`.

Out of scope:

- Do not replace or change `LegacyProjectRuntime`.
- Do not port concrete legacy node runtimes.
- Do not implement source/project loading.
- Do not add render products.
- Do not add UI/wire sync.

Suggested sub-agent model: `gpt-5.5`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public items and entry points near the top, helpers below them, and
  `#[cfg(test)] mod tests` at the bottom of Rust files.
- Keep related functionality grouped together.
- Any temporary code must have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Create:

```text
lp-core/lpc-engine/src/engine/
├── mod.rs
├── engine.rs
└── engine_error.rs
```

Export from `lp-core/lpc-engine/src/lib.rs`:

- `pub mod engine;`
- re-export `Engine` and `EngineError`

Initial `Engine` ownership shape:

```rust
pub struct Engine {
    frame_id: FrameId,
    frame_time: FrameTime,
    tree: NodeTree<Box<dyn Node>>,
    bindings: BindingRegistry,
    resolver: Resolver,
    artifacts: ArtifactManager<()>, // use a placeholder payload if no concrete artifact exists yet
    demand_roots: Vec<NodeId>,
}
```

Adjust `ArtifactManager<()>` if a better existing artifact payload type is
available and does not create scope creep. The important point is ownership of
the manager, not loading real artifacts in M2.

Add basic methods:

- `new(root_path: TreePath) -> Self`
- accessors for `frame_id`, `bindings`, `bindings_mut` if needed by tests
- methods to add/register nodes for tests or later builder use
- `add_demand_root(node: NodeId)`
- `tick(delta_ms: u32) -> Result<(), EngineError>`

Implement `ResolveHost` for `Engine`:

- For `QueryKey::NodeOutput { node, output }`, ensure the producer node runs at
  most once for the current frame, then read the requested output from
  `node.props()`.
- If the value is missing, return a structured error.
- Use `Versioned<LpsValueF32>` for returned values. If existing `RuntimePropAccess`
  returns `(LpsValueF32, FrameId)`, wrap that in `Versioned::new(frame, value)`.

Producer run tracking can live in `ResolverCache`, `ResolveTrace`, or an
engine-local per-frame set. Keep the invariant visible in tests: a producer
demanded more than once in a frame runs once.

Borrowing warning:

The engine owns both the tree and resolver. Avoid unsafe. If borrowing the
resolver session and mutable node storage at the same time is difficult, prefer
temporarily taking the node state out of the tree, ticking it, then putting it
back, or use a small internal host adapter. Do not copy the legacy runtime's
unsafe render workaround.

Tests to add:

- `Engine::new` owns frame state and an empty binding registry/resolver
- `tick` advances frame id and frame time
- demand root ticking happens inside a resolve session
- node-output production through `ResolveHost` reads `RuntimePropAccess`
- same producer demanded twice in one frame is ticked once

## Validate

Run:

```bash
cargo test -p lpc-engine engine
cargo test -p lpc-engine resolver
```

If this touches broader library exports, also run:

```bash
cargo test -p lpc-engine --lib
```
