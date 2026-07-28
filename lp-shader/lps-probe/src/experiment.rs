//! [`run_experiment`]: compile a shader + probe wrappers, evaluate, report.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lps_shared::path_resolve::LpsTypePathExt;
use lps_shared::{LpsType, LpsValueF32};

use crate::experiment_result::{
    ExperimentResult, ProbeHistogram, ProbeOutcome, ProbeStats, ProbeStepHistogram, ProbeStepStats,
    ProbeValueRow, ProbeValues, ShaderCompileOutcome,
};
use crate::experiment_spec::{BindingValue, ExperimentSpec};
use crate::health::{HealthReport, evaluate_health, health_sites};
use crate::interp_harness::{CompiledShader, InterpInstance};
use crate::probe_domain::{ProbeDomain, ProbeSite};
use crate::probe_reduce::ProbeReduce;
use crate::probe_spec::{ProbeSpec, ProbeType};
use crate::probe_wrapper::{
    emit_wrapper, glsl_ident_is_valid, probe_id_is_valid, remap_probe_diagnostics,
};

/// Maximum probes evaluated per experiment; later probes are skipped.
pub const MAX_PROBES: usize = 8;
/// Maximum evaluations for one probe (`|domain| × |vary|`).
pub const MAX_EVALS_PER_PROBE: usize = 4096;
/// Maximum raw value rows for `reduce: none`.
pub const MAX_RAW_VALUES: usize = 64;
/// Hard cap on total experiment evaluations (probes + health).
pub const MAX_TOTAL_EVALS: usize = 16_384;
/// Interpreter op budget for a single evaluation (one probe/health/diff call
/// or `__shader_init`) — the bound that keeps an infinite-loop shader from
/// hanging the Studio wasm main thread (the interpreter has no other
/// termination guarantee). 64 ops per unit of the eval budget
/// ([`MAX_TOTAL_EVALS`]) ≈ 1M ops: a realistic ~100-line shader evaluates in
/// ~10³–10⁴ ops (see the perf test), so this is ~100× headroom, while one
/// exhausted call costs well under a second. Exhaustion aborts the enclosing
/// probe (and health bails after repeated failures), so a hostile shader
/// triggers only a handful of exhausted calls per experiment.
pub const MAX_OPS_PER_EVAL: u64 = MAX_TOTAL_EVALS as u64 * 64;

/// Compile `source` plus per-probe wrappers, evaluate everything on the f32
/// LPIR interpreter, and report diagnostics, health, and probe outcomes.
///
/// See the crate README for the determinism contract (site and vary
/// ordering) and the evaluation caps. Callers that need to interleave work
/// between stages (e.g. UI yield points) drive an [`ExperimentRunner`]
/// directly; this function is the single-shot wrapper over it.
pub fn run_experiment(source: &str, spec: &ExperimentSpec) -> ExperimentResult {
    let mut runner = ExperimentRunner::new(source, spec);
    if runner.compile() {
        for idx in 0..runner.probe_count() {
            runner.run_probe(idx);
        }
    }
    runner.finish()
}

/// Stepped form of [`run_experiment`]: `compile` (base shader + health),
/// then `run_probe` per spec probe in order, then `finish`. Behavior is
/// identical to the single-shot function — the split exists so a caller on
/// a single-threaded executor can yield between the (potentially slow)
/// stages.
pub struct ExperimentRunner<'a> {
    source: &'a str,
    spec: &'a ExperimentSpec,
    warnings: Vec<String>,
    /// `Some` once `compile` succeeded; `None` before, or after a failure.
    compiled: Option<CompiledShader>,
    health: Option<HealthReport>,
    probes: VecMap<String, ProbeOutcome>,
    total_evals: usize,
    /// Compile-stage diagnostics; set exactly when compilation failed.
    failure_diagnostics: Option<Vec<String>>,
}

impl<'a> ExperimentRunner<'a> {
    pub fn new(source: &'a str, spec: &'a ExperimentSpec) -> Self {
        Self {
            source,
            spec,
            warnings: Vec::new(),
            compiled: None,
            health: None,
            probes: VecMap::new(),
            total_evals: 0,
            failure_diagnostics: None,
        }
    }

    /// Number of probes the spec asks for (cap-independent; the per-probe
    /// caps apply inside [`Self::run_probe`]).
    pub fn probe_count(&self) -> usize {
        self.spec.probes.len()
    }

    /// Compile the base shader and evaluate the health report. Returns
    /// `false` on a compile/validation failure — probes are then skipped
    /// and [`Self::finish`] reports the diagnostics.
    pub fn compile(&mut self) -> bool {
        let compiled = match CompiledShader::compile(self.source) {
            Ok(c) => c,
            Err(diagnostics) => {
                self.failure_diagnostics = Some(diagnostics);
                return false;
            }
        };
        if let Err(diag) = validate_render_signature(&compiled) {
            self.failure_diagnostics = Some(vec![diag]);
            return false;
        }
        let mut health_inst = match compiled.instantiate() {
            Ok(i) => i,
            Err(e) => {
                self.failure_diagnostics = Some(vec![format!("shader init failed: {e}")]);
                return false;
            }
        };
        apply_bindings(&mut health_inst, self.spec, Some(&mut self.warnings));
        self.health = Some(evaluate_health(
            &mut health_inst,
            self.spec,
            &mut self.warnings,
        ));
        self.total_evals = health_sites(self.spec).len();
        self.compiled = Some(compiled);
        true
    }

