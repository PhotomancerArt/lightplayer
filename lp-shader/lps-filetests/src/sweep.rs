//! Corpus sweep for a single target: run every `// run:` directive and emit
//! one JSONL record per directive (pass and fail alike).
//!
//! This is the Agent-A half of the filetest-expectations P2 phase: the JSONL
//! feeds the human-triaged `p2-triage-report.md` (tolerance-bump / per-mode /
//! unsupported / bug? dispositions). It never mutates corpus files.

use std::path::PathBuf;

use lp_riscv_emu::LogLevel;
use lps_shared::LpsValueF32;
use walkdir::WalkDir;

use crate::parse::{self, ComparisonOp, TestType};
use crate::perf_model::PerfModel;
use crate::targets::Target;
use crate::test_run::filetest_lpvm::CompiledShader;
use crate::test_run::{compile, execution, parse_assert, set_uniform, texture_fixture};
use crate::util::format_glsl_value;

/// One sweep result row.
pub struct SweepRecord {
    /// Corpus-relative file path.
    pub file: String,
    /// `// run:` line number.
    pub line: usize,
    /// Expression text (e.g. `f(1.0)`).
    pub expr: String,
    /// Comparison operator (`==` / `~=`).
    pub op: &'static str,
    /// Expected value text from the directive.
    pub expected: String,
    /// Formatted actual value (when execution produced one).
    pub actual: Option<String>,
    /// Max absolute component delta between expected and actual, when the
    /// shapes are comparable.
    pub delta: Option<f64>,
    /// Explicit per-directive tolerance (None = suite default 5e-3).
    pub tolerance: Option<f32>,
    /// pass | value-mismatch | exec-error | setup-error | compile-fail | harness-error
    pub status: &'static str,
    /// Error text for non-pass statuses without a comparable value.
    pub error: Option<String>,
}

impl SweepRecord {
    /// Serialize as a single JSON object line (hand-rolled; no serde dep).
    pub fn to_json_line(&self) -> String {
        let mut s = String::from("{");
        push_kv(&mut s, "file", &self.file);
        s.push(',');
        s.push_str(&format!("\"line\":{}", self.line));
        s.push(',');
        push_kv(&mut s, "expr", &self.expr);
        s.push(',');
        push_kv(&mut s, "op", self.op);
        s.push(',');
        push_kv(&mut s, "expected", &self.expected);
        s.push(',');
        match &self.actual {
            Some(a) => {
                push_kv(&mut s, "actual", a);
                s.push(',');
            }
            None => {}
        }
        // Non-finite deltas (NaN/Inf divergence) are omitted: bare `inf` /
        // `NaN` are not valid JSON; the `actual` string still carries them.
        if let Some(d) = self.delta {
            if d.is_finite() {
                s.push_str(&format!("\"delta\":{d}"));
                s.push(',');
            }
        }
        if let Some(t) = self.tolerance {
            s.push_str(&format!("\"tolerance\":{t}"));
            s.push(',');
        }
        push_kv(&mut s, "status", self.status);
        if let Some(e) = &self.error {
            s.push(',');
            push_kv(&mut s, "error", e);
        }
        s.push('}');
        s
    }
}

