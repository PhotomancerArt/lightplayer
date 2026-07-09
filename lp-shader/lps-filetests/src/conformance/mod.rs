//! Conformance harness: Q32 device builtins vs the canonical-GLSL float
//! oracle.
//!
//! GLSL is the canonical source of truth for lpfn builtin float semantics
//! (`docs/adr/2026-07-08-glsl-canonical-builtins.md`; sources live in
//! `lp-shader/lps-builtins/glsl/`). This module:
//!
//! - builds a **float oracle** by compiling the canonical sources through
//!   `lps-frontend` and interpreting the LPIR natively in f32
//!   ([`oracle`]);
//! - invokes the **Q32 builtins the way shaders do**: a probe GLSL module
//!   calling `lpfn_*` is compiled through the normal Q32 pipeline and run
//!   on the `wasm.q32` host target ([`q32_probe`]);
//! - compares the two over deterministic corpora within per-builtin
//!   tolerances ([`spec`], tests at the bottom of this file).
//!
//! Placement note: the milestone offered extending the filetest directive
//! machinery with an interpreter-f32 target. The directive format asserts
//! exact per-target values and has no notion of "compare target A against
//! target B within a tolerance / statistically", so the harness lives here
//! as an ordinary test module instead, reusing the same compile/execute
//! plumbing (`test_run::filetest_lpvm`, `test_run::execution`).

pub mod oracle;
pub mod q32_probe;
pub mod spec;

#[cfg(test)]
mod tests {
    use lpir::Value;
    use lps_shared::LpsValueF32;

    use super::oracle::Oracle;
    use super::q32_probe::Q32Probes;
    use super::spec::{Arg, Mode, all_specs, probe_glsl};

