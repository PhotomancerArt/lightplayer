//! The live eval corpus: task definitions, probe assertions, provider
//! resolution, and reporting.
//!
//! Five tasks, each: a [`FakeHost`] with a starting source + a ring fixture
//! (24 LED points on a circle), one user prompt driven through
//! [`AgentSession::run`] against a real provider, then deterministic probe
//! assertions on the final staged source via `lps_probe::run_experiment`.
//!
//! Spend gate: [`ProviderCfg::from_env`] returns `None` without explicit
//! provider configuration — the entrypoint (`lpa-agent/tests/evals.rs`,
//! `#[ignore]`d) then prints a notice and skips cleanly, so no automated
//! path ever bills tokens. Transport construction (and the tokio runtime
//! driving it) stays with the entrypoint: this module never touches the
//! feature-gated host transport.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use lpa_agent::{
    AgentEvent, AgentSession, FixtureSummary, ModelProvider, OpenAiCompatConfig, ShaderContext,
    TokenUsage,
};
use lps_probe::{
    ExperimentResult, ExperimentSpec, HealthReport, LedPoint, ProbeDomain, ProbeOutcome,
    ProbeReduce, ProbeSpec, ProbeStepStats, ProbeType, ProbeValueRow, ShaderCompileOutcome,
    run_experiment,
};

use lpa_agent::test_double::FakeHost;

/// Ring fixture size (well under the 64-row raw cap).
const RING_LEDS: usize = 24;

/// claude-sonnet-5 pricing per million tokens (standard rates; the
/// introductory promo through 2026-08-31 is $2/$10).
const USD_PER_M_INPUT: f64 = 3.0;
const USD_PER_M_OUTPUT: f64 = 15.0;

const BLANK_SOURCE: &str = "vec4 render(vec2 pos) {\n    return vec4(0.0);\n}\n";

/// Starting source for the fix-it task: `brightnes` is undeclared, so this
/// fails to compile as-is.
const BROKEN_SOURCE: &str = "\
layout(binding = 0) uniform float time;

vec4 render(vec2 pos) {
    float glow = brightnes * 0.8;
    return vec4(glow, glow * 0.5, 0.1, 1.0);
}
";

/// Provider selection for one eval run, resolved from the environment:
/// `LPA_EVAL_BASE_URL` picks any OpenAI-compatible server, otherwise
/// `ANTHROPIC_API_KEY` picks Anthropic; neither set means skip. The caller
/// turns the resolved config into a live provider (the transport is
/// feature-gated in `lpa-agent`, so construction stays with the caller).
#[derive(Clone)]
pub enum ProviderCfg {
    Anthropic {
        api_key: String,
        model: Option<String>,
    },
    OpenAiCompat(OpenAiCompatConfig),
}

impl ProviderCfg {
    pub fn from_env() -> Option<Self> {
        Self::from_env_with(&|name| std::env::var(name).ok())
    }

    /// [`Self::from_env`] over an injected lookup, so the runner's
    /// spend-gate tests resolve against a fake environment instead of
    /// mutating the process env.
    pub fn from_env_with(lookup: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let model = lookup("LPA_EVAL_MODEL");
        if let Some(base_url) = lookup("LPA_EVAL_BASE_URL") {
            let model = model
                .expect("LPA_EVAL_BASE_URL requires LPA_EVAL_MODEL (compat servers have no default model id)");
            return Some(Self::OpenAiCompat(OpenAiCompatConfig {
                base_url,
                api_key: lookup("LPA_EVAL_API_KEY"),
                model,
                extra_headers: Vec::new(),
            }));
        }
        let api_key = lookup("ANTHROPIC_API_KEY")?;
        Some(Self::Anthropic { api_key, model })
    }

    /// Human-readable `provider model` label for the report header.
    pub fn label(&self) -> String {
        match self {
            Self::Anthropic { model, .. } => format!(
                "anthropic {}",
                model.clone().unwrap_or_else(|| "(default model)".into())
            ),
            Self::OpenAiCompat(c) => format!("openai-compat {} @ {}", c.model, c.base_url),
        }
    }
}

// -- task definitions ------------------------------------------------------

pub struct EvalTask {
    pub name: &'static str,
    pub starting_source: &'static str,
    pub prompt: &'static str,
    pub build_spec: fn(&[LedPoint]) -> ExperimentSpec,
    pub check: fn(&ExperimentResult) -> Vec<Check>,
}