    /// Evaluate spec probe `idx` (duplicate-id and cap rules identical to
    /// the single-shot path). No-op before a successful `compile` or for an
    /// out-of-range index.
    pub fn run_probe(&mut self, idx: usize) {
        if self.compiled.is_none() {
            return;
        }
        let Some(probe) = self.spec.probes.get(idx) else {
            return;
        };
        if self.probes.get(&probe.id).is_some() {
            self.warnings.push(format!(
                "duplicate probe id '{}'; later occurrence ignored",
                probe.id
            ));
            return;
        }
        let outcome = if idx >= MAX_PROBES {
            ProbeOutcome::Skipped {
                reason: format!(
                    "experiment cap: at most {MAX_PROBES} probes per experiment; \
                     remove probes or split the experiment"
                ),
            }
        } else {
            run_probe(self.source, self.spec, probe, &mut self.total_evals)
        };
        self.probes.insert(probe.id.clone(), outcome);
    }

    /// Assemble the [`ExperimentResult`].
    pub fn finish(self) -> ExperimentResult {
        if let Some(diagnostics) = self.failure_diagnostics {
            return failed_result(self.spec, diagnostics);
        }
        ExperimentResult {
            shader: ShaderCompileOutcome::Ok,
            compiled: self.compiled,
            health: self.health,
            probes: self.probes,
            warnings: self.warnings,
        }
    }
}

/// Apply `outputSize` (iff declared) and the spec bindings to an instance.
/// With `warnings`, bindings that don't match a declared uniform are
/// reported (they are never hard errors — the agent may probe a source it
/// just removed a uniform from).
pub(crate) fn apply_bindings(
    inst: &mut InterpInstance,
    spec: &ExperimentSpec,
    mut warnings: Option<&mut Vec<String>>,
) {
    let uniforms = inst.signatures().uniforms_type.clone();

    if let Some(ut) = &uniforms {
        if matches!(ut.type_at_path("outputSize"), Ok(LpsType::Vec2)) {
            let value = LpsValueF32::Vec2([spec.size[0] as f32, spec.size[1] as f32]);
            // Declared with an incompatible layout at worst; nothing to do.
            let _ = inst.set_uniform("outputSize", &value);
        }
    }

    for (path, value) in spec.bindings.iter() {
        let mut warn = |msg: String| {
            if let Some(w) = warnings.as_deref_mut() {
                w.push(msg);
            }
        };
        let Some(ut) = &uniforms else {
            warn(format!(
                "binding '{path}' does not match a declared uniform (ignored)"
            ));
            continue;
        };
        let leaf = match ut.type_at_path(path) {
            Ok(ty) => ty,
            Err(_) => {
                warn(format!(
                    "binding '{path}' does not match a declared uniform (ignored)"
                ));
                continue;
            }
        };
        match value.coerce_to(&leaf) {
            Ok(v) => {
                if let Err(e) = inst.set_uniform(path, &v) {
                    warn(format!("binding '{path}': {e}"));
                }
            }
            Err(e) => warn(format!("binding '{path}': {e}")),
        }
    }
}

/// A failed-shader result: no health, every probe skipped.
fn failed_result(spec: &ExperimentSpec, diagnostics: Vec<String>) -> ExperimentResult {
    let mut probes = VecMap::new();
    for p in &spec.probes {
        if probes.get(&p.id).is_none() {
            probes.insert(
                p.id.clone(),
                ProbeOutcome::Skipped {
                    reason: String::from("shader failed to compile"),
                },
            );
        }
    }
    ExperimentResult {
        shader: ShaderCompileOutcome::Err { diagnostics },
        compiled: None,
        health: None,
        probes,
        warnings: Vec::new(),
    }
}

/// The entry contract: user shaders define `vec4 render(vec2 pos)`.
fn validate_render_signature(compiled: &CompiledShader) -> Result<(), String> {
    let Some(f) = compiled
        .signatures()
        .functions
        .iter()
        .find(|f| f.name == "render")
    else {
        return Err(String::from(
            "shader must define `vec4 render(vec2 pos)` (no render function found)",
        ));
    };
    let params_ok = f.parameters.len() == 1 && f.parameters[0].ty == LpsType::Vec2;
    if !params_ok || f.return_type != LpsType::Vec4 {
        return Err(format!(
            "shader must define `vec4 render(vec2 pos)`; found a render function \
             with {} parameter(s) returning {:?}",
            f.parameters.len(),
            f.return_type
        ));
    }
    Ok(())
}

