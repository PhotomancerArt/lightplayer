//! Live eval corpus for the shader agent (P6).
//!
//! Five tasks, each: a stub [`AgentHost`] with a starting source + a ring
//! fixture (24 LED points on a circle), one user prompt driven through
//! [`AgentSession::run`] against the real Anthropic API, then deterministic
//! probe assertions on the final staged source via `lps_probe::run_experiment`.
//!
//! Opt-in and never CI-blocking:
//! - the test is `#[ignore]`d, so plain `cargo test` never touches it;
//! - without provider configuration it prints a notice and skips cleanly.
//!
//! Run against Anthropic (the default provider):
//!
//! ```sh
//! ANTHROPIC_API_KEY=... cargo test -p lpa-agent --features host-transport -- --ignored evals
//! ```
//!
//! Or against any OpenAI-compatible server (Ollama, LM Studio, llama.cpp,
//! vLLM) — `LPA_EVAL_MODEL` is required here since compat servers have no
//! default model id, and `LPA_EVAL_API_KEY` is optional:
//!
//! ```sh
//! LPA_EVAL_BASE_URL=http://localhost:11434/v1 LPA_EVAL_MODEL=qwen3.5:9b \
//!     cargo test -p lpa-agent --features host-transport -- --ignored evals
//! ```
//!
//! Optional: `LPA_EVAL_MODEL` overrides the model id (default
//! `AnthropicConfig::new`'s default model).
//!
//! The summary table is printed and also written to `target/eval-report.txt`
//! (workspace target dir) so runs are diffable.

#![cfg(feature = "host-transport")]

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use lpa_agent::provider::ReqwestTransport;
use lpa_agent::{
    AgentEvent, AgentHost, AgentSession, AnthropicConfig, AnthropicProvider, FixtureSummary,
    HostError, ModelProvider, OpenAiCompatConfig, OpenAiCompatProvider, ShaderContext, TokenUsage,
};
use lps_probe::{
    ExperimentResult, ExperimentSpec, HealthReport, LedPoint, ProbeDomain, ProbeOutcome,
    ProbeReduce, ProbeSpec, ProbeStepStats, ProbeType, ProbeValueRow, ShaderCompileOutcome,
    run_experiment,
};

/// Ring fixture size (well under the 64-row raw cap).
const RING_LEDS: usize = 24;

/// claude-sonnet-5 pricing per million tokens (standard rates; the
/// introductory promo through 2026-08-31 is $2/$10).
const USD_PER_M_INPUT: f64 = 3.0;
const USD_PER_M_OUTPUT: f64 = 15.0;

const BLANK_SOURCE: &str = "vec4 render_2d(vec2 pos) {\n    return vec4(0.0);\n}\n";

/// Starting source for the fix-it task: `brightnes` is undeclared, so this
/// fails to compile as-is.
const BROKEN_SOURCE: &str = "\
layout(binding = 0) uniform float time;

vec4 render_2d(vec2 pos) {
    float glow = brightnes * 0.8;
    return vec4(glow, glow * 0.5, 0.1, 1.0);
}
";

/// Provider selection for one eval run, resolved from the environment:
/// `LPA_EVAL_BASE_URL` picks any OpenAI-compatible server, otherwise
/// `ANTHROPIC_API_KEY` picks Anthropic; neither set means skip.
enum ProviderCfg {
    Anthropic {
        api_key: String,
        model: Option<String>,
    },
    OpenAiCompat(OpenAiCompatConfig),
}

impl ProviderCfg {
    fn from_env() -> Option<Self> {
        let model = std::env::var("LPA_EVAL_MODEL").ok();
        if let Ok(base_url) = std::env::var("LPA_EVAL_BASE_URL") {
            let model = model
                .expect("LPA_EVAL_BASE_URL requires LPA_EVAL_MODEL (compat servers have no default model id)");
            return Some(Self::OpenAiCompat(OpenAiCompatConfig {
                base_url,
                api_key: std::env::var("LPA_EVAL_API_KEY").ok(),
                model,
                extra_headers: Vec::new(),
            }));
        }
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        Some(Self::Anthropic { api_key, model })
    }

