//! The scripted (token-free) battery: full runner passes over scripted
//! sessions through the REAL in-process studio stack, one scenario per
//! regression this round hit live — happy path (all four artifacts, real
//! engine verdict), truncation mid-tool-JSON, upsert-on-broken-compile,
//! and the probe-ok/engine-error split. Rides `just test` (the justfile
//! runs this crate with `--features runner`); no key, no gate, no network
//! — see `scripted_mode_never_consults_the_environment`.

#![cfg(feature = "runner")]

use std::path::{Path, PathBuf};

use lpa_agent::ContentBlock;
use lpa_agent_harness::runner::{ModeDecision, RunMode, RunnerArgs, decide_mode, run};
use lpa_agent_harness::{Dump, parse_dump};
use lpa_studio_core::harness::ShaderFrontend;
use serde_json::Value;

const GREEN: &str = "vec4(0.0, 1.0, 0.0, 1.0)";

fn scratch_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join(name)
}

/// Run one scripted scenario end-to-end into `out`.
fn run_scripted(script: &str, prompt: &str, out: &Path) -> lpa_agent_harness::runner::RunSummary {
    let script = script_path(script);
    let args = RunnerArgs {
        prompt: prompt.into(),
        frontend: ShaderFrontend::Naga,
        out: Some(out.to_path_buf()),
        model: None,
        scripted: Some(script.clone()),
        max_turns: None,
    };
    run(&args, RunMode::Scripted(script)).expect("scripted run completes")
}

fn read_artifact(out: &Path, name: &str) -> String {
    std::fs::read_to_string(out.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every tool-result content string in the dump, in transcript order.
fn tool_result_contents(dump: &Dump) -> Vec<Value> {
    dump.messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|block| match block {
            ContentBlock::ToolResult { content, .. } => {
                Some(serde_json::from_str(content).expect("tool result is JSON"))
            }
            _ => None,
        })
        .collect()
}

/// The Anthropic replay contract: every `tool_use` block must be answered
/// by exactly one `tool_result`, and no result may answer a call that was
/// never recorded — a dump violating this cannot be replayed to the API.
fn assert_replay_protocol_valid(dump: &Dump) {
    let mut pending: Vec<&str> = Vec::new();
    for message in &dump.messages {
        for block in &message.content {
            match block {
                ContentBlock::ToolUse { id, .. } => pending.push(id),
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    let at = pending
                        .iter()
                        .position(|id| id == tool_use_id)
                        .unwrap_or_else(|| {
                            panic!("tool_result {tool_use_id:?} answers no pending tool_use")
                        });
                    pending.remove(at);
                }
                _ => {}
            }
        }
    }
    assert!(
        pending.is_empty(),
        "dangling tool_use without a tool_result: {pending:?}"
    );
}

#[test]
fn scripted_run_writes_all_four_artifacts() {
    let out = scratch_dir("scripted-run");
    let summary = run_scripted("green-demo.json", "make it green", &out);
    assert_eq!(summary.status, "idle");
    assert_eq!(summary.out_dir, out);
    // Usage totals flow from the script's TurnDone events (20+40 / 30+10).
    assert_eq!(summary.usage.input_tokens, 60);
    assert_eq!(summary.usage.output_tokens, 40);

    // debug.json: format-1 dump with the staged edit and both stop reasons.
    let dump = parse_dump(&read_artifact(&out, "debug.json")).expect("dump parses");
    let stops: Vec<&str> = dump.turns.iter().map(|t| t.stop_reason.as_str()).collect();
    assert_eq!(stops, ["tool_use", "end_turn"]);
    assert_eq!(dump.edits.len(), 1, "one staged edit");
    assert!(
        dump.edits[0].source.contains(GREEN),
        "{}",
        dump.edits[0].source
    );
    assert_eq!(dump.edits[0].note.as_deref(), Some("go green"));
    assert_replay_protocol_valid(&dump);

    // The naga engine verdict is REAL, not the frontend-missing error the
    // first real-provider smoke hit (the runner binary must compile the
    // naga feature in — lpa-studio-core `harness` → lpa-server/naga).
    assert_eq!(
        dump.edits[0].engine_ok,
        Some(true),
        "naga engine verdict must be a real OK"
    );
    let results = tool_result_contents(&dump);
    assert_eq!(results[0]["engine"]["status"], "ok", "{}", results[0]);

    // final.glsl: the staged source at run end (the overlay-aware editor
    // content — the green shader, not the project's starting gradient).
    let final_glsl = read_artifact(&out, "final.glsl");
    assert!(final_glsl.contains(GREEN), "{final_glsl}");

    // run.md: the chat log with the user prompt, tool row, engine line,
    // and usage.
    let run_md = read_artifact(&out, "run.md");
    assert!(run_md.contains("## User"), "{run_md}");
    assert!(run_md.contains("make it green"), "{run_md}");
    assert!(run_md.contains("go green"), "{run_md}");
    assert!(run_md.contains("- engine: ok"), "{run_md}");
    assert!(run_md.contains("The lights are green now."), "{run_md}");

    // triage.txt: states the exercised compiler path and the (clean) triage.
    let triage = read_artifact(&out, "triage.txt");
    assert!(triage.contains("frontend = naga"), "{triage}");
    assert!(triage.contains("lpvm-wasm::rt_wasmtime"), "{triage}");
    assert!(
        triage.contains("no known bug-class signatures matched"),
        "{triage}"
    );
    assert!(
        !triage.contains("not built into this binary"),
        "the engine ran WITHOUT the naga frontend — feature wiring regressed: {triage}"
    );
}

