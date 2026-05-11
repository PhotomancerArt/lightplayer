# Phase 2: Node-owned resource init

## Scope of phase

Replace M4 loader-side resource allocation with node-owned resource creation
during init/attachment.

In scope:

- Add a narrow resource init context for core nodes.
- Let `ShaderNode`, `FixtureNode`, and `OutputNode` allocate owned resources.
- Update `CoreProjectLoader` so it no longer manually knows each node's resource
  allocation details.
- Preserve existing render and output behavior.

Out of scope:

- Wire protocol changes.
- Client resource cache.
- Source reload or teardown behavior.

## Code organization reminders

- Prefer granular files and small context types.
- Public entry points first, helpers near the bottom.
- Keep node resource ownership clear and local to nodes.
- Avoid broad abstractions beyond the M4.1 need.

## Sub-agent reminders

- Do not commit.
- Stay strictly within this phase.
- Do not suppress warnings or weaken tests.
- If borrow/ownership constraints force a larger design change, stop and report.
- Report changed files, validation output, and deviations.

## Implementation details

Read:

- `00-notes.md` Q25-Q27.
- `00-design.md` "Node-owned resource initialization".
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/nodes/core/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/core/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/core/output_node.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

Design target:

- Introduce a `NodeResourceInitContext` or equivalent that has mutable access to
  `RenderProductStore` and `RuntimeBufferStore`.
- The context should provide narrow methods like:
  - `insert_render_product(Box<dyn RenderProduct>) -> RenderProductId`
  - `insert_runtime_buffer(Versioned<RuntimeBuffer>) -> RuntimeBufferId`
- Core nodes should own the decision to allocate their resources.

Expected node changes:

- `ShaderNode` allocates its render product id during init, using a placeholder
  texture product sized from its texture config or a safe empty placeholder.
- `OutputNode` allocates its output channel buffer during init.
- `FixtureNode` allocates a fixture-colors buffer during init and receives or
  resolves the output sink id via constructor/config as needed.

If the current `Node` trait cannot support init cleanly, add a small hook with a
default no-op implementation. Keep the hook scoped to resource initialization,
not broad lifecycle redesign.

Update tests so authored project loading still attaches concrete nodes and
demand roots, but resource ids are produced by nodes rather than hard-coded in
the loader.

## Validate

Run:

```bash
cargo test -p lpc-engine project_runtime
cargo test -p lpc-engine fixture
cargo test -p lpc-engine shader
cargo test -p lpc-engine output
```