pub fn tasks() -> Vec<EvalTask> {
    vec![
        EvalTask {
            name: "solid_red",
            starting_source: BLANK_SOURCE,
            prompt: "Make all the lights red.",
            build_spec: |leds| ExperimentSpec {
                probes: vec![ProbeSpec {
                    id: "leds".into(),
                    ty: ProbeType::Vec4,
                    expr: "render(pos)".into(),
                    domain: ProbeDomain::Leds { indices: None },
                    vary: None,
                    reduce: ProbeReduce::Stats,
                }],
                led_points: leds.to_vec(),
                ..ExperimentSpec::default()
            },
            check: check_solid_red,
        },
        EvalTask {
            name: "pulse_blue",
            starting_source: BLANK_SOURCE,
            prompt: "Make it pulse blue, roughly 2 pulses per second.",
            build_spec: |leds| ExperimentSpec {
                probes: vec![ProbeSpec {
                    id: "leds".into(),
                    ty: ProbeType::Vec4,
                    expr: "render(pos)".into(),
                    domain: ProbeDomain::Leds { indices: None },
                    vary: Some(lps_probe::ProbeVary {
                        binding: "time".into(),
                        // 16 steps over one second: enough to see 2 pulses
                        // without aliasing them away.
                        values: (0..16u16).map(|i| f32::from(i) / 16.0).collect(),
                    }),
                    reduce: ProbeReduce::Stats,
                }],
                led_points: leds.to_vec(),
                ..ExperimentSpec::default()
            },
            check: check_pulse_blue,
        },
        EvalTask {
            name: "fix_broken",
            starting_source: BROKEN_SOURCE,
            prompt: "The shader seems broken — fix it and keep the intended visual the same.",
            build_spec: |leds| ExperimentSpec {
                led_points: leds.to_vec(),
                ..ExperimentSpec::default()
            },
            check: check_fix_broken,
        },
        EvalTask {
            name: "radial_gradient",
            starting_source: BLANK_SOURCE,
            prompt: "Make it bright in the center, fading to dark at the edges.",
            build_spec: |leds| ExperimentSpec {
                probes: vec![ProbeSpec {
                    id: "lum".into(),
                    ty: ProbeType::Float,
                    expr: "dot(render(pos).rgb, vec3(0.2126, 0.7152, 0.0722))".into(),
                    domain: ProbeDomain::Line {
                        from: [0.5, 0.5],
                        to: [1.0, 1.0],
                        n: 16,
                    },
                    vary: None,
                    reduce: ProbeReduce::None,
                }],
                led_points: leds.to_vec(),
                ..ExperimentSpec::default()
            },
            check: check_radial_gradient,
        },
        EvalTask {
            name: "rainbow_ring",
            starting_source: BLANK_SOURCE,
            prompt: "Give the ring a rainbow — a different hue at each position around the ring.",
            build_spec: |leds| ExperimentSpec {
                probes: vec![ProbeSpec {
                    id: "led_rgb".into(),
                    ty: ProbeType::Vec3,
                    expr: "render(pos).rgb".into(),
                    domain: ProbeDomain::Leds { indices: None },
                    vary: None,
                    reduce: ProbeReduce::None,
                }],
                led_points: leds.to_vec(),
                ..ExperimentSpec::default()
            },
            check: check_rainbow_ring,
        },
    ]
}

// -- per-task assertions ---------------------------------------------------

fn check_solid_red(result: &ExperimentResult) -> Vec<Check> {
    let mut checks = vec![shader_compiles(result)];
    if let Some(step) = single_stats_step(result, "leds") {
        checks.push(Check::new(
            "mean.r > 0.7",
            step.mean[0] > 0.7,
            format!("mean.r = {:.3}", step.mean[0]),
        ));
        checks.push(Check::new(
            "mean.g < 0.15",
            step.mean[1] < 0.15,
            format!("mean.g = {:.3}", step.mean[1]),
        ));
        checks.push(Check::new(
            "mean.b < 0.15",
            step.mean[2] < 0.15,
            format!("mean.b = {:.3}", step.mean[2]),
        ));
    } else {
        checks.push(Check::missing_probe("leds"));
    }
    if let Some(h) = &result.health {
        checks.push(Check::new(
            "near_black_fraction < 0.1",
            h.near_black_fraction < 0.1,
            format!("near_black_fraction = {:.3}", h.near_black_fraction),
        ));
        checks.push(nan_free(h));
    }
    checks
}

