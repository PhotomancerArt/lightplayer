//! Per-builtin conformance specs: probe functions, input corpora, and
//! tolerance modes with rationale.
//!
//! ## Tolerance scheme
//!
//! Two comparison modes:
//!
//! - **Pointwise**: max absolute component error over the corpus (all lpfn
//!   outputs are order-1 ranges, so absolute comparison is meaningful).
//!   An outlier budget covers builtins with genuine discontinuities
//!   (value-Worley cell ownership) where input quantization can flip a
//!   branch; outliers are counted, not ignored silently.
//! - **Statistical**: for the sin-hash ("chaotic") family. The device Q32
//!   implementation computes `fract(sin(angle) * 43758.5453)` in Q16.16;
//!   any representation difference in `angle` (quantization ~1.5e-5) is
//!   amplified by the multiplier into a full wrap of `fract`, so pointwise
//!   agreement between *any* two finite-precision implementations of this
//!   family is mathematically impossible. Conformance instead checks output
//!   range, mean agreement, and (separately) seed sensitivity. This applies
//!   to random/srandom and everything built on them (gnoise, gnoise3_tile,
//!   fbm3_tile).
//!
//! Numeric tolerances are calibrated against measured max errors (see the
//! conformance test output) with headroom of roughly 2x, and documented per
//! spec below.

/// A probe argument, in both pipelines' terms.
#[derive(Debug, Clone, Copy)]
pub enum Arg {
    /// Scalar float.
    F32(f32),
    /// vec2.
    Vec2(f32, f32),
    /// vec3.
    Vec3(f32, f32, f32),
    /// vec4.
    Vec4(f32, f32, f32, f32),
    /// Signed int (e.g. fbm octaves).
    I32(i32),
    /// Unsigned int (seeds).
    U32(u32),
}

/// Conformance comparison mode.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// Max absolute component error must stay within `tol` for all but at
    /// most `max_outliers` corpus cases.
    Pointwise {
        /// Absolute tolerance per output component.
        tol: f32,
        /// Number of cases allowed to exceed `tol` (discontinuity flips).
        max_outliers: usize,
    },
    /// Distribution-level agreement for the chaotic sin-hash family.
    Statistical {
        /// Expected output minimum (checked with small slack).
        min: f32,
        /// Expected output maximum (checked with small slack).
        max: f32,
        /// Allowed |mean(oracle) - mean(q32)| over the corpus.
        mean_tol: f32,
    },
}

/// One builtin-overload conformance spec.
pub struct BuiltinSpec {
    /// Display name.
    pub name: &'static str,
    /// Probe function name in the probe module.
    pub probe: &'static str,
    /// Comparison mode with tolerances.
    pub mode: Mode,
    /// Corpus builder (deterministic).
    pub cases: fn() -> Vec<Vec<Arg>>,
    /// `Some(reason)` marks a known Q32 implementation bug: the runner then
    /// *expects* the comparison to fail and errors if it starts passing
    /// (remove the annotation once the Q32 side is fixed). Mirrors the
    /// filetest `@broken` semantics.
    pub known_q32_bug: Option<&'static str>,
}

/// Probe module source (calls `lpfn_*`; the oracle build renames it).
pub fn probe_glsl() -> String {
    PROBES.to_string()
}