#[test]
fn lps_glsl_mode_states_its_compiler_path() {
    let script = script_path("green-demo.json");
    let out = scratch_dir("scripted-run-lps");
    let args = RunnerArgs {
        prompt: "make it green".into(),
        frontend: ShaderFrontend::LpsGlsl,
        out: Some(out.clone()),
        model: None,
        scripted: Some(script.clone()),
        max_turns: Some(4),
    };
    let summary = run(&args, RunMode::Scripted(script)).expect("scripted run completes");
    assert_eq!(summary.status, "idle");
    let triage = read_artifact(&out, "triage.txt");
    assert!(triage.contains("frontend = lps-glsl"), "{triage}");
}

#[test]
fn truncation_mid_tool_json_warns_and_leaves_a_replayable_transcript() {
    // A MaxTokens cut mid-tool-input-JSON (the wire shape of a real
    // output-token-limit stop). The run must end idle with a warning
    // notice, no tool row left pulsing "running", and a transcript that
    // replays: the dangling tool_use is dropped, so no tool_use is
    // missing its tool_result.
    let out = scratch_dir("scripted-truncation");
    let summary = run_scripted("truncation-demo.json", "add sparkle", &out);
    assert_eq!(summary.status, "idle");

    let dump = parse_dump(&read_artifact(&out, "debug.json")).expect("dump parses");
    let stops: Vec<&str> = dump.turns.iter().map(|t| t.stop_reason.as_str()).collect();
    assert_eq!(stops, ["max_tokens"]);
    assert_replay_protocol_valid(&dump);
    assert!(
        !dump.messages.iter().flat_map(|m| &m.content).any(|block| {
            matches!(block, ContentBlock::ToolUse { .. })
        }),
        "the truncated tool_use must be dropped from the replayed transcript"
    );
    assert!(dump.edits.is_empty(), "nothing was staged");

    // final.glsl: still the project's starting gradient (no edit landed).
    let final_glsl = read_artifact(&out, "final.glsl");
    assert!(final_glsl.contains("pos.x"), "{final_glsl}");

    // run.md: the warning notice AND the resolved (not dangling) tool row.
    let run_md = read_artifact(&out, "run.md");
    assert!(
        run_md.contains("output-token limit while writing the edit"),
        "{run_md}"
    );
    assert!(
        run_md.contains("- error: cut off by the output-token limit"),
        "no dangling running row — the cut row must resolve: {run_md}"
    );
}

