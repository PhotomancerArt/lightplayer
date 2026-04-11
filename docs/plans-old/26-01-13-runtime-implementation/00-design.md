# Design: Runtime Implementation (Contexts, Texture, Fixture)

## File Structure

```
lp-model/src/nodes/fixture/
├── config.rs                         # MODIFY: Add ColorOrder enum, use in FixtureConfig
└── ...

lp-engine/src/
├── error.rs                          # MODIFY: Add WrongNodeKind variant
├── runtime/
│   └── contexts.rs                   # MODIFY: Implement InitContext, RenderContextImpl
│                                      #        Update TextureHandle/OutputHandle to wrap NodeHandle
├── nodes/
│   ├── texture/
│   │   └── runtime.rs                # MODIFY: Add Texture field, implement init(), get_state()
│   ├── fixture/
│   │   ├── sampling_kernel.rs        # NEW: SamplingKernel and SamplePoint types
│   │   └── runtime.rs                 # MODIFY: Add fields, implement init(), render()
│   └── mod.rs                        # MODIFY: Export sampling types
└── project/
    └── runtime.rs                    # MODIFY: Update InitContext/RenderContextImpl creation
                                       #        Add lazy texture rendering logic
```

## New Types and Functions

### Error Type
```rust
// error.rs
pub enum Error {
    // ... existing variants ...
    WrongNodeKind {
        specifier: String,
        expected: NodeKind,
        actual: NodeKind,
    },
}
```

### Color Order Enum (lp-model)
```rust
// lp-model/src/nodes/fixture/config.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorOrder {
    Rgb,
    Grb,
    Rbg,
    Gbr,
    Brg,
    Bgr,
    // RGB variants only, no RGBA for now
}

impl ColorOrder {
    pub fn as_str(&self) -> &'static str;
    pub fn bytes_per_pixel(&self) -> usize;  // Always 3 for RGB
    pub fn write_rgb(&self, buffer: &mut [u8], offset: usize, r: u8, g: u8, b: u8);
}

pub struct FixtureConfig {
    pub output_spec: NodeSpecifier,
    pub texture_spec: NodeSpecifier,
    pub mapping: String,  // todo!() - will be structured type later
    pub lamp_type: String,  // todo!() - will be enum later
    pub color_order: ColorOrder,  // Changed from String
    pub transform: [[f32; 4]; 4],
}
```

### Runtime Contexts
```rust
// runtime/contexts.rs
pub struct TextureHandle(NodeHandle);  // Changed from u32
pub struct OutputHandle(NodeHandle);   // Changed from u32

impl TextureHandle {
    pub fn new(handle: NodeHandle) -> Self;
    pub fn as_node_handle(&self) -> NodeHandle;
}

impl OutputHandle {
    pub fn new(handle: NodeHandle) -> Self;
    pub fn as_node_handle(&self) -> NodeHandle;
}

pub struct InitContext<'a> {
    runtime: &'a ProjectRuntime,
    node_path: &'a LpPath,  // For chroot filesystem
    node_fs: Option<Box<dyn LpFs>>,  // Cached chroot filesystem
}

impl<'a> NodeInitContext for InitContext<'a> {
    fn resolve_node(&self, spec: &NodeSpecifier) -> Result<NodeHandle, Error>;
    fn resolve_output(&self, spec: &NodeSpecifier) -> Result<OutputHandle, Error>;
    fn resolve_texture(&self, spec: &NodeSpecifier) -> Result<TextureHandle, Error>;
    fn get_node_fs(&self) -> &dyn LpFs;
}

pub struct RenderContextImpl<'a> {
    runtime: &'a mut ProjectRuntime,
    frame_id: FrameId,
}

impl<'a> RenderContext for RenderContextImpl<'a> {
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&Texture, Error>;
    fn get_output(&mut self, handle: OutputHandle, universe: u32, start_ch: u32, ch_count: u32) -> Result<&mut [u8], Error>;
}
```

