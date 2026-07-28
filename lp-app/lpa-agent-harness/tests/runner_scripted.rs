//! Scripted-mode integration test: a full runner pass over a scripted
//! session writes all four artifacts with the expected content (the P3
//! battery's foundation). Token-free — no key, no gate, no network.

#![cfg(feature = "runner")]

use std::path::PathBuf;

use lpa_agent_harness::parse_dump;
use lpa_agent_harness::runner::{ModeDecision, RunMode, RunnerArgs, decide_mode, run};
use lpa_studio_core::harness::ShaderFrontend;

const GREEN: &str = "vec4(0.0, 1.0, 0.0, 1.0)";

fn scratch_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn scripted_run_writes_all_four_artifacts() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/green-demo.json");
    let out = scratch_dir("scripted-run");
    let args = RunnerArgs {
        prompt: "make it green".into(),
        frontend: ShaderFrontend::Naga,
        out: Some(out.clone()),
        model: None,
        scripted: Some(script.clone()),
        max_turns: None,
    };
    let summary = run(&args, RunMode::Scripted(script)).expect("scripted run completes");
    assert_eq!(summary.status, "idle");
    assert_eq!(summary.out_dir, out);
    // Usage totals flow from the script's TurnDone events (20+40 / 30+10).
    assert_eq!(summary.usage.input_tokens, 60);
    assert_eq!(summary.usage.output_tokens, 40);

    // debug.json: format-1 dump with the staged edit and both stop reasons.
    let debug_json = std::fs::read_to_string(out.join("debug.json")).expect("debug.json");
    let dump = parse_dump(&debug_json).expect("dump parses");
    let stops: Vec<&str> = dump.turns.iter().map(|t| t.stop_reason.as_str()).collect();
    assert_eq!(stops, ["tool_use", "end_turn"]);
    assert_eq!(dump.edits.len(), 1, "one staged edit");
    assert!(
        dump.edits[0].source.contains(GREEN),
        "{}",
        dump.edits[0].source
    );
    assert_eq!(dump.edits[0].note.as_deref(), Some("go green"));

    // final.glsl: the staged source at run end (the overlay-aware editor
    // content — the green shader, not the project's starting gradient).
    let final_glsl = std::fs::read_to_string(out.join("final.glsl")).expect("final.glsl");
    assert!(final_glsl.contains(GREEN), "{final_glsl}");

    // run.md: the chat log with the user prompt, tool row, and usage.
    let run_md = std::fs::read_to_string(out.join("run.md")).expect("run.md");
    assert!(run_md.contains("## User"), "{run_md}");
    assert!(run_md.contains("make it green"), "{run_md}");
    assert!(run_md.contains("go green"), "{run_md}");
    assert!(run_md.contains("The lights are green now."), "{run_md}");

    // triage.txt: states the exercised compiler path and the (clean) triage.
    let triage = std::fs::read_to_string(out.join("triage.txt")).expect("triage.txt");
    assert!(triage.contains("frontend = naga"), "{triage}");
    assert!(triage.contains("lpvm-wasm::rt_wasmtime"), "{triage}");
    assert!(
        triage.contains("no known bug-class signatures matched"),
        "{triage}"
    );
}

#[test]
fn lps_glsl_mode_states_its_compiler_path() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/green-demo.json");
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
    let triage = std::fs::read_to_string(out.join("triage.txt")).expect("triage.txt");
    assert!(triage.contains("frontend = lps-glsl"), "{triage}");
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
