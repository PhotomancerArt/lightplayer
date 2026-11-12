# Phase 6: Implement Fixture Runtime

## Goal

Implement `FixtureRuntime` with initialization (resolve handles, setup kernel) and rendering (sample texture, write to output).

## Dependencies

- Phase 5
- Phase 4 (RenderContext)

## Implementation

### 1. Add Fields to FixtureRuntime

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

```rust
use crate::runtime::contexts::{TextureHandle, OutputHandle};
use crate::nodes::fixture::sampling_kernel::SamplingKernel;
use lp_model::nodes::fixture::{FixtureConfig, ColorOrder};
use lp_shared::util::Texture;

// Simplified mapping point (will be replaced with structured type later)
#[derive(Debug, Clone)]
struct MappingPoint {
    channel: u32,
    center: [f32; 2],  // UV coordinates in fixture space [-1,-1] to [1,1]
    radius: f32,
}

pub struct FixtureRuntime {
    texture_handle: Option<TextureHandle>,
    output_handle: Option<OutputHandle>,
    kernel: SamplingKernel,
    color_order: ColorOrder,
    mapping: Vec<MappingPoint>,
    transform: [[f32; 4]; 4],
}

impl FixtureRuntime {
    pub fn new() -> Self {
        Self {
            texture_handle: None,
            output_handle: None,
            kernel: SamplingKernel::new(0.1),  // Default small radius
            color_order: ColorOrder::Rgb,
            mapping: Vec::new(),
            transform: [[1.0, 0.0, 0.0, 0.0],
                        [0.0, 1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0, 0.0],
                        [0.0, 0.0, 0.0, 1.0]],  // Identity matrix
        }
    }
}
```

### 2. Implement init()

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

```rust
impl NodeRuntime for FixtureRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // Get config (similar issue as TextureRuntime - need to extract config)
        // For now, assume config is passed or stored somehow
        // todo!("Get FixtureConfig from context or stored config")
        
        // Resolve texture handle
        let texture_handle = ctx.resolve_texture(&config.texture_spec)?;
        self.texture_handle = Some(texture_handle);
        
        // Resolve output handle
        let output_handle = ctx.resolve_output(&config.output_spec)?;
        self.output_handle = Some(output_handle);
        
        // Store config values
        self.color_order = config.color_order;
        self.transform = config.transform;
        
        // Parse mapping (simplified for now - will be structured later)
        // For now, create empty mapping or parse from string
        // todo!("Parse mapping from config.mapping string")
        self.mapping = Vec::new();  // Placeholder
        
        // Create sampling kernel based on first mapping's radius (if any)
        if let Some(first_mapping) = self.mapping.first() {
            let normalized_radius = first_mapping.radius.min(1.0).max(0.0);
            self.kernel = SamplingKernel::new(normalized_radius);
        } else {
            // No mappings, use default small radius
            self.kernel = SamplingKernel::new(0.1);
        }
        
        Ok(())
    }
}
```

### 3. Implement render()

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

```rust
impl NodeRuntime for FixtureRuntime {
    fn render(&mut self, ctx: &dyn RenderContext) -> Result<(), Error> {
        // Get texture handle
        let texture_handle = self.texture_handle
            .ok_or_else(|| Error::Other {
                message: "Texture handle not resolved".to_string(),
            })?;
        
        // Get texture (triggers lazy rendering if needed)
        let texture = ctx.get_texture(texture_handle)?;
        
        let texture_width = texture.width() as f32;
        let texture_height = texture.height() as f32;
        
        // Sample all mapping points and collect results
        let mut sampled_values: Vec<(u32, [u8; 4])> = Vec::new();
        
        for mapping in &self.mapping {
            // Transform fixture coordinates to texture UV coordinates
            // Fixture space: [-1, -1] to [1, 1]
            // Texture space: [0, 0] to [1, 1]
            let fixture_u = mapping.center[0];
            let fixture_v = mapping.center[1];
            
            // Apply transform matrix (4x4 affine transform)
            // For now, simple transform: map [-1,1] to [0,1]
            // Full matrix multiplication will be implemented later
            let center_u = (fixture_u + 1.0) * 0.5;  // Simplified for now
            let center_v = (fixture_v + 1.0) * 0.5;
            
            let radius = mapping.radius;
            
            // Sample texture at kernel positions
            let mut r_sum = 0.0f32;
            let mut g_sum = 0.0f32;
            let mut b_sum = 0.0f32;
            let mut a_sum = 0.0f32;
            let mut total_weight = 0.0f32;
            
            for sample in &self.kernel.samples {
                // Calculate sample position (scale kernel by mapping radius)
                let sample_u = center_u + sample.offset_u * radius;
                let sample_v = center_v + sample.offset_v * radius;
                
                // Clamp to [0, 1]
                let sample_u = sample_u.max(0.0).min(1.0);
                let sample_v = sample_v.max(0.0).min(1.0);
                
                // Convert normalized coordinates to pixel coordinates
                let x = (sample_u * texture_width).clamp(0.0, texture_width - 1.0) as u32;
                let y = (sample_v * texture_height).clamp(0.0, texture_height - 1.0) as u32;
                
                // Sample texture
                if let Some(pixel) = texture.get_pixel(x, y) {
                    let weight = sample.weight;
                    r_sum += pixel[0] as f32 * weight;
                    g_sum += pixel[1] as f32 * weight;
                    b_sum += pixel[2] as f32 * weight;
                    a_sum += pixel[3] as f32 * weight;
                    total_weight += weight;
                }
            }
            
            // Normalize by total weight
            if total_weight > 0.0 {
                r_sum /= total_weight;
                g_sum /= total_weight;
                b_sum /= total_weight;
                a_sum /= total_weight;
            }
            
            // Convert to u8
            let r = r_sum as u8;
            let g = g_sum as u8;
            let b = b_sum as u8;
            let a = a_sum as u8;
            
            sampled_values.push((mapping.channel, [r, g, b, a]));
        }
        
        // Get output handle
        let output_handle = self.output_handle
            .ok_or_else(|| Error::Other {
                message: "Output handle not resolved".to_string(),
            })?;
        
        // Write sampled values to output buffer
        // For now, assume universe 0 and write sequentially
        // todo!("Get proper universe/channel mapping from config")
        let mut channel_offset = 0u32;
        for (channel, [r, g, b, _a]) in sampled_values {
            let start_ch = channel_offset + channel * 3;  // 3 bytes per RGB
            let buffer = ctx.get_output(output_handle, 0, start_ch, 3)?;
            self.color_order.write_rgb(buffer, 0, r, g, b);
        }
        
        Ok(())
    }
}
```

### 4. Parse Mapping (Simplified)

For now, we'll use a simple placeholder mapping. Later phases will implement proper mapping parsing.

## Success Criteria

- All code compiles
- `init()` resolves texture and output handles
- `init()` creates sampling kernel based on mapping radius
- `render()` samples texture using kernel
- `render()` writes RGB values to output using color order
- Tests pass

## Notes

- Mapping parsing is simplified for now (will be structured type later)
- Transform matrix application is simplified (full 4x4 matrix math later)
- Output universe/channel mapping is simplified (assumes universe 0, sequential channels)
- Texture sampling uses kernel-based weighted averaging
- Color order enum handles RGB channel ordering
