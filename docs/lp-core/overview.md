# LP Core Domain Overview

LP Core is the runtime domain model for LightPlayer projects. It connects
authored project files, a demand-driven node runtime, dynamic slot data, and
hardware output without making every node know about every other node.

The current target mental model is:

```text
ShaderNode  -> VisualProduct  -> FixtureNode
FixtureNode -> ControlProduct -> OutputNode
```

Shaders produce visual material. Fixtures consume visual material and render it
onto logical device-control samples. Outputs consume control material and map it
to hardware or protocol writes.

## Core Ideas

- [Nodes](nodes.md) own execution and runtime state.
- [Slots](slots.md) are the named, versioned data surfaces exposed by nodes and
  authored definitions.
- [Values](values.md) are opaque logical values carried inside slot leaves.
- [Bindings](bindings.md) connect consumed slots to produced slots.
- [Resources](resources.md) are registry-owned runtime objects referenced by id.
- [Products](products.md) are lazy graph values that can be materialized on
  demand.
- [Probes](probes.md) are explicit, request-scoped diagnostics over runtime
  values, products, and systems.

## Why This Shape

LightPlayer runs on embedded targets, so duplicated frame-sized data is costly.
The runtime therefore separates logical flow from materialization:

- A slot can carry a lightweight product handle.
- A downstream node can request materialization only when needed.
- The requesting node can own the destination buffer.
- The producer renders into that buffer through the engine session.

This preserves a clean dataflow graph while still supporting low-copy,
on-demand rendering.

## Authored And Runtime Data

Project files describe node definitions and bindings. At runtime, nodes read
their authored definitions through resolver-backed slot views rather than
directly poking config structs. That matters because bindings can override
consumed values.

Runtime nodes expose their own slot roots for state and outputs. Those roots are
the source of truth for produced values such as `VisualProduct` and
`ControlProduct`.
