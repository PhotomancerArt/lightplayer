//! The headless session driver: one full agent run against the real
//! in-process studio stack (lpa-studio-core's `harness` world), leaving
//! the four triage artifacts behind.
//!
//! Composition: the same actor/controller/bridge/overlay path the browser
//! runs — only the shell differs (a tokio current-thread `LocalSet`
//! standing in for `spawn_local`, real `tokio::time` timers standing in
//! for `setTimeout`). Providers are factory-injected exactly like the web
//! shell does it: scripted playback (token-free) or the real host
//! transport (spend-gated upstream, in `spend_gate` — by the time this
//! module runs, spending was already authorized).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use lpa_agent::provider::ReqwestTransport;
use lpa_agent::provider::model_provider::TurnEvent;
use lpa_agent::{AnthropicProvider, ModelProvider, OpenAiCompatProvider};
use lpa_studio_core::harness::{AgentWorld, ShaderFrontend, project_action, try_find_asset_editor};
use lpa_studio_core::{
    AgentProvider, AgentProviderConfig, ProjectOp, SettingsCommand, StudioActor, StudioCommand,
    UiAgentStatus, UiAgentTurn, UiAgentUsage, UiAgentView, UiStudioView,
};

use crate::ScriptedProvider;
use crate::dump::parse_dump;
use crate::eval_tasks::ProviderCfg;
use crate::runner::cli_args::RunnerArgs;
use crate::runner::run_artifacts::{
    DEBUG_JSON, FINAL_GLSL, RUN_MD, TRIAGE_TXT, resolve_out_dir, write_artifact,
};
use crate::runner::run_script::RunScript;
use crate::runner::spend_gate::RunMode;
use crate::triage::triage_dump;

/// What one completed run reports back (the bin prints it; the scripted
/// integration test asserts on it).
pub struct RunSummary {
    pub out_dir: PathBuf,
    /// Final session status line (`idle` or the error message).
    pub status: String,
    pub usage: UiAgentUsage,
    pub estimated_cost: Option<String>,
    /// Matched bug-class lines (also written to `triage.txt`).
    pub triage: Vec<String>,
}

/// Run one headless session. The spend gate already ran: `mode` carries
/// either the script or an authorized provider config.
pub fn run(args: &RunnerArgs, mode: RunMode) -> Result<RunSummary, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(drive_session(args, mode)))
}

