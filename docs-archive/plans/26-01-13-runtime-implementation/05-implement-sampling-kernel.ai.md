# Phase 5: Implement Sampling Kernel

## Goal

Implement `SamplingKernel` and `SamplePoint` types for texture sampling in fixtures. Copy approach from old implementation.

## Dependencies

- Phase 4

## Implementation

### 1. Create Sampling Kernel Module

**File**: `lp-engine/src/nodes/fixture/sampling_kernel.rs`

```rust
/// Precomputed sample point for texture sampling
#[derive(Debug, Clone)]
pub struct SamplePoint {
    /// Relative offset in U coordinate (normalized)
    pub offset_u: f32,
    /// Relative offset in V coordinate (normalized)
    pub offset_v: f32,
    /// Weight for this sample
    pub weight: f32,
}

/// Sampling kernel for texture sampling
///
/// Precomputed sample points in a circle, reused for all mapping points.
#[derive(Debug, Clone)]
pub struct SamplingKernel {
    /// Normalized sampling radius (same for all pixels)
    pub radius: f32,
    /// Precomputed sample points
    pub samples: Vec<SamplePoint>,
}

impl SamplingKernel {
    /// Create a new sampling kernel with the given radius
    ///
    /// Generates sample points in a circle using a simple grid pattern.
    /// The radius is normalized (0.0 to 1.0).
    pub fn new(radius: f32) -> Self {
        // Generate sample points in a circle
        // Use a simple approach: sample on a grid within the circle
        let mut samples = Vec::new();

        // Number of samples per dimension (creates a square grid)
        let sample_count = 5; // 5x5 = 25 samples

        // Total weight for normalization
        let mut total_weight = 0.0;

        for i in 0..sample_count {
            for j in 0..sample_count {
                // Map from [0, sample_count-1] to [-radius, radius]
                let u = (i as f32 / (sample_count - 1) as f32) * 2.0 - 1.0;
                let v = (j as f32 / (sample_count - 1) as f32) * 2.0 - 1.0;

                // Check if point is within circle
                let dist = (u * u + v * v).sqrt();
                if dist <= 1.0 {
                    // Scale by radius
                    let offset_u = u * radius;
                    let offset_v = v * radius;

                    // Weight: closer to center = higher weight (Gaussian-like)
                    let weight = 1.0 - (dist * dist);
                    total_weight += weight;

                    samples.push(SamplePoint {
                        offset_u,
                        offset_v,
                        weight,
                    });
                }
            }
        }

        // Normalize weights so they sum to 1.0
        if total_weight > 0.0 {
            for sample in &mut samples {
                sample.weight /= total_weight;
            }
        }

        Self { radius, samples }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_kernel_new() {
        let kernel = SamplingKernel::new(0.5);
        assert!(!kernel.samples.is_empty());
        assert_eq!(kernel.radius, 0.5);

        // Check that weights sum to approximately 1.0
        let total_weight: f32 = kernel.samples.iter().map(|s| s.weight).sum();
        assert!((total_weight - 1.0).abs() < 0.01);
    }
    
    #[test]
    fn test_sampling_kernel_samples_in_circle() {
        let kernel = SamplingKernel::new(0.5);
        for sample in &kernel.samples {
            let dist = (sample.offset_u * sample.offset_u + sample.offset_v * sample.offset_v).sqrt();
            assert!(dist <= kernel.radius);
        }
    }
}
```

### 2. Export from fixture module

**File**: `lp-engine/src/nodes/fixture/mod.rs`

```rust
pub mod runtime;
pub mod sampling_kernel;

pub use runtime::FixtureRuntime;
pub use sampling_kernel::{SamplePoint, SamplingKernel};
```

### 3. Export from nodes module

**File**: `lp-engine/src/nodes/mod.rs`

```rust
// ... existing code ...
pub use fixture::{FixtureRuntime, SamplePoint, SamplingKernel};
```

## Success Criteria

- All code compiles
- `SamplingKernel::new()` generates sample points in a circle
- Weights sum to approximately 1.0
- All sample points are within the specified radius
- Tests pass

## Notes

- Kernel uses 5x5 grid pattern (25 samples max)
- Weights use Gaussian-like distribution (closer to center = higher weight)
- Weights are normalized to sum to 1.0
- Radius is normalized (0.0 to 1.0) in texture UV space