### Texture Runtime
```rust
// nodes/texture/runtime.rs
use lp_shared::util::Texture;

pub struct TextureRuntime {
    texture: Texture,  // From lp-shared::util::Texture
    node_handle: NodeHandle,  // For state extraction
}

impl TextureRuntime {
    pub fn new() -> Self;
    pub fn get_state(&self) -> TextureState;  // Extract state for sync API
    pub fn texture(&self) -> &Texture;
    pub fn texture_mut(&mut self) -> &mut Texture;
}

impl NodeRuntime for TextureRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
    fn render(&mut self, ctx: &dyn RenderContext) -> Result<(), Error>;  // No-op (shaders render to textures)
}
```

### Sampling Kernel
```rust
// nodes/fixture/sampling_kernel.rs
pub struct SamplePoint {
    pub offset_u: f32,  // Relative offset in U coordinate (normalized)
    pub offset_v: f32,  // Relative offset in V coordinate (normalized)
    pub weight: f32,    // Weight for this sample
}

pub struct SamplingKernel {
    pub radius: f32,     // Normalized sampling radius
    pub samples: Vec<SamplePoint>,
}

impl SamplingKernel {
    pub fn new(radius: f32) -> Self;
    // Generates 5x5 grid of sample points in a circle
    // Uses Gaussian-like weights, normalized to sum to 1.0
}
```

### Fixture Runtime
```rust
// nodes/fixture/runtime.rs
pub struct FixtureRuntime {
    texture_handle: TextureHandle,
    output_handle: OutputHandle,
    kernel: SamplingKernel,
    color_order: ColorOrder,
    mapping: Vec<MappingPoint>,  // Simplified for now (will be structured later)
    transform: [[f32; 4]; 4],
}

// Simplified mapping point (will be replaced with structured type later)
struct MappingPoint {
    channel: u32,
    center: [f32; 2],  // UV coordinates
    radius: f32,
}

impl FixtureRuntime {
    pub fn new() -> Self;
}

impl NodeRuntime for FixtureRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
    fn render(&mut self, ctx: &dyn RenderContext) -> Result<(), Error>;
}
```

### ProjectRuntime Updates
```rust
// project/runtime.rs
impl ProjectRuntime {
    // Update initialize_nodes() to create proper InitContext with node_path and chroot fs
    // Update render() to create proper RenderContextImpl
    // Add helper: ensure_texture_rendered(handle: TextureHandle) -> Result<(), Error>
    //   - Checks if texture.state_ver < frame_id
    //   - Finds shaders targeting texture (by texture_spec in ShaderConfig)
    //   - Runs shaders in render_order
    //   - Updates texture state_ver to frame_id
    //   - Returns texture reference
}
```

## Key Design Decisions

1. **Handles wrap NodeHandle**: `TextureHandle` and `OutputHandle` wrap `NodeHandle` for type safety and easy lookup in `ProjectRuntime.nodes`.

2. **Lazy texture rendering**: Triggered in `RenderContextImpl::get_texture()` when `state_ver < frame_id`. Finds and runs shaders targeting the texture, then updates `state_ver`.

3. **Per-node filesystem**: `InitContext` uses `LpFs::chroot()` to create node-specific filesystem view. Cached in `InitContext` to avoid repeated chroot calls.

4. **Sampling kernel**: Precomputed in `FixtureRuntime::init()` based on mapping radius, reused for all mapping points. Uses 5x5 grid pattern with Gaussian-like weights.

5. **State extraction**: `TextureRuntime::get_state()` extracts `TextureState` for sync API by copying texture data.

6. **Color order enum**: `ColorOrder` enum with `write_rgb()` helper method for clean output writing. Only RGB variants supported (no RGBA for now).

7. **Transform matrix**: Applied to fixture coordinate space [-1,-1] to [1,1] to map to texture UV space [0,1] before sampling.

8. **Texture config**: Note that `TextureConfig` should be a struct (not enum) with `width`, `height`, `format` fields. Future: optional `file` property for initialization.

## Implementation Notes

- Texture rendering is a no-op - shaders render TO textures, not textures rendering themselves
- Fixture coordinate space defaults to unit square [-1,-1] to [1,1]
- Transform matrix maps fixture space to texture UV space
- Sampling uses precomputed kernel with weighted averaging
- Output writing uses `ColorOrder::write_rgb()` for clean channel ordering
- Lazy rendering ensures textures are up-to-date when accessed by fixtures
