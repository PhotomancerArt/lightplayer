# Architecture Design: LP-Core Runtime

## File Structure

```
lp-core/src/
├── nodes/
│   ├── id.rs                    # NEW: TextureId, OutputId, ShaderId, FixtureId
│   ├── output/
│   │   ├── mod.rs
│   │   ├── config.rs           # OutputNodeConfig (renamed from output.rs)
│   │   └── runtime.rs          # OutputNodeRuntime
│   ├── texture/
│   │   ├── mod.rs
│   │   ├── config.rs           # TextureNodeConfig (renamed)
│   │   └── runtime.rs          # TextureNodeRuntime
│   ├── shader/
│   │   ├── mod.rs
│   │   ├── config.rs           # ShaderNodeConfig (renamed)
│   │   └── runtime.rs          # ShaderNodeRuntime
│   └── fixture/
│       ├── mod.rs
│       ├── config.rs           # FixtureNodeConfig (renamed, add texture_id)
│       └── runtime.rs          # FixtureNodeRuntime
├── util/
│   └── texture.rs              # NEW: Texture core abstraction (low-level)
├── runtime/
│   ├── lifecycle.rs             # NodeLifecycle trait
│   ├── frame_time.rs            # FrameTime struct
│   └── contexts.rs              # InitContext, RenderContext
├── project/
│   ├── config.rs                # ProjectConfig, Nodes
│   └── runtime.rs               # ProjectRuntime
└── builder.rs                   # NEW: ProjectBuilder
```

## Types Summary

