# Phase 6: Integrate pre-computed mapping into FixtureRuntime

## Scope of phase

Add `PrecomputedMapping` field to `FixtureRuntime` and update `regenerate_mapping_if_needed()` to use version tracking and trigger recomputation.

## Code Organization Reminders

- Place new fields in struct definition
- Update methods that need to check/regenerate mapping
- Keep version tracking logic clear and well-documented
- Update existing tests

## Implementation Details

### 1. Add PrecomputedMapping field to FixtureRuntime

Update `lp-app/crates/lp-engine/src/nodes/fixture/runtime.rs`:

```rust
use crate::nodes::fixture::mapping_compute::{PrecomputedMapping, compute_mapping};

pub struct FixtureRuntime {
    // ... existing fields ...
    /// Pre-computed pixel-to-channel mapping
    precomputed_mapping: Option<PrecomputedMapping>,
}
```

### 2. Update regenerate_mapping_if_needed

Modify `regenerate_mapping_if_needed()` to:
- Check config versions (fixture config version and texture config version)
- Compare with `mapping_data_ver` from `PrecomputedMapping`
- Recompute if `max(our_config_ver, texture_config_ver) > mapping_data_ver`

```rust
fn regenerate_mapping_if_needed(
    &mut self,
    texture_width: u32,
    texture_height: u32,
    our_config_ver: FrameId,
    texture_config_ver: FrameId,
) -> Result<(), Error> {
    let needs_regeneration = self
        .texture_width
        .map(|w| w != texture_width)
        .unwrap_or(true)
        || self
            .texture_height
            .map(|h| h != texture_height)
            .unwrap_or(true)
        || self
            .precomputed_mapping
            .as_ref()
            .map(|m| {
                let max_config_ver = our_config_ver.max(texture_config_ver);
                max_config_ver > m.mapping_data_ver
            })
            .unwrap_or(true);

    if needs_regeneration {
        let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: String::from("fixture"),
            reason: String::from("Config not set"),
        })?;

        // Compute new mapping
        let max_config_ver = our_config_ver.max(texture_config_ver);
        let mapping = compute_mapping(
            &config.mapping,
            texture_width,
            texture_height,
            max_config_ver,
        );
        
        self.precomputed_mapping = Some(mapping);

        // Update texture dimensions
        self.texture_width = Some(texture_width);
        self.texture_height = Some(texture_height);

        // Keep existing mapping points for now (used by state extraction)
        self.mapping = generate_mapping_points(&config.mapping, texture_width, texture_height);
    }

    Ok(())
}
```

### 3. Update render() to get config versions

We'll need to get config versions from the render context. For now, we can use `FrameId::new(0)` as a placeholder and update in the next phase when we have proper version tracking.

### 4. Update constructor

```rust
pub fn new() -> Self {
    Self {
        // ... existing fields ...
        precomputed_mapping: None,
    }
}
```

## Validate

Run:
```bash
cd lp-app && cargo check --package lp-engine
```

Expected: Code compiles. We'll need to update the call sites to pass config versions in the next phase.
