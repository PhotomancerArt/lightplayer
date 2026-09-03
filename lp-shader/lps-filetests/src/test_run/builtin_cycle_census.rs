//! Cycle census for the hot Q32 math builtins on the RV32 emulator.
//!
//! One tiny GLSL module wraps each builtin behind the corpus's runtime-opaque
//! `rt(x) = x + u_runtime_zero` pattern (so nothing folds at compile time),
//! compiles it for `rv32n.q32` (`lpvm-native` → RV32 emulator + the linked
//! builtins image) and sweeps a fixed, documented input set per builtin,
//! reading the guest cycle count of every call under
//! [`CycleModel::Esp32C6`]. The cost of the wrapper itself (`ident` /
//! `ident2`) is measured the same way and subtracted, so the numbers reported
//! are **cycles per builtin call**: min / median / max over the sweep, with
//! the input at the max.
//!
//! Absolute numbers are on the filetests image, which
//! `scripts/build-builtins.sh` compiles at `opt-level=1`; the profiler and
//! the device compile `lps-builtins` at `opt-level=3`. The census exists to
//! rank builtins and to measure deltas across a rewrite; cross-check absolute
//! values against `lp-cli profile function` on a real workload.
//!
//! Run it (the image must exist — `scripts/build-builtins.sh` — and the run
//! takes a few seconds, so the table test is ignored by default):
//!
//! ```bash
//! cargo test -p lps-filetests --release builtin_cycle_census -- --ignored --nocapture
//! ```
//!
//! Set `LP_CENSUS_DETAIL=1` to also print every sample (input → cycles) under
//! each row, which is how a rewrite's effect at a specific input is read.
//!
//! The non-ignored smoke test compiles the same module and runs one call, so
//! the harness cannot rot silently.

use anyhow::{Context, Result};
use lp_collection::VecMap;
use lp_emu_core::{CycleModel, LogLevel};
use lpir::CompilerConfig;
use lpvm::LpsValueF32;

use crate::targets::Target;
use crate::test_run::execution;
use crate::test_run::filetest_lpvm::CompiledShader;

/// The census module. Every wrapper routes its arguments through `rt` so the
/// builtin sees runtime values; `ident`/`ident2` measure that plumbing alone.
const CENSUS_GLSL: &str = r#"
layout(binding = 0) uniform float u_runtime_zero;
float rt(float x) { return x + u_runtime_zero; }
float ident(float x) { return rt(x); }
float ident2(float x, float y) { return rt(x) + rt(y); }
float f_exp(float x) { return exp(rt(x)); }
float f_sqrt(float x) { return sqrt(rt(x)); }
float f_inversesqrt(float x) { return inversesqrt(rt(x)); }
float f_sin(float x) { return sin(rt(x)); }
float f_cos(float x) { return cos(rt(x)); }
float f_div(float x, float y) { return rt(x) / rt(y); }
float f_mod(float x, float y) { return mod(rt(x), rt(y)); }
"#;

const TARGET: &str = "rv32n.q32";

const EXP_INPUTS: &[f32] = &[
    -11.0, -8.0, -6.0, -4.0, -2.0, -1.0, -0.5, -0.1, 0.1, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0,
];
const SQRT_INPUTS: &[f32] = &[
    1e-4, 0.01, 0.25, 0.5, 1.0, 2.0, 3.0, 10.0, 100.0, 1000.0, 30000.0,
];
const DIV_LHS: &[f32] = &[0.03, 0.5, 1.0, 3.0, 7.0, 1000.0];
const DIV_RHS: &[f32] = &[0.02, -0.5, 1.0, 3.0, 100.0];
const MOD_PAIRS: &[(f32, f32)] = &[(7.0, 3.0), (-7.5, 2.0), (100.0, 0.7), (0.3, 1.0)];

/// Angles across `[-4π, 4π]` plus the exact quarter points.
fn trig_inputs() -> Vec<f32> {
    let pi = core::f32::consts::PI;
    let mut v: Vec<f32> = (0..24)
        .map(|i| -4.0 * pi + (i as f32) * (8.0 * pi / 23.0))
        .collect();
    v.extend([0.0, pi / 2.0, -pi / 2.0]);
    v
}

/// One row of the census table.
struct Row {
    name: &'static str,
    min: u64,
    median: u64,
    max: u64,
    max_at: String,
    calls: usize,
    /// Every (cycles, input) sample, sorted by cycles.
    samples: Vec<(u64, String)>,
}

struct Census {
    compiled: CompiledShader,
    target: &'static Target,
    /// Median guest cycles of `ident` (one arg) and `ident2` (two args).
    overhead1: u64,
    overhead2: u64,
}

impl Census {
    fn new() -> Result<Self> {
        let target = Target::from_name(TARGET).map_err(|e| anyhow::anyhow!(e))?;
        let compiled = CompiledShader::compile_glsl(
            CENSUS_GLSL,
            target,
            LogLevel::None,
            &CompilerConfig::default(),
            &VecMap::new(),
        )
        .context(
            "compile census module for rv32n.q32 (is the builtins image built? \
             run scripts/build-builtins.sh)",
        )?;
        let mut census = Self {
            compiled,
            target,
            overhead1: 0,
            overhead2: 0,
        };
        let one: Vec<u64> = EXP_INPUTS
            .iter()
            .map(|&x| census.raw_cycles("ident", &[LpsValueF32::F32(x)]))
            .collect::<Result<_>>()?;
        let two: Vec<u64> = DIV_LHS
            .iter()
            .map(|&x| census.raw_cycles("ident2", &[LpsValueF32::F32(x), LpsValueF32::F32(2.0)]))
            .collect::<Result<_>>()?;
        census.overhead1 = median(&one);
        census.overhead2 = median(&two);
        Ok(census)
    }