/// Evaluate one probe end to end (validate → cap-check → compile → run →
/// reduce). Every early exit is an actionable `Skipped` or a
/// probe-attributed `CompileError`.
fn run_probe(
    source: &str,
    spec: &ExperimentSpec,
    probe: &ProbeSpec,
    total_evals: &mut usize,
) -> ProbeOutcome {
    let skipped = |reason: String| ProbeOutcome::Skipped { reason };

    if !probe_id_is_valid(&probe.id) {
        return skipped(format!(
            "probe id '{}' is invalid; ids must match [a-z0-9_]+",
            probe.id
        ));
    }
    if let ProbeDomain::Sweep { var, .. } = &probe.domain {
        if !glsl_ident_is_valid(var) {
            return skipped(format!("sweep var '{var}' is not a valid GLSL identifier"));
        }
    }
    let sites = match probe.domain.sites(&spec.led_points) {
        Ok(s) => s,
        Err(reason) => return skipped(reason),
    };
    if sites.is_empty() {
        return skipped(String::from(
            "domain has no sites (empty point list, or no led points supplied)",
        ));
    }
    if let Some(v) = &probe.vary {
        if v.values.is_empty() {
            return skipped(String::from(
                "vary.values is empty; drop vary or add values",
            ));
        }
    }
    if let ProbeReduce::Histogram { bins: 0 } = probe.reduce {
        return skipped(String::from("histogram needs bins >= 1"));
    }

    let steps = probe.vary.as_ref().map_or(1, |v| v.values.len());
    let evals = sites.len() * steps;
    if evals > MAX_EVALS_PER_PROBE {
        return skipped(format!(
            "probe would evaluate {} sites × {steps} vary steps = {evals} evals, over \
             the per-probe cap of {MAX_EVALS_PER_PROBE}; shrink the domain or the vary list",
            sites.len()
        ));
    }
    if matches!(probe.reduce, ProbeReduce::None) && evals > MAX_RAW_VALUES {
        return skipped(format!(
            "reduce 'none' returns raw values, capped at {MAX_RAW_VALUES} rows \
             ({evals} requested); add a reduce (stats or histogram) or shrink the domain"
        ));
    }
    if *total_evals + evals > MAX_TOTAL_EVALS {
        return skipped(format!(
            "total experiment evaluation budget of {MAX_TOTAL_EVALS} evals exceeded; \
             shrink domains, vary lists, or remove probes"
        ));
    }
    *total_evals += evals;

    // Compile shader + this one wrapper (per-probe compile isolation).
    let wrapper = emit_wrapper(probe);
    let mut unit = String::from(source);
    if !unit.ends_with('\n') {
        unit.push('\n');
    }
    let wrapper_first_line = unit.lines().count() + 1;
    unit.push_str(&wrapper.text);
    let compiled = match CompiledShader::compile(&unit) {
        Ok(c) => c,
        Err(diags) => {
            return ProbeOutcome::CompileError {
                diagnostics: remap_probe_diagnostics(
                    diags,
                    &probe.id,
                    wrapper_first_line,
                    wrapper.expr_offset,
                ),
            };
        }
    };
    let mut inst = match compiled.instantiate() {
        Ok(i) => i,
        Err(e) => return skipped(format!("probe instantiation failed: {e}")),
    };
    apply_bindings(&mut inst, spec, None);

    // Resolve the vary binding's leaf type once.
    let vary_leaf: Option<LpsType> = match &probe.vary {
        None => None,
        Some(v) => {
            let leaf = inst
                .signatures()
                .uniforms_type
                .clone()
                .and_then(|ut| ut.type_at_path(&v.binding).ok());
            match leaf {
                Some(ty) => Some(ty),
                None => {
                    return skipped(format!(
                        "vary binding '{}' does not match a declared uniform",
                        v.binding
                    ));
                }
            }
        }
    };

    // Evaluation order (the determinism contract): vary values in given
    // order (outer), then sites in domain order (inner).
    let vary_steps: Vec<Option<f32>> = match &probe.vary {
        None => vec![None],
        Some(v) => v.values.iter().map(|&x| Some(x)).collect(),
    };
    let mut samples: Vec<(Option<f32>, ProbeSite, Vec<f32>)> = Vec::with_capacity(evals);
    for step in &vary_steps {
        if let (Some(x), Some(v), Some(leaf)) = (step, &probe.vary, &vary_leaf) {
            let value = match BindingValue::Scalar(*x).coerce_to(leaf) {
                Ok(val) => val,
                Err(e) => return skipped(format!("vary binding '{}': {e}", v.binding)),
            };
            if let Err(e) = inst.set_uniform(&v.binding, &value) {
                return skipped(format!("vary binding '{}': {e}", v.binding));
            }
        }
        for site in &sites {
            let arg = match site {
                ProbeSite::Pos(p) => {
                    LpsValueF32::Vec2([p[0] * spec.size[0] as f32, p[1] * spec.size[1] as f32])
                }
                ProbeSite::Var(t) => LpsValueF32::F32(*t),
            };
            let out = match inst.call(&wrapper.fn_name, &[arg]) {
                Ok(v) => v,
                Err(e) => return skipped(format!("evaluation failed: {e}")),
            };
            let comps = match value_components(&out, probe.ty) {
                Ok(c) => c,
                Err(e) => return skipped(e),
            };
            samples.push((*step, *site, comps));
        }
    }

    reduce_samples(probe, sites.len(), &samples)
}

/// Flatten one probe result into f32 components (`int` is reported as f32).
fn value_components(v: &LpsValueF32, ty: ProbeType) -> Result<Vec<f32>, String> {
    match (ty, v) {
        (ProbeType::Float, LpsValueF32::F32(x)) => Ok(vec![*x]),
        (ProbeType::Int, LpsValueF32::I32(x)) => Ok(vec![*x as f32]),
        (ProbeType::Vec2, LpsValueF32::Vec2(a)) => Ok(a.to_vec()),
        (ProbeType::Vec3, LpsValueF32::Vec3(a)) => Ok(a.to_vec()),
        (ProbeType::Vec4, LpsValueF32::Vec4(a)) => Ok(a.to_vec()),
        (ty, v) => Err(format!("probe returned {v:?}, expected {ty:?}")),
    }
}

