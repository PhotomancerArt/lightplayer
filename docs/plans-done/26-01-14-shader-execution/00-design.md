# Design: Shader Execution

## File Structure

```
lp-engine/src/
├── nodes/
│   └── shader/
│       └── runtime.rs                # MODIFY: Add fields, implement init(), render(), get_state()
├── runtime/
│   └── contexts.rs                   # MODIFY: Add get_texture_mut() to RenderContext trait
└── project/
    └── runtime.rs                    # MODIFY: Update ensure_texture_rendered() to execute shaders
```

## New Types and Functions

### Shader Runtime
```rust
// nodes/shader/runtime.rs
use lp_glsl_compiler::{GlslCompiler, GlslExecutable, GlslJitModule};
use lp_model::nodes::shader::{ShaderConfig, ShaderState};
use crate::runtime::contexts::{NodeInitContext, RenderContext, TextureHandle};

pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    glsl_source: Option<String>,           // Stored for state extraction
    executable: Option<Box<dyn GlslExecutable>>,  // Compiled shader
    texture_handle: Option<TextureHandle>,  // Resolved texture handle
    compilation_error: Option<String>,      // Compilation error if any
    node_handle: NodeHandle,
}

impl ShaderRuntime {
    pub fn new(node_handle: NodeHandle) -> Self;
    pub fn set_config(&mut self, config: ShaderConfig);
    pub fn get_state(&self) -> ShaderState;  // Extract state for sync API
}

impl NodeRuntime for ShaderRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
```

### Frame Time Tracking
```rust
// project/runtime.rs (or new runtime/frame_time.rs)
pub struct FrameTime {
    pub delta_ms: u32,   // Time since last frame in milliseconds
    pub total_ms: u32,   // Total time since project start in milliseconds
}

impl ProjectRuntime {
    pub fn tick(&mut self, delta_ms: u32) {  // MODIFY: Add delta_ms parameter
        self.frame_id = FrameId(self.frame_id.as_i64() + 1);
        // Update frame time
        self.frame_time.total_ms += delta_ms;
        self.frame_time.delta_ms = delta_ms;
    }
}
```

### Render Context Updates
```rust
// runtime/contexts.rs
pub trait RenderContext {
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&Texture, Error>;
    fn get_texture_mut(&mut self, handle: TextureHandle) -> Result<&mut Texture, Error>;  // NEW
    fn get_output(&mut self, handle: OutputHandle, universe: u32, start_ch: u32, ch_count: u32) -> Result<&mut [u8], Error>;
    fn get_time(&self) -> f32;  // NEW: Get current frame time in seconds (total_ms / 1000.0)
}
```

### Lazy Texture Rendering Updates
```rust
// project/runtime.rs
impl<'a> RenderContextImpl<'a> {
    fn ensure_texture_rendered(
        nodes: &mut BTreeMap<NodeHandle, NodeEntry>,
        handle: TextureHandle,
        frame_id: FrameId,
    ) -> Result<(), Error> {
        // Check if already rendered
        if let Some(entry) = nodes.get(&node_handle) {
            if entry.state_ver >= frame_id {
                return Ok(());
            }
        }
        
        // Find shaders targeting this texture
        let shader_handles = Self::find_shaders_for_texture(nodes, handle);
        
        // Sort by render_order
        let mut shader_entries: Vec<_> = shader_handles.iter()
            .filter_map(|h| nodes.get(h))
            .collect();
        shader_entries.sort_by_key(|e| {
            // Extract render_order from config (need to downcast)
            // For now, assume all shaders have render_order 0
            0
        });
        
        // Execute each shader
        for shader_entry in shader_entries {
            if let Some(runtime) = shader_entry.runtime.as_mut() {
                if let Some(shader_runtime) = runtime.as_any_mut().downcast_mut::<ShaderRuntime>() {
                    // Create render context for shader execution
                    let mut ctx = RenderContextImpl {
                        nodes,
                        frame_id,
                    };
                    shader_runtime.render(&mut ctx)?;
                }
            }
        }
        
        // Mark texture as rendered
        if let Some(entry) = nodes.get_mut(&node_handle) {
            entry.state_ver = frame_id;
        }
        
        Ok(())
    }
    
    fn find_shaders_for_texture(
        nodes: &BTreeMap<NodeHandle, NodeEntry>,
        texture_handle: TextureHandle,
    ) -> Vec<NodeHandle> {
        // Iterate through shader nodes, check if texture_spec matches
        // For now, simplified: just find all shader nodes
        // Full implementation needs to resolve texture_spec and compare handles
        nodes.iter()
            .filter(|(_, entry)| entry.kind == NodeKind::Shader)
            .map(|(handle, _)| *handle)
            .collect()
    }
}
```