fn check_pulse_blue(result: &ExperimentResult) -> Vec<Check> {
    let mut checks = vec![shader_compiles(result)];
    if let Some(steps) = stats_steps(result, "leds") {
        let b: Vec<f32> = steps.iter().map(|s| s.mean[2]).collect();
        let r_max = steps.iter().map(|s| s.mean[0]).fold(0.0f32, f32::max);
        let b_min = b.iter().copied().fold(f32::INFINITY, f32::min);
        let b_max = b.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        checks.push(Check::new(
            "b varies over 1s (max-min > 0.3)",
            b_max - b_min > 0.3,
            format!(
                "b range = {:.3} (min {b_min:.3}, max {b_max:.3})",
                b_max - b_min
            ),
        ));
        checks.push(Check::new(
            "r stays low (max mean.r < 0.3)",
            r_max < 0.3,
            format!("max mean.r = {r_max:.3}"),
        ));
        let rises = b.windows(2).any(|w| w[1] > w[0] + 0.02);
        let falls = b.windows(2).any(|w| w[1] < w[0] - 0.02);
        checks.push(Check::new(
            "at least one rise and one fall",
            rises && falls,
            format!("rises = {rises}, falls = {falls}, b = {b:.3?}"),
        ));
    } else {
        checks.push(Check::missing_probe("leds"));
    }
    if let Some(h) = &result.health {
        checks.push(nan_free(h));
    }
    checks
}

fn check_fix_broken(result: &ExperimentResult) -> Vec<Check> {
    let mut checks = vec![shader_compiles(result)];
    if let Some(h) = &result.health {
        checks.push(nan_free(h));
    } else {
        checks.push(Check::new(
            "health report present",
            false,
            "no health report (shader did not compile)".into(),
        ));
    }
    checks
}

fn check_radial_gradient(result: &ExperimentResult) -> Vec<Check> {
    let mut checks = vec![shader_compiles(result)];
    if let Some(rows) = value_rows(result, "lum") {
        let lum: Vec<f32> = rows.iter().map(|r| r.value[0]).collect();
        let center = lum.first().copied().unwrap_or(0.0);
        let edge = lum.last().copied().unwrap_or(1.0);
        checks.push(Check::new(
            "center luminance > 0.6",
            center > 0.6,
            format!("center = {center:.3}"),
        ));
        checks.push(Check::new(
            "edge luminance < 0.2",
            edge < 0.2,
            format!("edge = {edge:.3}"),
        ));
        let monotonic = lum.windows(2).all(|w| w[1] <= w[0] + 0.02);
        checks.push(Check::new(
            "luminance non-increasing center->corner (eps 0.02)",
            monotonic,
            format!("lum = {lum:.3?}"),
        ));
    } else {
        checks.push(Check::missing_probe("lum"));
    }
    if let Some(h) = &result.health {
        checks.push(nan_free(h));
    }
    checks
}

fn check_rainbow_ring(result: &ExperimentResult) -> Vec<Check> {
    let mut checks = vec![shader_compiles(result)];
    if let Some(rows) = value_rows(result, "led_rgb") {
        checks.push(Check::new(
            "one row per LED",
            rows.len() == RING_LEDS,
            format!("rows = {}", rows.len()),
        ));
        let mut variances = [0.0f32; 3];
        let mut spreads = [0.0f32; 3];
        for (c, variance) in variances.iter_mut().enumerate() {
            let vals: Vec<f32> = rows.iter().map(|r| r.value[c]).collect();
            let n = vals.len().max(1) as f32;
            let mean = vals.iter().sum::<f32>() / n;
            *variance = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
            let lo = vals.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = vals.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            spreads[c] = hi - lo;
        }
        let varied_channels = variances.iter().filter(|&&v| v > 0.01).count();
        checks.push(Check::new(
            "hue spread: >= 2 channels with variance > 0.01",
            varied_channels >= 2,
            format!("variances (r,g,b) = {variances:.4?}"),
        ));
        let max_spread = spreads.iter().copied().fold(0.0f32, f32::max);
        checks.push(Check::new(
            "not all points equal (max channel spread > 0.3)",
            max_spread > 0.3,
            format!("spreads (r,g,b) = {spreads:.3?}"),
        ));
    } else {
        checks.push(Check::missing_probe("led_rgb"));
    }
    if let Some(h) = &result.health {
        checks.push(nan_free(h));
    }
    checks
}

// -- shared check helpers --------------------------------------------------

pub struct Check {
    pub name: &'static str,
    pub pass: bool,
    pub detail: String,
}

impl Check {
    pub fn new(name: &'static str, pass: bool, detail: String) -> Self {
        Self { name, pass, detail }
    }

    pub fn missing_probe(id: &str) -> Self {
        Self {
            name: "probe evaluated",
            pass: false,
            detail: format!("probe '{id}' produced no usable outcome"),
        }
    }
}

