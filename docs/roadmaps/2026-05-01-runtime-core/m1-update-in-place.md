# Milestone 1: Update-in-place runtime reorganization

## Goal

Reorganize the runtime-core work around an update-in-place strategy: bring the
legacy runtime and node implementations back under the core `lpc-*` direction,
then upgrade the legacy shader → fixture → output flow onto the new spine
piece by piece.

This milestone is preparatory: it reduces indirection before the new `Engine`
contract is designed in detail.

## Implemented topology (current)

The temporary `lpl-model` and `lpl-runtime` crates were **removed**. Legacy
surface now lives inside the existing `lpc-*` crates as `legacy` modules:

| Area | Location |
|------|-----------|
| Authored legacy configs / source-facing node specs | `lpc_source::legacy` |
| Legacy state, project responses, node changes/details, serializable wrappers, legacy message aliases | `lpc_wire::legacy` |
| Concrete legacy node runtimes (texture/shader/fixture/output) and legacy project integration | `lpc_engine::legacy` |
| Loader + `LegacyProjectRuntime` public name | `lpc_engine::legacy_project` |

`LegacyProjectRuntime` keeps its name and behavior; **hook registration and
`lpl_runtime::install()` are gone** — `init_nodes`, `tick`, `handle_fs_changes`,
and `get_changes` are implemented directly again.

Detailed phase notes and decisions: [`m1-update-in-place/`](m1-update-in-place/)
(including [`summary.md`](m1-update-in-place/summary.md)).

## Context (historical)

An earlier direction split legacy work into `lpl-model` / `lpl-runtime`, with a
possible separate visual stack. That added indirection while the core runtime
owner contract was still unsettled.

The adopted direction:

- Keep `lpc-model`, `lpc-source`, `lpc-wire`, `lpc-engine`, and `lpc-view` as
  the primary family.
- Use modules (`legacy`, and later visual/fixture/rig slices) for separation.
- Avoid a generic `ProjectDomain` or parallel `lpl-*` / `lpv-*` engine stacks
  until a second implementation proves the need.

## In scope (this milestone) — done

- Move legacy authored configs into `lpc-source::legacy`.
- Move legacy wire state/protocol into `lpc-wire::legacy`.
- Move concrete legacy runtimes into `lpc-engine::legacy`.
- Remove `lpl-model` and `lpl-runtime` and update imports across the workspace.
- Remove `LegacyProjectHooks`, `set_project_hooks` / `with_hooks`, and all
  `install()` registration call sites.
- Preserve `LegacyProjectRuntime` and the shader → texture → fixture → output
  compatibility slice.
- Update this roadmap to describe the layout above.

## Out of scope (unchanged)

- Final `Engine` API design.
- Pull-based bus providers.
- Queryable visual outputs.
- Porting all legacy nodes to the new `Node` trait.
- Removing or renaming `LegacyProjectRuntime`.
- New visual node types.

## Next milestone

Focus on the **runtime owner / value-resolution contract** (`Engine` shape,
demand roots, resolve semantics) without re-litigating crate topology.

## Success criteria

- Update-in-place direction is documented and matches the tree.
- Legacy split is folded into `lpc-*`; no `lpl-*` compatibility crates.
- Legacy render tests and host/firmware checks relevant to this stack still pass.