    /// Human-readable `provider model` label for the report header.
    fn label(&self) -> String {
        match self {
            Self::Anthropic { model, .. } => format!(
                "anthropic {}",
                model.clone().unwrap_or_else(|| "(default model)".into())
            ),
            Self::OpenAiCompat(c) => format!("openai-compat {} @ {}", c.model, c.base_url),
        }
    }

    fn build(&self) -> Box<dyn ModelProvider> {
        match self {
            Self::Anthropic { api_key, model } => {
                let mut config = AnthropicConfig::new(api_key);
                if let Some(model) = model {
                    config.model = model.clone();
                }
                Box::new(AnthropicProvider::new(config, ReqwestTransport::new()))
            }
            Self::OpenAiCompat(config) => Box::new(OpenAiCompatProvider::new(
                config.clone(),
                ReqwestTransport::new(),
            )),
        }
    }
}

#[test]
#[ignore = "live eval: needs a provider (see module docs) and network; run with -- --ignored evals"]
fn evals() {
    let Some(provider_cfg) = ProviderCfg::from_env() else {
        eprintln!(
            "evals: set ANTHROPIC_API_KEY, or LPA_EVAL_BASE_URL + LPA_EVAL_MODEL for a local \
             OpenAI-compatible server; skipping the live eval corpus"
        );
        return;
    };
    eprintln!("evals: provider = {}", provider_cfg.label());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut outcomes = Vec::new();
    for task in tasks() {
        eprintln!("evals: running task {}...", task.name);
        let outcome = run_task(&rt, &provider_cfg, &task);
        eprintln!(
            "evals: task {} -> {} ({} turns, {:.1}s)",
            task.name,
            if outcome.passed() { "PASS" } else { "FAIL" },
            outcome.turns,
            outcome.wall_secs
        );
        outcomes.push((task, outcome));
    }

    let report = render_report(&outcomes, &provider_cfg);
    eprintln!("\n{report}");
    write_report(&report);

    let failed: Vec<&str> = outcomes
        .iter()
        .filter(|(_, o)| !o.passed())
        .map(|(t, _)| t.name)
        .collect();
    assert!(
        failed.is_empty(),
        "eval tasks failed: {failed:?} (see the report above / target/eval-report.txt)"
    );
}

// -- task definitions ------------------------------------------------------

struct EvalTask {
    name: &'static str,
    starting_source: &'static str,
    prompt: &'static str,
    build_spec: fn(&[LedPoint]) -> ExperimentSpec,
    check: fn(&ExperimentResult) -> Vec<Check>,
}

