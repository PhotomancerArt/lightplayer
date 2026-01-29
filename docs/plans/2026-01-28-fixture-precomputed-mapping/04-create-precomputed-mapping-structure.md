# Phase 4: Create PrecomputedMapping structure

## Scope of phase

Create the `PrecomputedMapping` struct that holds the pre-computed mapping data and version tracking.

## Code Organization Reminders

- Place type definitions first
- Place helper methods after the struct definition
- Keep related functionality grouped together
- Add tests for the structure

## Implementation Details

### 1. Add PrecomputedMapping struct

Add to `lp-app/crates/lp-engine/src/nodes/fixture/mapping_compute.rs`:

```rust
use lp_model::FrameId;

/// Pre-computed texture-to-fixture mapping
/// 
/// Contains a flat list of `PixelMappingEntry` values ordered by pixel (x, y).
/// Each pixel's entries are consecutive, with the last entry having `has_more = false`.
/// Pixels with no contributions have a SKIP sentinel entry.
#[derive(Debug, Clone)]
pub struct PrecomputedMapping {
    /// Flat list of mapping entries, ordered by pixel (x, y)
    pub entries: Vec<PixelMappingEntry>,
    /// Texture width (for validation)
    pub texture_width: u32,
    /// Texture height (for validation)
    pub texture_height: u32,
    /// FrameId when this mapping was computed
    pub mapping_data_ver: FrameId,
}

impl PrecomputedMapping {
    /// Create a new empty mapping
    pub fn new(texture_width: u32, texture_height: u32, mapping_data_ver: FrameId) -> Self {
        Self {
            entries: Vec::new(),
            texture_width,
            texture_height,
            mapping_data_ver,
        }
    }
    
    /// Check if mapping is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    
    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    /// Get total number of pixels
    pub fn pixel_count(&self) -> u32 {
        self.texture_width * self.texture_height
    }
}

#[cfg(test)]
mod precomputed_mapping_tests {
    use super::*;
    
    #[test]
    fn test_new_empty() {
        let mapping = PrecomputedMapping::new(100, 200, FrameId::new(42));
        assert!(mapping.is_empty());
        assert_eq!(mapping.len(), 0);
        assert_eq!(mapping.texture_width, 100);
        assert_eq!(mapping.texture_height, 200);
        assert_eq!(mapping.mapping_data_ver, FrameId::new(42));
        assert_eq!(mapping.pixel_count(), 20000);
    }
    
    #[test]
    fn test_with_entries() {
        let mut mapping = PrecomputedMapping::new(10, 10, FrameId::new(1));
        mapping.entries.push(PixelMappingEntry::new(0, Q32::from_f32(1.0), false));
        mapping.entries.push(PixelMappingEntry::skip());
        
        assert!(!mapping.is_empty());
        assert_eq!(mapping.len(), 2);
    }
}
```

## Validate

Run:
```bash
cd lp-app && cargo test --package lp-engine precomputed_mapping
```

Expected: All tests pass, code compiles without warnings.