```
nodes/id.rs
  #[derive(Serialize, Deserialize)]
  #[serde(transparent)]  # Serializes as u32 (becomes string in JSON)
  TextureId(u32)

  #[derive(Serialize, Deserialize)]
  #[serde(transparent)]
  OutputId(u32)

  #[derive(Serialize, Deserialize)]
  #[serde(transparent)]
  ShaderId(u32)

  #[derive(Serialize, Deserialize)]
  #[serde(transparent)]
  FixtureId(u32)

  From/Into conversions for u32

util/texture.rs
  Texture { width, height, format: String, data: Vec<u8> }

  Texture::new(width: u32, height: u32, format: String) -> Texture
    # Allocates buffer: Vec::with_capacity(width * height * bytes_per_pixel(format))
    # Initializes buffer to zeros
    # Validates format string

  Methods:
    format() -> &str
    bytes_per_pixel() -> usize  # Derives from format string
    get_pixel(x, y) -> Option<[u8; 4]>
    set_pixel(x, y, color: [u8; 4])  # Writes based on format (RGB8=first 3 bytes, R8=first byte, etc)
    sample(u, v) -> Option<[u8; 4]>
    compute_all<F>(f: F) where F: Fn(u32, u32) -> [u8; 4]

runtime/lifecycle.rs
  trait NodeLifecycle {
    type Config;
    type RenderContext;
    fn init(&mut self, config: &Self::Config, ctx: &InitContext) -> Result<(), Error>
    fn update(&mut self, ctx: &Self::RenderContext) -> Result<(), Error>
    fn destroy(&mut self) -> Result<(), Error>
  }

  // Each node type implements with its specific context:
  // TextureNodeRuntime: RenderContext = TextureRenderContext
  // ShaderNodeRuntime: RenderContext = ShaderRenderContext
  // FixtureNodeRuntime: RenderContext = FixtureRenderContext
  // OutputNodeRuntime: RenderContext = OutputRenderContext

runtime/frame_time.rs
  FrameTime { delta_ms: u32, total_ms: u32 }

runtime/contexts.rs
  InitContext<'a> { project_config: &'a ProjectConfig }
    Methods: get_texture_config, get_shader_config, etc.

  Time {
    delta_ms: u32,
    total_ms: u32,
  }

  ShaderRenderContext<'a> {
    time: FrameTime,
    textures: &'a mut HashMap<TextureId, TextureNodeRuntime>,
  }
    Methods:
      get_texture_mut(texture_id: TextureId) -> Option<&mut Texture>

  FixtureRenderContext<'a> {
    time: FrameTime,
    textures: &'a HashMap<TextureId, TextureNodeRuntime>,
    outputs: &'a mut HashMap<OutputId, OutputNodeRuntime>,
  }
    Methods:
      get_texture(texture_id: TextureId) -> Option<&Texture>
      get_output_mut(output_id: OutputId) -> Option<&mut OutputNodeRuntime>

  OutputRenderContext {
    time: Time,
    // No access to other nodes needed
  }

  TextureRenderContext {
    time: Time,
    // No access to other nodes needed
  }

nodes/*/config.rs
  OutputNodeConfig { ... }  # Uses OutputId where appropriate
  TextureNodeConfig { ... }  # No ID references
  ShaderNodeConfig { texture_id: TextureId, ... }  # Type-safe ID
  FixtureNodeConfig { output_id: OutputId, texture_id: TextureId, ... }  # Type-safe IDs, adds texture_id field

nodes/*/runtime.rs
  OutputNodeRuntime {
    handle: Option<Box<dyn LedOutput>>,  # HAL-style LED hardware access
    pixel_count: usize,  # From OutputNodeConfig.count
    bytes_per_pixel: usize,  # Derived from OutputNodeConfig (chip type: "ws2812" = 3, etc.) - stored for convenience
    buffer: Vec<u8>,  # Pixel buffer (written by fixtures, read by update())
    status
  }
    Methods: buffer_mut() -> &mut [u8]  # Provides mutable access to buffer for fixtures

    # init() derives bytes_per_pixel from config.chip type (or explicit format if added)
    # Allocates buffer: Vec::with_capacity(pixel_count * bytes_per_pixel), initialized to zeros

  TextureNodeRuntime { texture: Texture, status }
    Methods: texture() -> &Texture, texture_mut() -> &mut Texture

  ShaderNodeRuntime {
    executable: Option<Box<dyn GlslExecutable>>,
    texture_id,
    status
  }
    # Shader main signature: vec4 main(vec2 fragCoord, vec2 outputSize, float time)
    # Validated during init() - compilation fails if signature doesn't match

  FixtureNodeRuntime { output_id, texture_id, kernel: SamplingKernel, channel_order, status }

  SamplingKernel {
    radius: f32,                    // Normalized sampling radius (same for all pixels)
    samples: Vec<SamplePoint>,       // Precomputed sample points (relative to center)
  }

  SamplePoint {
    offset_u: f32,    // Offset from center in U direction (normalized)
    offset_v: f32,    // Offset from center in V direction (normalized)
    weight: f32,      // Weight for this sample (for averaging)
  }

  All implement NodeLifecycle

project/runtime.rs
  ProjectRuntime {
    uid: String,
    frame_time: FrameTime,  # Tracks delta_ms and total_ms
    # No config field - not needed after init
    textures: HashMap<TextureId, TextureNodeRuntime>,
    shaders: HashMap<ShaderId, ShaderNodeRuntime>,
    fixtures: HashMap<FixtureId, FixtureNodeRuntime>,
    outputs: HashMap<OutputId, OutputNodeRuntime>,
    # No separate RuntimeNodes - runtime instances are source of truth
  }
    Methods:
      init(output_provider: &dyn OutputProvider) -> Result<(), Error>
      update(delta_ms: u32) -> Result<(), Error>
      # Creates type-specific contexts and calls node.update() with appropriate context
      get_runtime_nodes() -> RuntimeNodes  # Derived from runtime instances for serialization
      set_status(node_type, node_id, status)  # Updates status in runtime instance
      get_status(node_type, node_id) -> Option<&NodeStatus>  # Reads from runtime instance

  trait OutputProvider {
    fn create_output(&self, config: &OutputNodeConfig) -> Result<Box<dyn LedOutput>, Error>
    # Creates and configures LED hardware based on config
    # For GpioStrip: configures GPIO pin (from config.gpio_pin), sets up chip driver (ws2812, etc.)
    # Returns owned LedOutput instance for lifecycle management
    # LedOutput trait remains simple (HAL-style) - setup is handled by provider
  }

  # LedOutput trait (from traits/led_output.rs) - HAL-style LED hardware access
  # This is for built-in LED hardware. Future outputs (UDP, etc.) will have different traits.
  # Methods: write_pixels(&mut self, pixels: &[u8]), get_pixel_count()

  enum NodeStatus { Ok, Error { status_message: String } }  # Warn removed

builder.rs
  ProjectBuilder {
    uid, name, next_id, nodes
  }
    Methods:
      new() -> Self
      with_uid(&mut self, uid: String) -> &mut Self
      with_name(&mut self, name: String) -> &mut Self
      add_texture(&mut self, config: TextureNodeConfig) -> TextureId
      add_shader(&mut self, texture_id: TextureId, config: ShaderNodeConfig) -> ShaderId
      add_output(&mut self, config: OutputNodeConfig) -> OutputId
      add_fixture(&mut self, output_id: OutputId, texture_id: TextureId, config: FixtureNodeConfig) -> FixtureId
      build(self) -> Result<ProjectConfig, Error>
```

