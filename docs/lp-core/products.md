# Products

Products are lazy graph values.

A product says: "this node can produce this kind of material if a downstream
consumer asks for it." It is a logical dataflow value, not necessarily an owned
buffer.

## VisualProduct

`VisualProduct` represents visual material produced by shaders or future visual
nodes.

Fixtures consume it and may request materialization as a texture or samples.
The producer owns compilation, shader state, and rendering behavior.

## ControlProduct

`ControlProduct` represents logical device-control material produced by fixtures
or future device nodes.

Outputs consume it and request rendering into an output-owned control target.
The native LightPlayer control sample format is `unorm16`, which preserves
precision for interpolation and dithering before final hardware/protocol
quantization.

`ControlProduct` advertises a preferred extent, but the output owns the actual
buffer and can choose an output-specific extent.

## Why Products Are Handles

Products avoid pushing large data through slots. A slot can carry a lightweight
handle, and the downstream demand root can decide when and where to materialize
the data.

This is the key trick that lets LightPlayer have a clean logical graph while
still rendering into one output-owned buffer.