async fn drive_session(args: &RunnerArgs, mode: RunMode) -> Result<RunSummary, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("clock: {error}"))?
        .as_secs();
    let out_dir = resolve_out_dir(args.out.clone(), now);

    // -- world + run header ------------------------------------------------
    let mode_label = match &mode {
        RunMode::Scripted(path) => format!("scripted ({})", path.display()),
        RunMode::Real(cfg) => format!("real ({})", cfg.label()),
    };
    let mut world = AgentWorld::new(args.frontend);
    let frontend_name = frontend_name(args.frontend);
    println!("agent-run: mode = {mode_label}");
    println!(
        "agent-run: frontend = {frontend_name} · vm runtime = {} (resolved host-side)",
        world.backend_name
    );
    let mut parity_note = None;
    match args.frontend {
        ShaderFrontend::Naga if world.backend_name.contains("lpvm-wasm") => {
            println!(
                "agent-run: browser-parity path CONFIRMED — naga frontend + lpvm-wasm emission \
                 (rt_wasmtime executes the same emitted shader WASM rt_browser does)"
            );
        }
        ShaderFrontend::Naga => {
            let note = format!(
                "FOLLOW-UP: naga+lpvm-wasm emission is NOT what this server executes host-side \
                 (backend = {}) — this run fuzzed a different path than the browser",
                world.backend_name
            );
            eprintln!("agent-run: {note}");
            parity_note = Some(note);
        }
        ShaderFrontend::LpsGlsl => {
            // The VM engine is target-fixed (host = lpvm-wasm/rt_wasmtime;
            // rv32 native JIT exists only on riscv32 builds), so lps-glsl
            // mode matches the DEVICE frontend but not its backend.
            println!(
                "agent-run: note — lps-glsl device parity is frontend-only: hardware executes \
                 the rv32 native JIT, so rv32lpn-only classes (e.g. builtin-inventory gaps) \
                 are NOT reachable host-side"
            );
        }
    }

    // -- settings + provider factory --------------------------------------
    install_provider_settings(&mut world, args, &mode);
    if let Some(turns) = args.max_turns {
        world.controller.set_agent_max_turns(turns);
    }
    match mode {
        RunMode::Scripted(path) => {
            let scripts = RunScript::load(&path)?.to_run_scripts();
            let remaining: RefCell<VecDeque<Vec<Vec<TurnEvent>>>> = RefCell::new(scripts.into());
            world.controller.set_agent_provider_factory(move |_config| {
                let turns = remaining.borrow_mut().pop_front().unwrap_or_else(|| {
                    vec![vec![TurnEvent::ProviderError {
                        message: "script exhausted: no runs left in the script file".into(),
                        retryable: false,
                    }]]
                });
                Box::new(ScriptedProvider::new(turns))
            });
        }
        RunMode::Real(_) => {
            // The ONLY place a transport is built — downstream of the gate.
            world.controller.set_agent_provider_factory(|config| {
                let provider: Box<dyn ModelProvider> = match config {
                    AgentProviderConfig::Anthropic(config) => Box::new(AnthropicProvider::new(
                        config.clone(),
                        ReqwestTransport::new(),
                    )),
                    AgentProviderConfig::OpenAiCompat(config) => Box::new(
                        OpenAiCompatProvider::new(config.clone(), ReqwestTransport::new()),
                    ),
                };
                provider
            });
        }
    }
    world
        .controller
        .set_agent_spawner(|future| drop(tokio::task::spawn_local(future)));

    // -- actor + tick pump -------------------------------------------------
    let (actor, handle) = StudioActor::new(world.controller, |delay| tokio::time::sleep(delay));
    let tx = handle.tx;
    let mut view = handle.view;
    let delay = handle.delay;
    tokio::task::spawn_local(actor.run());
    tokio::task::spawn_local({
        let tx = tx.clone();
        let delay = Rc::clone(&delay);
        async move {
            loop {
                tokio::time::sleep(delay.get().max(Duration::from_millis(10))).await;
                tx.send(StudioCommand::RefreshTick);
            }
        }
    });

    // -- connect -----------------------------------------------------------
    tx.send(project_action(ProjectOp::ConnectRunningProject));
    let agent = await_view(
        &mut view,
        Duration::from_secs(120),
        "agent chat DTO",
        agent_view,
    )
    .await?;
    if !matches!(
        agent.availability,
        lpa_studio_core::UiAgentAvailability::Ready
    ) {
        return Err("agent availability is NeedsKey after settings install (bug)".into());
    }

    // -- send + stream the run --------------------------------------------
    println!("agent-run: sending prompt: {}", args.prompt);
    tx.send(StudioCommand::Action(agent.send_action(&args.prompt)));
    let mut printed_turns = 0usize;
    let mut saw_busy = false;
    // The staged source at run end, refreshed from every snapshot that
    // carries the editor (snapshots coalesce, so capture as we go).
    let mut final_source = String::new();
    let mut capture_source = |snap: &UiStudioView| {
        if let Some(text) = try_find_asset_editor(snap).and_then(|editor| {
            editor
                .content
                .as_ref()
                .and_then(|c| c.text().map(String::from))
        }) {
            final_source = text;
        }
    };
    let final_agent = await_view(&mut view, Duration::from_secs(600), "run end", |snap| {
        capture_source(snap);
        let agent = agent_view(snap)?;
        saw_busy |= agent.busy();
        print_settled_turns(
            &agent,
            &mut printed_turns,
            /* run_over */ !agent.busy(),
        );
        match &agent.status {
            UiAgentStatus::Error { .. } => Some(agent),
            UiAgentStatus::Idle if saw_busy || run_finished(&agent) => Some(agent),
            _ => None,
        }
    })
    .await?;
    print_settled_turns(&final_agent, &mut printed_turns, true);
    let status = match &final_agent.status {
        UiAgentStatus::Error { message, retryable } => {
            format!("error: {message} (retryable: {retryable})")
        }
        _ => "idle".to_string(),
    };
    println!("agent-run: run ended — {status}");

    // -- collect artifacts -------------------------------------------------
    tx.send(StudioCommand::Action(final_agent.export_debug_action()));
    let debug_json = await_view(
        &mut view,
        Duration::from_secs(120),
        "debug export",
        |snap| {
            capture_source(snap);
            agent_view(snap)?.debug.map(|dump| dump.json.to_string())
        },
    )
    .await
    .unwrap_or_else(|error| {
        // A run with zero completed turns has nothing to export; keep the
        // other artifacts rather than failing the whole run.
        eprintln!("agent-run: debug export unavailable ({error})");
        String::new()
    });
    drop(capture_source);

    let run_md = lpa_studio_core::chat_markdown(&final_agent);

    let mut triage = Vec::new();
    if let Some(note) = &parity_note {
        triage.push(note.clone());
    }
    if debug_json.is_empty() {
        triage.push("(no debug dump — session exported nothing)".to_string());
    } else {
        match parse_dump(&debug_json) {
            Ok(dump) => {
                let matches = triage_dump(&dump);
                if matches.is_empty() {
                    triage.push("no known bug-class signatures matched".to_string());
                } else {
                    triage.extend(matches);
                }
            }
            Err(error) => triage.push(format!("(dump unreadable: {error})")),
        }
    }
    let triage_text = format!(
        "frontend = {frontend_name} · vm runtime = {}\nstatus = {status}\n\n{}\n",
        world.backend_name,
        triage.join("\n")
    );

    write_artifact(&out_dir, DEBUG_JSON, &debug_json)?;
    write_artifact(&out_dir, FINAL_GLSL, &final_source)?;
    write_artifact(&out_dir, RUN_MD, &run_md)?;
    write_artifact(&out_dir, TRIAGE_TXT, &triage_text)?;

    // -- cost & shutdown ---------------------------------------------------
    let usage = final_agent.usage;
    println!(
        "agent-run: usage — {} in ({} fresh + {} cache-write + {} cache-read) · {} out{}",
        usage.total_input_tokens(),
        usage.input_tokens,
        usage.cache_write_tokens,
        usage.cache_read_tokens,
        usage.output_tokens,
        final_agent
            .estimated_cost
            .as_deref()
            .map(|cost| format!(" · est. {cost}"))
            .unwrap_or_default(),
    );
    println!("agent-run: artifacts in {}", out_dir.display());
    for line in &triage {
        println!("agent-run: triage: {line}");
    }
    tx.send(StudioCommand::Shutdown);

    Ok(RunSummary {
        out_dir,
        status,
        usage,
        estimated_cost: final_agent.estimated_cost.clone(),
        triage,
    })
}