    /// Guest cycles of one call, wrapper included.
    fn raw_cycles(&self, name: &str, args: &[LpsValueF32]) -> Result<u64> {
        let (_, cycles) = self.call(name, args)?;
        Ok(cycles)
    }

    fn call(&self, name: &str, args: &[LpsValueF32]) -> Result<(LpsValueF32, u64)> {
        let mut inst = self.compiled.instantiate()?;
        inst.set_uniform("u_runtime_zero", &LpsValueF32::F32(0.0))
            .map_err(|e| anyhow::anyhow!("set u_runtime_zero: {e}"))?;
        let gfn = self
            .compiled
            .get_function_signature(name)
            .with_context(|| format!("census function `{name}` missing from module signature"))?;
        let value =
            execution::execute_function(&mut inst, self.target, gfn, name, args, CycleModel::Esp32C6)?;
        let cycles = inst
            .last_guest_cycle_count()
            .with_context(|| format!("no guest cycle count after `{name}`"))?;
        Ok((value, cycles))
    }

    fn sweep1(&self, name: &'static str, inputs: &[f32]) -> Result<Row> {
        let samples = inputs
            .iter()
            .map(|&x| {
                self.raw_cycles(name, &[LpsValueF32::F32(x)])
                    .map(|c| (c.saturating_sub(self.overhead1), format!("{x}")))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(summarize(name, samples))
    }

    fn sweep2(&self, name: &'static str, pairs: &[(f32, f32)]) -> Result<Row> {
        let samples = pairs
            .iter()
            .map(|&(x, y)| {
                self.raw_cycles(name, &[LpsValueF32::F32(x), LpsValueF32::F32(y)])
                    .map(|c| (c.saturating_sub(self.overhead2), format!("{x}, {y}")))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(summarize(name, samples))
    }

    fn table(&self) -> Result<Vec<Row>> {
        let div_pairs: Vec<(f32, f32)> = DIV_LHS
            .iter()
            .flat_map(|&x| DIV_RHS.iter().map(move |&y| (x, y)))
            .collect();
        let trig = trig_inputs();
        Ok(vec![
            self.sweep1("f_exp", EXP_INPUTS)?,
            self.sweep1("f_sqrt", SQRT_INPUTS)?,
            self.sweep1("f_inversesqrt", SQRT_INPUTS)?,
            self.sweep1("f_sin", &trig)?,
            self.sweep1("f_cos", &trig)?,
            self.sweep2("f_div", &div_pairs)?,
            self.sweep2("f_mod", MOD_PAIRS)?,
        ])
    }
}

fn summarize(name: &'static str, mut samples: Vec<(u64, String)>) -> Row {
    samples.sort_by_key(|(c, _)| *c);
    let cycles: Vec<u64> = samples.iter().map(|(c, _)| *c).collect();
    let (max, max_at) = samples.last().cloned().unwrap_or((0, String::new()));
    Row {
        name,
        min: cycles.first().copied().unwrap_or(0),
        median: median(&cycles),
        max,
        max_at,
        calls: cycles.len(),
        samples,
    }
}

fn median(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2
    }
}

fn render_markdown(census: &Census, rows: &[Row]) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "Census target `{TARGET}`, cycle model Esp32C6; wrapper overhead subtracted: \
         ident={} cycles, ident2={} cycles\n\n",
        census.overhead1, census.overhead2
    ));
    s.push_str("| builtin | calls | min | median | max | max at |\n");
    s.push_str("|---|---:|---:|---:|---:|---|\n");
    for r in rows {
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            r.name, r.calls, r.min, r.median, r.max, r.max_at
        ));
    }
    if std::env::var_os("LP_CENSUS_DETAIL").is_some() {
        for r in rows {
            s.push_str(&format!("\n`{}` samples (cycles @ input):\n", r.name));
            for (c, at) in &r.samples {
                s.push_str(&format!("  {c} @ {at}\n"));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full census table. Ignored: needs the rv32 builtins image and takes
    /// a few seconds. See the module doc for the command.
    #[test]
    #[ignore = "needs scripts/build-builtins.sh; prints the census table"]
    fn builtin_cycle_census_table() {
        let census = Census::new().expect("census module compiles for rv32n.q32");
        let rows = census.table().expect("census sweep");
        println!("\n{}", render_markdown(&census, &rows));
        assert!(rows.iter().all(|r| r.calls > 0 && r.max > 0));
    }

    /// Cheap guard that the harness still compiles and reads cycles.
    #[test]
    fn builtin_cycle_census_smoke() {
        let census = Census::new().expect("census module compiles for rv32n.q32");
        let (value, cycles) = census
            .call("f_sqrt", &[LpsValueF32::F32(4.0)])
            .expect("sqrt call");
        match value {
            LpsValueF32::F32(v) => assert!((v - 2.0).abs() < 1e-3, "sqrt(4) = {v}"),
            other => panic!("unexpected return {other:?}"),
        }
        assert!(cycles > census.overhead1, "sqrt call ({cycles}) must cost more than the wrapper ({})", census.overhead1);
    }
}