/// Apply the probe's reduction. `samples` is step-major (each consecutive
/// `sites_per_step` entries share one vary step).
fn reduce_samples(
    probe: &ProbeSpec,
    sites_per_step: usize,
    samples: &[(Option<f32>, ProbeSite, Vec<f32>)],
) -> ProbeOutcome {
    match probe.reduce {
        ProbeReduce::None => ProbeOutcome::Values(ProbeValues {
            rows: samples
                .iter()
                .map(|(vary, site, comps)| ProbeValueRow {
                    vary: *vary,
                    at: match site {
                        ProbeSite::Pos(p) => vec![p[0], p[1]],
                        ProbeSite::Var(t) => vec![*t],
                    },
                    value: comps.clone(),
                })
                .collect(),
        }),
        ProbeReduce::Stats => ProbeOutcome::Stats(ProbeStats {
            steps: samples
                .chunks(sites_per_step)
                .map(|chunk| step_stats(probe.ty.component_count(), chunk))
                .collect(),
        }),
        ProbeReduce::Histogram { bins } => ProbeOutcome::Histogram(ProbeHistogram {
            steps: samples
                .chunks(sites_per_step)
                .map(|chunk| step_histogram(bins, chunk))
                .collect(),
        }),
    }
}

fn step_stats(components: usize, chunk: &[(Option<f32>, ProbeSite, Vec<f32>)]) -> ProbeStepStats {
    let mut min = vec![f32::INFINITY; components];
    let mut max = vec![f32::NEG_INFINITY; components];
    let mut sum = vec![0.0f64; components];
    let mut finite = vec![0u32; components];
    let mut nan_count = 0u32;
    let mut inf_count = 0u32;

    for (_, _, comps) in chunk {
        for (i, &x) in comps.iter().enumerate() {
            if x.is_nan() {
                nan_count += 1;
            } else if x.is_infinite() {
                inf_count += 1;
            } else {
                min[i] = min[i].min(x);
                max[i] = max[i].max(x);
                sum[i] += f64::from(x);
                finite[i] += 1;
            }
        }
    }
    let mean = sum
        .iter()
        .zip(&finite)
        .map(|(&s, &n)| {
            if n > 0 {
                (s / f64::from(n)) as f32
            } else {
                0.0
            }
        })
        .collect();
    // Components with no finite values keep 0 rather than reversed infinities.
    for i in 0..components {
        if finite[i] == 0 {
            min[i] = 0.0;
            max[i] = 0.0;
        }
    }
    ProbeStepStats {
        vary: chunk.first().and_then(|(v, _, _)| *v),
        min,
        max,
        mean,
        nan_count,
        inf_count,
    }
}

