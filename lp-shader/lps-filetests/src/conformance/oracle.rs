//! Float oracle: canonical GLSL → `lps-frontend` (naga) → LPIR → f32
//! interpretation (`lpir::interpret`).
//!
//! `lps-frontend` reserves the `lpfn_` name prefix: every call to an
//! `lpfn_*` function is lowered to an `@lpfn::` builtin import, even when
//! the module defines a body for it. To compile the canonical sources as
//! ordinary local GLSL functions, the assembly step renames the `lpfn_`
//! identifier prefix to `lpo_` ("lp oracle") before handing the unit to the
//! frontend. Probe functions (unique, un-prefixed names) are appended and
//! serve as `interpret` entry points, so GLSL overload resolution still
//! happens in naga.
//!
//! Transcendental imports (`@glsl::sin` etc., `@lpir::sqrt`) are evaluated
//! host-side in f32 by `lps_frontend::std_math_handler::StdMathHandler`
//! (libm) — the interpreter itself delegates all imports to the caller.

use anyhow::Context;
use lpir::{LpirModule, Value, interpret};
use lps_builtins::canonical_glsl::CANONICAL_GLSL;
use lps_frontend::std_math_handler::StdMathHandler;

/// Compiled oracle module: all canonical sources (renamed) + probes.
pub struct Oracle {
    ir: LpirModule,
}

/// Assemble the canonical compilation unit: every canonical source plus
/// `probes`, with the `lpfn_` → `lpo_` rename applied throughout so the
/// canonical bodies compile as ordinary local GLSL functions.
pub fn canonical_unit_source(probes: &str) -> String {
    canonical_subset_source(|_| true, probes)
}

/// Assemble a reduced canonical unit containing only the sources selected
/// by `keep` (dependency order preserved), plus `probes`; everything gets
/// the `lpfn_` → `lpo_` rename (used by the Q32-compiled canonical tier,
/// which only exercises the simple builtins).
pub fn canonical_subset_source(keep: impl Fn(&str) -> bool, probes: &str) -> String {
    let mut src = String::new();
    // CANONICAL_GLSL is dependency-ordered (asserted by its unit tests),
    // so plain concatenation satisfies GLSL declaration-before-use.
    for c in CANONICAL_GLSL {
        if keep(c.name) {
            src.push_str(&rename_lpfn_prefix(c.source));
            src.push('\n');
        }
    }
    src.push_str(&rename_lpfn_prefix(probes));
    src
}

impl Oracle {
    /// Assemble and compile the oracle module. `probes` is GLSL text that
    /// calls `lpfn_*` functions; it is renamed alongside the canonicals.
    pub fn build(probes: &str) -> anyhow::Result<Self> {
        let src = canonical_unit_source(probes);
        let naga = lps_frontend::compile(&src)
            .map_err(|e| anyhow::anyhow!("oracle GLSL compile: {e:?}"))?;
        let (ir, _meta) =
            lps_frontend::lower(&naga).map_err(|e| anyhow::anyhow!("oracle lower: {e}"))?;
        Ok(Self { ir })
    }

    /// Run a probe entry point; returns the flattened scalar results.
    pub fn run(&self, entry: &str, args: &[Value]) -> anyhow::Result<Vec<f32>> {
        let mut handler = StdMathHandler::default();
        let out = interpret(&self.ir, entry, args, &mut handler)
            .map_err(|e| anyhow::anyhow!("oracle interpret {entry}: {e}"))?;
        out.iter()
            .map(|v| {
                v.as_f32()
                    .with_context(|| format!("{entry}: non-f32 result {v:?}"))
            })
            .collect()
    }

    /// Run a probe entry point that returns integer/uint scalars.
    pub fn run_i32(&self, entry: &str, args: &[Value]) -> anyhow::Result<Vec<i32>> {
        let mut handler = StdMathHandler::default();
        let out = interpret(&self.ir, entry, args, &mut handler)
            .map_err(|e| anyhow::anyhow!("oracle interpret {entry}: {e}"))?;
        out.iter()
            .map(|v| {
                v.as_i32()
                    .with_context(|| format!("{entry}: non-i32 result {v:?}"))
            })
            .collect()
    }
}

/// Rename the `lpfn_` identifier prefix to `lpo_` at identifier boundaries.
pub fn rename_lpfn_prefix(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_boundary && src[i..].starts_with("lpfn_") {
            out.push_str("lpo_");
            i += "lpfn_".len();
        } else {
            // Advance one UTF-8 scalar (sources are ASCII; be safe anyway).
            let ch = src[i..].chars().next().expect("in-bounds char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_respects_identifier_boundaries() {
        assert_eq!(rename_lpfn_prefix("lpfn_hash(x)"), "lpo_hash(x)");
        assert_eq!(rename_lpfn_prefix("my_lpfn_hash"), "my_lpfn_hash");
        assert_eq!(rename_lpfn_prefix("a lpfn_a(lpfn_b)"), "a lpo_a(lpo_b)");
    }

    #[test]
    fn oracle_builds_and_runs_scalar_probe() {
        let oracle = Oracle::build("float probe_saturate1(float x) { return lpfn_saturate(x); }\n")
            .expect("oracle build");
        let out = oracle
            .run("probe_saturate1", &[Value::F32(1.5)])
            .expect("run");
        assert_eq!(out, vec![1.0]);
    }

    #[test]
    fn oracle_hash_matches_rust_bit_for_bit() {
        let oracle = Oracle::build(
            "uint probe_hash1(uint x, uint seed) { return lpfn_hash(x, seed); }\n\
             uint probe_hash2(uvec2 v, uint seed) { return lpfn_hash(v, seed); }\n\
             uint probe_hash3(uvec3 v, uint seed) { return lpfn_hash(v, seed); }\n",
        )
        .expect("oracle build");
        for &(x, y, z, seed) in &[
            (0u32, 0u32, 0u32, 0u32),
            (1, 2, 3, 0),
            (42, 1337, 7, 123),
            (0xFFFF_FFFF, 0x8000_0000, 12345, 0xDEAD_BEEF),
        ] {
            let got1 = oracle
                .run_i32(
                    "probe_hash1",
                    &[Value::I32(x as i32), Value::I32(seed as i32)],
                )
                .expect("hash1");
            assert_eq!(
                got1[0] as u32,
                lps_builtins::builtins::lpfn::hash::lpfn_hash(x, seed),
                "hash1({x}, {seed})"
            );
            let got2 = oracle
                .run_i32(
                    "probe_hash2",
                    &[
                        Value::I32(x as i32),
                        Value::I32(y as i32),
                        Value::I32(seed as i32),
                    ],
                )
                .expect("hash2");
            assert_eq!(
                got2[0] as u32,
                lps_builtins::builtins::lpfn::hash::lpfn_hash2(x, y, seed),
                "hash2({x}, {y}, {seed})"
            );
            let got3 = oracle
                .run_i32(
                    "probe_hash3",
                    &[
                        Value::I32(x as i32),
                        Value::I32(y as i32),
                        Value::I32(z as i32),
                        Value::I32(seed as i32),
                    ],
                )
                .expect("hash3");
            assert_eq!(
                got3[0] as u32,
                lps_builtins::builtins::lpfn::hash::lpfn_hash3(x, y, z, seed),
                "hash3({x}, {y}, {z}, {seed})"
            );
        }
    }
}
