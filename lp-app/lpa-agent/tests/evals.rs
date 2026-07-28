//! Live eval corpus for the shader agent (P6): the `#[ignore]`d entrypoint
//! over `lpa_agent_harness::eval_tasks` (task definitions, probe
//! assertions, provider resolution, and reporting live there).
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

use lpa_agent::provider::ReqwestTransport;
use lpa_agent::{AnthropicConfig, AnthropicProvider, ModelProvider, OpenAiCompatProvider};
use lpa_agent_harness::{ProviderCfg, render_report, run_task, tasks, write_report};

/// Turn the env-resolved config into a live provider. Stays here (not in
/// the harness) because the reqwest transport is `host-transport`-gated.
fn build_provider(cfg: &ProviderCfg) -> Box<dyn ModelProvider> {
    match cfg {
        ProviderCfg::Anthropic { api_key, model } => {
            let mut config = AnthropicConfig::new(api_key);
            if let Some(model) = model {
                config.model = model.clone();
            }
            Box::new(AnthropicProvider::new(config, ReqwestTransport::new()))
        }
        ProviderCfg::OpenAiCompat(config) => Box::new(OpenAiCompatProvider::new(
            config.clone(),
            ReqwestTransport::new(),
        )),
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
        let outcome = rt.block_on(run_task(build_provider(&provider_cfg), &task));
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