## Design Details

### Type-Safe IDs

All node references use newtype wrappers (`TextureId`, `OutputId`, etc.) instead of raw `u32` for compile-time type safety. IDs use `#[serde(transparent)]` to serialize as `u32` (which becomes a string in JSON). IDs implement `From<u32>` and `Into<u32>` for conversion. Configs use type-safe IDs directly - no conversion needed during init.

### Texture Abstraction

The `Texture` struct in `util/texture.rs` is a low-level utility for managing pixel buffers. It provides:

- Fixed-size buffer (not resizable)
- Format metadata (RGB8, RGBA8, R8)
- Format query methods (`format()`, `bytes_per_pixel()`)
- Sampling methods (get_pixel, sample with normalized coordinates)
- Helper methods like `compute_all` for batch operations
- `set_pixel()` writes based on format: shaders always return vec4 (RGBA), but only relevant bytes are written (RGB8=first 3 bytes, R8=first byte, RGBA8=all 4 bytes)

This will eventually move to `lp-builtins` as part of the core GLSL system.

### Node Lifecycle

All node runtimes implement `NodeLifecycle` trait with:

- `init()`: Initialize from config, validate dependencies (including shader signature validation), allocate resources, compile shaders
- `update()`: Update state using type-specific render context
- `destroy()`: Cleanup resources (called when unloading entire project)

For now, we only support unloading/reloading the whole project. `ProjectRuntime` calls `destroy()` on all nodes when being replaced. Future: per-node updates will be supported later.

The trait uses two associated types:

- `Config`: The config type for this node
- `RenderContext`: The specific render context type this node needs

This ensures type safety - each node can only access what it needs, and the compiler enforces this.

### Contexts

**InitContext**: Provides read-only access to project config during initialization. Used for dependency validation.

**Type-Specific Render Contexts**: Each node type has its own render context with only the access it needs:

- **TextureRenderContext**: Only timing (no other node access needed)
- **ShaderRenderContext**: Timing + mutable access to textures (for writing rendered pixels)
- **FixtureRenderContext**: Timing + read-only access to textures + mutable access to outputs
- **OutputRenderContext**: Only timing (no other node access needed)

This approach:

- Provides type safety (each node can only access what it needs)
- Avoids borrow checker issues (contexts are created per-node with appropriate borrows)
- Makes dependencies explicit in the type system

### Node Runtimes

