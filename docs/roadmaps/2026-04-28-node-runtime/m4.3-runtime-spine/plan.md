# M4.3 — Runtime spine

**Status:** Phases 01–07 landed. The engine-side spine lives **next to** the
legacy `ProjectRuntime` / `LegacyNodeRuntime` path; storage and node cutover
are **M5** ([`../m5-node-spine-cutover.md`](../m5-node-spine-cutover.md)).

## What landed (by phase)

| Phase | Focus |
| --- | --- |
| [01](01-add-node-contracts.md) | `node`: `Node`, contexts, `NodeError`, `PressureLevel` |
| [02](02-add-artifact-manager.md) | `artifact`: `ArtifactManager`, `ArtifactRef`, state machine |
| [03](03-extend-spine-node-entry.md) | `tree::NodeEntry`: `SrcNodeConfig`, `ArtifactRef`, `ResolverCache` |
| [04](04-implement-resolver-context-and-cascade.md) | Resolver cascade, `ResolverContext`, `ResolveError` |
| [05](05-wire-tick-context.md) | `TickContext` wired to resolver, bus, artifact frame |
| [06](06-source-artifact-loader-orchestration.md) | `load_source_artifact` → `lpc_source::load_artifact` |
| [07](07-runtime-spine-integration-tests.md) | Integration tests (`runtime_spine`, module tests) |

References: [`00-design.md`](00-design.md), [`00-notes.md`](00-notes.md),
[`summary.md`](summary.md),
[`../design/02-node.md`](../design/02-node.md),
[`../design/03-artifact.md`](../design/03-artifact.md),
[`../design/04-config.md`](../design/04-config.md),
[`../design/06-bindings-and-resolution.md`](../design/06-bindings-and-resolution.md).

## Naming

- New contracts: `lpc-engine::node` (`src/node/`). Legacy runtimes:
  `LegacyNodeRuntime` in `src/nodes/`.
- Use `RuntimePropAccess`, `SrcNodeConfig`, `SrcArtifactSpec`, `LpsValueF32`.
  No `PropAccess` compatibility alias.
- `#[derive(RuntimePropAccess)]` / `lpc-derive` remain future work.

## Follow-on

- **M4.4:** produced-prop wire deltas, `lpc-view` mirror — [`../m4.4-domain-sync/plan.md`](../m4.4-domain-sync/plan.md).
- **M5:** port legacy nodes and cut `ProjectRuntime` to the spine tree.

## Still out of scope for M4.3

- `ProjectRuntime` map → `NodeTree` cutover; retiring `LegacyNodeRuntime`.
- Wire/view `PropsChanged` and client prop caches (M4.4).
- `ProjectDomain` generic runtime (unless a later milestone introduces it).