fn shader_compiles(result: &ExperimentResult) -> Check {
    match &result.shader {
        ShaderCompileOutcome::Ok => Check::new("final source compiles", true, String::new()),
        ShaderCompileOutcome::Err { diagnostics } => Check::new(
            "final source compiles",
            false,
            format!("diagnostics: {diagnostics:?}"),
        ),
    }
}

fn nan_free(health: &HealthReport) -> Check {
    Check::new(
        "health nan_count == 0",
        health.nan_count == 0,
        format!("nan_count = {}", health.nan_count),
    )
}

fn stats_steps<'a>(result: &'a ExperimentResult, id: &str) -> Option<&'a [ProbeStepStats]> {
    match result.probes.get(&id.to_string()) {
        Some(ProbeOutcome::Stats(s)) if !s.steps.is_empty() => Some(&s.steps),
        _ => None,
    }
}

fn single_stats_step<'a>(result: &'a ExperimentResult, id: &str) -> Option<&'a ProbeStepStats> {
    stats_steps(result, id).map(|steps| &steps[0])
}

fn value_rows<'a>(result: &'a ExperimentResult, id: &str) -> Option<&'a [ProbeValueRow]> {
    match result.probes.get(&id.to_string()) {
        Some(ProbeOutcome::Values(v)) if !v.rows.is_empty() => Some(v.rows.as_slice()),
        _ => None,
    }
}

// -- harness ---------------------------------------------------------------

pub struct TaskOutcome {
    pub checks: Vec<Check>,
    pub turns: u32,
    pub usage: TokenUsage,
    pub wall_secs: f64,
    /// Assistant text + tool summaries, printed when the task fails.
    pub transcript: String,
    pub final_source: String,
    pub session_error: Option<String>,
}

impl TaskOutcome {
    pub fn passed(&self) -> bool {
        self.session_error.is_none() && self.checks.iter().all(|c| c.pass)
    }
}

/// Drive one task through a full agent session against `provider`, then run
/// the task's deterministic probe assertions on the final staged source.
/// The caller drives the future (evals: `tokio` current-thread `block_on`,
/// since the host transport needs a reactor).
pub async fn run_task(provider: Box<dyn ModelProvider>, task: &EvalTask) -> TaskOutcome {
    let leds = ring_points();
    let mut host = FakeHost::new(task.starting_source);
    host.leds = leds.clone();
    host.context = ShaderContext {
        project_name: "Eval Project".into(),
        node_name: task.name.into(),
        fixture: Some(FixtureSummary {
            name: "ring24".into(),
            led_count: RING_LEDS as u32,
            mapping_kind: "ring".into(),
        }),
        bindings: Vec::new(),
    };
    // Shared-source handle: the session owns the host, so the final staged
    // source is read back through the Rc after the run.
    let source = Rc::clone(&host.source);
    let mut session = AgentSession::new(provider, host);

    let mut turns = 0u32;
    let mut usage = TokenUsage::default();
    let mut transcript = String::new();
    let start = Instant::now();
    let run_result = session
        .run(task.prompt.to_string(), |event| match event {
            AgentEvent::TextDelta(text) => transcript.push_str(&text),
            AgentEvent::ToolExecuted { summary_json, .. } => {
                let _ = write!(transcript, "\n[iterate: {summary_json}]\n");
            }
            AgentEvent::TurnDone { .. } => turns += 1,
            AgentEvent::MaxTurnsReached { turns } => {
                let _ = write!(transcript, "\n[max turns reached: {turns}]\n");
            }
            AgentEvent::SessionDone { usage_total } => usage = usage_total,
            AgentEvent::ProviderError { message, .. } => {
                let _ = write!(transcript, "\n[provider error: {message}]\n");
            }
            _ => {}
        })
        .await;
    let wall_secs = start.elapsed().as_secs_f64();

    let final_source = source.borrow().clone();
    let spec = (task.build_spec)(&leds);
    let result = run_experiment(&final_source, &spec);
    let checks = (task.check)(&result);

    TaskOutcome {
        checks,
        turns,
        usage,
        wall_secs,
        transcript,
        final_source,
        session_error: run_result.err().map(|e| format!("{e:?}")),
    }
}

/// 24 LED points on a circle of radius 0.4 around the center.
pub fn ring_points() -> Vec<LedPoint> {
    (0..RING_LEDS)
        .map(|i| {
            let theta = (i as f32) / (RING_LEDS as f32) * core::f32::consts::TAU;
            LedPoint {
                label: format!("led{i}"),
                channel: i as u32,
                at: [0.5 + 0.4 * theta.cos(), 0.5 + 0.4 * theta.sin()],
            }
        })
        .collect()
}

