# Plan Notes: Fixture Path Mapping Support

## Questions

### Q1: RingArray LED Position Generation

**Context**: The `RingArray` path spec defines concentric rings with:

- Center position in texture space [0, 1]
- Diameter in texture space [0, 1]
- Ring indices (start_ring_inclusive, end_ring_exclusive)
- Number of lamps per ring (ring_lamp_counts)
- Offset angle in radians
- Order (InnerFirst or OuterFirst)

**Answer**:

- For each ring from start_ring_inclusive to end_ring_exclusive:
  - Calculate ring radius: evenly space rings within the diameter (e.g., `radius = (diameter / 2) * (ring_index / max_ring_index)`)
  - Get lamp count for this ring from `ring_lamp_counts[ring_index]`
  - For each lamp in the ring:
    - Calculate angle: `angle = (2 * PI * lamp_index / lamp_count) + offset_angle`
    - Calculate position: `center_x + radius * cos(angle), center_y + radius * sin(angle)`
- Order the LEDs based on `RingOrder`:
  - `InnerFirst`: Process rings from inner to outer, lamps in each ring sequentially
  - `OuterFirst`: Process rings from outer to inner, lamps in each ring sequentially
- Assign sequential channel numbers starting from 0
- Note: `led_count` was removed from `PathConfig` - it was a mistake

### Q2: Sample Diameter vs Radius

**Context**: `PathPoints` has `sample_diameter: f32` which represents the diameter of the sampling circle in texture pixels. The runtime uses `MappingPoint.radius` for sampling.

**Question**: How should we convert `sample_diameter` to the `radius` used in `MappingPoint`?

**Answer**:

- `sample_diameter` is in texture pixels (not normalized)
- For pixel-perfect mappings (like 2D arrays showing sprites), this is important
- We need to recompute mapping when texture resolution changes
- We need a mechanism to detect texture resolution changes
- Mapping might not be available if there's no texture, and that's okay
- When generating `MappingPoint`, convert pixel-based `sample_diameter` to normalized radius: `radius = (sample_diameter / 2.0) / max(texture_width, texture_height)` or similar normalization
- Store `sample_diameter` in the config, convert to normalized radius when generating mapping points (which happens in runtime when we have texture dimensions)

### Q3: Multiple Paths Handling

**Context**: `PathPoints` contains `paths: Vec<PathConfig>`, where each `PathConfig` has a `path_spec`. Multiple paths can be specified.

**Answer**:

- Process paths sequentially
- For each path, generate LED positions and assign channels sequentially
- If path 1 has 10 LEDs (channels 0-9), path 2 starts at channel 10
- This allows combining multiple ring arrays or other path types into a single fixture

### Q4: Coordinate Space Conversion

**Context**: `RingArray` center and diameter are in texture space [0, 1]. `MappingPoint.center` is currently in fixture space [-1, -1] to [1, 1]. The transform matrix converts from fixture space to texture space.

**Answer**:

- **Decision: Standardize on texture space [0, 1] across the app**
- Update `MappingPoint.center` to use texture space [0, 1] instead of fixture space [-1, 1]
- Store RingArray positions directly in texture space [0, 1] - no conversion needed
- Update transform matrix to work with [0, 1] input (transform from [0, 1] to [0, 1])
- Update all fixture code, comments, and documentation to reflect [0, 1] standard
- This simplifies the codebase and makes coordinates more intuitive

### Q5: Backward Compatibility

**Context**: The runtime currently checks for `config.mapping == "linear"` (string comparison), but `config.mapping` is now `MappingConfig` enum. The builder and example JSON still use string format.

**Answer**:

- No backward compatibility needed
- Remove string-based mapping support completely
- Update builder to use `MappingConfig`
- Update example JSON to use new format
- This is cleaner and avoids maintaining two code paths

### Q6: Path Generation Function Location

**Context**: We need a function to convert `PathSpec::RingArray` to `Vec<MappingPoint>`. Since `sample_diameter` is in pixels, we need texture dimensions to convert to normalized radius.

**Answer**:

- Generation must happen in runtime crate where we have access to texture dimensions
- Keep `MappingPoint` in runtime crate (it's runtime-specific)
- Create a function in runtime crate like `generate_mapping_points(config: &MappingConfig, texture_width: u32, texture_height: u32) -> Vec<MappingPoint>`
- This function will be called during `init()` or when texture resolution changes
- We need to track texture dimensions and regenerate mapping when they change