/// Install the provider settings the availability check and the factory's
/// [`AgentProviderConfig`] resolve from.
fn install_provider_settings(world: &mut AgentWorld, args: &RunnerArgs, mode: &RunMode) {
    let controller = &mut world.controller;
    match mode {
        RunMode::Scripted(_) => {
            // The scripted factory ignores the config; the key only makes
            // availability Ready (never leaves the process, never used).
            controller.apply_settings_command(SettingsCommand::SetAgentProvider(Some(
                AgentProvider::Anthropic,
            )));
            controller.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
                "sk-scripted-placeholder".into(),
            )));
            controller.apply_settings_command(SettingsCommand::SetAgentModel(args.model.clone()));
        }
        RunMode::Real(ProviderCfg::Anthropic { api_key, model }) => {
            controller.apply_settings_command(SettingsCommand::SetAgentProvider(Some(
                AgentProvider::Anthropic,
            )));
            controller.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
                api_key.clone(),
            )));
            controller.apply_settings_command(SettingsCommand::SetAgentModel(
                args.model.clone().or_else(|| model.clone()),
            ));
        }
        RunMode::Real(ProviderCfg::OpenAiCompat(config)) => {
            controller.apply_settings_command(SettingsCommand::SetAgentProvider(Some(
                AgentProvider::Custom,
            )));
            controller.apply_settings_command(SettingsCommand::SetAgentCustomBaseUrl(Some(
                config.base_url.clone(),
            )));
            controller.apply_settings_command(SettingsCommand::SetAgentCustomApiKey(
                config.api_key.clone(),
            ));
            controller.apply_settings_command(SettingsCommand::SetAgentModel(Some(
                args.model.clone().unwrap_or_else(|| config.model.clone()),
            )));
        }
    }
}

fn frontend_name(frontend: ShaderFrontend) -> &'static str {
    match frontend {
        ShaderFrontend::Naga => "naga",
        ShaderFrontend::LpsGlsl => "lps-glsl",
    }
}

/// The agent chat DTO from a snapshot (the shader editor's decoration).
fn agent_view(view: &UiStudioView) -> Option<UiAgentView> {
    try_find_asset_editor(view)?.agent
}

/// A run that completed before we ever saw it busy (snapshot coalescing):
/// the transcript ends in something other than the user's message.
fn run_finished(agent: &UiAgentView) -> bool {
    match agent.turns.last() {
        None | Some(UiAgentTurn::User { .. }) => false,
        Some(UiAgentTurn::Tool(row)) => row.done,
        Some(_) => true,
    }
}

/// Print transcript turns as they settle: everything before the last turn
/// is final; the last turn only once the run is over (streaming text and
/// live tool rows keep mutating in place until then).
fn print_settled_turns(agent: &UiAgentView, printed: &mut usize, run_over: bool) {
    let settled = if run_over {
        agent.turns.len()
    } else {
        agent.turns.len().saturating_sub(1)
    };
    for turn in &agent.turns[(*printed).min(settled)..settled] {
        match turn {
            UiAgentTurn::User { text } => println!("  [user] {text}"),
            UiAgentTurn::Assistant { text } => println!("  [agent] {text}"),
            UiAgentTurn::Thinking { .. } => {}
            UiAgentTurn::Tool(row) => println!("  [tool] {}", row.summary_line()),
            UiAgentTurn::Notice { text, .. } => println!("  [notice] {text}"),
        }
    }
    *printed = (*printed).max(settled);
}

/// Await the next snapshot for which `matcher` yields, with a wall-clock
/// timeout (a hung run fails loudly instead of forever).
async fn await_view<T>(
    view: &mut lpa_studio_core::StudioViewReceiver,
    timeout: Duration,
    what: &str,
    mut matcher: impl FnMut(&UiStudioView) -> Option<T>,
) -> Result<T, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let next = tokio::time::timeout_at(deadline, view.recv())
            .await
            .map_err(|_| format!("timed out after {timeout:?} waiting for {what}"))?;
        let Some(snapshot) = next else {
            return Err(format!("view channel closed while waiting for {what}"));
        };
        if let Some(matched) = matcher(&snapshot) {
            return Ok(matched);
        }
    }
}
