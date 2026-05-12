#### TOML Owns Shader Slot Shape

- **Decision:** Compute shader consumed and produced slot shapes are authored in TOML.
- **Why:** The domain model, UI, wire sync, and versioning need one authoritative shape source.
- **Rejected alternatives:** infer shape from GLSL; duplicate shape in TOML and GLSL manually.
- **Revisit when:** GLSL annotation support becomes compelling enough to generate TOML safely.

#### Generated Shader Header Region

- **Decision:** The UI may regenerate a bounded shader header region from TOML slot definitions.
- **Why:** This keeps shader source ergonomic without making GLSL the source of truth.
- **Rejected alternatives:** require users to hand-maintain all structs/globals; parse all source declarations as authoritative.

#### Serial Compute First

- **Decision:** First compute shaders run once per tick/frame as serial data programs.
- **Why:** It validates typed dataflow and shader ABI without GPU workgroup complexity.
- **Rejected alternatives:** full GPU compute semantics; hardcoded native-only emitter generation.
- **Revisit when:** wgpu abstraction work needs parallel dispatch.

#### Fluid Proves Compute Usefulness

- **Decision:** Fluid is the first major consumer of compute shader output.
- **Why:** Fluid needs rich emitter data and pressures the domain more honestly than visual-only shaders.
- **Rejected alternatives:** hardcode emitters in the fluid node as the main path.

#### No ComputeProduct Initially

- **Decision:** Compute shader outputs are produced slot values at first, not lazy product handles.
- **Why:** Emitter data is small and needed every fluid tick, so direct produced slots are simpler.
- **Rejected alternatives:** introduce `ComputeProduct` immediately.
- **Revisit when:** compute outputs become lazy, large, or expensive to materialize.

