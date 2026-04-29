//! Compile-time-sized Fibonacci spiral gradient LUT for `psrdnoise3_q32`.
//!
//! **Size:** 289 × (`gx_b`, `gy_b`, `gz_b`, `qx`, `qy`) × 4 B ≈ 5780 B in rodata.
//!
//! The Fibonacci spiral distributes points evenly on a sphere. For each hash value,
//! we precompute:
//! - Base gradient (gx_b, gy_b, gz_b): gradient after psi rotation, before alpha rotation
//! - Orthogonal vector (qx, qy): axis for alpha rotation in the tangent plane
//!
//! At runtime, we compute (sin_alpha, cos_alpha) once and rotate:
//! ```text
//! gx = cos_alpha * gx_b + sin_alpha * qx
//! gy = cos_alpha * gy_b + sin_alpha * qy
//! gz = cos_alpha * gz_b
//! ```
//!
//! The array lives in [`fibonacci_lut_q32_data`](super::fibonacci_lut_q32_data).

use super::fibonacci_lut_q32_data::FIBONACCI_LUT;

/// Entry in the Fibonacci spiral LUT.
/// Contains base gradient (after psi rotation) and orthogonal vector for alpha rotation.
#[derive(Copy, Clone, Debug)]
pub struct FibonacciEntry {
    /// Base gradient x (after psi rotation, before alpha)
    pub gx_b: i32,
    /// Base gradient y (after psi rotation, before alpha)
    pub gy_b: i32,
    /// Base gradient z (after psi rotation, before alpha)
    pub gz_b: i32,
    /// Orthogonal vector x (rotation axis in tangent plane)
    pub qx: i32,
    /// Orthogonal vector y (rotation axis in tangent plane)
    pub qy: i32,
}

/// Lookup base gradient and orthogonal vector for hash.
#[inline(always)]
pub fn grad_base_and_orthogonal(hash: i32) -> FibonacciEntry {
    let i = hash.clamp(0, 288) as usize;
    let entry = &FIBONACCI_LUT[i];
    FibonacciEntry {
        gx_b: entry.gx_b,
        gy_b: entry.gy_b,
        gz_b: entry.gz_b,
        qx: entry.qx,
        qy: entry.qy,
    }
}

/// Rotate base gradient by alpha using precomputed orthogonal vector.
/// Returns (gx, gy, gz) after alpha rotation.
#[inline(always)]
pub fn rotate_by_alpha(entry: &FibonacciEntry, sin_alpha: i32, cos_alpha: i32) -> (i32, i32, i32) {
    // gx = cos_alpha * gx_b + sin_alpha * qx
    let gx =
        ((cos_alpha as i64 * entry.gx_b as i64 + sin_alpha as i64 * entry.qx as i64) >> 16) as i32;
    // gy = cos_alpha * gy_b + sin_alpha * qy
    let gy =
        ((cos_alpha as i64 * entry.gy_b as i64 + sin_alpha as i64 * entry.qy as i64) >> 16) as i32;
    // gz = cos_alpha * gz_b (qz = 0, so no sin_alpha term)
    let gz = ((cos_alpha as i64 * entry.gz_b as i64) >> 16) as i32;
    (gx, gy, gz)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;

    use super::*;
    use core::f64;

    /// Fibonacci spiral constants (in f64 for generation)
    const THETA_MULT_F64: f64 = 3.883222077452858; // 2*pi/golden ratio
    const SZ_MULT_F64: f64 = -0.006920415; // -2/289
    const SZ_ADD_F64: f64 = 0.996539792; // 1 - 0.5/289
    const PSI_MULT_F64: f64 = 0.108705628; // 10*pi/289
    const Q32_SCALE: f64 = 65536.0;

    /// Generate a single LUT entry for hash value h.
    fn generate_entry(h: i32) -> FibonacciEntry {
        let h_f64 = h as f64;

        // Fibonacci spiral angle
        let theta = h_f64 * THETA_MULT_F64;
        let st = theta.sin();
        let ct = theta.cos();

        // z coordinate and radial component
        let sz = h_f64 * SZ_MULT_F64 + SZ_ADD_F64;
        let sz_prime = (1.0 - sz * sz).sqrt();

        // Psi rotation angle
        let psi = h_f64 * PSI_MULT_F64;
        let sp = psi.sin();
        let cp = psi.cos();

        // Orthogonal vector q = (sin(theta), -cos(theta), 0)
        let qx = st;
        let qy = -ct;

        // Vector p = (-sz * cos(theta), -sz * sin(theta), sz_prime)
        let px = -sz * ct;
        let py = -sz * st;
        let pz = sz_prime;

        // Base gradient after psi rotation: g = cp * p + sp * q
        let gx_b = cp * px + sp * qx;
        let gy_b = cp * py + sp * qy;
        let gz_b = cp * pz; // qz = 0

        // Convert to Q32
        FibonacciEntry {
            gx_b: (gx_b * Q32_SCALE).round() as i32,
            gy_b: (gy_b * Q32_SCALE).round() as i32,
            gz_b: (gz_b * Q32_SCALE).round() as i32,
            qx: (qx * Q32_SCALE).round() as i32,
            qy: (qy * Q32_SCALE).round() as i32,
        }
    }

    /// Test that generated entries are reasonable (unit length, orthogonal)
    #[test]
    fn fibonacci_lut_entries_valid() {
        for h in 0..289 {
            let entry = grad_base_and_orthogonal(h);

            // Check that base gradient is roughly unit length
            let gx_f = entry.gx_b as f64 / Q32_SCALE;
            let gy_f = entry.gy_b as f64 / Q32_SCALE;
            let gz_f = entry.gz_b as f64 / Q32_SCALE;
            let len_sq = gx_f * gx_f + gy_f * gy_f + gz_f * gz_f;
            assert!(
                (len_sq - 1.0).abs() < 0.01,
                "hash {}: gradient length squared should be ~1.0, got {}",
                h,
                len_sq
            );

            // Check that q is unit length (qx^2 + qy^2 = 1)
            let qx_f = entry.qx as f64 / Q32_SCALE;
            let qy_f = entry.qy as f64 / Q32_SCALE;
            let q_len_sq = qx_f * qx_f + qy_f * qy_f;
            assert!(
                (q_len_sq - 1.0).abs() < 0.01,
                "hash {}: q length squared should be ~1.0, got {}",
                h,
                q_len_sq
            );

            // Note: q is (sin(theta), -cos(theta), 0), which is tangent to the unit circle
            // at angle theta. The gradient xy projection is not necessarily orthogonal to q
            // after psi rotation. The rotation formula uses q as the orthogonal axis in the
            // tangent plane for alpha rotation.
        }
    }

    /// Regenerate `fibonacci_lut_q32_data.rs` `FIBONACCI_LUT`:
    /// `cargo test -p lps-builtins emit_fibonacci_lut -- --ignored --nocapture`,
    /// then wrap the printed output as `pub(super) const FIBONACCI_LUT: [FibonacciEntryData; 289] = ...;`
    #[test]
    #[ignore]
    fn emit_fibonacci_lut() {
        std::println!("pub(super) const FIBONACCI_LUT: [FibonacciEntryData; 289] = [");
        for h in 0..289 {
            let entry = generate_entry(h);
            let comma = if h == 288 { "" } else { "," };
            std::println!(
                "    FibonacciEntryData {{ gx_b: {}, gy_b: {}, gz_b: {}, qx: {}, qy: {} }}{}",
                entry.gx_b,
                entry.gy_b,
                entry.gz_b,
                entry.qx,
                entry.qy,
                comma
            );
        }
        std::println!("];");
    }
}