const PROBES: &str = r#"
float probe_saturate1(float x) { return lpfn_saturate(x); }
vec3 probe_saturate3(vec3 v) { return lpfn_saturate(v); }
vec4 probe_saturate4(vec4 v) { return lpfn_saturate(v); }
vec3 probe_hue2rgb(float h) { return lpfn_hue2rgb(h); }
vec3 probe_hsv2rgb3(vec3 hsv) { return lpfn_hsv2rgb(hsv); }
vec4 probe_hsv2rgb4(vec4 hsv) { return lpfn_hsv2rgb(hsv); }
vec3 probe_rgb2hsv3(vec3 rgb) { return lpfn_rgb2hsv(rgb); }
vec4 probe_rgb2hsv4(vec4 rgb) { return lpfn_rgb2hsv(rgb); }
float probe_random1(float x, uint seed) { return lpfn_random(x, seed); }
float probe_random2(vec2 p, uint seed) { return lpfn_random(p, seed); }
float probe_random3(vec3 p, uint seed) { return lpfn_random(p, seed); }
float probe_srandom1(float x, uint seed) { return lpfn_srandom(x, seed); }
float probe_srandom2(vec2 p, uint seed) { return lpfn_srandom(p, seed); }
float probe_srandom3(vec3 p, uint seed) { return lpfn_srandom(p, seed); }
vec3 probe_srandom3_vec(vec3 p, uint seed) { return lpfn_srandom3_vec(p, seed); }
vec3 probe_srandom3_tile(vec3 p, float tileLength, uint seed) {
    return lpfn_srandom3_tile(p, tileLength, seed);
}
float probe_snoise1(float x, uint seed) { return lpfn_snoise(x, seed); }
float probe_snoise2(vec2 p, uint seed) { return lpfn_snoise(p, seed); }
float probe_snoise3(vec3 p, uint seed) { return lpfn_snoise(p, seed); }
float probe_gnoise1(float x, uint seed) { return lpfn_gnoise(x, seed); }
float probe_gnoise2(vec2 p, uint seed) { return lpfn_gnoise(p, seed); }
float probe_gnoise3(vec3 p, uint seed) { return lpfn_gnoise(p, seed); }
float probe_gnoise3_tile(vec3 p, float tileLength, uint seed) {
    return lpfn_gnoise(p, tileLength, seed);
}
float probe_fbm2(vec2 p, int octaves, uint seed) { return lpfn_fbm(p, octaves, seed); }
float probe_fbm3(vec3 p, int octaves, uint seed) { return lpfn_fbm(p, octaves, seed); }
float probe_fbm3_tile(vec3 p, float tileLength, int octaves, uint seed) {
    return lpfn_fbm(p, tileLength, octaves, seed);
}
float probe_worley2(vec2 p, uint seed) { return lpfn_worley(p, seed); }
float probe_worley3(vec3 p, uint seed) { return lpfn_worley(p, seed); }
float probe_worley2_value(vec2 p, uint seed) { return lpfn_worley_value(p, seed); }
float probe_worley3_value(vec3 p, uint seed) { return lpfn_worley_value(p, seed); }
float probe_psrdnoise2_value(vec2 x, vec2 period, float alpha, uint seed) {
    vec2 g = vec2(0.0);
    return lpfn_psrdnoise(x, period, alpha, g, seed);
}
vec2 probe_psrdnoise2_grad(vec2 x, vec2 period, float alpha, uint seed) {
    vec2 g = vec2(0.0);
    float n = lpfn_psrdnoise(x, period, alpha, g, seed);
    return g;
}
float probe_psrdnoise3_value(vec3 x, vec3 period, float alpha, uint seed) {
    vec3 g = vec3(0.0);
    return lpfn_psrdnoise(x, period, alpha, g, seed);
}
vec3 probe_psrdnoise3_grad(vec3 x, vec3 period, float alpha, uint seed) {
    vec3 g = vec3(0.0);
    float n = lpfn_psrdnoise(x, period, alpha, g, seed);
    return g;
}
"#;

/// Known Q32 bug in `psrdnoise3_q32.rs` (2026-07-08, M2 conformance):
/// the simplex rank-order step is inverted. `Vec3Q32::step(self, edge)`
/// computes `step(edge, self)` (1 when `self >= edge`), so
/// `f0.xyx().step(f0.yzz())` evaluates the *opposite* of the GLSL
/// `step(f0.xyx, f0.yzz)` it ports (the code comment documents the intended
/// GLSL semantics). The wrong simplex corners are traversed; some inputs
/// fall outside every corner's radial support and return exactly 0
/// ("dead zones"). Measured against the float oracle over the psrdnoise3
/// corpus (196 cases): noise value max_err 0.9267 (55 cases > 0.05),
/// gradient max_err 4.3533 (67 cases > 0.5). An inverted-step f32
/// reference reproduces the Q32 output to ~1e-3, confirming the root
/// cause. Fix is follow-up work (recorded in the GPU-preview roadmap
/// notes); the canonical GLSL keeps the correct stegu ordering.
const PSRDNOISE3_STEP_BUG: &str =
    "psrdnoise3_q32 inverted simplex rank-order step (Vec3Q32::step argument order)";

