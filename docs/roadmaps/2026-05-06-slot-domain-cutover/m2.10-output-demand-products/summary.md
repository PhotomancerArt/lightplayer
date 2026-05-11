# Summary

## What was built

- Used the developer-facing domain docs under `docs/lp-core/` as the phase 0 reference for nodes, slots, values, bindings, resources, products, and the shader -> fixture -> output flow.
- Finished the public graph-value rename from render product to `VisualProduct`, while keeping concrete materialized texture payloads named `TextureRenderProduct`.
- Added `ControlProduct`, `ControlExtent`, `ControlProductSlot`, and control-product value/type/editor support in `lpc-model`.
- Added engine control rendering dispatch: `ControlNode`, `ControlRenderContext`, `ControlRenderRequest`, `ControlRenderTarget`, and `ControlLayout`.
- Reworked fixture/output flow so fixtures publish a control product and outputs are demand roots that ask fixtures to render into output-owned `unorm16` samples.
- Removed `FixtureDef.output_loc` from the active flow; output input and fixture output now use authored `BindingDefs`.
- Updated `examples/basic` and project-builder output/fixture bindings to use the new bus-shaped visual/control flow.
- Updated wire/view/client resource naming from render-product payloads to visual-product payloads.

## Decisions for future reference

#### Visual Versus Texture

- **Decision:** `VisualProduct` is the logical graph value; `TextureRenderProduct` is only a materialized texture payload.
- **Why:** The graph should describe visual material independently of whether it is sampled, rendered to texture, or rendered another way later.
- **Rejected alternatives:** Keep `RenderProduct` as the public graph name; rename every concrete texture payload too.
- **Revisit when:** We add non-texture visual materialization paths.

#### Control Product Boundary

- **Decision:** Fixture -> output uses `ControlProduct`, not DMX terminology.
- **Why:** The core product is logical device-control data in `unorm16` samples; protocol details like DMX/E1.31/Art-Net belong below output mapping.
- **Rejected alternatives:** `DmxProduct`, `ChannelProduct`.
- **Revisit when:** We add protocol-specific output adapters or non-lighting device classes.

#### Output As Demand Root

- **Decision:** Outputs are demand roots; fixtures produce control products and render on request.
- **Why:** Hardware outputs own the final buffer and decide when/where materialized control data is needed.
- **Rejected alternatives:** Fixture pushes directly into an output `RuntimeBufferId`.
- **Revisit when:** We introduce a separate control-mapping layer between fixtures and outputs.

#### Transitional Runtime Buffer Mirror

- **Decision:** `OutputNode` currently mirrors its output-owned `unorm16` samples into the existing output `RuntimeBuffer` for provider flushing.
- **Why:** This preserves the current `RuntimeServices` provider API while moving graph demand in the right direction.
- **Rejected alternatives:** Rewrite output providers in the same milestone.
- **Revisit when:** Output providers can accept an output-owned control target directly.