fn push_kv(s: &mut String, k: &str, v: &str) {
    s.push('"');
    s.push_str(k);
    s.push_str("\":\"");
    for ch in v.chars() {
        match ch {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
}

/// Max absolute component delta between two values with matching shape.
pub fn max_abs_delta(a: &LpsValueF32, b: &LpsValueF32) -> Option<f64> {
    fn fold(items: impl IntoIterator<Item = f64>) -> Option<f64> {
        let mut m: Option<f64> = None;
        for d in items {
            m = Some(m.map_or(d, |x: f64| x.max(d)));
        }
        m
    }
    use LpsValueF32 as V;
    match (a, b) {
        (V::F32(x), V::F32(y)) => Some(((*x as f64) - (*y as f64)).abs()),
        (V::I32(x), V::I32(y)) => Some(((*x as f64) - (*y as f64)).abs()),
        (V::U32(x), V::U32(y)) => Some(((*x as f64) - (*y as f64)).abs()),
        (V::Bool(x), V::Bool(y)) => Some(if x == y { 0.0 } else { 1.0 }),
        (V::Vec2(x), V::Vec2(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::Vec3(x), V::Vec3(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::Vec4(x), V::Vec4(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::IVec2(x), V::IVec2(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::IVec3(x), V::IVec3(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::IVec4(x), V::IVec4(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::UVec2(x), V::UVec2(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::UVec3(x), V::UVec3(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::UVec4(x), V::UVec4(y)) => fold(
            x.iter()
                .zip(y)
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::BVec2(x), V::BVec2(y)) => {
            fold(x.iter().zip(y).map(|(p, q)| if p == q { 0.0 } else { 1.0 }))
        }
        (V::BVec3(x), V::BVec3(y)) => {
            fold(x.iter().zip(y).map(|(p, q)| if p == q { 0.0 } else { 1.0 }))
        }
        (V::BVec4(x), V::BVec4(y)) => {
            fold(x.iter().zip(y).map(|(p, q)| if p == q { 0.0 } else { 1.0 }))
        }
        (V::Mat2x2(x), V::Mat2x2(y)) => fold(
            x.iter()
                .flatten()
                .zip(y.iter().flatten())
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::Mat3x3(x), V::Mat3x3(y)) => fold(
            x.iter()
                .flatten()
                .zip(y.iter().flatten())
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::Mat4x4(x), V::Mat4x4(y)) => fold(
            x.iter()
                .flatten()
                .zip(y.iter().flatten())
                .map(|(p, q)| ((*p as f64) - (*q as f64)).abs()),
        ),
        (V::Array(x), V::Array(y)) if x.len() == y.len() => {
            let mut m: Option<f64> = None;
            for (p, q) in x.iter().zip(y.iter()) {
                let d = max_abs_delta(p, q)?;
                m = Some(m.map_or(d, |v: f64| v.max(d)));
            }
            m
        }
        (V::Struct { fields: fx, .. }, V::Struct { fields: fy, .. }) if fx.len() == fy.len() => {
            let mut m: Option<f64> = None;
            for ((_, p), (_, q)) in fx.iter().zip(fy.iter()) {
                let d = max_abs_delta(p, q)?;
                m = Some(m.map_or(d, |v: f64| v.max(d)));
            }
            m
        }
        _ => None,
    }
}

/// Run the whole corpus on `target`, returning one record per applicable
/// `// run:` directive. Never mutates any corpus file.
pub fn sweep_corpus(target: &Target) -> anyhow::Result<Vec<SweepRecord>> {
    let filetests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");
    let mut files: Vec<PathBuf> = WalkDir::new(&filetests_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("glsl"))
        .collect();
    files.sort();

    let mut out = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&filetests_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        sweep_file(path, &rel, target, &mut out);
    }
    Ok(out)
}

fn op_str(op: ComparisonOp) -> &'static str {
    match op {
        ComparisonOp::Exact => "==",
        ComparisonOp::Approx => "~=",
    }
}

fn sweep_file(path: &std::path::Path, rel: &str, target: &Target, out: &mut Vec<SweepRecord>) {
    // `// test parse-error` files intentionally fail to parse; skip.
    if std::fs::read_to_string(path)
        .map(|s| s.lines().any(|l| l.trim() == "// test parse-error"))
        .unwrap_or(false)
    {
        return;
    }
    let tf = match parse::parse_test_file(path) {
        Ok(tf) => tf,
        Err(e) => {
            out.push(SweepRecord {
                file: rel.to_string(),
                line: 0,
                expr: String::new(),
                op: "",
                expected: String::new(),
                actual: None,
                delta: None,
                tolerance: None,
                status: "harness-error",
                error: Some(format!("parse: {e:#}")),
            });
            return;
        }
    };
    if !tf.test_types.contains(&TestType::Run) || tf.test_types.contains(&TestType::Error) {
        return;
    }

    let applicable: Vec<_> = tf
        .run_directives
        .iter()
        .filter(|d| d.mode_filter.applies_to(target))
        .collect();
    if applicable.is_empty() {
        return;
    }

    let compiler_config = match compile::build_compiler_config(&tf.config_overrides) {
        Ok(c) => c,
        Err(e) => {
            for d in &applicable {
                out.push(record_error(rel, d, "harness-error", format!("{e:#}")));
            }
            return;
        }
    };
    let compiled = match compile::compile_for_target(
        &tf.glsl_source,
        target,
        rel,
        LogLevel::None,
        &compiler_config,
        &tf.texture_specs,
    ) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("{e:#}");
            for d in &applicable {
                out.push(record_error(rel, d, "compile-fail", msg.clone()));
            }
            return;
        }
    };

    let cycle_model = PerfModel::default().cycle_model();
    for d in &applicable {
        out.push(sweep_directive(rel, d, &tf, &compiled, target, cycle_model));
    }
}

fn record_error(
    rel: &str,
    d: &parse::RunDirective,
    status: &'static str,
    error: String,
) -> SweepRecord {
    SweepRecord {
        file: rel.to_string(),
        line: d.line_number,
        expr: d.expression_str.clone(),
        op: op_str(d.comparison),
        expected: d.expected_str.clone(),
        actual: None,
        delta: None,
        tolerance: d.tolerance,
        status,
        error: Some(error),
    }
}

fn sweep_directive(
    rel: &str,
    d: &parse::RunDirective,
    tf: &parse::TestFile,
    compiled: &CompiledShader,
    target: &Target,
    cycle_model: lp_riscv_emu::CycleModel,
) -> SweepRecord {
    let (func_name, arg_strings) = match parse_assert::parse_function_call(&d.expression_str) {
        Ok(v) => v,
        Err(e) => return record_error(rel, d, "harness-error", format!("parse call: {e:#}")),
    };
    let args = match parse_assert::parse_function_arguments(&arg_strings) {
        Ok(v) => v,
        Err(e) => return record_error(rel, d, "harness-error", format!("parse args: {e:#}")),
    };
    let Some(gfn) = compiled.get_function_signature(&func_name) else {
        return record_error(
            rel,
            d,
            "exec-error",
            format!("function '{func_name}' not found"),
        );
    };
    let mut inst = match compiled.instantiate() {
        Ok(i) => i,
        Err(e) => return record_error(rel, d, "setup-error", format!("instantiate: {e:#}")),
    };

    let _tex = match texture_fixture::bind_texture_fixtures_for_run(
        compiled,
        &mut inst,
        &tf.texture_specs,
        &tf.texture_fixtures,
    ) {
        Ok(v) => {
            if d.expected_setup_failure.is_some() {
                return record_error(
                    rel,
                    d,
                    "setup-error",
                    "expected setup failure but bind succeeded".to_string(),
                );
            }
            v
        }
        Err(e) => {
            let msg = format!("{e:#}");
            if let Some(exp) = d.expected_setup_failure.as_ref() {
                if msg.contains(exp.as_str()) {
                    return SweepRecord {
                        file: rel.to_string(),
                        line: d.line_number,
                        expr: d.expression_str.clone(),
                        op: op_str(d.comparison),
                        expected: d.expected_str.clone(),
                        actual: None,
                        delta: None,
                        tolerance: d.tolerance,
                        status: "pass",
                        error: None,
                    };
                }
            }
            return record_error(rel, d, "setup-error", format!("texture bind: {msg}"));
        }
    };

    if let Err(e) =
        set_uniform::apply_set_uniforms(&mut inst, compiled.module_sig(), &d.set_uniforms)
    {
        return record_error(rel, d, "setup-error", format!("set_uniform: {e:#}"));
    }

    let actual =
        match execution::execute_function(&mut inst, target, gfn, &func_name, &args, cycle_model) {
            Ok(v) => v,
            Err(e) => return record_error(rel, d, "exec-error", format!("{e:#}")),
        };

    let expected = match parse_assert::parse_glsl_value(&d.expected_str) {
        Ok(v) => v,
        Err(e) => return record_error(rel, d, "harness-error", format!("parse expected: {e:#}")),
    };

    let delta = max_abs_delta(&expected, &actual);
    let cmp = parse_assert::compare_results(&actual, &expected, d.comparison, d.tolerance);
    SweepRecord {
        file: rel.to_string(),
        line: d.line_number,
        expr: d.expression_str.clone(),
        op: op_str(d.comparison),
        expected: d.expected_str.clone(),
        actual: Some(format_glsl_value(&actual)),
        delta,
        tolerance: d.tolerance,
        status: if cmp.is_ok() {
            "pass"
        } else {
            "value-mismatch"
        },
        error: cmp.err(),
    }
}
