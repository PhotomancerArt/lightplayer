# Products

Products are lazy graph values.

A product says: "this node can produce this kind of material if a downstream
consumer asks for it." It is a logical dataflow value, not necessarily an owned
buffer.

Products can contain metadata about the shape or capabilities of what can be
materialized. Such as a fixture indicating how much control data it can write,
or a visual indicating a "native" resolution.

This allows information about what can be produced to flow forwards through the
node graph, while delaying computation until needed, and with final parameters
dictated by the downstream consumer.

## VisualProduct

`VisualProduct` represents visual material produced by shaders or future visual
nodes.

Fixtures consume it and may request materialization as a texture or samples.
The producer node owns compilation, shader state, and rendering behavior.

## ControlProduct

`ControlProduct` represents logical device-control material produced by fixtures
or future device nodes (such as a fog machine or mechanical device).

Outputs consume it and request rendering into an output-owned control target.
The native LightPlayer control sample format is `unorm16`, which has enough
depth for linear color, exactly represents 8-bit data, and avoids float
operations. Future expansion may allow output nodes to control the bit depth.

`ControlProduct` advertises a preferred extent, but the output owns the actual
buffer and can choose an output-specific extent.
