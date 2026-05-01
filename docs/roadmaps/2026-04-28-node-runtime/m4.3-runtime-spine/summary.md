### What was built

- **`node`:** Object-safe `Node` trait with `tick`, `destroy`, `handle_memory_pressure`, and `props() -> &dyn RuntimePropAccess`; focused `NodeError` and `PressureLevel`; tick/destroy/memory-pressure contexts including `TickContext` wired to resolver cache, `SrcNodeConfig`, artifact content frame, bus reads, and slot resolution.
- **`artifact`:** Generic `ArtifactManager<A>` with refcounting, `Resolved → Loaded` transitions, idle/error states, `content_frame` bumps, and `ArtifactRef` handles; `load_source_artifact` bridges to `lpc_source::load_artifact` without coupling the manager to `ProjectDomain`.
- **`tree`:** Spine `NodeEntry` carries `SrcNodeConfig`, `ArtifactRef`, and `ResolverCache` alongside existing lifecycle fields (legacy `ProjectRuntime` storage unchanged).
- **`resolver`:** Three-layer cascade (instance overrides → artifact bind → default), `SrcBinding` literal/bus/node-prop handling with outputs-only `NodeProp` dereference via target `RuntimePropAccess`, `ResolveError`, and `ResolverContext` facade.
- **Tests:** Phase/module tests plus integration-style `runtime_spine` coverage; legacy `LegacyNodeRuntime` remains exported beside `Node`.

### Decisions for future reference

#### M4.3 stages spine beside legacy; M5 cuts over

- **Decision:** Ship the new contracts, artifact manager, resolver, and spine `NodeEntry` fields without replacing `ProjectRuntime`’s legacy node map or porting `Texture` / `Shader` / etc. to `Node` in this milestone.
- **Why:** Preserves working app/tests while proving the spine under unit and integration tests; cutover risk is isolated to M5 with an explicit conformance strategy.
- **Rejected alternatives:** Replacing storage immediately (high blast radius); splitting artifact/resolver across more milestones without runnable integration.
- **Revisit when:** Planning or executing `m5-node-spine-cutover.md`.

#### Artifact manager stays generic and closure-loaded

- **Decision:** `ArtifactManager<A>` owns state and refcounts; loading uses caller-supplied closures and `load_source_artifact` maps failures into `ArtifactError` — no `ProjectDomain` trait in M4.3.
- **Why:** Keeps engine orchestration testable and defers domain abstraction until sync/cutover milestones need it.
- **Rejected alternatives:** Hard-wiring filesystem or domain types into the manager; placeholder managers without real transitions.
- **Revisit when:** M4.4 sync adapters or M5 load paths need richer domain hooks.

#### M4.3 owns engine-side props; M4.4 owns wire/view mirror

- **Decision:** Produced values exposed through `RuntimePropAccess` and consumed by `NodeProp` resolution live in M4.3; wire deltas (`PropsChanged`-style), `lpc-view` prop caches, and client mirroring stay M4.4.
- **Why:** Separates value semantics and resolution from protocol and UI projection.
- **Rejected alternatives:** Pushing resolver-only concerns into M4.4; implementing wire deltas before engine resolution exists.
- **Revisit when:** Defining M4.4 produced-prop delta shapes and apply paths.

#### `node/` vs `nodes/` split

- **Decision:** New spine contracts live under `lpc-engine/src/node/`; legacy runtime implementations and `LegacyNodeRuntime` remain under `src/nodes/`.
- **Why:** Clear boundary during side-by-side operation; grep and module paths reflect legacy vs new without compatibility aliases.
- **Rejected alternatives:** Reusing `nodes/` for both; renaming legacy modules wholesale in M4.3.
- **Revisit when:** M5 removes `LegacyNodeRuntime` after ports complete.
