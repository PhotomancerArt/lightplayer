# Bindings

Bindings connect consumed slots to produced slots.

A node can expose slots it produces and slots it consumes. Bindings describe
where consumed data comes from, or where produced data should be published.

## Direct And Bus Bindings

Direct bindings connect one node slot to another node slot. Bus bindings let
artifacts stay reusable by publishing to or consuming from conventional bus
slots.

Example direction:

```text
shader output -> `bus:visual_out`
fixture input <- `bus:visual_out`
fixture output -> `bus:control_1`
output input <- `bus:control_1`
```

The bus details are still evolving, but the intent is stable: authored artifacts
should not need to know every concrete neighbor in the project.

## Resolution

Nodes do not normally read consumed values directly from their authored structs.
They ask the resolver for a consumed slot. The resolver applies the nearest
binding and falls back to authored/default slot data when appropriate.

This keeps authored data, overrides, and runtime products behind one access
path.
