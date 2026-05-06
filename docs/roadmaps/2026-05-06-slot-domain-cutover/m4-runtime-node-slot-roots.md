# Milestone 4: Runtime Node Slot Roots

## Title And Goal

Expose runtime node state, params, and outputs as slot roots from real engine nodes.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m4-runtime-node-slot-roots/`

## Scope

In scope:

- Add runtime slot-root exposure to `Node` or an adjacent runtime-node access trait.
- Convert texture/output/shader/fixture runtime state surfaces to slot roots.
- Represent shader runtime params as dynamic slot data derived from source param definitions.
- Represent produced outputs through `output` roots where appropriate.
- Replace legacy projection hooks with slot-root equivalents where practical.
- Keep `RuntimeProduct` for produced payloads that are values/resources rather than plain `ModelValue`.

Out of scope:

- Full generic UI replacement.
- Removing all legacy projection code.
- Client-driven mutation.

## Key Decisions

- Nodes own a single conceptual slot namespace, but access is direction-aware at a higher layer.
- Consumption still resolves through the resolver because bindings matter.
- Production can be exposed directly by runtime nodes.
- Resource refs should be ordinary slot leaf values; raw bytes remain resource payload sync.

## Deliverables

- Runtime nodes expose watchable `state`, `params`, and/or `output` roots.
- Project slot sync can include runtime roots for watched nodes.
- Tests prove shader params, fixture state, output resources, and texture dimensions sync generically.
- Legacy `fixture_projection_info()` and `shader_projection_wire()` have clear replacement paths.

## Dependencies

- Milestone 1 runtime `SlotPath` cleanup.
- Milestone 3 slot sync bridge.

## Execution Strategy

Full plan. This is the hardest domain cutover slice because it crosses runtime node internals, resolver behavior, resources, and sync.