fn tasks() -> Vec<EvalTask> {
    vec![
        EvalTask {
            name: "solid_red",
            starting_source: BLANK_SOURCE,
            prompt: "Make all the lights red.",
            build_spec: |leds| ExperimentSpec {
                probes: vec![ProbeSpec {
                    id: "leds".into(),
                    ty: ProbeType::Vec4,
                    expr: "render_2d(pos)".into(),
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
                    expr: "render_2d(pos)".into(),
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
                    expr: "dot(render_2d(pos).rgb, vec3(0.2126, 0.7152, 0.0722))".into(),
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
                    expr: "render_2d(pos).rgb".into(),
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

struct Check {
    name: &'static str,
    pass: bool,
    detail: String,
}

impl Check {
    fn new(name: &'static str, pass: bool, detail: String) -> Self {
        Self { name, pass, detail }
    }

    fn missing_probe(id: &str) -> Self {
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

struct TaskOutcome {
    checks: Vec<Check>,
    turns: u32,
    usage: TokenUsage,
    wall_secs: f64,
    /// Assistant text + tool summaries, printed when the task fails.
    transcript: String,
    final_source: String,
    session_error: Option<String>,
}

impl TaskOutcome {
    fn passed(&self) -> bool {
        self.session_error.is_none() && self.checks.iter().all(|c| c.pass)
    }
}

fn run_task(
    rt: &tokio::runtime::Runtime,
    provider_cfg: &ProviderCfg,
    task: &EvalTask,
) -> TaskOutcome {
    let provider = provider_cfg.build();

    let leds = ring_points();
    let source = Rc::new(RefCell::new(task.starting_source.to_string()));
    let host = EvalHost {
        source: Rc::clone(&source),
        leds: leds.clone(),
        node_name: task.name,
    };
    let mut session = AgentSession::new(provider, host);

    let mut turns = 0u32;
    let mut usage = TokenUsage::default();
    let mut transcript = String::new();
    let start = Instant::now();
    let run_result = rt.block_on(session.run(task.prompt.to_string(), |event| match event {
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
    }));
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
fn ring_points() -> Vec<LedPoint> {
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

/// Stub host: shared-source cell so the harness can read the final staged
/// source after the session (which owns the host) finishes.
struct EvalHost {
    source: Rc<RefCell<String>>,
    leds: Vec<LedPoint>,
    node_name: &'static str,
}

impl AgentHost for EvalHost {
    fn current_source(&self) -> Result<String, HostError> {
        Ok(self.source.borrow().clone())
    }

    fn stage_source<'a>(
        &'a mut self,
        source: &'a str,
    ) -> lpa_agent::HostFuture<'a, Result<(), HostError>> {
        Box::pin(async move {
            *self.source.borrow_mut() = source.to_string();
            Ok(())
        })
    }

    fn led_points(&self) -> Vec<LedPoint> {
        self.leds.clone()
    }

    fn shader_context(&self) -> ShaderContext {
        ShaderContext {
            project_name: "Eval Project".into(),
            node_name: self.node_name.into(),
            fixture: Some(FixtureSummary {
                name: "ring24".into(),
                led_count: RING_LEDS as u32,
                mapping_kind: "ring".into(),
            }),
            bindings: Vec::new(),
            ..ShaderContext::default()
        }
    }
}

// -- reporting -------------------------------------------------------------

fn render_report(outcomes: &[(EvalTask, TaskOutcome)], provider_cfg: &ProviderCfg) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "shader agent eval report — {}", provider_cfg.label());
    let _ = writeln!(
        out,
        "{:<17} {:<6} {:>7} {:>6} {:>9} {:>9} {:>9} {:>9} {:>8}",
        "task", "result", "checks", "turns", "in_tok", "cache_w", "cache_r", "out_tok", "wall_s"
    );
    let mut total = TokenUsage::default();
    for (task, o) in outcomes {
        let passed = o.checks.iter().filter(|c| c.pass).count();
        let _ = writeln!(
            out,
            "{:<17} {:<6} {:>7} {:>6} {:>9} {:>9} {:>9} {:>9} {:>8.1}",
            task.name,
            if o.passed() { "PASS" } else { "FAIL" },
            format!("{passed}/{}", o.checks.len()),
            o.turns,
            o.usage.input_tokens,
            o.usage.cache_write_tokens,
            o.usage.cache_read_tokens,
            o.usage.output_tokens,
            o.wall_secs,
        );
        total.add(o.usage);
    }
    match provider_cfg {
        ProviderCfg::Anthropic { .. } => {
            // Cache rates follow the provider convention (and the app's
            // `AgentCostRates::from_io`): write = 1.25x input, read = 0.1x.
            let cost = f64::from(total.input_tokens) / 1e6 * USD_PER_M_INPUT
                + f64::from(total.cache_write_tokens) / 1e6 * USD_PER_M_INPUT * 1.25
                + f64::from(total.cache_read_tokens) / 1e6 * USD_PER_M_INPUT * 0.1
                + f64::from(total.output_tokens) / 1e6 * USD_PER_M_OUTPUT;
            let _ = writeln!(
                out,
                "total: {} uncached-in + {} cache-write + {} cache-read + {} out tokens; \
                 est. ${cost:.2} at claude-sonnet-5 standard rates (${USD_PER_M_INPUT}/M in \
                 [write 1.25x, read 0.1x], ${USD_PER_M_OUTPUT}/M out)",
                total.input_tokens,
                total.cache_write_tokens,
                total.cache_read_tokens,
                total.output_tokens
            );
        }
        ProviderCfg::OpenAiCompat(_) => {
            let _ = writeln!(
                out,
                "total: {} input ({} cached reads) + {} output tokens (local/compat server; \
                 no cost estimate)",
                total.input_tokens, total.cache_read_tokens, total.output_tokens
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

fn write_report(report: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/eval-report.txt");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, report) {
        Ok(()) => eprintln!("evals: report written to {}", path.display()),
        Err(e) => eprintln!("evals: could not write {}: {e}", path.display()),
    }
}