#[test]
fn upsert_on_broken_compile_lands_records_and_verdicts_after_repair() {
    // The live-reported repair order: stage source that declares a uniform
    // but does NOT compile → upsert the param record anyway (textual-scan
    // path) → repair the body → engine verdict ok.
    let out = scratch_dir("scripted-upsert-broken");
    let summary = run_scripted("upsert-broken-demo.json", "add a speed control", &out);
    assert_eq!(summary.status, "idle");

    let dump = parse_dump(&read_artifact(&out, "debug.json")).expect("dump parses");
    assert_replay_protocol_valid(&dump);
    let results = tool_result_contents(&dump);
    assert_eq!(results.len(), 3, "iterate, upsert, iterate");

    // Call 1: the broken stage — probe compile fails, edit still lands.
    assert!(
        results[0]["shader"]["err"]["diagnostics"].is_array(),
        "{}",
        results[0]
    );
    assert_eq!(results[0]["staged"], true);

    // Call 2: the record lands WHILE the source does not compile — the
    // textual declaration scan is what accepted `speed` here.
    assert_eq!(results[1]["applied"], true, "{}", results[1]);
    assert_eq!(results[1]["param"]["name"], "speed");
    assert_eq!(results[1]["param"]["label"], "Speed");

    // Call 3: repaired — probe ok, params clean both ways, engine ok.
    assert_eq!(results[2]["shader"], "ok", "{}", results[2]);
    assert_eq!(results[2]["params"]["orphans"]["declared_only"], serde_json::json!([]));
    assert_eq!(results[2]["params"]["orphans"]["def_only"], serde_json::json!([]));
    assert_eq!(results[2]["engine"]["status"], "ok", "{}", results[2]);

    // The edit history agrees: broken stage then repaired stage, and the
    // ENGINE verdict after repair is the real outcome.
    assert_eq!(dump.edits.len(), 2, "two staged edits");
    assert_eq!(dump.edits[1].engine_ok, Some(true));

    let final_glsl = read_artifact(&out, "final.glsl");
    assert!(final_glsl.contains("fract(time * speed)"), "{final_glsl}");

    let triage = read_artifact(&out, "triage.txt");
    assert!(
        triage.contains("no known bug-class signatures matched"),
        "{triage}"
    );
}

#[test]
fn engine_error_with_probe_ok_shows_the_split_and_triages_as_new_class() {
    // A staged source the probe world accepts but the engine rejects (a
    // declared uniform with no def record fails at render time — the class
    // the probe harness cannot see). The iterate result's engine section
    // must carry the error, run.md must show BOTH verdicts, and triage
    // must flag the unmatched split as "NEW class — investigate".
    let out = scratch_dir("scripted-engine-split");
    let summary = run_scripted("engine-split-demo.json", "make a speed pulse", &out);
    assert_eq!(summary.status, "idle");

    let dump = parse_dump(&read_artifact(&out, "debug.json")).expect("dump parses");
    assert_replay_protocol_valid(&dump);
    let results = tool_result_contents(&dump);
    assert_eq!(results[0]["shader"], "ok", "probe world accepts: {}", results[0]);
    assert_eq!(results[0]["engine"]["status"], "error", "{}", results[0]);
    let message = results[0]["engine"]["message"].as_str().expect("message");
    assert!(message.contains("missing uniform field"), "{message}");
    assert_eq!(dump.edits[0].engine_ok, Some(false));

    // run.md says BOTH: probe ok AND engine error.
    let run_md = read_artifact(&out, "run.md");
    assert!(run_md.contains("- probe compile ok"), "{run_md}");
    assert!(
        run_md.contains("- engine: ERROR (backend rejected the staged source)"),
        "{run_md}"
    );

    // triage.txt: the interesting line.
    let triage = read_artifact(&out, "triage.txt");
    assert!(triage.contains("[NEW]"), "{triage}");
    assert!(triage.contains("missing uniform field"), "{triage}");
    assert!(
        summary.triage.iter().any(|line| line.starts_with("[NEW]")),
        "{:?}",
        summary.triage
    );
}

#[test]
fn scripted_mode_never_consults_the_environment() {
    // The no-network proof for the whole scripted battery: mode decision
    // short-circuits BEFORE any env read (a consulted env panics here), and
    // `ReqwestTransport` is constructed only inside the `RunMode::Real`
    // branch of the session runner — structurally unreachable from
    // `RunMode::Scripted` (see runner::spend_gate).
    let env = |name: &str| -> Option<String> {
        panic!("scripted mode consulted the environment ({name:?})")
    };
    let decision = decide_mode(Some(PathBuf::from("script.json")), &env);
    assert!(matches!(
        decision,
        ModeDecision::Run(RunMode::Scripted(path)) if path.ends_with("script.json")
    ));
}

/// Spend-gate integration shape: with a key but no LPA_SPEND_OK the
/// decision refuses BEFORE any runner/world/transport construction — the
/// refusal is a pure string, and `run` is never entered (the unit tests in
/// `runner::spend_gate` cover the full env matrix).
#[test]
fn gate_refusal_carries_instructions_and_never_reaches_the_runner() {
    let env = |name: &str| (name == "ANTHROPIC_API_KEY").then(|| "sk-test".to_string());
    let ModeDecision::Refuse { message } = decide_mode(None, &env) else {
        panic!("must refuse without LPA_SPEND_OK");
    };
    assert!(message.contains("LPA_SPEND_OK=1"), "{message}");
    assert!(message.contains("Yona's explicit permission"), "{message}");
}
