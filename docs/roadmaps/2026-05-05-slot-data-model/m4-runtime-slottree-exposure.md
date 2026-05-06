# Milestone 4: Runtime SlotTree Exposure

## Title And Goal

Expose node config, params, and state through slot trees at runtime.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m4-runtime-slottree-exposure/`

## Scope

In scope:

- Add runtime access surfaces for node-owned `SlotTree`s.
- Expose at least one node's config/state through the new model.
- Represent resource references in runtime state as `ModelValue::Resource`.
- Keep existing resolver and produced-slot behavior working.
- Add compatibility projection where needed for existing tests/client code.

Out of scope:

- Full generic wire sync.
- Artifact mutation APIs.
- Replacing every node in one step.
- Removing all legacy state projection.

## Key Decisions

- Runtime slot trees are observed data surfaces; mutation semantics can wait.
- Resource payload bytes remain separate from resource refs.
- Rich slot data must not be treated as shader ABI data without explicit
  conversion.

## Deliverables

- Runtime slot-tree access API.
- One or two node runtime examples using it.
- Tests for config/state/resource-ref exposure.
- Documentation of the boundary between slot data and shader values.

## Dependencies

- Milestone 3 node config slice.

## Execution Strategy

Full plan. This milestone crosses engine runtime APIs and compatibility
projection.

