# Phase 7: Update render() to use pre-computed mapping

## Scope of phase

Replace kernel-based sampling in `render()` with pre-computed mapping iteration. Implement channel accumulation with Q32 math and convert to u8 for output.

## Code Organization Reminders

- Place main render logic first
- Place helper functions at the bottom
- Keep accumulation logic clear and well-documented
- Update existing tests

## Implementation Details

### 1. Update render() method

Replace the sampling loop in `render()`:

```rust
fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
    // Get texture handle
    let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
        message: String::from("Texture handle not resolved"),
    })?;

    // Get texture (triggers lazy rendering if needed)
    let texture = ctx.get_texture(texture_handle)?;

    let texture_width = texture.width();
    let texture_height = texture.height();

    // Get config versions (TODO: get from context properly)
    let our_config_ver = FrameId::new(0); // Placeholder
    let texture_config_ver = FrameId::new(0); // Placeholder

    // Regenerate mapping if needed
    self.regenerate_mapping_if_needed(
        texture_width,
        texture_height,
        our_config_ver,
        texture_config_ver,
    )?;

    // Get pre-computed mapping
    let mapping = self.precomputed_mapping.as_ref().ok_or_else(|| Error::Other {
        message: String::from("Precomputed mapping not available"),
    })?;

    // Initialize channel accumulators (16.16 fixed-point)
    let max_channel = mapping
        .entries
        .iter()
        .filter_map(|e| {
            if !e.is_skip() {
                Some(e.channel())
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0);

    let mut ch_values: Vec<i32> = vec![0; (max_channel + 1) as usize];

    // Iterate through entries and accumulate
    let mut pixel_index = 0u32;
    let mut entry_iter = mapping.entries.iter();

    while let Some(entry) = entry_iter.next() {
        if entry.is_skip() {
            // SKIP entry - advance to next pixel
            pixel_index += 1;
            continue;
        }

        // Get pixel coordinates
        let x = pixel_index % texture_width;
        let y = pixel_index / texture_width;

        // Get pixel value from texture
        if let Some(pixel) = texture.get_pixel(x, y) {
            // Decode contribution: (65536 - stored) / 65536
            let stored = (entry.to_raw() >> 16) & 0xFFFF;
            let contribution_fractional = 65536u32.saturating_sub(stored as u32);

            // Convert pixel to 16.16 fixed-point (shift left by 16)
            let pixel_r = (pixel[0] as i32) << 16;
            let pixel_g = (pixel[1] as i32) << 16;
            let pixel_b = (pixel[2] as i32) << 16;

            // Accumulate: ch_value += contribution * pixel_value
            // contribution is 0-65535 (fractional part), so we multiply and shift
            let channel = entry.channel() as usize;
            if channel < ch_values.len() {
                // For RGB, we need to accumulate each channel separately
                // For now, accumulate R channel (we'll need to handle RGB properly)
                let contribution = contribution_fractional as i64;
                let accumulated_r = (contribution * pixel_r as i64) >> 16;
                ch_values[channel] += accumulated_r as i32;
            }
        }

        // Advance pixel_index if this is the last entry for this pixel
        if !entry.has_more() {
            pixel_index += 1;
        }
    }

    // Convert accumulated values to u8 and store
    let max_channel = ch_values.len() - 1;
    self.lamp_colors.clear();
    self.lamp_colors.resize((max_channel + 1) * 3, 0);

    for (channel, &accumulated) in ch_values.iter().enumerate() {
        // Convert from 16.16 fixed-point to u8
        // Right-shift 16 bits and clamp to 0-255
        let value = (accumulated >> 16).clamp(0, 255) as u8;
        let idx = channel * 3;
        self.lamp_colors[idx] = value;     // R
        self.lamp_colors[idx + 1] = value; // G (for now, same as R)
        self.lamp_colors[idx + 2] = value; // B (for now, same as R)
    }

    // Write to output (existing code)
    let output_handle = self.output_handle.ok_or_else(|| Error::Other {
        message: String::from("Output handle not resolved"),
    })?;

    let universe = 0u32;
    let channel_offset = 0u32;
    for (channel, &r) in ch_values.iter().enumerate() {
        let g = r; // TODO: Get actual G value
        let b = r; // TODO: Get actual B value
        let r_u8 = (r >> 16).clamp(0, 255) as u8;
        let g_u8 = (g >> 16).clamp(0, 255) as u8;
        let b_u8 = (b >> 16).clamp(0, 255) as u8;

        let start_ch = channel_offset + (channel as u32) * 3;
        let buffer = ctx.get_output(output_handle, universe, start_ch, 3)?;
        self.color_order.write_rgb(buffer, 0, r_u8, g_u8, b_u8);
    }

    Ok(())
}
```

Note: This is a simplified version. We'll need to:

1. Handle RGB channels properly (currently just using R)
2. Get proper config versions from context
3. Fix the accumulation math to handle Q32 properly

### 2. Update tests

Update existing tests to work with the new rendering approach.

## Validate

Run:

```bash
cd lp-app && cargo test --package lp-engine fixture
```

Expected: Tests compile. Some may fail until we complete the implementation. We'll fix issues in the cleanup phase.
