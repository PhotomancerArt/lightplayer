## Tree-owned resource slots

- **Idea:** Move node-owned resource slot identity into `NodeTree` / `NodeEntry`
state so replacement runtime payloads can bind to existing runtime buffers and
render products during same-node reloads.
- **Why not now:** M4.2 only needs reload correctness; preserving resource ids
during reload is nice-to-have and would add cross-cutting lifecycle design.
- **Useful context:** `NodeResourceInitContext` currently allocates ids during
`Node::init_resources`, while `RenderProductStore` and `RuntimeBufferStore`
already support replacing existing ids.

## Child lifecycle reload

- **Idea:** Treat reload as node-scoped source reconciliation where a changed
  node owns creating, destroying, or reloading lifecycle children (`Input`,
  `Sidecar`, `Inline`) when its config/artifact shape changes.
- **Why not now:** M4.2 is still on the legacy `/src/*.kind` bridge and does not
  have long-term child authoring/reload implemented yet.
- **Useful context:** Existing node-runtime notes describe all child kinds as
  `NodeId`s in the parent's `children` list; data-flow dependents should observe
  prop/resource version changes rather than being rebuilt.