    /// Run every spec'd builtin through both pipelines and enforce its
    /// conformance mode. Prints a per-builtin result table (visible with
    /// `cargo test -p lps-filetests conformance -- --nocapture`).
    #[test]
    fn q32_matches_float_oracle_within_tolerances() {
        let probes = probe_glsl();
        let oracle = Oracle::build(&probes).expect("oracle build");
        let mut q32 = Q32Probes::build(&probes).expect("q32 probe build");

        let mut failures: Vec<String> = Vec::new();
        println!(
            "{:<16} {:<12} {:>8} {:>12} {:>10}",
            "builtin", "mode", "cases", "max_err", "outliers"
        );
        for spec in all_specs() {
            let cases = (spec.cases)();
            let mut oracle_vals: Vec<Vec<f32>> = Vec::with_capacity(cases.len());
            let mut q32_vals: Vec<Vec<f32>> = Vec::with_capacity(cases.len());
            for case in &cases {
                let o = oracle
                    .run(spec.probe, &oracle_args(case))
                    .unwrap_or_else(|e| panic!("{}: oracle: {e}", spec.name));
                let q = q32
                    .run(spec.probe, &q32_args(case))
                    .unwrap_or_else(|e| panic!("{}: q32: {e}", spec.name));
                assert_eq!(o.len(), q.len(), "{}: component count mismatch", spec.name);
                oracle_vals.push(o);
                q32_vals.push(q);
            }

            match spec.mode {
                Mode::Pointwise { tol, max_outliers } => {
                    let mut max_err = 0.0f32;
                    let mut outliers = 0usize;
                    let mut worst: Option<String> = None;
                    for ((case, o), q) in cases.iter().zip(&oracle_vals).zip(&q32_vals) {
                        let mut case_err = 0.0f32;
                        for (a, b) in o.iter().zip(q) {
                            case_err = case_err.max((a - b).abs());
                        }
                        if case_err > tol {
                            outliers += 1;
                        }
                        if case_err > max_err {
                            max_err = case_err;
                            worst = Some(format!("{case:?} oracle={o:?} q32={q:?}"));
                        }
                    }
                    let broken = spec.known_q32_bug.is_some();
                    println!(
                        "{:<16} {:<12} {:>8} {:>12.6} {:>10}{}",
                        spec.name,
                        format!("pw({tol})"),
                        cases.len(),
                        max_err,
                        outliers,
                        if broken { "  KNOWN-Q32-BUG" } else { "" }
                    );
                    match spec.known_q32_bug {
                        None => {
                            if outliers > max_outliers {
                                failures.push(format!(
                                    "{}: {} cases exceeded tol {} (allowed {}), max_err {} at {}",
                                    spec.name,
                                    outliers,
                                    tol,
                                    max_outliers,
                                    max_err,
                                    worst.unwrap_or_default()
                                ));
                            }
                        }
                        Some(reason) => {
                            // Expect-fail: the annotated Q32 bug must still
                            // reproduce; if it conforms now, the annotation
                            // is stale.
                            if outliers <= max_outliers {
                                failures.push(format!(
                                    "{}: annotated known Q32 bug ({reason}) no longer \
                                     reproduces — remove the annotation",
                                    spec.name
                                ));
                            }
                        }
                    }
                }
                Mode::Statistical { min, max, mean_tol } => {
                    let o_flat: Vec<f32> = oracle_vals.iter().flatten().copied().collect();
                    let q_flat: Vec<f32> = q32_vals.iter().flatten().copied().collect();
                    let (o_mean, o_lo, o_hi) = stats(&o_flat);
                    let (q_mean, q_lo, q_hi) = stats(&q_flat);
                    println!(
                        "{:<16} {:<12} {:>8} {:>12.6} {:>10}",
                        spec.name,
                        "stat",
                        cases.len(),
                        (o_mean - q_mean).abs(),
                        "-"
                    );
                    // Range slack: Q16.16 quantization plus a little headroom.
                    let slack = 0.01;
                    for (side, lo, hi) in [("oracle", o_lo, o_hi), ("q32", q_lo, q_hi)] {
                        if lo < min - slack || hi > max + slack {
                            failures.push(format!(
                                "{}: {side} range [{lo}, {hi}] outside [{min}, {max}]",
                                spec.name
                            ));
                        }
                    }
                    if (o_mean - q_mean).abs() > mean_tol {
                        failures.push(format!(
                            "{}: means diverge: oracle {o_mean} vs q32 {q_mean} (tol {mean_tol})",
                            spec.name
                        ));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "conformance failures:\n{}",
            failures.join("\n")
        );
    }

    /// Seeded builtins must respond to the seed on both sides.
    ///
    /// Seed delta is 123, not 1: the Q32 sin-hash family adds the raw seed
    /// word to a Q16.16 angle whose sine is evaluated with coarser
    /// granularity than 2^-16 rad, so *adjacent* seeds (delta < ~10) can
    /// produce identical device fields. Recorded as a Q32 finding in the
    /// milestone report; the canonical float semantics inherit the same
    /// 2^-16 seed scaling, so tiny seed deltas are equally weak there by
    /// design.
    #[test]
    fn seeded_builtins_vary_with_seed() {
        let probes = probe_glsl();
        let oracle = Oracle::build(&probes).expect("oracle build");
        let mut q32 = Q32Probes::build(&probes).expect("q32 probe build");

        // (probe, arg builder taking seed)
        let seeded: &[(&str, fn(u32) -> Vec<Arg>)] = &[
            ("probe_random2", |s| vec![Arg::Vec2(1.25, 2.5), Arg::U32(s)]),
            ("probe_snoise2", |s| vec![Arg::Vec2(1.25, 2.5), Arg::U32(s)]),
            ("probe_gnoise2", |s| vec![Arg::Vec2(1.25, 2.5), Arg::U32(s)]),
            ("probe_worley2", |s| vec![Arg::Vec2(1.25, 2.5), Arg::U32(s)]),
        ];
        for (probe, build) in seeded {
            let o0 = oracle
                .run(probe, &oracle_args_slice(&build(0)))
                .expect("oracle run");
            let o1 = oracle
                .run(probe, &oracle_args_slice(&build(123)))
                .expect("oracle run");
            assert_ne!(o0, o1, "{probe} (oracle): seed 0 vs 123 identical");
            let q0 = q32.run(probe, &q32_args_slice(&build(0))).expect("q32 run");
            let q1 = q32
                .run(probe, &q32_args_slice(&build(123)))
                .expect("q32 run");
            assert_ne!(q0, q1, "{probe} (q32): seed 0 vs 123 identical");
        }
    }

    /// Canonical-source sanity: the oracle must reproduce closed-form
    /// reference formulas exactly (this replaces the dropped "cross-check
    /// vs Rust `_f32` impls" leg — those turned out to be Q32-delegating
    /// stubs with no independent float semantics).
    ///
    /// References use `libm` (the interpreter evaluates `@glsl::*` imports
    /// with `libm` too), so agreement is bit-level where evaluation order
    /// matches.
    #[test]
    fn oracle_matches_reference_formulas() {
        let oracle = Oracle::build(&probe_glsl()).expect("oracle build");
        let fract = |v: f32| v - libm::floorf(v);
        let seed_phase = |seed: u32| seed as f32 * (1.0 / 65536.0);

        // saturate / hue2rgb / hsv2rgb / rgb2hsv: exact formula mirrors.
        for &h in &[-0.25f32, 0.0, 0.109375, 0.328125, 0.5, 0.75, 1.0, 1.25] {
            let got = oracle.run("probe_hue2rgb", &[Value::F32(h)]).expect("run");
            let h6 = h * 6.0;
            let want = [
                ((h6 - 3.0).abs() - 1.0).clamp(0.0, 1.0),
                (2.0 - (h6 - 2.0).abs()).clamp(0.0, 1.0),
                (2.0 - (h6 - 4.0).abs()).clamp(0.0, 1.0),
            ];
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 1e-6, "hue2rgb({h}): {got:?} vs {want:?}");
            }
        }
        for &(x, seed) in &[(0.5f32, 0u32), (-3.25, 0), (7.75, 123), (0.0, 1)] {
            let got = oracle
                .run("probe_random1", &[Value::F32(x), Value::I32(seed as i32)])
                .expect("run")[0];
            let want = fract(libm::sinf(x + seed_phase(seed)) * 43758.5453);
            assert!(
                (got - want).abs() < 1e-6,
                "random1({x}, {seed}): {got} vs {want}"
            );
        }
        for &(x, y, seed) in &[(0.5f32, 1.25f32, 0u32), (-2.0, 3.5, 123)] {
            let got = oracle
                .run(
                    "probe_random2",
                    &[Value::F32(x), Value::F32(y), Value::I32(seed as i32)],
                )
                .expect("run")[0];
            let d = x * 12.9898 + y * 78.233;
            let want = fract(libm::sinf(d + seed_phase(seed)) * 43758.5453);
            assert!(
                (got - want).abs() < 1e-6,
                "random2(({x},{y}), {seed}): {got} vs {want}"
            );
        }
        // snoise1: gradient sign from the (bit-exact) integer hash.
        for &(x, seed) in &[(0.75f32, 0u32), (-1.5, 123), (4.25, 7)] {
            let got = oracle
                .run("probe_snoise1", &[Value::F32(x), Value::I32(seed as i32)])
                .expect("run")[0];
            let cell = libm::floorf(x);
            let dist = x - cell;
            let h = lps_builtins::builtins::lpfn::hash::lpfn_hash(cell as i32 as u32, seed);
            let grad = if h & 1 == 0 { 1.0f32 } else { -1.0 };
            let t = 1.0 - dist * dist;
            let want = if t > 0.0 {
                let (t2, t3) = (t * t, t * t * t);
                let (t4, t5) = (t2 * t2, t3 * t2);
                grad * dist * (6.0 * t5 - 15.0 * t4 + 10.0 * t3)
            } else {
                0.0
            };
            assert!(
                (got - want).abs() < 1e-5,
                "snoise1({x}, {seed}): {got} vs {want}"
            );
        }
    }

    /// The smooth (pointwise-conforming) noise builtins must be continuous:
    /// neighboring oracle samples stay within a Lipschitz-style bound.
    #[test]
    fn oracle_smooth_noise_is_continuous() {
        let oracle = Oracle::build(&probe_glsl()).expect("oracle build");
        let d = 1.0 / 128.0;
        // Generous Lipschitz bounds (|gradient| stays well under these).
        let checks: &[(&str, f32)] = &[
            ("probe_snoise2", 8.0),
            ("probe_worley2", 16.0),
            ("probe_gnoise2", 8.0),
        ];
        for &(probe, lipschitz) in checks {
            let budget = lipschitz * d * core::f32::consts::SQRT_2;
            let mut x = -3.0f32;
            while x < 3.0 {
                let mut y = -2.0f32;
                while y < 2.0 {
                    let a = oracle
                        .run(probe, &[Value::F32(x), Value::F32(y), Value::I32(0)])
                        .expect("run")[0];
                    let b = oracle
                        .run(
                            probe,
                            &[Value::F32(x + d), Value::F32(y + d), Value::I32(0)],
                        )
                        .expect("run")[0];
                    assert!(
                        (a - b).abs() <= budget,
                        "{probe} jump at ({x}, {y}): {a} -> {b} (budget {budget})"
                    );
                    y += 0.8046875;
                }
                x += 0.703125;
            }
        }
    }

    /// Optional annotated tier: the canonical GLSL compiled through the
    /// normal **Q32** pipeline vs the handwritten Rust `_q32` builtins, for
    /// the simple builtins where a naive Q32 compilation survives (math +
    /// color conversions).
    ///
    /// The generative family is deliberately excluded (`unsupported`):
    /// - random/srandom (and gnoise/fbm3_tile built on them): the sin-hash
    ///   multiplier 43758.5453 exceeds Q16.16's ±32768, so the compiled
    ///   constant saturates; the handwritten impls stage that multiply
    ///   through i64.
    /// - snoise/worley/psrdnoise: the handwritten impls rely on wrapping
    ///   ops, i64 staging, and quantized LUT constants that a naive Q32
    ///   compilation of the float algorithm does not reproduce (and the
    ///   float-permute variants of such algorithms overflow Q16.16 by
    ///   design — see the milestone's "why this shape" discovery note).
    /// The float oracle above is the binding conformance test for those.
    #[test]
    fn q32_compiled_canonicals_match_rust_q32_for_color_math() {
        // Only the color/math canonicals and their probes are compiled in
        // Q32 mode; see the exclusion rationale above.
        //
        // The wasm.q32 backend rejects modules with *overloaded* local GLSL
        // functions (duplicate export names fail wasm validation — verified
        // 2026-07-08, pre-existing backend limitation, not introduced
        // here). The canonical sources legitimately use overloading, so
        // this tier uniquifies the color/math overloads with an
        // assertion-guarded textual transform before compiling.
        let rust_probes = "\
float probe_saturate1(float x) { return lpfn_saturate(x); }\n\
vec3 probe_saturate3(vec3 v) { return lpfn_saturate(v); }\n\
vec4 probe_saturate4(vec4 v) { return lpfn_saturate(v); }\n\
vec3 probe_hue2rgb(float h) { return lpfn_hue2rgb(h); }\n\
vec3 probe_hsv2rgb3(vec3 hsv) { return lpfn_hsv2rgb(hsv); }\n\
vec4 probe_hsv2rgb4(vec4 hsv) { return lpfn_hsv2rgb(hsv); }\n\
vec3 probe_rgb2hsv3(vec3 rgb) { return lpfn_rgb2hsv(rgb); }\n\
vec4 probe_rgb2hsv4(vec4 rgb) { return lpfn_rgb2hsv(rgb); }\n";
        let canonical_probes = "\
float probe_saturate1(float x) { return lpo_saturate_f(x); }\n\
vec3 probe_saturate3(vec3 v) { return lpo_saturate_v3(v); }\n\
vec4 probe_saturate4(vec4 v) { return lpo_saturate_v4(v); }\n\
vec3 probe_hue2rgb(float h) { return lpo_hue2rgb(h); }\n\
vec3 probe_hsv2rgb3(vec3 hsv) { return lpo_hsv2rgb_v3(hsv); }\n\
vec4 probe_hsv2rgb4(vec4 hsv) { return lpo_hsv2rgb_v4(hsv); }\n\
vec3 probe_rgb2hsv3(vec3 rgb) { return lpo_rgb2hsv_v3(rgb); }\n\
vec4 probe_rgb2hsv4(vec4 rgb) { return lpo_rgb2hsv_v4(rgb); }\n";
        let keep = ["saturate", "hue2rgb", "hsv2rgb", "rgb2hsv"];
        let mut unit = super::oracle::canonical_subset_source(|name| keep.contains(&name), "");
        // Uniquify overload definitions and their (few) internal call
        // sites; each pattern must occur exactly once or the canonical
        // sources changed and this transform needs updating.
        for (from, to) in [
            (
                "float lpo_saturate(float x)",
                "float lpo_saturate_f(float x)",
            ),
            ("vec3 lpo_saturate(vec3 v)", "vec3 lpo_saturate_v3(vec3 v)"),
            ("vec4 lpo_saturate(vec4 v)", "vec4 lpo_saturate_v4(vec4 v)"),
            (
                "vec3 lpo_hsv2rgb(vec3 hsv)",
                "vec3 lpo_hsv2rgb_v3(vec3 hsv)",
            ),
            (
                "vec4 lpo_hsv2rgb(vec4 hsv)",
                "vec4 lpo_hsv2rgb_v4(vec4 hsv)",
            ),
            (
                "vec3 lpo_rgb2hsv(vec3 rgb)",
                "vec3 lpo_rgb2hsv_v3(vec3 rgb)",
            ),
            (
                "vec4 lpo_rgb2hsv(vec4 rgb)",
                "vec4 lpo_rgb2hsv_v4(vec4 rgb)",
            ),
            (
                "return lpo_saturate(vec3(r, g, b));",
                "return lpo_saturate_v3(vec3(r, g, b));",
            ),
            (
                "return vec4(lpo_hsv2rgb(hsv.xyz), hsv.w);",
                "return vec4(lpo_hsv2rgb_v3(hsv.xyz), hsv.w);",
            ),
            (
                "return vec4(lpo_rgb2hsv(rgb.xyz), rgb.w);",
                "return vec4(lpo_rgb2hsv_v3(rgb.xyz), rgb.w);",
            ),
        ] {
            assert_eq!(
                unit.matches(from).count(),
                1,
                "canonical source changed: expected exactly one `{from}`"
            );
            unit = unit.replace(from, to);
        }
        unit.push_str(canonical_probes);
        let mut compiled = Q32Probes::build(&unit).expect("q32-compiled canonical build");
        let mut rust = Q32Probes::build(rust_probes).expect("q32 probe build");

        let simple = [
            "saturate1",
            "saturate3",
            "saturate4",
            "hue2rgb",
            "hsv2rgb3",
            "hsv2rgb4",
            "rgb2hsv3",
            "rgb2hsv4",
        ];
        // Both sides are Q16.16; differences come from operation order and
        // constant rounding only.
        let tol = 2e-3;
        for spec in all_specs() {
            if !simple.contains(&spec.name) {
                continue;
            }
            let mut max_err = 0.0f32;
            for case in (spec.cases)() {
                let a = compiled
                    .run(spec.probe, &q32_args(&case))
                    .unwrap_or_else(|e| panic!("{}: compiled canonical: {e}", spec.name));
                let b = rust
                    .run(spec.probe, &q32_args(&case))
                    .unwrap_or_else(|e| panic!("{}: rust q32: {e}", spec.name));
                for (x, y) in a.iter().zip(&b) {
                    max_err = max_err.max((x - y).abs());
                }
                assert!(
                    a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tol),
                    "{}: q32-compiled canonical {a:?} vs rust q32 {b:?} at {case:?}",
                    spec.name
                );
            }
            println!("q32-mode {:<12} max_err {max_err:.6}", spec.name);
        }
    }

