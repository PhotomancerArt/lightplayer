# Milestone 6: Runtime Node Slot Roots

## Title And Goal

Expose runtime node state, params, and outputs as canonical slot roots.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m6-runtime-node-slot-roots/`

## Scope

In scope:

- Add runtime `state`, `params`, and `output` slot roots where each node has real data to expose.
- Replace old projection hooks such as shader/fixture compatibility projection with slot-root equivalents where practical.
- Move runtime produced slot identity toward `SlotPath`.
- Ensure dynamic shader params can carry per-instance shape updates through the canonical registry/sync path.
- Add tests for runtime root full sync and incremental changes.

Out of scope:

- Rewriting resolver binding semantics unless required by runtime slot identity cleanup.
- Client-driven mutation.
- Final resource sync cleanup beyond what runtime roots require.

## Key Decisions

- Source roots proved the sync model; runtime roots must now use the same vocabulary.
- Consumption still goes through resolver/binding machinery, while produced/owned runtime data is exposed as slot roots.
- Dynamic param shapes are first-class registry changes.

## Deliverables

- Runtime nodes expose canonical slot roots for the data they own.
- Canonical sync can watch runtime roots without legacy detail projection.
- Tests cover source plus runtime root sync together.

## Dependencies

- Milestone 5 generic debug UI rebuild.

## Execution Strategy

Full plan. Runtime roots cross engine/node/resolver/resource boundaries and should be staged node by node.
