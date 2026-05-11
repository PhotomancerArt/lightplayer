# M2.10 Output Demand Products Design

## Scope

Replace the current fixture-to-output push path with an output-demand product
path.

In scope:

- Finish the visual product terminology cleanup started in the worktree.
- Add `ControlProduct` as the graph-level product produced by fixtures/devices
  and consumed by outputs.
- Add output-owned control render targets using native `unorm16` samples.
- Make outputs demand roots in project-loaded runtime graphs.
- Remove the direct fixture -> output sink dependency from runtime node
  construction.
- Keep provider flushing in `RuntimeServices` for this milestone.

Out of scope:

- Full protocol implementations for E1.31/Art-Net/PixLite mapping.
- A separate control-mapping node/layer.
- Replacing `RuntimeServices` with a general engine service trait.
- Real client/wire UI replacement beyond names that must compile.

## File Structure

```text
lp-core/lpc-model/src/resource/
  visual_product.rs
  control_product.rs

lp-core/lpc-model/src/value/
  lp_value.rs

lp-core/lpc-model/src/slots/
  visual_product.rs
  control_product.rs

lp-core/lpc-engine/src/visual_product/
  mod.rs
  render_texture_request.rs
  sample_request.rs
  sample_result.rs
  texture_product.rs

lp-core/lpc-engine/src/control_product/
  mod.rs
  control_extent.rs
  control_layout.rs
  control_render_request.rs
  control_render_target.rs

lp-core/lpc-engine/src/node/
  render_node.rs
  control_node.rs
  node_call.rs
  contexts.rs

lp-core/lpc-engine/src/nodes/fixture/
  fixture_node.rs

lp-core/lpc-engine/src/nodes/output/
  output_node.rs

lp-core/lpc-engine/src/project_runtime/
  project_loader.rs
  runtime_services.rs
```

## Architecture Summary

The runtime graph has two product domains:

```text
ShaderNode  -> VisualProduct  -> FixtureNode
FixtureNode -> ControlProduct -> OutputNode
```

`VisualProduct` is the graph-level visual material handle. A fixture consumes a
visual product and may request visual materialization from the owning node.

`ControlProduct` is the graph-level control-material handle. It is not DMX and
it is not an owned frame. It identifies a node that can render logical control
samples into an output-owned buffer.

`OutputNode` owns the destination control buffer and is the project demand root.
On tick, it resolves its `input` binding as a `ControlProduct`, decides the
actual output extent, passes a mutable render target to the producing node, and
marks its runtime buffer changed. `RuntimeServices` then flushes dirty output
buffers through the existing `OutputProvider` plumbing.

## Main Types

```rust
pub struct ControlProduct {
    pub node: NodeId,
    pub output: u32,
    pub preferred_extent: ControlExtent,
}

pub struct ControlExtent {
    pub rows: u32,
    pub samples_per_row: u32,
}

pub enum ControlSampleFormat {
    Unorm16,
}

pub struct ControlRenderRequest {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
}

pub struct ControlRenderTarget<'a> {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
    pub samples: &'a mut [u16],
}
```

`rows` and `samples_per_row` deliberately avoid DMX universe language. Outputs
may later map rows to protocol universes, PixLite ports, GPIO strips, or other
hardware concepts.

The native LightPlayer control sample format is `unorm16`. Fixtures produce
linear/gamma-adjusted control levels at this precision. Outputs own
interpolation, dithering, and final quantization to `u8` protocols or hardware.

## Main Interactions

1. Shader tick publishes `VisualProduct` on shader runtime state `output`.
2. Fixture tick resolves its visual `input`, updates any cached mapping, and
   publishes `ControlProduct` on fixture runtime state `output`.
3. Output tick resolves its control `input`.
4. Output determines actual `ControlExtent`:
   - autosized outputs can use `product.preferred_extent`;
   - fixed outputs use their configured extent.
5. Output ensures its runtime buffer is sized for `ControlExtent * u16` samples.
6. Output calls engine/session control materialization for the product, passing
   a mutable `ControlRenderTarget<'_>` into the output-owned runtime buffer.
7. The fixture/control producer renders into that target and returns
   `ControlLayout` hints.
8. Output marks the buffer changed at the current revision.
9. `RuntimeServices` flushes dirty output buffers through the existing provider.

## Bindings

- `FixtureDef.bindings.input` remains the visual input convention.
- `OutputDef` gains `bindings.input` for the control product source.
- `FixtureDef.output_loc` should be removed in this milestone.
- `CoreProjectLoader` should add output nodes as demand roots instead of
  fixture nodes.

## Layout Hints

`ControlLayout` is metadata describing regions of the control target. It exists
for debug UI and inspection, not protocol serialization. Initial hints can stay
small:

```rust
pub struct ControlSpan {
    pub row: u32,
    pub start: u32,
    pub len: u32,
    pub hint: ControlHint,
}

pub enum ControlHint {
    RgbPixels { count: u32, color_order: ColorOrder },
    Raw,
}
```

The output may ignore layout hints at first. Tests should at least prove the
fixture can return a useful span for rendered RGB pixel data.