### Shader Execution
```rust
// nodes/shader/runtime.rs
impl NodeRuntime for ShaderRuntime {
    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
            message: "Texture handle not resolved".to_string(),
        })?;
        
        let texture = ctx.get_texture_mut(texture_handle)?;
        let executable = self.executable.as_mut().ok_or_else(|| Error::Other {
            message: "Shader not compiled".to_string(),
        })?;
        
        let width = texture.width();
        let height = texture.height();
        let output_size = [width as f32, height as f32];
        let time = ctx.get_time();
        
        // Execute shader for each pixel
        for y in 0..height {
            for x in 0..width {
                let frag_coord = [x as f32, y as f32];
                
                // Call shader main function
                let result = execute_function(
                    executable.as_mut(),
                    "main",
                    &[
                        GlslValue::Vec2(frag_coord),
                        GlslValue::Vec2(output_size),
                        GlslValue::F32(time),
                    ],
                )?;
                
                // Extract RGBA from vec4 result
                let rgba = match result {
                    GlslValue::Vec4([r, g, b, a]) => {
                        // Convert from [0, 1] to [0, 255]
                        [
                            (r.clamp(0.0, 1.0) * 255.0) as u8,
                            (g.clamp(0.0, 1.0) * 255.0) as u8,
                            (b.clamp(0.0, 1.0) * 255.0) as u8,
                            (a.clamp(0.0, 1.0) * 255.0) as u8,
                        ]
                    }
                    _ => return Err(Error::Other {
                        message: "Shader main() must return vec4".to_string(),
                    }),
                };
                
                // Write to texture
                texture.set_pixel(x, y, &rgba)?;
            }
        }
        
        Ok(())
    }
}
```

## Design Decisions

1. **Compilation Timing**: Compile during `init()`, not `render()`. This allows catching compilation errors early and avoids recompiling every frame.

2. **Executable Storage**: Use `Box<dyn GlslExecutable>` trait object to allow different execution backends (JIT, emulator) without changing the runtime code.

3. **Texture Mutability**: Add `get_texture_mut()` to `RenderContext` trait. This is cleaner than accessing texture runtime directly and maintains the abstraction.

4. **Time Calculation**: Track frame time with `delta_ms` and `total_ms` (like old engine). `tick()` takes `delta_ms: u32` parameter. Convert to seconds: `time = total_ms as f32 / 1000.0`. This allows variable frame rates and accurate time tracking.

5. **Error Handling**: Store compilation errors in state, don't fail initialization. Execution errors fail the render but don't crash the system.

6. **Shader Finding**: For now, simplified approach: find all shader nodes. Full implementation needs to resolve `texture_spec` and compare handles. This can be optimized later.

7. **Render Order**: Sort shaders by `render_order` before execution. For now, assume all shaders have the same order (0), but structure supports sorting.

## Implementation Notes

- `lp-glsl-compiler` provides `GlslCompiler` and `GlslExecutable` traits
- Shader main signature: `vec4 main(vec2 fragCoord, vec2 outputSize, float time)`
- Texture pixels are RGBA8 format (4 bytes per pixel)
- Shader execution is per-pixel (can be optimized with vectorization later)
