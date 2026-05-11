# Phase 2: Implement ProjectConfig data structures

## Goal

Implement the `ProjectConfig` structure that represents the project configuration JSON.

## Tasks

1. Create `src/project/config.rs` with:
   - `ProjectConfig` struct with:
     - `uid: String`
     - `name: String`
     - `nodes: Nodes` (see below)
   - `Nodes` struct with:
     - `outputs: HashMap<u32, OutputNode>`
     - `textures: HashMap<u32, TextureNode>`
     - `shaders: HashMap<u32, ShaderNode>`
     - `fixtures: HashMap<u32, FixtureNode>`
   - Node type enums with `$type` discriminator:
     - `OutputNode` with `GpioStrip { chip: String, gpio_pin: u32, count: u32 }`
     - `TextureNode` with `Memory { size: [u32; 2], format: String }`
     - `ShaderNode` with `Single { glsl: String, texture_id: u32 }`
     - `FixtureNode` with `CircleList { output_id: u32, channel_order: String, mapping: Vec<Mapping> }`
   - `Mapping` struct: `{ channel: u32, center: [f32; 2], radius: f32 }`
2. Implement `serde::Serialize` and `serde::Deserialize` for all types
3. Handle u32 IDs as strings in JSON (custom serialization)
4. Handle `$type` discriminator field for enums
5. Export from `src/project/mod.rs`

## Success Criteria

- `ProjectConfig` can be serialized to/from JSON matching the specified format
- u32 IDs serialize as strings in JSON
- `$type` discriminator works correctly
- All code compiles without warnings

