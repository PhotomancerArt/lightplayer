//! The deterministic stage-A legs: every scenario passes on its golden
//! project, and the negative fixtures fail exactly the checks they should
//! — the proof the measuring stick measures.

use serde_json::Value;

use super::app_agent_checks::{
    D6_ENDPOINT, XIAO_C6_BOARD_ID, minimal_strip_diff, output_on, playlist_cycles, strip_of,
};
use super::app_agent_eval_harness::{
    EvalDriver, EvalStudio, eval_run_dir, golden_tree, run_scenario, write_outcome,
};
use super::app_agent_project_tree::ProjectTree;
use super::app_agent_scenario::Scenario;
use crate::app::home::generate_board_project;

#[test]
fn every_scenario_passes_on_its_golden_project() {
    let run_dir = eval_run_dir("golden");
    for scenario in Scenario::all().expect("scenarios load") {
        let golden = scenario
            .golden
            .clone()
            .unwrap_or_else(|| panic!("{} names no golden", scenario.name));
        let outcome = run_scenario(&scenario, &EvalDriver::Golden(golden));
        write_outcome(&run_dir, &outcome).expect("the outcome is written");
        assert!(
            outcome.passed(),
            "{} failed on its golden:\n{}",
            scenario.name,
            outcome.failures()
        );
    }
}

#[test]
fn the_generated_board_project_fails_the_d6_and_strip_checks() {
    // `generate_board_project` for a XIAO C6: 256 LEDs on the board's
    // default wire, D10 — right board, wrong pin, wrong count.
    let generated = generate_board_project(XIAO_C6_BOARD_ID, None).expect("XIAO C6 generates");
    let tree = ProjectTree::from_files(generated.files);
    let reason = output_on(&tree, D6_ENDPOINT).expect_err("D10 is not D6");
    assert!(reason.contains("ws281x:local:D10"), "{reason}");
    let reason = strip_of(&tree, 250).expect_err("256 is not 250");
    assert!(reason.contains("256"), "{reason}");
    // …while the pieces it does get right still pass, so the failures
    // above are about the pin and the count, not a broken reader.
    strip_of(&tree, 256).expect("the generated strip is 256 lamps in order");
}

#[test]
fn a_playlist_that_does_not_cycle_fails_playlist_cycles() {
    let golden = golden_tree("sean-250-d6");
    let colourful = colourful();
    playlist_cycles(&golden, 3, [10.0, 60.0], &colourful).expect("the golden cycles");

    let no_cycle = edit_json(&golden, "playlist.json", |playlist| {
        playlist.as_object_mut().expect("object").remove("cycle");
    });
    let reason = playlist_cycles(&no_cycle, 3, [10.0, 60.0], &colourful).expect_err("no cycle");
    assert!(reason.contains("does not cycle"), "{reason}");

    let too_fast = edit_json(&golden, "playlist.json", |playlist| {
        playlist["cycle"]["step_seconds"] = 2.0.into();
    });
    let reason = playlist_cycles(&too_fast, 3, [10.0, 60.0], &colourful).expect_err("2 s step");
    assert!(reason.contains("outside"), "{reason}");

    // Meteor is a catalog pattern but not on the colourful list.
    let reason = playlist_cycles(&golden, 3, [10.0, 60.0], &["meteor".to_string()])
        .expect_err("nothing colourful");
    assert!(reason.contains("not on the colourful list"), "{reason}");
}

#[test]
fn a_300_led_change_that_also_touches_the_playlist_fails_minimal_diff() {
    let start = golden_tree("sean-250-d6");
    let end = golden_tree("sean-300-d6");
    minimal_strip_diff(&start, &end).expect("the 300 golden only resized the strip");

    let sloppy = edit_json(&end, "playlist.json", |playlist| {
        playlist["cycle"]["step_seconds"] = 45.0.into();
    });
    let reason = minimal_strip_diff(&start, &sloppy).expect_err("the cycle changed too");
    assert!(reason.contains("playlist.json"), "{reason}");
    minimal_strip_diff(&start, &start).expect_err("nothing changed");
}

#[test]
fn the_in_process_server_wears_the_xiao_c6_pin_map() {
    // D6 opens: every node of the golden runs.
    let mut studio = EvalStudio::with_project(&golden_tree("sean-250-d6"));
    studio.settle(6);
    let statuses = studio.node_statuses();
    let output = statuses
        .iter()
        .find(|row| row.kind == "Output")
        .expect("an Output row");
    assert!(output.ok, "the D6 output runs: {statuses:#?}");
    assert!(!studio.unsaved());
}

/// The live leg (`just app-agent-eval`). Until the app session lands
/// (plan P02) there is no model driver, so this says so instead of
/// pretending.
#[test]
#[ignore = "live leg: `just app-agent-eval <scenario> --model <slug>`"]
fn app_agent_eval_live() {
    let which = std::env::var("LPA_APP_EVAL_SCENARIO").unwrap_or_else(|_| "all".to_string());
    let scenarios = Scenario::select(&which).expect("scenario");
    let plan: Vec<String> = scenarios
        .iter()
        .map(|s| {
            format!(
                "{} — user {:?}, context {:?}, {} scripted repl(ies) ({}), budget {} turns / ${}",
                s.name,
                s.user,
                s.context,
                s.replies.len(),
                s.replies
                    .iter()
                    .map(|r| format!("{}: {:?}", r.when_asked_about, r.text))
                    .collect::<Vec<_>>()
                    .join(", "),
                s.budget.turns,
                s.budget.usd
            )
        })
        .collect();
    panic!(
        "the app agent's model driver is not built yet (plan P02); would run:\n{}",
        plan.join("\n")
    );
}

// --- helpers ---------------------------------------------------------------

fn colourful() -> Vec<String> {
    Scenario::load("e1-sean-from-empty")
        .expect("e1")
        .playlist
        .colourful
}

/// `tree` with one JSON file edited.
fn edit_json(tree: &ProjectTree, path: &str, edit: impl FnOnce(&mut Value)) -> ProjectTree {
    let mut value = tree.json(path).unwrap_or_else(|| panic!("{path} is JSON"));
    edit(&mut value);
    let mut out = tree.clone();
    out.files.insert(
        path.to_string(),
        serde_json::to_vec_pretty(&value).expect("serializes"),
    );
    out
}
