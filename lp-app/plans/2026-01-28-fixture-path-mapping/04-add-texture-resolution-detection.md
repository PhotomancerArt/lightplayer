# Phase 4: Add Texture Resolution Change Detection

## Goal

Track texture dimensions and regenerate mappings when texture resolution changes. This ensures pixel-perfect mappings work correctly when textures are resized.

## Implementation

### 1. Update FixtureRuntime Fields

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Ensure `FixtureRuntime` has texture dimension tracking:

```rust
pub struct FixtureRuntime {
    // ... existing fields ...
    texture_width: Option<u32>,
    texture_height: Option<u32>,
}
```

### 2. Add Regeneration Function

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Add function to regenerate mapping when texture resolution changes:

```rust
fn regenerate_mapping_if_needed(
    &mut self,
    texture_width: u32,
    texture_height: u32,
) -> Result<(), Error> {
    let needs_regeneration = self
        .texture_width
        .map(|w| w != texture_width)
        .unwrap_or(true)
        || self
            .texture_height
            .map(|h| h != texture_height)
            .unwrap_or(true);

    if needs_regeneration {
        let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: String::from("fixture"),
            reason: String::from("Config not set"),
        })?;

        // Regenerate mapping points
        self.mapping = generate_mapping_points(
            &config.mapping,
            texture_width,
            texture_height,
        );

        // Update texture dimensions
        self.texture_width = Some(texture_width);
        self.texture_height = Some(texture_height);

        // Update sampling kernel based on first mapping's radius
        if let Some(first_mapping) = self.mapping.first() {
            let normalized_radius = first_mapping.radius.min(1.0).max(0.0);
            self.kernel = SamplingKernel::new(normalized_radius);
        }
    }

    Ok(())
}
```

### 3. Update init() Method

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Update `init()` to get texture dimensions and generate mapping:

```rust
fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
    // ... existing handle resolution ...

    // Get texture to determine dimensions
    let texture = ctx.get_texture(texture_handle)?;
    let texture_width = texture.width();
    let texture_height = texture.height();

    // Store config values
    self.color_order = config.color_order;
    self.transform = config.transform;

    // Generate mapping points
    self.mapping = generate_mapping_points(
        &config.mapping,
        texture_width,
        texture_height,
    );

    // Store texture dimensions
    self.texture_width = Some(texture_width);
    self.texture_height = Some(texture_height);

    // Create sampling kernel
    if let Some(first_mapping) = self.mapping.first() {
        let normalized_radius = first_mapping.radius.min(1.0).max(0.0);
        self.kernel = SamplingKernel::new(normalized_radius);
    } else {
        self.kernel = SamplingKernel::new(0.1);
    }

    Ok(())
}
```

### 4. Update render() Method

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Update `render()` to check for texture resolution changes:

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

    // Regenerate mapping if texture resolution changed
    self.regenerate_mapping_if_needed(texture_width, texture_height)?;

    // ... rest of render code ...
}
```

### 5. Update update_config() Method

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Update `update_config()` to regenerate mapping when config changes:

```rust
fn update_config(
    &mut self,
    new_config: Box<dyn NodeConfig>,
    ctx: &dyn NodeInitContext,
) -> Result<(), Error> {
    // ... existing config update code ...

    // Regenerate mapping if we have texture dimensions
    if let (Some(width), Some(height)) = (self.texture_width, self.texture_height) {
        self.mapping = generate_mapping_points(
            &fixture_config.mapping,
            width,
            height,
        );

        // Update sampling kernel
        if let Some(first_mapping) = self.mapping.first() {
            let normalized_radius = first_mapping.radius.min(1.0).max(0.0);
            self.kernel = SamplingKernel::new(normalized_radius);
        }
    }

    Ok(())
}
```

## Success Criteria

- Texture dimensions tracked in `FixtureRuntime`
- Mapping regenerated when texture resolution changes
- Mapping regenerated during `init()` with current texture dimensions
- Mapping regenerated during `render()` if texture resolution changed
- Mapping regenerated during `update_config()` if config changed
- Handles case where texture not yet available (mapping empty until texture loaded)
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`
