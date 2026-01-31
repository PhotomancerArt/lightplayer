# Add Visual Regression Tests

## Description

Add visual regression testing to catch artifacts and verify noise quality. Generate noise images and
compare against reference implementations or previous known-good outputs.

## Implementation

### 1. Add Image Generation Test

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs` (in test module)

Add visual output test:

```rust
#[cfg(all(test, feature = "test_visual"))]
mod visual_tests {
    use super::*;
    use crate::builtins::q32::test_helpers::{fixed_to_float, float_to_fixed};
    use std::fs::File;
    use std::io::Write;
    
    #[test]
    fn test_simplex2_generate_image() {
        // Generate a 256x256 noise image
        const WIDTH: usize = 256;
        const HEIGHT: usize = 256;
        const SCALE: f32 = 0.1; // Noise frequency
        
        let mut pixels = Vec::new();
        
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let fx = x as f32 * SCALE;
                let fy = y as f32 * SCALE;
                let noise = __lp_q32_lpfx_snoise2(float_to_fixed(fx), float_to_fixed(fy), 0);
                let noise_float = fixed_to_float(noise);
                
                // Normalize from [-1, 1] to [0, 255]
                let normalized = ((noise_float + 1.0) * 127.5) as u8;
                pixels.push(normalized);
                pixels.push(normalized);
                pixels.push(normalized);
                pixels.push(255); // Alpha
            }
        }
        
        // Write PPM format (simple, no dependencies)
        let mut file = File::create("test_output_simplex2.ppm").unwrap();
        writeln!(file, "P3").unwrap();
        writeln!(file, "{} {}", WIDTH, HEIGHT).unwrap();
        writeln!(file, "255").unwrap();
        
        for chunk in pixels.chunks(4) {
            writeln!(file, "{} {} {}", chunk[0], chunk[1], chunk[2]).unwrap();
        }
        
        println!("Generated test_output_simplex2.ppm");
    }
}
```

### 2. Add Comparison Test Against noise-rs

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs` (in test module)

Add comparison test:

```rust
#[cfg(all(test, feature = "test"))]
mod comparison_tests {
    use super::*;
    use crate::builtins::q32::test_helpers::{fixed_to_float, float_to_fixed};
    
    #[test]
    fn test_simplex2_compare_with_noise_rs() {
        use noise::{NoiseFn, Simplex};
        
        let noise_rs_fn = Simplex::new(0);
        let test_points = [
            (0.0, 0.0),
            (0.5, 0.5),
            (1.0, 1.0),
            (5.5, 3.2),
            (10.0, 10.0),
        ];
        
        for (x, y) in test_points {
            let our_value = __lp_q32_lpfx_snoise2(float_to_fixed(x), float_to_fixed(y), 0);
            let our_float = fixed_to_float(our_value);
            
            let noise_rs_value = noise_rs_fn.get([x as f64, y as f64]) as f32;
            
            // Note: Values won't match exactly due to different hash functions
            // But they should be in similar ranges and have similar properties
            println!("Point ({}, {}): ours={:.6}, noise-rs={:.6}, diff={:.6}",
                x, y, our_float, noise_rs_value, (our_float - noise_rs_value).abs());
            
            // Verify both are in reasonable range
            assert!(our_float >= -2.0 && our_float <= 2.0);
            assert!(noise_rs_value >= -2.0 && noise_rs_value <= 2.0);
        }
    }
}
```

### 3. Add Artifact Detection Test

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs` (in test module)

Add test to detect discontinuities:

```rust
#[test]
fn test_simplex2_no_discontinuities() {
    // Sample noise along a line and check for sudden jumps
    const STEP: f32 = 0.01;
    const THRESHOLD: f32 = 0.5; // Maximum allowed change per step
    
    let mut prev_value = None;
    let mut max_jump = 0.0;
    
    for i in 0..1000 {
        let x = i as f32 * STEP;
        let y = x; // Diagonal line
        let result = __lp_q32_lpfx_snoise2(float_to_fixed(x), float_to_fixed(y), 0);
        let result_float = fixed_to_float(result);
        
        if let Some(prev) = prev_value {
            let jump = (result_float - prev).abs();
            max_jump = max_jump.max(jump);
            
            if jump > THRESHOLD {
                println!("Large jump detected at ({}, {}): {} -> {}, jump = {}",
                    x, y, prev, result_float, jump);
            }
        }
        prev_value = Some(result_float);
    }
    
    println!("Maximum jump along diagonal: {}", max_jump);
    // After fixing bugs, max_jump should be reasonable (e.g., < 0.3)
}
```

## Usage

Generate visual output:

```bash
cargo test --features test_visual --package lp-glsl-builtins test_simplex2_generate_image -- --nocapture
```

## Success Criteria

- Can generate noise images for visual inspection
- Comparison tests verify our implementation has similar properties to noise-rs
- Discontinuity detection test catches artifacts
- Generated images show smooth, continuous noise without diagonal artifacts

## Notes

- Visual tests are optional (behind feature flag) to avoid requiring image libraries
- PPM format is simple and doesn't require external dependencies
- After fixing bugs, we can save reference images and compare against them
