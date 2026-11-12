# Nodes

Nodes are the units of runtime ownership and execution.

A node owns:

- its runtime state;
- any compiled or cached resources it needs;
- the code that ticks or materializes its products;
- the slot roots it exposes to the graph.

Node definitions are authored data. Runtime nodes are executable instances of
those definitions.

## Current Runtime Classes

- `ProjectNode` is the implied root of the loaded project.
- `ShaderNode` produces `VisualProduct`.
- `FixtureNode` consumes `VisualProduct` and produces `ControlProduct`.
- `OutputNode` consumes `ControlProduct` and writes to hardware/provider output.

## Demand Roots

Demand roots are nodes the engine ticks to drive the graph. The intended model
is that output nodes are demand roots, because hardware output is the final sink.

When an output needs data, it resolves its input binding. That may cause the
engine to tick or query upstream nodes, but the demand starts at the sink.

## Capabilities

Not every node supports every operation. A shader can render visual material; a
fixture can render control material; an output can write hardware.

These optional capabilities are modeled explicitly instead of adding every
method to every node type.