/// Seeds used across seeded corpora.
const SEEDS: [u32; 2] = [0, 123];

/// All conformance specs.
pub fn all_specs() -> Vec<BuiltinSpec> {
    vec![
        // ---- math ----
        // saturate: pure clamp; only error source is the f32→Q16.16 argument
        // conversion (inputs chosen exactly representable), so effectively
        // exact.
        BuiltinSpec {
            name: "saturate1",
            probe: "probe_saturate1",
            mode: Mode::Pointwise {
                tol: 1e-4,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_scalar_unit,
        },
        BuiltinSpec {
            name: "saturate3",
            probe: "probe_saturate3",
            mode: Mode::Pointwise {
                tol: 1e-4,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_vec3_unit,
        },
        BuiltinSpec {
            name: "saturate4",
            probe: "probe_saturate4",
            mode: Mode::Pointwise {
                tol: 1e-4,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_vec4_unit,
        },
        // ---- color ----
        // hue2rgb: piecewise-linear ramp; Q16.16 rounding through *6 and
        // abs stays ~1e-4.
        BuiltinSpec {
            name: "hue2rgb",
            probe: "probe_hue2rgb",
            mode: Mode::Pointwise {
                tol: 1e-3,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_scalar_unit,
        },
        // hsv2rgb: hue2rgb plus two multiplies; error ~1e-3.
        BuiltinSpec {
            name: "hsv2rgb3",
            probe: "probe_hsv2rgb3",
            mode: Mode::Pointwise {
                tol: 2e-3,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_vec3_unit,
        },
        BuiltinSpec {
            name: "hsv2rgb4",
            probe: "probe_hsv2rgb4",
            mode: Mode::Pointwise {
                tol: 2e-3,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_vec4_unit,
        },
        // rgb2hsv: hue channel divides by 6*delta (+1/65536 epsilon); for
        // small deltas the Q16.16 division loses precision, so the bound is
        // looser than the encoders'.
        BuiltinSpec {
            name: "rgb2hsv3",
            probe: "probe_rgb2hsv3",
            mode: Mode::Pointwise {
                // measured max_err 2.5e-5 on this corpus
                tol: 2e-3,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_vec3_unit,
        },
        BuiltinSpec {
            name: "rgb2hsv4",
            probe: "probe_rgb2hsv4",
            mode: Mode::Pointwise {
                // measured max_err 2.5e-5 on this corpus
                tol: 2e-3,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_vec4_unit,
        },
        // ---- chaotic sin-hash family (statistical; see module docs) ----
        BuiltinSpec {
            name: "random1",
            probe: "probe_random1",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.15,
            },
            known_q32_bug: None,
            cases: cases_seeded_scalar,
        },
        BuiltinSpec {
            name: "random2",
            probe: "probe_random2",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.15,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec2,
        },
        BuiltinSpec {
            name: "random3",
            probe: "probe_random3",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.15,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        BuiltinSpec {
            name: "srandom1",
            probe: "probe_srandom1",
            mode: Mode::Statistical {
                min: -1.0,
                max: 1.0,
                mean_tol: 0.3,
            },
            known_q32_bug: None,
            cases: cases_seeded_scalar,
        },
        BuiltinSpec {
            name: "srandom2",
            probe: "probe_srandom2",
            mode: Mode::Statistical {
                min: -1.0,
                max: 1.0,
                mean_tol: 0.3,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec2,
        },
        BuiltinSpec {
            name: "srandom3",
            probe: "probe_srandom3",
            mode: Mode::Statistical {
                min: -1.0,
                max: 1.0,
                mean_tol: 0.3,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        BuiltinSpec {
            name: "srandom3_vec",
            probe: "probe_srandom3_vec",
            mode: Mode::Statistical {
                min: -1.0,
                max: 1.0,
                mean_tol: 0.3,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        BuiltinSpec {
            name: "srandom3_tile",
            probe: "probe_srandom3_tile",
            mode: Mode::Statistical {
                min: -1.0,
                max: 1.0,
                mean_tol: 0.3,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3_tile,
        },
        // ---- simplex noise (integer hash → pointwise) ----
        // The lattice hash is exact integer math on both sides; remaining
        // error is Q16.16 quantization through skew/falloff polynomials.
        BuiltinSpec {
            name: "snoise1",
            probe: "probe_snoise1",
            mode: Mode::Pointwise {
                // measured max_err 0.00014
                tol: 0.005,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_seeded_scalar,
        },
        BuiltinSpec {
            name: "snoise2",
            probe: "probe_snoise2",
            mode: Mode::Pointwise {
                // measured max_err 0.0101 (Q16.16 quantization through the
                // t^4 falloff, amplified near simplex boundaries)
                tol: 0.05,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec2,
        },
        BuiltinSpec {
            name: "snoise3",
            probe: "probe_snoise3",
            mode: Mode::Pointwise {
                // measured max_err 0.0014
                tol: 0.01,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        // ---- gnoise (built on chaotic random → statistical) ----
        BuiltinSpec {
            name: "gnoise1",
            probe: "probe_gnoise1",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.2,
            },
            known_q32_bug: None,
            cases: cases_seeded_scalar,
        },
        BuiltinSpec {
            name: "gnoise2",
            probe: "probe_gnoise2",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.2,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec2,
        },
        BuiltinSpec {
            name: "gnoise3",
            probe: "probe_gnoise3",
            mode: Mode::Statistical {
                min: -1.0,
                max: 1.0,
                mean_tol: 0.4,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        BuiltinSpec {
            name: "gnoise3_tile",
            probe: "probe_gnoise3_tile",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.2,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3_tile,
        },
        // ---- fbm on snoise (pointwise, error accumulates per octave) ----
        BuiltinSpec {
            name: "fbm2",
            probe: "probe_fbm2",
            mode: Mode::Pointwise {
                // measured max_err 0.0023 over 4 octaves
                tol: 0.02,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_fbm2,
        },
        BuiltinSpec {
            name: "fbm3",
            probe: "probe_fbm3",
            mode: Mode::Pointwise {
                // measured max_err 0.0008 over 4 octaves
                tol: 0.02,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_fbm3,
        },
        // fbm3_tile is built on gnoise3_tile (chaotic) → statistical.
        BuiltinSpec {
            name: "fbm3_tile",
            probe: "probe_fbm3_tile",
            mode: Mode::Statistical {
                min: 0.0,
                max: 1.0,
                mean_tol: 0.2,
            },
            known_q32_bug: None,
            cases: cases_fbm3_tile,
        },
        // ---- worley (integer hash → pointwise) ----
        // Distance variant is continuous; a small outlier budget covers
        // range-check pruning flips at quantized cell midlines.
        BuiltinSpec {
            name: "worley2",
            probe: "probe_worley2",
            mode: Mode::Pointwise {
                // measured max_err 7e-5
                tol: 0.01,
                max_outliers: 1,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec2,
        },
        BuiltinSpec {
            name: "worley3",
            probe: "probe_worley3",
            mode: Mode::Pointwise {
                // measured max_err 4e-5
                tol: 0.01,
                max_outliers: 1,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        // Value variant returns the owning cell's hash → genuinely
        // discontinuous at ownership boundaries; quantization can flip
        // ownership, so a small outlier budget is expected.
        BuiltinSpec {
            name: "worley2_value",
            probe: "probe_worley2_value",
            mode: Mode::Pointwise {
                tol: 0.02,
                max_outliers: 3,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec2,
        },
        BuiltinSpec {
            name: "worley3_value",
            probe: "probe_worley3_value",
            mode: Mode::Pointwise {
                tol: 0.02,
                max_outliers: 3,
            },
            known_q32_bug: None,
            cases: cases_seeded_vec3,
        },
        // ---- psrdnoise (integer mod-289 hash → pointwise) ----
        // Gradient magnitudes reach ~|10.9 * sum| ≈ 10, so the gradient
        // bound is proportionally looser than the value bound.
        BuiltinSpec {
            name: "psrdnoise2",
            probe: "probe_psrdnoise2_value",
            mode: Mode::Pointwise {
                // measured max_err 0.0021
                tol: 0.02,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_psrdnoise2,
        },
        BuiltinSpec {
            name: "psrdnoise2_grad",
            probe: "probe_psrdnoise2_grad",
            mode: Mode::Pointwise {
                // measured max_err 0.0114 (gradient magnitudes reach ~10)
                tol: 0.1,
                max_outliers: 0,
            },
            known_q32_bug: None,
            cases: cases_psrdnoise2,
        },
        BuiltinSpec {
            name: "psrdnoise3",
            probe: "probe_psrdnoise3_value",
            mode: Mode::Pointwise {
                tol: 0.05,
                max_outliers: 0,
            },
            known_q32_bug: Some(PSRDNOISE3_STEP_BUG),
            cases: cases_psrdnoise3,
        },
        BuiltinSpec {
            name: "psrdnoise3_grad",
            probe: "probe_psrdnoise3_grad",
            mode: Mode::Pointwise {
                tol: 0.5,
                max_outliers: 0,
            },
            known_q32_bug: Some(PSRDNOISE3_STEP_BUG),
            cases: cases_psrdnoise3,
        },
    ]
}

// ---- corpora ------------------------------------------------------------
//
// All coordinates are multiples of 1/64 so the f32 → Q16.16 argument
// conversion is exact and input quantization does not contribute error.

fn scalar_grid() -> Vec<f32> {
    let mut v: Vec<f32> = Vec::new();
    let mut x = -6.0f32;
    while x <= 6.0 {
        v.push(x);
        x += 0.703125; // 45/64
    }
    v.extend([0.0, 0.5, -0.5, 100.25, -100.25]);
    v
}

fn grid2() -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    let mut x = -3.5f32;
    while x <= 3.5 {
        let mut y = -3.5f32;
        while y <= 3.5 {
            v.push((x, y));
            y += 1.171875; // 75/64
        }
        x += 1.171875;
    }
    v.extend([(0.0, 0.0), (100.25, -37.5), (-64.015625, 17.484375)]);
    v
}

fn grid3() -> Vec<(f32, f32, f32)> {
    let mut v = Vec::new();
    let mut x = -2.5f32;
    while x <= 2.5 {
        let mut y = -2.5f32;
        while y <= 2.5 {
            let mut z = -1.5f32;
            while z <= 1.5 {
                v.push((x, y, z));
                z += 1.484375; // 95/64
            }
            y += 1.640625; // 105/64
        }
        x += 1.640625;
    }
    v.extend([(0.0, 0.0, 0.0), (42.5, -10.25, 5.75)]);
    v
}

/// Values in and around [0, 1] (color inputs, saturate).
fn unit_grid() -> Vec<f32> {
    vec![
        -1.5, -1.0, -0.25, 0.0, 0.109375, 0.25, 0.328125, 0.5, 0.671875, 0.75, 0.890625, 1.0, 1.25,
        2.0,
    ]
}

fn cases_scalar_unit() -> Vec<Vec<Arg>> {
    unit_grid().iter().map(|&x| vec![Arg::F32(x)]).collect()
}

fn cases_vec3_unit() -> Vec<Vec<Arg>> {
    let g = [0.0f32, 0.25, 0.328125, 0.5, 0.75, 1.0];
    let mut v = Vec::new();
    for &a in &g {
        for &b in &g {
            for &c in &g {
                v.push(vec![Arg::Vec3(a, b, c)]);
            }
        }
    }
    // Near-equal channels stress the rgb2hsv epsilon path.
    v.push(vec![Arg::Vec3(0.5, 0.515625, 0.5)]);
    v.push(vec![Arg::Vec3(0.5, 0.5, 0.5)]);
    v
}

fn cases_vec4_unit() -> Vec<Vec<Arg>> {
    let g = [0.0f32, 0.25, 0.5, 0.75, 1.0];
    let mut v = Vec::new();
    for &a in &g {
        for &b in &g {
            for &c in &g {
                v.push(vec![Arg::Vec4(a, b, c, 0.75)]);
            }
        }
    }
    v
}

fn cases_seeded_scalar() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &seed in &SEEDS {
        for &x in &scalar_grid() {
            v.push(vec![Arg::F32(x), Arg::U32(seed)]);
        }
    }
    v
}

fn cases_seeded_vec2() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &seed in &SEEDS {
        for &(x, y) in &grid2() {
            v.push(vec![Arg::Vec2(x, y), Arg::U32(seed)]);
        }
    }
    v
}

fn cases_seeded_vec3() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &seed in &SEEDS {
        for &(x, y, z) in &grid3() {
            v.push(vec![Arg::Vec3(x, y, z), Arg::U32(seed)]);
        }
    }
    v
}

fn cases_seeded_vec3_tile() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &seed in &SEEDS {
        for &(x, y, z) in &grid3() {
            v.push(vec![Arg::Vec3(x, y, z), Arg::F32(4.0), Arg::U32(seed)]);
        }
    }
    v
}

fn cases_fbm2() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &octaves in &[1i32, 4] {
        for &(x, y) in &grid2() {
            // Keep coordinates small: octaves scale them by 2^k and large
            // lattice coordinates are out of Q16.16 range by design.
            if x.abs() > 8.0 || y.abs() > 8.0 {
                continue;
            }
            v.push(vec![Arg::Vec2(x, y), Arg::I32(octaves), Arg::U32(123)]);
        }
    }
    v
}

fn cases_fbm3() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &octaves in &[1i32, 4] {
        for &(x, y, z) in &grid3() {
            if x.abs() > 8.0 || y.abs() > 8.0 || z.abs() > 8.0 {
                continue;
            }
            v.push(vec![Arg::Vec3(x, y, z), Arg::I32(octaves), Arg::U32(123)]);
        }
    }
    v
}

fn cases_fbm3_tile() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &(x, y, z) in &grid3() {
        if x.abs() > 8.0 || y.abs() > 8.0 || z.abs() > 8.0 {
            continue;
        }
        v.push(vec![
            Arg::Vec3(x, y, z),
            Arg::F32(4.0),
            Arg::I32(3),
            Arg::U32(123),
        ]);
    }
    v
}

fn cases_psrdnoise2() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &(px, py) in &[(0.0f32, 0.0f32), (4.0, 4.0)] {
        for &alpha in &[0.0f32, 0.5] {
            for &(x, y) in &grid2() {
                if x.abs() > 8.0 || y.abs() > 8.0 {
                    continue;
                }
                v.push(vec![
                    Arg::Vec2(x, y),
                    Arg::Vec2(px, py),
                    Arg::F32(alpha),
                    Arg::U32(0),
                ]);
            }
        }
    }
    v
}

fn cases_psrdnoise3() -> Vec<Vec<Arg>> {
    let mut v = Vec::new();
    for &(px, py, pz) in &[(0.0f32, 0.0f32, 0.0f32), (4.0, 4.0, 4.0)] {
        for &alpha in &[0.0f32, 0.5] {
            for &(x, y, z) in &grid3() {
                if x.abs() > 8.0 || y.abs() > 8.0 || z.abs() > 8.0 {
                    continue;
                }
                v.push(vec![
                    Arg::Vec3(x, y, z),
                    Arg::Vec3(px, py, pz),
                    Arg::F32(alpha),
                    Arg::U32(0),
                ]);
            }
        }
    }
    v
}
