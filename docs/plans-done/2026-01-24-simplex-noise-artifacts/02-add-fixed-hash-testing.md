# Add Fixed Hash Testing Infrastructure

## Description

Add a feature flag and testing infrastructure to use deterministic hash values for comprehensive
testing of simplex noise functions. This allows testing against known reference values and verifying
algorithm correctness independent of hash function behavior.

## Implementation

### 1. Add Feature Flag

**File**: `lp-glsl/lp-glsl-builtins/Cargo.toml`

Add feature:

```toml
[features]
test_hash_fixed = []
```

### 2. Create Test Hash Function Module

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/shared/test_hash.rs` (new file)

Create deterministic hash functions for testing:

```rust
#[cfg(feature = "test_hash_fixed")]
pub mod test_hash {
    /// Deterministic hash function for testing
    /// Returns a simple deterministic value based on inputs
    pub fn hash_1(x: u32, seed: u32) -> u32 {
        // Simple deterministic hash: x * 2654435761 + seed * 2246822519
        x.wrapping_mul(2654435761).wrapping_add(seed.wrapping_mul(2246822519))
    }
    
    pub fn hash_2(x: u32, y: u32, seed: u32) -> u32 {
        let combined = x.wrapping_mul(2654435761).wrapping_add(y.wrapping_mul(2246822519));
        hash_1(combined, seed)
    }
    
    pub fn hash_3(x: u32, y: u32, z: u32, seed: u32) -> u32 {
        let combined = x.wrapping_mul(2654435761)
            .wrapping_add(y.wrapping_mul(2246822519))
            .wrapping_add(z.wrapping_mul(3266489917));
        hash_1(combined, seed)
    }
}
```

### 3. Update Hash Module to Use Test Hash When Enabled

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/shared/lpfx_hash.rs`

Add conditional compilation:

```rust
#[cfg(feature = "test_hash_fixed")]
use crate::builtins::shared::test_hash::{hash_1 as test_hash_1, hash_2 as test_hash_2, hash_3 as test_hash_3};

#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hash_1(x: u32, seed: u32) -> u32 {
    #[cfg(feature = "test_hash_fixed")]
    return test_hash_1(x, seed);
    
    #[cfg(not(feature = "test_hash_fixed"))]
    hash_impl(x, seed)
}

// Similar for __lpfx_hash_2 and __lpfx_hash_3
```

### 4. Add Reference Value Tests

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs` (in test module)

Add test with fixed hash:

```rust
#[cfg(all(test, feature = "test_hash_fixed"))]
mod fixed_hash_tests {
    use super::*;
    use crate::builtins::q32::test_helpers::{fixed_to_float, float_to_fixed};
    
    #[test]
    fn test_simplex2_known_values() {
        // Test with fixed hash to get deterministic outputs
        // These values can be verified against reference implementations
        let test_cases = [
            ((0.0, 0.0), 0, /* expected_value */),
            ((0.5, 0.5), 0, /* expected_value */),
            ((1.0, 1.0), 0, /* expected_value */),
            // Add more test cases with known expected values
        ];
        
        for ((x, y), seed) in test_cases {
            let result = __lp_q32_lpfx_snoise2(float_to_fixed(x), float_to_fixed(y), seed);
            let result_float = fixed_to_float(result);
            // Verify against expected value (to be filled in after fixing bugs)
            println!("Simplex2({}, {}, seed={}) = {}", x, y, seed, result_float);
        }
    }
    
    #[test]
    fn test_simplex2_boundary_continuity() {
        // Test continuity across cell boundaries
        // Use fixed hash to ensure deterministic behavior
        let boundary_points = [
            // Points near cell boundaries
            (0.0, 0.0),
            (0.001, 0.001),
            (0.999, 0.999),
            (1.0, 1.0),
            (1.001, 1.001),
        ];
        
        let mut prev_value = None;
        for (x, y) in boundary_points {
            let result = __lp_q32_lpfx_snoise2(float_to_fixed(x), float_to_fixed(y), 0);
            let result_float = fixed_to_float(result);
            
            if let Some(prev) = prev_value {
                let diff = (result_float - prev).abs();
                // Noise should be relatively continuous (small changes for small position changes)
                assert!(
                    diff < 0.5, // Reasonable threshold for continuity
                    "Discontinuity detected: Simplex2({}, {}) = {}, previous = {}, diff = {}",
                    x, y, result_float, prev, diff
                );
            }
            prev_value = Some(result_float);
        }
    }
}
```

### 5. Add Similar Tests for 3D

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise3.rs`

Add similar test module with fixed hash tests.

## Usage

Run tests with fixed hash:

```bash
cargo test --features test_hash_fixed --package lp-glsl-builtins
```

## Success Criteria

- Feature flag compiles and works correctly
- Tests with fixed hash produce deterministic outputs
- Boundary continuity tests verify smooth transitions
- Can generate reference values for comparison with other implementations

## Notes

- The deterministic hash function should be simple but produce good distribution
- After fixing the offset bugs, we can generate expected values for known test cases
- This infrastructure enables regression testing and algorithm verification