    fn stats(vals: &[f32]) -> (f32, f32, f32) {
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        let mut sum = 0.0f64;
        for &v in vals {
            lo = lo.min(v);
            hi = hi.max(v);
            sum += v as f64;
        }
        ((sum / vals.len() as f64) as f32, lo, hi)
    }

    fn oracle_args(case: &[Arg]) -> Vec<Value> {
        oracle_args_slice(case)
    }

    fn oracle_args_slice(case: &[Arg]) -> Vec<Value> {
        let mut out = Vec::new();
        for a in case {
            match *a {
                Arg::F32(v) => out.push(Value::F32(v)),
                Arg::Vec2(x, y) => out.extend([Value::F32(x), Value::F32(y)]),
                Arg::Vec3(x, y, z) => {
                    out.extend([Value::F32(x), Value::F32(y), Value::F32(z)]);
                }
                Arg::Vec4(x, y, z, w) => {
                    out.extend([Value::F32(x), Value::F32(y), Value::F32(z), Value::F32(w)])
                }
                Arg::I32(v) => out.push(Value::I32(v)),
                Arg::U32(v) => out.push(Value::I32(v as i32)),
            }
        }
        out
    }

    fn q32_args(case: &[Arg]) -> Vec<LpsValueF32> {
        q32_args_slice(case)
    }

    fn q32_args_slice(case: &[Arg]) -> Vec<LpsValueF32> {
        case.iter()
            .map(|a| match *a {
                Arg::F32(v) => LpsValueF32::F32(v),
                Arg::Vec2(x, y) => LpsValueF32::Vec2([x, y]),
                Arg::Vec3(x, y, z) => LpsValueF32::Vec3([x, y, z]),
                Arg::Vec4(x, y, z, w) => LpsValueF32::Vec4([x, y, z, w]),
                Arg::I32(v) => LpsValueF32::I32(v),
                Arg::U32(v) => LpsValueF32::U32(v),
            })
            .collect()
    }
}