fn step_histogram(bins: u8, chunk: &[(Option<f32>, ProbeSite, Vec<f32>)]) -> ProbeStepHistogram {
    let values: Vec<f32> = chunk
        .iter()
        .flat_map(|(_, _, comps)| comps.iter().copied())
        .collect();
    let finite: Vec<f32> = values.iter().copied().filter(|x| x.is_finite()).collect();
    let non_finite_count = (values.len() - finite.len()) as u32;

    let (lo, hi) = finite
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &x| {
            (lo.min(x), hi.max(x))
        });
    let (lo, hi) = if finite.is_empty() {
        (0.0, 0.0)
    } else {
        (lo, hi)
    };
    let mut counts = vec![0u32; bins as usize];
    for &x in &finite {
        let idx = if hi > lo {
            (((x - lo) / (hi - lo)) * bins as f32) as usize
        } else {
            0
        };
        counts[idx.min(bins as usize - 1)] += 1;
    }
    ProbeStepHistogram {
        vary: chunk.first().and_then(|(v, _, _)| *v),
        lo,
        hi,
        counts,
        non_finite_count,
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::string::ToString;

    use super::*;
    use crate::probe_domain::ProbeDomain;

    /// Linear-ramp shader: exact f32 math at grid/point sites.
    const RAMP: &str = "vec4 render(vec2 pos) { return vec4(pos.x, pos.y, 0.25, 1.0); }";

    fn probe(id: &str, ty: ProbeType, expr: &str, domain: ProbeDomain) -> ProbeSpec {
        ProbeSpec {
            id: id.to_string(),
            ty,
            expr: expr.to_string(),
            domain,
            vary: None,
            reduce: ProbeReduce::None,
        }
    }

    fn spec_with(probes: Vec<ProbeSpec>) -> ExperimentSpec {
        ExperimentSpec {
            size: [2, 2],
            probes,
            ..ExperimentSpec::default()
        }
    }

    fn values(outcome: &ProbeOutcome) -> &ProbeValues {
        match outcome {
            ProbeOutcome::Values(v) => v,
            other => panic!("expected values, got {other:?}"),
        }
    }

    #[test]
    fn golden_point_domain() {
        let spec = spec_with(vec![probe(
            "center",
            ProbeType::Vec4,
            "render(pos)",
            ProbeDomain::Point { at: [0.5, 0.5] },
        )]);
        let result = run_experiment(RAMP, &spec);
        assert_eq!(result.shader, ShaderCompileOutcome::Ok);
        assert!(result.compiled.is_some());
        let v = values(result.probes.get(&"center".to_owned()).expect("probe"));
        // pos = (0.5·2, 0.5·2) = (1, 1); exact.
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].at, vec![0.5, 0.5]);
        assert_eq!(v.rows[0].value, vec![1.0, 1.0, 0.25, 1.0]);
    }

    #[test]
    fn golden_points_and_grid_and_line_domains() {
        let spec = spec_with(vec![
            probe(
                "pts",
                ProbeType::Float,
                "render(pos).x",
                ProbeDomain::Points {
                    at: vec![[0.0, 0.0], [1.0, 0.0]],
                },
            ),
            probe(
                "grid",
                ProbeType::Float,
                "render(pos).y",
                ProbeDomain::Grid {
                    nx: 2,
                    ny: 2,
                    rect: None,
                },
            ),
            probe(
                "line",
                ProbeType::Float,
                "render(pos).x",
                ProbeDomain::Line {
                    from: [0.0, 0.0],
                    to: [1.0, 0.0],
                    n: 3,
                },
            ),
        ]);
        let result = run_experiment(RAMP, &spec);
        let pts = values(result.probes.get(&"pts".to_owned()).expect("pts"));
        assert_eq!(pts.rows[0].value, vec![0.0]);
        assert_eq!(pts.rows[1].value, vec![2.0]);
        let grid = values(result.probes.get(&"grid".to_owned()).expect("grid"));
        // Cell-center y in row-major order: 0.25, 0.25, 0.75, 0.75 (×2).
        let ys: Vec<f32> = grid.rows.iter().map(|r| r.value[0]).collect();
        assert_eq!(ys, vec![0.5, 0.5, 1.5, 1.5]);
        let line = values(result.probes.get(&"line".to_owned()).expect("line"));
        let xs: Vec<f32> = line.rows.iter().map(|r| r.value[0]).collect();
        assert_eq!(xs, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn golden_leds_domain() {
        let mut spec = spec_with(vec![probe(
            "leds",
            ProbeType::Float,
            "render(pos).x",
            ProbeDomain::Leds { indices: None },
        )]);
        spec.led_points = vec![
            crate::experiment_spec::LedPoint {
                label: "a".to_string(),
                channel: 0,
                at: [0.25, 0.5],
            },
            crate::experiment_spec::LedPoint {
                label: "b".to_string(),
                channel: 0,
                at: [0.75, 0.5],
            },
        ];
        let result = run_experiment(RAMP, &spec);
        let v = values(result.probes.get(&"leds".to_owned()).expect("leds"));
        assert_eq!(v.rows[0].value, vec![0.5]);
        assert_eq!(v.rows[1].value, vec![1.5]);
    }

    #[test]
    fn probe_render_equals_direct_entry_evaluation() {
        let sites = [[0.1, 0.2], [0.6, 0.4], [1.0, 1.0]];
        let spec = spec_with(vec![probe(
            "p",
            ProbeType::Vec4,
            "render(pos)",
            ProbeDomain::Points { at: sites.to_vec() },
        )]);
        let result = run_experiment(RAMP, &spec);
        let compiled = result.compiled.as_ref().expect("compiled");
        let mut inst = compiled.instantiate().expect("instantiate");
        let rows = &values(result.probes.get(&"p".to_owned()).expect("p")).rows;
        for (site, row) in sites.iter().zip(rows) {
            let pos = [site[0] * 2.0, site[1] * 2.0];
            let direct = inst
                .call("render", &[LpsValueF32::Vec2(pos)])
                .expect("direct render");
            let LpsValueF32::Vec4(direct) = direct else {
                panic!("render must return vec4");
            };
            assert_eq!(row.value, direct.to_vec(), "site {site:?}");
        }
    }

    #[test]
    fn sweep_plots_a_user_helper() {
        let src = "float tri(float x) { return 1.0 - abs(x * 2.0 - 1.0); }\n\
                   vec4 render(vec2 pos) { return vec4(tri(pos.x), 0.0, 0.0, 1.0); }";
        let spec = spec_with(vec![probe(
            "sweep",
            ProbeType::Float,
            "tri(t)",
            ProbeDomain::Sweep {
                var: "t".to_string(),
                from: 0.0,
                to: 1.0,
                n: 5,
            },
        )]);
        let result = run_experiment(src, &spec);
        let v = values(result.probes.get(&"sweep".to_owned()).expect("sweep"));
        let ys: Vec<f32> = v.rows.iter().map(|r| r.value[0]).collect();
        assert_eq!(ys, vec![0.0, 0.5, 1.0, 0.5, 0.0]);
        assert_eq!(v.rows[0].at, vec![0.0]);
        assert_eq!(v.rows[4].at, vec![1.0]);
    }

    #[test]
    fn vary_order_drives_stateful_globals() {
        // acc accumulates u across calls: ordered vary approximates
        // frame-sequential behavior (see README determinism contract).
        let src = "layout(binding = 0) uniform float u;\nfloat acc = 0.0;\n\
                   vec4 render(vec2 pos) { acc = acc + u; return vec4(acc, 0.0, 0.0, 1.0); }";
        let mut p = probe(
            "acc",
            ProbeType::Float,
            "render(pos).x",
            ProbeDomain::Point { at: [0.5, 0.5] },
        );
        p.vary = Some(crate::probe_spec::ProbeVary {
            binding: "u".to_string(),
            values: vec![1.0, 2.0, 4.0],
        });
        let spec = spec_with(vec![p]);
        let result = run_experiment(src, &spec);
        let v = values(result.probes.get(&"acc".to_owned()).expect("acc"));
        let seq: Vec<f32> = v.rows.iter().map(|r| r.value[0]).collect();
        assert_eq!(seq, vec![1.0, 3.0, 7.0]);
        assert_eq!(
            v.rows.iter().map(|r| r.vary).collect::<Vec<_>>(),
            vec![Some(1.0), Some(2.0), Some(4.0)]
        );
    }

    #[test]
    fn probe_isolation_bad_expr_only_fails_its_probe() {
        let mk = |id: &str, expr: &str| {
            probe(
                id,
                ProbeType::Float,
                expr,
                ProbeDomain::Point { at: [0.5, 0.5] },
            )
        };
        let spec = spec_with(vec![
            mk("good1", "render(pos).x"),
            mk("bad", "bogus_fn(pos)"),
            mk("good2", "render(pos).y"),
        ]);
        let result = run_experiment(RAMP, &spec);
        assert!(matches!(
            result.probes.get(&"good1".to_owned()),
            Some(ProbeOutcome::Values(_))
        ));
        assert!(matches!(
            result.probes.get(&"good2".to_owned()),
            Some(ProbeOutcome::Values(_))
        ));
        let ProbeOutcome::CompileError { diagnostics } =
            result.probes.get(&"bad".to_owned()).expect("bad")
        else {
            panic!("expected compile error");
        };
        let joined = diagnostics.join("\n");
        assert!(joined.contains("probe 'bad'"), "{joined}");
    }

    #[test]
    fn shader_diagnostics_are_user_relative() {
        let src = "vec4 render(vec2 pos) {\n    return vec4(0.0);\n}\nfloat bad = ;\n";
        let result = run_experiment(
            src,
            &spec_with(vec![probe(
                "p",
                ProbeType::Float,
                "1.0",
                ProbeDomain::Point { at: [0.0, 0.0] },
            )]),
        );
        let ShaderCompileOutcome::Err { diagnostics } = &result.shader else {
            panic!("expected shader error");
        };
        let joined = diagnostics.join("\n");
        assert!(
            joined.contains("glsl:4:"),
            "user-relative line expected:\n{joined}"
        );
        assert!(result.health.is_none());
        assert!(matches!(
            result.probes.get(&"p".to_owned()),
            Some(ProbeOutcome::Skipped { .. })
        ));
    }

    #[test]
    fn probe_diagnostics_are_expr_attributed() {
        let spec = spec_with(vec![probe(
            "bad",
            ProbeType::Float,
            "render(pos).x +",
            ProbeDomain::Point { at: [0.5, 0.5] },
        )]);
        let result = run_experiment(RAMP, &spec);
        let ProbeOutcome::CompileError { diagnostics } =
            result.probes.get(&"bad".to_owned()).expect("bad")
        else {
            panic!("expected compile error");
        };
        let joined = diagnostics.join("\n");
        assert!(
            joined.contains("probe 'bad' expr:1:"),
            "expr-attributed position expected:\n{joined}"
        );
        assert!(
            !joined.contains("glsl:"),
            "no unit-relative position:\n{joined}"
        );
    }

    #[test]
    fn caps_produce_actionable_skips() {
        // Per-probe eval cap.
        let big_grid = probe(
            "big",
            ProbeType::Float,
            "render(pos).x",
            ProbeDomain::Grid {
                nx: 100,
                ny: 100,
                rect: None,
            },
        );
        // Raw-values cap.
        let mut raw = probe(
            "raw",
            ProbeType::Float,
            "render(pos).x",
            ProbeDomain::Grid {
                nx: 10,
                ny: 10,
                rect: None,
            },
        );
        raw.reduce = ProbeReduce::None;
        let spec = spec_with(vec![big_grid, raw]);
        let result = run_experiment(RAMP, &spec);
        let ProbeOutcome::Skipped { reason } = result.probes.get(&"big".to_owned()).expect("big")
        else {
            panic!("expected skip");
        };
        assert!(reason.contains("per-probe cap"), "{reason}");
        assert!(reason.contains("shrink the domain"), "{reason}");
        let ProbeOutcome::Skipped { reason } = result.probes.get(&"raw".to_owned()).expect("raw")
        else {
            panic!("expected skip");
        };
        assert!(reason.contains("add a reduce"), "{reason}");
    }

    #[test]
    fn probe_count_and_total_budget_caps() {
        let full_grid = |id: &str| {
            let mut p = probe(
                id,
                ProbeType::Float,
                "render(pos).x",
                ProbeDomain::Grid {
                    nx: 64,
                    ny: 64,
                    rect: None,
                },
            );
            p.reduce = ProbeReduce::Stats;
            p
        };
        // 9 probes: the 9th is over MAX_PROBES; among the first 8, the 4th
        // exceeds the total budget (256 health + 4·4096 > 16384).
        let mut probes: Vec<ProbeSpec> = (0..9).map(|i| full_grid(&format!("p{i}"))).collect();
        for p in probes.iter_mut().skip(4).take(4) {
            // Keep probes 4..8 cheap so only p3 trips the total budget.
            p.domain = ProbeDomain::Point { at: [0.5, 0.5] };
        }
        let spec = spec_with(probes);
        let result = run_experiment(RAMP, &spec);
        assert!(matches!(
            result.probes.get(&"p2".to_owned()),
            Some(ProbeOutcome::Stats(_))
        ));
        let ProbeOutcome::Skipped { reason } = result.probes.get(&"p3".to_owned()).expect("p3")
        else {
            panic!("expected budget skip");
        };
        assert!(
            reason.contains("total experiment evaluation budget"),
            "{reason}"
        );
        let ProbeOutcome::Skipped { reason } = result.probes.get(&"p8".to_owned()).expect("p8")
        else {
            panic!("expected probe-count skip");
        };
        assert!(reason.contains("at most 8 probes"), "{reason}");
    }

    #[test]
    fn infinite_loop_shader_is_bounded_and_actionable() {
        // `x` never grows past the bound because the increment is 0.0, so
        // `render` loops forever; the per-evaluation op budget must turn that
        // into bounded, actionable failures instead of a hang.
        let src = "vec4 render(vec2 pos) {\n\
                       float x = 0.0;\n\
                       while (x < 1.0) { x = x + 0.0; }\n\
                       return vec4(x, 0.0, 0.0, 1.0);\n\
                   }";
        let spec = spec_with(vec![probe(
            "p",
            ProbeType::Float,
            "render(pos).x",
            ProbeDomain::Point { at: [0.5, 0.5] },
        )]);
        let result = run_experiment(src, &spec);
        assert_eq!(result.shader, ShaderCompileOutcome::Ok);

        // Health warns per failed site, then bails instead of burning the
        // op budget on every remaining site.
        let health = result.health.expect("health");
        assert_eq!(health.sites_evaluated, 0);
        assert!(
            result.warnings.iter().any(|w| w.contains("op budget")),
            "{:?}",
            result.warnings
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("abandoned after")),
            "{:?}",
            result.warnings
        );

        let ProbeOutcome::Skipped { reason } = result.probes.get(&"p".to_owned()).expect("p")
        else {
            panic!("expected skip");
        };
        assert!(reason.contains("op budget"), "{reason}");
        assert!(reason.contains("loop"), "{reason}");
    }

    #[test]
    fn health_flags_nan_and_black_shaders() {
        let nan_src = "layout(binding = 0) uniform float z;\nvec4 render(vec2 pos) { return vec4(z / z, 0.0, 0.0, 1.0); }";
        let result = run_experiment(nan_src, &ExperimentSpec::default());
        let health = result.health.expect("health");
        assert!(health.nan_count > 0, "{health:?}");
        assert_eq!(health.sites_evaluated, 256);

        let black_src = "vec4 render(vec2 pos) { return vec4(0.0, 0.0, 0.0, 1.0); }";
        let result = run_experiment(black_src, &ExperimentSpec::default());
        let health = result.health.expect("health");
        assert_eq!(health.near_black_fraction, 1.0);
        assert_eq!(health.mean_luminance, 0.0);
        assert_eq!(health.clip_fraction, 0.0);
    }

    #[test]
    fn health_runs_with_zero_probes_and_reads_bindings() {
        let src = "layout(binding = 0) uniform float level;\n\
                   vec4 render(vec2 pos) { return vec4(level, level, level, 1.0); }";
        let mut spec = ExperimentSpec::default();
        spec.bindings
            .insert("level".to_string(), BindingValue::Scalar(0.5));
        let result = run_experiment(src, &spec);
        let health = result.health.expect("health");
        assert!(
            (health.mean_luminance - 0.5).abs() < 1e-6,
            "bindings must reach health: {health:?}"
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn unknown_binding_warns_but_does_not_fail() {
        let mut spec = ExperimentSpec::default();
        spec.bindings
            .insert("missing".to_string(), BindingValue::Scalar(1.0));
        let result = run_experiment(RAMP, &spec);
        assert_eq!(result.shader, ShaderCompileOutcome::Ok);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("binding 'missing'")),
            "{:?}",
            result.warnings
        );
    }

    #[test]
    fn missing_render_is_a_shader_error() {
        let result = run_experiment(
            "float helper(float x) { return x; }",
            &ExperimentSpec::default(),
        );
        let ShaderCompileOutcome::Err { diagnostics } = &result.shader else {
            panic!("expected error");
        };
        assert!(
            diagnostics[0].contains("vec4 render(vec2 pos)"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn stats_and_histogram_reduce() {
        let mut stats = probe(
            "stats",
            ProbeType::Vec2,
            "vec2(render(pos).x, 2.0)",
            ProbeDomain::Grid {
                nx: 4,
                ny: 4,
                rect: None,
            },
        );
        stats.reduce = ProbeReduce::Stats;
        let mut hist = probe(
            "hist",
            ProbeType::Float,
            "render(pos).x",
            ProbeDomain::Grid {
                nx: 4,
                ny: 4,
                rect: None,
            },
        );
        hist.reduce = ProbeReduce::Histogram { bins: 4 };
        let spec = spec_with(vec![stats, hist]);
        let result = run_experiment(RAMP, &spec);

        let ProbeOutcome::Stats(s) = result.probes.get(&"stats".to_owned()).expect("stats") else {
            panic!("expected stats");
        };
        assert_eq!(s.steps.len(), 1);
        let step = &s.steps[0];
        // x centers: 0.125..0.875 × size 2 → 0.25..1.75.
        assert_eq!(step.min, vec![0.25, 2.0]);
        assert_eq!(step.max, vec![1.75, 2.0]);
        assert_eq!(step.mean, vec![1.0, 2.0]);
        assert_eq!((step.nan_count, step.inf_count), (0, 0));

        let ProbeOutcome::Histogram(h) = result.probes.get(&"hist".to_owned()).expect("hist")
        else {
            panic!("expected histogram");
        };
        let step = &h.steps[0];
        assert_eq!((step.lo, step.hi), (0.25, 1.75));
        assert_eq!(step.counts.iter().sum::<u32>(), 16);
        assert_eq!(step.counts, vec![4, 4, 4, 4]);
        assert_eq!(step.non_finite_count, 0);
    }

    #[test]
    fn lpfn_builtins_work_in_shader_and_probe_expr() {
        let src = "vec4 render(vec2 pos) { return vec4(lpfn_saturate(pos.x), 0.0, 0.0, 1.0); }";
        let spec = spec_with(vec![probe(
            "sat",
            ProbeType::Float,
            "lpfn_saturate(render(pos).x + 4.0)",
            ProbeDomain::Point { at: [0.5, 0.5] },
        )]);
        let result = run_experiment(src, &spec);
        assert_eq!(result.shader, ShaderCompileOutcome::Ok);
        let v = values(result.probes.get(&"sat".to_owned()).expect("sat"));
        assert_eq!(v.rows[0].value, vec![1.0]);
    }

    /// Perf sanity: ~100-line shader, 4096 `render` evals through one grid
    /// probe. Prints the duration — this number decides whether P5 needs a
    /// worker offload.
    #[test]
    fn perf_4096_render_evals_under_10s_debug() {
        let src = perf_shader_100_lines();
        let mut p = probe(
            "grid",
            ProbeType::Vec4,
            "render(pos)",
            ProbeDomain::Grid {
                nx: 64,
                ny: 64,
                rect: None,
            },
        );
        p.reduce = ProbeReduce::Stats;
        let spec = ExperimentSpec {
            probes: vec![p],
            ..ExperimentSpec::default()
        };
        let start = std::time::Instant::now();
        let result = run_experiment(&src, &spec);
        let elapsed = std::time::Instant::now() - start;
        assert_eq!(result.shader, ShaderCompileOutcome::Ok);
        assert!(matches!(
            result.probes.get(&"grid".to_owned()),
            Some(ProbeOutcome::Stats(_))
        ));
        std::println!(
            "PERF: run_experiment (~100-line shader, 4096 render evals + 256 health evals) took {elapsed:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "perf bound exceeded: {elapsed:?}"
        );
    }

    /// A ~100-line shader with helpers, loops, and mild trig — realistic
    /// agent-iteration shape without lpfn dependencies.
    fn perf_shader_100_lines() -> String {
        let mut src = String::new();
        src.push_str("layout(binding = 0) uniform vec2 outputSize;\n");
        src.push_str("layout(binding = 1) uniform float time;\n\n");
        src.push_str("float hash21(vec2 p) {\n");
        src.push_str("    vec2 q = fract(p * vec2(123.34, 456.21));\n");
        src.push_str("    q += dot(q, q + 45.32);\n");
        src.push_str("    return fract(q.x * q.y);\n");
        src.push_str("}\n\n");
        src.push_str("float vnoise(vec2 p) {\n");
        src.push_str("    vec2 i = floor(p);\n");
        src.push_str("    vec2 f = fract(p);\n");
        src.push_str("    vec2 u = f * f * (3.0 - 2.0 * f);\n");
        src.push_str("    float a = hash21(i);\n");
        src.push_str("    float b = hash21(i + vec2(1.0, 0.0));\n");
        src.push_str("    float c = hash21(i + vec2(0.0, 1.0));\n");
        src.push_str("    float d = hash21(i + vec2(1.0, 1.0));\n");
        src.push_str("    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);\n");
        src.push_str("}\n\n");
        src.push_str("float fbm(vec2 p) {\n");
        src.push_str("    float acc = 0.0;\n");
        src.push_str("    float amp = 0.5;\n");
        src.push_str("    for (int i = 0; i < 5; i++) {\n");
        src.push_str("        acc += amp * vnoise(p);\n");
        src.push_str("        p = p * 2.03 + vec2(1.7, 9.2);\n");
        src.push_str("        amp *= 0.5;\n");
        src.push_str("    }\n");
        src.push_str("    return acc;\n");
        src.push_str("}\n\n");
        src.push_str("vec3 palette(float t) {\n");
        src.push_str("    vec3 a = vec3(0.5, 0.5, 0.5);\n");
        src.push_str("    vec3 b = vec3(0.5, 0.5, 0.5);\n");
        src.push_str("    vec3 c = vec3(1.0, 1.0, 1.0);\n");
        src.push_str("    vec3 d = vec3(0.263, 0.416, 0.557);\n");
        src.push_str("    return a + b * cos(6.28318 * (c * t + d));\n");
        src.push_str("}\n\n");
        // Pad with distinct small helpers to reach ~100 lines.
        for i in 0..12 {
            src.push_str(&format!("float layer{i}(vec2 p) {{\n"));
            src.push_str(&format!(
                "    return fbm(p * {}.5 + vec2({i}.0, {i}.5)) * 0.5;\n",
                i % 3 + 1
            ));
            src.push_str("}\n\n");
        }
        src.push_str("vec4 render(vec2 pos) {\n");
        src.push_str("    vec2 uv = pos / outputSize;\n");
        src.push_str("    float n = fbm(uv * 4.0 + time * 0.1);\n");
        src.push_str("    n += layer0(uv) * 0.25;\n");
        src.push_str("    n += layer5(uv * 2.0) * 0.125;\n");
        src.push_str("    n += layer11(uv * 3.0) * 0.0625;\n");
        src.push_str("    vec3 col = palette(n + uv.x * 0.5);\n");
        src.push_str("    col *= 0.5 + 0.5 * sin(time + uv.y * 6.283);\n");
        src.push_str("    return vec4(col, 1.0);\n");
        src.push_str("}\n");
        assert!(src.lines().count() >= 90, "shader should be ~100 lines");
        src
    }
}
