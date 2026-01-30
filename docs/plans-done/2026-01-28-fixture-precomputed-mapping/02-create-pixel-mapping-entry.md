# Phase 2: Create PixelMappingEntry bit-packed type

## Scope of phase

Create the `PixelMappingEntry` type with bit manipulation methods for encoding/decoding channel index, contribution fraction, and continuation flag.

## Code Organization Reminders

- Place type definitions first
- Place helper/utility functions at the bottom
- Keep related functionality grouped together
- Add comprehensive tests

## Implementation Details

### 1. Create mapping_compute.rs module

Create `lp-app/crates/lp-engine/src/nodes/fixture/mapping_compute.rs`:

```rust
//! Pre-computed texture-to-fixture mapping utilities

use lp_builtins::glsl::q32::types::Q32;

/// Sentinel value for channel index indicating no mapping (SKIP)
pub const CHANNEL_SKIP: u32 = 0x7FFF; // Max value for 15-bit channel index

/// Packed pixel-to-channel mapping entry
///
/// Bit layout:
/// - Bit 0: `has_more` flag (1 = more entries for this pixel follow)
/// - Bits 1-15: Channel index (15 bits, max 32767; CHANNEL_SKIP = no mapping)
/// - Bits 16-31: Contribution fraction (16 bits, stored as 65536 - contribution)
///   - 0 = 100% contribution
///   - 65535 = ~0% contribution
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PixelMappingEntry(u32);

impl PixelMappingEntry {
    /// Create a new entry
    ///
    /// # Arguments
    /// * `channel` - Channel index (0-32766, CHANNEL_SKIP reserved for sentinel)
    /// * `contribution` - Contribution fraction as Q32 (0.0 = 0%, 1.0 = 100%)
    /// * `has_more` - True if more entries follow for this pixel
    pub fn new(channel: u32, contribution: Q32, has_more: bool) -> Self {
        assert!(channel < CHANNEL_SKIP, "Channel index must be < CHANNEL_SKIP");

        let continue_flag = if has_more { 1 } else { 0 };
        let channel_bits = (channel & 0x7FFF) << 1;

        // Contribution: Store (65536 - fractional_part) where fractional_part is 0-65535
        // Q32 has 16 fractional bits, so we extract the fractional part
        let contribution_raw = contribution.to_fixed() as u32;
        let fractional_part = contribution_raw & 0xFFFF;
        let stored_contribution = 65536u32.saturating_sub(fractional_part);
        let contribution_bits = (stored_contribution & 0xFFFF) << 16;

        Self(continue_flag | channel_bits | contribution_bits)
    }

    /// Create SKIP sentinel entry (no mapping for this pixel)
    pub fn skip() -> Self {
        Self((CHANNEL_SKIP << 1) | 1) // has_more = true, channel = SKIP
    }

    /// Extract channel index
    pub fn channel(&self) -> u32 {
        ((self.0 >> 1) & 0x7FFF) as u32
    }

    /// Extract contribution as Q32 (0.0 = 0%, 1.0 = 100%)
    /// Decodes: contribution = (65536 - stored_value) / 65536
    pub fn contribution(&self) -> Q32 {
        let stored = ((self.0 >> 16) & 0xFFFF) as u32;
        let fractional_part = 65536u32.saturating_sub(stored);
        Q32::from_fixed(fractional_part as i32)
    }

    /// Check if more entries follow for this pixel
    pub fn has_more(&self) -> bool {
        (self.0 & 1) != 0
    }

    /// Check if this is the SKIP sentinel
    pub fn is_skip(&self) -> bool {
        self.channel() == CHANNEL_SKIP
    }

    /// Get raw u32 value
    pub fn to_raw(&self) -> u32 {
        self.0
    }

    /// Create from raw u32
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_entry() {
        let entry = PixelMappingEntry::new(5, Q32::from_f32(0.5), false);
        assert_eq!(entry.channel(), 5);
        assert!((entry.contribution().to_f32() - 0.5).abs() < 0.001);
        assert!(!entry.has_more());
        assert!(!entry.is_skip());
    }

    #[test]
    fn test_full_contribution() {
        // 0 stored = 100% contribution
        let entry = PixelMappingEntry::new(0, Q32::from_f32(1.0), false);
        assert_eq!(entry.channel(), 0);
        assert!((entry.contribution().to_f32() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_zero_contribution() {
        let entry = PixelMappingEntry::new(0, Q32::from_f32(0.0), false);
        assert_eq!(entry.channel(), 0);
        assert!(entry.contribution().to_f32() < 0.01);
    }

    #[test]
    fn test_has_more_flag() {
        let entry_more = PixelMappingEntry::new(1, Q32::from_f32(0.5), true);
        assert!(entry_more.has_more());

        let entry_last = PixelMappingEntry::new(1, Q32::from_f32(0.5), false);
        assert!(!entry_last.has_more());
    }

    #[test]
    fn test_skip_sentinel() {
        let skip = PixelMappingEntry::skip();
        assert!(skip.is_skip());
        assert_eq!(skip.channel(), CHANNEL_SKIP);
        assert!(skip.has_more()); // SKIP entries have has_more = true
    }

    #[test]
    fn test_round_trip() {
        let original = PixelMappingEntry::new(42, Q32::from_f32(0.75), true);
        let raw = original.to_raw();
        let reconstructed = PixelMappingEntry::from_raw(raw);

        assert_eq!(original.channel(), reconstructed.channel());
        assert!((original.contribution().to_f32() - reconstructed.contribution().to_f32()).abs() < 0.01);
        assert_eq!(original.has_more(), reconstructed.has_more());
    }

    #[test]
    fn test_max_channel() {
        let entry = PixelMappingEntry::new(CHANNEL_SKIP - 1, Q32::from_f32(0.5), false);
        assert_eq!(entry.channel(), CHANNEL_SKIP - 1);
    }
}
```

### 2. Export module

Update `lp-app/crates/lp-engine/src/nodes/fixture/mod.rs` to export the new module:

```rust
pub mod mapping_compute;
```

## Validate

Run:

```bash
cd lp-app && cargo test --package lp-engine mapping_compute
```

Expected: All tests pass, code compiles without warnings.