- **TextureNodeRuntime**: Wraps a `Texture` instance. `init()` creates texture via `Texture::new()` with config size and format, initializing buffer to zeros.
- **ShaderNodeRuntime**: Stores compiled `Box<dyn GlslExecutable>` (None if compilation failed). Shader main signature: `vec4 main(vec2 fragCoord, vec2 outputSize, float time)`. During `init()`, validates GLSL has matching signature before compilation. During `update()`, iterates over all texture pixels, calls shader with pixel coordinates, texture size, and `frame_time.total_ms` as time, writes result via `texture.set_pixel()`. Note: `set_pixel()` abstraction is slower than optimized pointer-based code, but shader call overhead is much larger, so acceptable for now. Shaders currently only write to textures (no texture reading/sampling) - texture sampling will be added later when GLSL compiler supports it.
- **FixtureNodeRuntime**: Precomputes one `SamplingKernel` in `init()` (reused for all mapping points), samples textures and writes to outputs in `update()` via `FixtureRenderContext` (which provides mutable access to outputs). Each mapping point uses the same kernel but at its own center position.
- **OutputNodeRuntime**: Holds firmware-specific `LedOutput` (HAL-style LED hardware access) and pixel buffer. `init()` derives `bytes_per_pixel` from config chip type (e.g., "ws2812" = 3 bytes RGB) and allocates buffer (`pixel_count * bytes_per_pixel`). `OutputProvider.create_output()` sets up hardware based on config (for `GpioStrip`: configures GPIO pin from `config.gpio_pin`, sets up chip driver). `LedOutput` trait remains simple (HAL-style) - setup is handled by provider. Fixtures write to buffer via `FixtureRenderContext.get_output_mut().buffer_mut()` which returns `&mut [u8]`. Multiple fixtures can write to the same output (valid use case - fixtures can be strung together). Each fixture writes to specific channels/pixels based on its mapping. For now, no overlap validation - if mappings overlap, later fixtures overwrite earlier ones. Future: could add validation to ensure mappings don't overlap. `update()` reads buffer and calls `handle.write_pixels()` to send to hardware (ESP32) or update UI (host). Note: `LedOutput` is for built-in LED hardware; future outputs (UDP, etc.) will have different traits.

### Project Runtime

`ProjectRuntime` manages the lifecycle of all nodes:

- `init()`: Initializes nodes in order (textures → shaders → fixtures → outputs), allows partial failures
- `update(delta_ms)`: Updates nodes in hard-coded order (shaders → fixtures → outputs), updates `frame_time.total_ms`. Creates appropriate type-specific contexts with `FrameTime` struct. Creates appropriate type-specific contexts for each node:
  - Shaders get `ShaderRenderContext` with mutable texture access (for writing rendered pixels)
  - Fixtures get `FixtureRenderContext` with read-only texture access and mutable output buffer access (write pixel data)
  - Outputs get `OutputRenderContext` with no other node access. `OutputNodeRuntime.update()` reads its buffer and calls `handle.write_pixels()` to send to hardware/UI
- `destroy()`: Calls `destroy()` on all nodes in reverse order (outputs → fixtures → shaders → textures) when unloading entire project. For now, only whole-project unloading is supported; per-node updates will be added later.
- Runtime instances are the source of truth for status. `get_runtime_nodes()` derives `RuntimeNodes` from runtime instances for serialization.

### Project Builder

Fluent API for constructing test projects:

- Auto-generates IDs
- Methods return IDs for linking (e.g., `add_shader(texture_id)`)
- Validates at `build()` time
- Returns `Result<ProjectConfig, Error>`

### Error Handling

- Removed `Warn` status, only `Ok`/`Error`
- Lifecycle methods return `Result<(), Error>`
- Errors update `NodeStatus` in runtime
- Partial failures allowed - project can init even if some nodes fail
- **Error Cascading**: If a node's dependency fails (e.g., fixture's texture is missing), the node's `update()` returns `Err` and sets status to `Error`. Output buffers keep previous frame's values (graceful degradation). Project continues running.
