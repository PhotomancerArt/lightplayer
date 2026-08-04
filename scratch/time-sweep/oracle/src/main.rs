//! P5 sweep oracle: A/B the original and converted GLSL bodies on the LPIR
//! f32 interpreter and report the worst per-component difference.
//!
//! Reads `../cases.json` (written by `sweep.py cases`), writes
//! `../oracle-results.json`. Disposable — deleted in P9.

use std::collections::BTreeMap;

use lp_collection::VecMap;
use lps_probe::{
    BindingValue, ExperimentSpec, ProbeDomain, ProbeOutcome, ProbeReduce, ProbeSpec, ProbeType,
    ShaderCompileOutcome, run_experiment,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Cases {
    threshold: f32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    original: String,
    converted: String,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    t: f32,
    original: BTreeMap<String, f32>,
    converted: BTreeMap<String, f32>,
}

#[derive(Serialize)]
struct Results {
    threshold: f32,
    results: Vec<CaseResult>,
}

#[derive(Serialize)]
struct CaseResult {
    id: String,
    max_abs_diff: f32,
    worst_t: f32,
    per_t: Vec<(f32, f32)>,
    /// Fraction of the converted grid whose luminance is above black, per t —
    /// the "still renders something" check.
    lit_fraction: Vec<(f32, f32)>,
    /// The same measure on the ORIGINAL body — "non-black where the original
    /// was non-black" is only a claim if both sides are recorded.
    orig_lit_fraction: Vec<(f32, f32)>,
    /// Sanity: how much the ORIGINAL body moves between t=0 and each t. A
    /// case whose self-variation is 0 is not exercising the timebase at all,
    /// so its A/B agreement would prove nothing.
    self_variation: Vec<(f32, f32)>,
    status: String,
    notes: Vec<String>,
}

const GRID: u16 = 8;
const SIZE: [u32; 2] = [32, 32];

fn spec(bindings: &BTreeMap<String, f32>) -> ExperimentSpec {
    let mut map: VecMap<String, BindingValue> = VecMap::new();
    for (k, v) in bindings {
        map.insert(k.clone(), BindingValue::Scalar(*v));
    }
    ExperimentSpec {
        size: SIZE,
        bindings: map,
        probes: vec![ProbeSpec {
            id: "o".into(),
            ty: ProbeType::Vec4,
            expr: "render(pos)".into(),
            domain: ProbeDomain::Grid {
                nx: GRID,
                ny: GRID,
                rect: None,
            },
            vary: None,
            reduce: ProbeReduce::None,
        }],
        led_points: Vec::new(),
    }
}

fn values(source: &str, bindings: &BTreeMap<String, f32>, notes: &mut Vec<String>) -> Vec<Vec<f32>> {
    let spec = spec(bindings);
    let result = run_experiment(source, &spec);
    if let ShaderCompileOutcome::Err { diagnostics } = &result.shader {
        notes.push(format!("compile failed: {diagnostics:?}"));
        return Vec::new();
    }
    for warning in &result.warnings {
        let note = format!("warning: {warning}");
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    match result.probes.get(&"o".to_string()) {
        Some(ProbeOutcome::Values(v)) => v.rows.iter().map(|row| row.value.clone()).collect(),
        Some(other) => {
            notes.push(format!("probe outcome: {other:?}"));
            Vec::new()
        }
        None => {
            notes.push("probe missing".into());
            Vec::new()
        }
    }
}

fn main() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let cases: Cases =
        serde_json::from_reader(std::fs::File::open(here.join("cases.json")).unwrap()).unwrap();

    let mut out = Vec::new();
    for case in &cases.cases {
        let mut notes = Vec::new();
        let mut per_t = Vec::new();
        let mut lit = Vec::new();
        let mut orig_lit = Vec::new();
        let mut worst = 0.0f32;
        let mut worst_t = 0.0f32;
        let mut base: Vec<Vec<f32>> = Vec::new();
        let mut self_var = Vec::new();
        for step in &case.steps {
            let a = values(&case.original, &step.original, &mut notes);
            let b = values(&case.converted, &step.converted, &mut notes);
            if base.is_empty() {
                base = a.clone();
            } else if !a.is_empty() && a.len() == base.len() {
                let mut d = 0.0f32;
                for (ra, rb) in a.iter().zip(base.iter()) {
                    for (x, y) in ra.iter().zip(rb.iter()) {
                        d = d.max((x - y).abs());
                    }
                }
                self_var.push((step.t, d));
            }
            if a.is_empty() || b.is_empty() || a.len() != b.len() {
                notes.push(format!("no comparable rows at t={}", step.t));
                continue;
            }
            let mut step_max = 0.0f32;
            let mut nonblack = 0usize;
            let mut nonblack_a = 0usize;
            for (ra, rb) in a.iter().zip(b.iter()) {
                if ra.iter().take(3).any(|c| *c > 1.0 / 255.0) {
                    nonblack_a += 1;
                }
                for (x, y) in ra.iter().zip(rb.iter()) {
                    let d = (x - y).abs();
                    if d > step_max {
                        step_max = d;
                    }
                }
                if rb.iter().take(3).any(|c| *c > 1.0 / 255.0) {
                    nonblack += 1;
                }
            }
            per_t.push((step.t, step_max));
            lit.push((step.t, nonblack as f32 / b.len() as f32));
            orig_lit.push((step.t, nonblack_a as f32 / a.len() as f32));
            if step_max > worst {
                worst = step_max;
                worst_t = step.t;
            }
        }
        let status = if !notes.iter().all(|n| n.starts_with("warning:")) {
            "error".to_string()
        } else if worst <= cases.threshold {
            "pass".to_string()
        } else {
            "over-threshold".to_string()
        };
        let motion = self_var.iter().fold(0.0f32, |m, (_, d)| m.max(*d));
        println!(
            "{:<22} max|diff| = {:.4e} @ t={}  [{}]  (body moves {:.3} over the grid)",
            case.id, worst, worst_t, status, motion
        );
        for note in &notes {
            println!("    {note}");
        }
        out.push(CaseResult {
            id: case.id.clone(),
            max_abs_diff: worst,
            worst_t,
            per_t,
            lit_fraction: lit,
            orig_lit_fraction: orig_lit,
            self_variation: self_var,
            status,
            notes,
        });
    }

    serde_json::to_writer_pretty(
        std::fs::File::create(here.join("oracle-results.json")).unwrap(),
        &Results {
            threshold: cases.threshold,
            results: out,
        },
    )
    .unwrap();
}