// -- reporting -------------------------------------------------------------

pub fn render_report(outcomes: &[(EvalTask, TaskOutcome)], provider_cfg: &ProviderCfg) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "shader agent eval report — {}", provider_cfg.label());
    let _ = writeln!(
        out,
        "{:<17} {:<6} {:>7} {:>6} {:>9} {:>9} {:>8}",
        "task", "result", "checks", "turns", "in_tok", "out_tok", "wall_s"
    );
    let mut total = TokenUsage::default();
    for (task, o) in outcomes {
        let passed = o.checks.iter().filter(|c| c.pass).count();
        let _ = writeln!(
            out,
            "{:<17} {:<6} {:>7} {:>6} {:>9} {:>9} {:>8.1}",
            task.name,
            if o.passed() { "PASS" } else { "FAIL" },
            format!("{passed}/{}", o.checks.len()),
            o.turns,
            o.usage.input_tokens,
            o.usage.output_tokens,
            o.wall_secs,
        );
        total.add(o.usage);
    }
    match provider_cfg {
        ProviderCfg::Anthropic { .. } => {
            let input_m = f64::from(total.input_tokens) / 1e6;
            let output_m = f64::from(total.output_tokens) / 1e6;
            let cost = input_m * USD_PER_M_INPUT + output_m * USD_PER_M_OUTPUT;
            let _ = writeln!(
                out,
                "total: {} input + {} output tokens; est. ${cost:.2} at claude-sonnet-5 \
                 standard rates (${USD_PER_M_INPUT}/M in, ${USD_PER_M_OUTPUT}/M out)",
                total.input_tokens, total.output_tokens
            );
        }
        ProviderCfg::OpenAiCompat(_) => {
            let _ = writeln!(
                out,
                "total: {} input + {} output tokens (local/compat server; no cost estimate)",
                total.input_tokens, total.output_tokens
            );
        }
    }

    // Per-check detail (always listed; transcripts only for failures).
    for (task, o) in outcomes {
        let _ = writeln!(out, "\n## {}", task.name);
        if let Some(err) = &o.session_error {
            let _ = writeln!(out, "  SESSION ERROR: {err}");
        }
        for c in &o.checks {
            let _ = writeln!(
                out,
                "  [{}] {} {}",
                if c.pass { "pass" } else { "FAIL" },
                c.name,
                if c.detail.is_empty() {
                    String::new()
                } else {
                    format!("— {}", c.detail)
                }
            );
        }
        if !o.passed() {
            let _ = writeln!(out, "  transcript:\n{}", indent(&o.transcript, "    "));
            let _ = writeln!(out, "  final source:\n{}", indent(&o.final_source, "    "));
        }
    }
    out
}

fn indent(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|l| format!("{prefix}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write the report next to the other diffable run artifacts
/// (`target/eval-report.txt` at the workspace root).
pub fn write_report(report: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/eval-report.txt");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, report) {
        Ok(()) => eprintln!("evals: report written to {}", path.display()),
        Err(e) => eprintln!("evals: could not write {}: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_list_is_wellformed_and_checks_run_deterministically() {
        let tasks = tasks();
        let names: Vec<&str> = tasks.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "solid_red",
                "pulse_blue",
                "fix_broken",
                "radial_gradient",
                "rainbow_ring"
            ]
        );
        // Every task's spec builds and its checks evaluate against the
        // STARTING source with no provider involved — the checks judge a
        // model's output, so failures here are expected data; only the
        // token-free shape matters.
        for task in &tasks {
            let leds = ring_points();
            let spec = (task.build_spec)(&leds);
            let result = run_experiment(task.starting_source, &spec);
            let checks = (task.check)(&result);
            assert!(!checks.is_empty(), "task {} produced no checks", task.name);
        }
    }

    #[test]
    fn blank_report_renders_for_both_provider_kinds() {
        let anthropic = ProviderCfg::Anthropic {
            api_key: "unused".into(),
            model: None,
        };
        let report = render_report(&[], &anthropic);
        assert!(report.contains("anthropic (default model)"), "{report}");
        assert!(report.contains("est. $"), "{report}");

        let compat = ProviderCfg::OpenAiCompat(OpenAiCompatConfig {
            base_url: "http://localhost:11434/v1".into(),
            api_key: None,
            model: "qwen3.5:9b".into(),
            extra_headers: Vec::new(),
        });
        let report = render_report(&[], &compat);
        assert!(
            report.contains("openai-compat qwen3.5:9b @ http://localhost:11434/v1"),
            "{report}"
        );
        assert!(report.contains("no cost estimate"), "{report}");
    }
}
