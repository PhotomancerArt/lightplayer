//! The deterministic stage-A legs: every scenario passes on its golden
//! project, and the negative fixtures fail exactly the checks they should
//! — the proof the measuring stick measures.

use serde_json::Value;

use lpa_agent::{StopReason, TokenUsage, TurnEvent};

use super::app_agent_check_spec::CheckSpec;
use super::app_agent_checks::{
    D6_ENDPOINT, XIAO_C6_BOARD_ID, all_nodes_ok, minimal_strip_diff, output_on, playlist_cycles,
    strip_of,
};
use super::app_agent_conversation_checks::asked_about;
use super::app_agent_eval_driver::{AgentEvalStudio, ModelSource, drive_scenario};
use super::app_agent_eval_harness::{
    EvalDriver, EvalStudio, eval_run_dir, golden_tree, run_scenario, write_outcome,
};
use super::app_agent_project_tree::ProjectTree;
use super::app_agent_scenario::{Scenario, Selection};
use super::app_agent_transcript::EvalStep;
use crate::app::home::generate_board_project;

#[test]
fn every_scenario_passes_on_its_golden_project() {
    let run_dir = eval_run_dir("golden");
    let mut judged = 0;
    for scenario in Scenario::all().expect("scenarios load") {
        // Pending scenarios run their golden too, when they have one.
        let Some(golden) = scenario.golden.clone() else {
            continue;
        };
        judged += 1;
        let outcome = run_scenario(&scenario, &EvalDriver::Golden(golden));
        write_outcome(&run_dir, &scenario.name, &outcome).expect("the outcome is written");
        assert!(
            outcome.passed(),
            "{} failed on its golden:\n{}",
            scenario.name,
            outcome.failures()
        );
    }
    assert!(judged >= 10, "only {judged} scenarios have a golden");
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
    playlist_cycles(&golden, 3, Some([10.0, 60.0]), &colourful).expect("the golden cycles");

    let no_cycle = edit_json(&golden, "playlist.json", |playlist| {
        playlist.as_object_mut().expect("object").remove("cycle");
    });
    let reason =
        playlist_cycles(&no_cycle, 3, Some([10.0, 60.0]), &colourful).expect_err("no cycle");
    assert!(reason.contains("does not cycle"), "{reason}");
    assert!(
        reason.contains("it holds 3 entries"),
        "a playlist that does not cycle says what it holds: {reason}"
    );

    let too_fast = edit_json(&golden, "playlist.json", |playlist| {
        playlist["cycle"]["step_seconds"] = 2.0.into();
    });
    let reason =
        playlist_cycles(&too_fast, 3, Some([10.0, 60.0]), &colourful).expect_err("2 s step");
    assert!(reason.contains("outside"), "{reason}");

    // Meteor is a catalog pattern but not on the colourful list.
    let reason = playlist_cycles(&golden, 3, Some([10.0, 60.0]), &["meteor".to_string()])
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

#[test]
fn the_app_chat_runs_a_scenario_end_to_end_on_a_scripted_model() {
    // E3 with a model that asks which board, gets the scripted reply, and
    // ends. No tools yet (plan P03 brings `edit_project`), so the project
    // stays blank — this proves the plumbing, not the build.
    let scenario = Scenario::load("e3-never-guess-the-board").expect("e3");
    let scripts = vec![
        vec![vec![
            TurnEvent::TextDelta("Which board is the ESP32-C6 on?".into()),
            turn_done(StopReason::EndTurn),
        ]],
        vec![vec![
            TurnEvent::TextDelta("Thanks — a XIAO it is.".into()),
            turn_done(StopReason::EndTurn),
        ]],
    ];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    let run = drive_scenario(&mut studio, &scenario);

    let steps = &run.transcript.steps;
    let kinds: Vec<&str> = steps
        .iter()
        .map(|step| match step {
            EvalStep::User { .. } => "user",
            EvalStep::State { .. } => "state",
            EvalStep::Assistant { .. } => "assistant",
            EvalStep::ToolCall { .. } => "tool_call",
            EvalStep::ToolResult { .. } => "tool_result",
            EvalStep::ScriptedReply { .. } => "reply",
            EvalStep::Question { .. } => "question",
            EvalStep::FallbackReply { .. } => "fallback",
            EvalStep::FollowUp { .. } => "follow_up",
            EvalStep::CardHanded { .. } => "card",
            EvalStep::CardClicked { .. } => "click",
            EvalStep::CardLeft { .. } => "leave",
            EvalStep::Stopped { .. } => "stopped",
            EvalStep::Notice { .. } => "notice",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "user",
            "state",
            "assistant",
            "question",
            "reply",
            "user",
            "state",
            "assistant"
        ],
        "{steps:#?}"
    );
    asked_about(&run.transcript, "board").expect("the agent asked");
    assert_eq!(run.turns, 2);

    // What the model saw: one static system prompt; the readout plus the
    // scenario's context line in the state block of each user turn.
    let requests = studio.scripted_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].system, requests[1].system);
    let EvalStep::State { text } = &steps[1] else {
        panic!("state");
    };
    assert!(text.contains("page: project editor"), "{text}");
    assert!(text.contains("is NOT known"), "{text}");
}

#[test]
fn the_sean_script_builds_e1_from_blank_through_the_real_tool() {
    // PD9: the committed edit script, replayed by a scripted model through
    // the real `edit_project` tool and the real Studio op, builds Sean's
    // project from Blank — the vocabulary is enough before any model runs.
    let scenario = Scenario::load("e1-sean-from-empty").expect("e1");
    let outcome = run_scenario(
        &scenario,
        &EvalDriver::Scripted(script_turns("sean-250-d6")),
    );
    let dir = write_outcome(&eval_run_dir("scripted"), &scenario.name, &outcome)
        .expect("the outcome is written");
    assert!(
        outcome.passed(),
        "the golden script fails E1 ({}):\n{}\n\ntool results:\n{}",
        dir.display(),
        outcome.failures(),
        tool_results(&outcome)
    );
}

#[test]
fn edit_results_carry_the_project_the_engine_reports() {
    // PD7 + P04: after the edits, the model reads node statuses and where
    // each port lands. The golden script ends all-ok on /gpio/16; the same
    // script on D99 shows the Output's error and why the pin is wrong.
    let scenario = Scenario::load("e1-sean-from-empty").expect("e1");
    let good = run_scenario(
        &scenario,
        &EvalDriver::Scripted(script_turns("sean-250-d6")),
    );
    let project = &last_tool_result(&good)["project"];
    assert_eq!(
        project["outputs"][0]["ports"][0]["pin"], "/gpio/16",
        "{project:#}"
    );
    assert!(
        project["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .all(|node| node["status"] == "ok"),
        "{project:#}"
    );
    assert_eq!(project["unsaved"], false, "{project:#}");

    let broken = script_turns_with("sean-250-d6", |script| {
        script.replace("ws281x:local:D6", "ws281x:local:D99")
    });
    let bad = run_scenario(&scenario, &EvalDriver::Scripted(broken));
    let project = &last_tool_result(&bad)["project"];
    let output = project["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"] == "Output")
        .expect("an Output row");
    assert_eq!(output["status"], "error", "{project:#}");
    assert!(
        output["message"]
            .as_str()
            .is_some_and(|message| message.contains("is not an output pin on this board")),
        "{project:#}"
    );
    let problem = project["outputs"][0]["ports"][0]["problem"]
        .as_str()
        .expect("a port problem");
    assert!(
        problem.contains("D99 is not an LED output on seeed/xiao-esp32-c6"),
        "{problem}"
    );
}

#[test]
fn the_make_it_300_script_passes_e2_through_the_real_tool() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let outcome = run_scenario(
        &scenario,
        &EvalDriver::Scripted(script_turns("make-it-300")),
    );
    let dir = write_outcome(&eval_run_dir("scripted"), &scenario.name, &outcome)
        .expect("the outcome is written");
    assert!(
        outcome.passed(),
        "the 300 script fails E2 ({}):\n{}\n\ntool results:\n{}",
        dir.display(),
        outcome.failures(),
        tool_results(&outcome)
    );
}

#[test]
fn a_rejected_edit_comes_back_in_band_and_the_batch_goes_on() {
    // A bad path, a wrong type and an unknown node each come back with
    // their index and reason; the edits around them still land, and the
    // session carries on to its next turn.
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let input = serde_json::json!({
        "edits": [
            { "set": { "node": "fixture", "path": "no_such_field", "value": 1 } },
            { "set": { "node": "fixture", "path": "render_size", "value": "wide" } },
            { "set": { "node": "nowhere", "path": "render_size.width", "value": 300 } },
            { "set": { "node": "fixture", "path": "render_size", "value": { "width": 300, "height": 8 } } }
        ]
    })
    .to_string();
    let scripts = vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_bad".into(),
                name: lpa_agent::EDIT_PROJECT_TOOL_NAME.into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_bad".into(),
                json_fragment: input,
            },
            turn_done(StopReason::ToolUse),
        ],
        vec![
            TurnEvent::TextDelta("Fixed what I could.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    let results = tool_results(&outcome);
    let value: serde_json::Value = serde_json::from_str(&results).expect("one result");
    let rows = value["results"].as_array().expect("results");
    assert_eq!(rows.len(), 4, "{results}");
    for (index, needle) in [
        (0, "no_such_field"),
        (
            1,
            "an object {width: a whole number, height: a whole number}",
        ),
        (2, "nowhere"),
    ] {
        assert_eq!(rows[index]["ok"], false, "{results}");
        assert_eq!(rows[index]["index"], index, "{results}");
        let reason = rows[index]["reason"].as_str().expect("a reason");
        assert!(reason.contains(needle), "edit {index}: {reason}");
    }
    assert_eq!(rows[3]["ok"], true, "{results}");
    assert!(
        outcome.transcript.steps.iter().any(
            |step| matches!(step, EvalStep::Assistant { text } if text == "Fixed what I could.")
        ),
        "the session continued after the rejections"
    );
}

#[test]
fn an_agent_that_only_talks_fails_every_project_check() {
    // The P02 baseline in miniature: a model with no tools leaves the Blank
    // project blank, and E1 says exactly why.
    let scenario = Scenario::load("e1-sean-from-empty").expect("e1");
    let scripts = vec![vec![vec![
        TurnEvent::TextDelta("I would love to help, but I cannot edit projects yet.".into()),
        turn_done(StopReason::EndTurn),
    ]]];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    assert!(!outcome.passed());
    let failed: Vec<&str> = outcome
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.name.as_str())
        .collect();
    assert_eq!(
        failed,
        [
            "output_on(D6)",
            "target_is(seeed/xiao-esp32-c6)",
            "strip_of(250)",
            "playlist",
            "graph_wired"
        ],
        // `all_nodes_ok` passes: the blank root module runs and nothing
        // failed — the other checks are what catch an empty project.
        "{}",
        outcome.failures()
    );
    assert_eq!(outcome.turns, 1);
}

#[test]
fn a_dead_output_endpoint_is_the_output_nodes_status() {
    // PD7: a pin the board does not have used to leave the Output reading
    // Running while nothing reached the wire. Now the node says so, and
    // `all_nodes_ok` fails with the port, the spec and why.
    // Text, not a JSON round trip: the def reader wants `kind` first.
    let mut dead = golden_tree("sean-250-d6");
    let output = dead
        .text("output.json")
        .expect("output.json")
        .replace("ws281x:local:D6", "ws281x:local:D99");
    dead.files
        .insert("output.json".to_string(), output.into_bytes());
    let mut studio = EvalStudio::with_project(&dead);
    studio.settle(6);
    let statuses = studio.node_statuses();
    let reason = all_nodes_ok(&statuses).expect_err("D99 is not a XIAO C6 pin");
    assert!(
        reason.contains("port 0: ws281x:local:D99 is not an output pin on this board"),
        "{reason}"
    );
}

#[test]
fn the_readout_of_seans_project_is_small_and_says_what_matters() {
    let mut studio = EvalStudio::with_project(&golden_tree("sean-250-d6"));
    studio.settle(6);
    let readout = studio.readout();
    // Loaded straight onto the server (no library), so no board is chosen
    // and the readout says so instead of resolving the pin.
    for needle in [
        "page: project editor",
        "- output (Output) ok",
        "- fixture (Fixture) ok",
        "port 0 → ws281x:local:D6 — problem: the project has no board",
        "actions",
    ] {
        assert!(readout.contains(needle), "{needle}:\n{readout}");
    }
    // ≤1.5k tokens (≈ 4 chars a token) on a 10-node project.
    eprintln!(
        "readout: {} chars ≈ {} tokens\n{readout}",
        readout.len(),
        readout.len() / 4
    );
    assert!(readout.len() / 4 <= 1_500, "{readout}");
}

/// M7's budget: on a big project the readout lists in full only what the
/// user is looking at and counts the rest, so it stays under its ceiling
/// however many verbs each node card publishes — while every offer stays
/// pressable by path. The project is real (Sean's, with more patterns); the
/// extra verbs per node stand in for the editor offers M6 adds.
#[test]
fn a_big_projects_readout_leads_with_place_and_stays_under_budget() {
    let mut studio = EvalStudio::with_project(&big_project(PATTERN_COPIES));
    studio.settle(6);
    studio.act(crate::StudioCommand::Place(
        crate::UiPlace::new(crate::UiPage::Project {
            uid: "prjbig".to_string(),
            view: crate::UiProjectView::Nodes,
        })
        .with_panel(crate::UiPanel::AppChat),
    ));
    let nodes = studio.node_statuses().len();
    assert!(nodes >= 15, "a big project: {nodes} nodes");
    let mut snapshot = studio.readout_snapshot();
    let real = snapshot.render();
    eprintln!(
        "{nodes} nodes, {} offers: readout {} chars ≈ {} tokens, a flat listing ≈ {} \
         tokens\n{real}",
        snapshot.offers.len(),
        real.len(),
        real.len() / 4,
        flat_len(&snapshot) / 4
    );
    assert!(
        real.starts_with("page: project editor, nodes view\nyou are looking at: node "),
        "it leads with where the user is: {real}"
    );
    assert!(
        real.contains("open over the page: the assistant chat\n"),
        "{real}"
    );

    // M6's editor offers, simulated: ten more verbs on every node card.
    let mut node_owners: Vec<crate::OfferPath> = Vec::new();
    for offer in &snapshot.offers {
        if let Some(owner) = offer.path.owner().filter(crate::OfferPath::names_node)
            && !node_owners.contains(&owner)
        {
            node_owners.push(owner);
        }
    }
    let template = snapshot.offers[0].clone();
    for owner in &node_owners {
        for at in 0..10 {
            let mut offer = template.clone();
            offer.path = owner.clone().child(format!("set-knob-{at}"));
            snapshot.offers.push(offer);
        }
    }
    let readout = snapshot.render();
    let flat = flat_len(&snapshot);
    eprintln!(
        "with ten more verbs a node: {} offers, readout ≈ {} tokens, a flat listing ≈ {} \
         tokens\n{readout}",
        snapshot.offers.len(),
        readout.len() / 4,
        flat / 4
    );
    let focus = snapshot
        .focus
        .node
        .clone()
        .expect("the editor focuses a node");
    for line in readout
        .lines()
        .filter(|line| line.starts_with("- project/"))
    {
        let (path, _) = line[2..].split_once(": ").expect("a listed action");
        let owner = crate::OfferPath::parse(path).unwrap().owner().unwrap();
        assert!(
            !owner.names_node() || owner == focus,
            "only the focused node ({focus}) lists its verbs: {line}"
        );
    }
    assert!(
        readout.contains("set-knob-0 ×"),
        "the other nodes' verbs are counted: {readout}"
    );
    // ≤1.5k tokens (≈ 4 chars a token), the small project's ceiling, where
    // listing every verb would not fit.
    assert!(readout.len() / 4 <= 1_500, "{readout}");
    assert!(flat / 4 > 1_500, "the case is big enough to need the bound");
}

/// Place is read, never obeyed: what the web reports moves the readout's
/// lead and the ⌘K order, and nothing else.
#[test]
fn the_reported_place_moves_the_readouts_lead_and_the_palettes_order() {
    let mut studio = EvalStudio::with_project(&big_project(2));
    studio.settle(6);
    let place = |view| {
        crate::StudioCommand::Place(crate::UiPlace::new(crate::UiPage::Project {
            uid: "prjbig".to_string(),
            view,
        }))
    };

    // Nothing reported (the headless default): the editor's focused node.
    let view = studio.view.clone().expect("a view");
    let focus = view.offers.focus().clone();
    let node = focus.node.clone().expect("the editor focuses a node");
    assert_eq!(focus.areas, [crate::OfferPath::project()]);
    let first = view.offers.search("remove")[0].path.clone();
    assert_eq!(
        first.owner(),
        Some(node.clone()),
        "⌘K: the focused node's Remove leads"
    );
    assert_eq!(
        view.offers.search("")[0].path.owner(),
        Some(node.clone()),
        "⌘K with nothing typed: the focused node's verbs first"
    );

    // Play mode shows a panel, not a node.
    studio.act(place(crate::UiProjectView::Play));
    let readout = studio.readout();
    assert!(
        readout.starts_with("page: project editor, play mode\nyou are looking at: no node"),
        "{readout}"
    );
    // No node is in focus, so no node's verbs are listed in full; the
    // project's own verbs (the root picker's add-node and friends) are the
    // page's actions.
    let here = readout
        .split("actions here")
        .nth(1)
        .and_then(|rest| rest.split("\nproject: ").next())
        .unwrap_or_default();
    assert!(
        !here.contains("/remove"),
        "no node verbs in full: {readout}"
    );
    assert!(here.contains("- project/add-node: "), "{readout}");
    let view = studio.view.clone().expect("a view");
    assert_eq!(view.offers.focus().node, None);

    // Back to the nodes view: the node is in focus again, its verbs listed.
    studio.act(place(crate::UiProjectView::Nodes));
    let readout = studio.readout();
    assert!(
        readout.contains(&format!("- {node}/remove: Remove node [undoable]\n")),
        "{readout}"
    );
    assert!(
        studio.view.clone().expect("a view").offers.focus().node == Some(node),
        "the same node as before"
    );
}

/// How long the readout would be listing every offer in full (its shape
/// before M7).
fn flat_len(snapshot: &crate::app::agent::app_agent_readout::AppReadoutSnapshot) -> usize {
    snapshot.lead.len()
        + snapshot.text.len()
        + snapshot
            .offers
            .iter()
            .map(|offer| crate::app::agent::app_agent_readout::offer_lines(offer).len())
            .sum::<usize>()
}

/// How many extra pattern modules [`big_project`] adds at the root. More
/// than four and the in-process eval server's project sync gives up.
const PATTERN_COPIES: usize = 4;

/// Sean's golden project with `copies` more pattern modules at the root,
/// each a module holding a shader: 2·`copies` more nodes. They all draw to
/// the one picture channel, so the fixture reports the ambiguity — a long
/// status line, which a readout must carry too.
fn big_project(copies: usize) -> ProjectTree {
    let mut tree = golden_tree("sean-250-d6");
    let mut module: Value = tree.json("module.json").expect("module.json");
    let spiral: Vec<(String, Vec<u8>)> = tree
        .files
        .iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix("modules/spiral/")
                .map(|file| (file.to_string(), bytes.clone()))
        })
        .collect();
    for at in 0..copies {
        for (file, bytes) in &spiral {
            tree.files
                .insert(format!("patterns/p{at}/{file}"), bytes.clone());
        }
        module["nodes"][format!("pattern_{at}")] =
            serde_json::json!({ "ref": format!("./patterns/p{at}/module.json") });
    }
    tree.files.insert(
        "module.json".to_string(),
        serde_json::to_vec_pretty(&module).expect("json"),
    );
    tree
}

#[test]
fn read_answers_nodes_patterns_boards_and_names_what_exists_on_a_miss() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let call = |id: &str, what: &str, name: &str| {
        vec![
            TurnEvent::ToolUseStart {
                id: id.into(),
                name: lpa_agent::READ_TOOL_NAME.into(),
            },
            TurnEvent::ToolInputDelta {
                id: id.into(),
                json_fragment: serde_json::json!({ "what": what, "name": name }).to_string(),
            },
        ]
    };
    let mut turn = Vec::new();
    turn.extend(call("r1", "node", "fixture"));
    turn.extend(call("r2", "pattern", "spiral"));
    turn.extend(call("r3", "board", "seeed/xiao-esp32-c6"));
    turn.extend(call("r4", "board", "acme/nope"));
    turn.push(turn_done(StopReason::ToolUse));
    let scripts = vec![vec![
        turn,
        vec![
            TurnEvent::TextDelta("Read.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    let results: Vec<serde_json::Value> = outcome
        .transcript
        .steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::ToolResult { content, .. } => serde_json::from_str(content).ok(),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 4, "{results:#?}");
    assert_eq!(results[0]["kind"], "Fixture", "{:#}", results[0]);
    assert_eq!(
        results[0]["definition"]["render_size"]["width"], 250,
        "{:#}",
        results[0]
    );
    assert!(
        results[1]["description"]
            .as_str()
            .is_some_and(|text| text.contains("rainbow")),
        "{:#}",
        results[1]
    );
    assert!(
        results[2]["led_pins"]
            .as_array()
            .expect("pins")
            .iter()
            .any(|pin| pin["label"] == "D6" && pin["gpio"] == 16),
        "{:#}",
        results[2]
    );
    assert!(
        results[3]["error"]
            .as_str()
            .is_some_and(|text| text.contains("seeed/xiao-esp32-c6")),
        "a miss lists the boards: {:#}",
        results[3]
    );
}

/// The least room under the run's cap a scenario starts with.
const MIN_SCENARIO_USD: f64 = 0.05;

/// The live leg (`just app-agent-eval`, `just app-agent-corpus`): every
/// selected scenario, against a real OpenRouter model, written under
/// `target/app-agent-evals/<run>/`. A measurement, not a gate: failures
/// are reported, not asserted.
///
/// Selection comes from the environment (`Selection::from_env`). The cap
/// (`LPA_APP_EVAL_MAX_USD`, default 2) is held by cutting each scenario's
/// own budget to the room left under it, and starting none with less than
/// [`MIN_SCENARIO_USD`] left; `LPA_APP_EVAL_DRY=1` lists what would run,
/// and why the rest would not, without a model or a key.
#[test]
#[ignore = "live leg: `just app-agent-corpus` / `just app-agent-eval <scenario> --model <slug>`"]
fn app_agent_eval_live() {
    let selection = Selection::from_env();
    let dry = std::env::var("LPA_APP_EVAL_DRY").is_ok_and(|value| value == "1");
    let model = std::env::var("LPA_EVAL_MODEL").unwrap_or_default();
    assert!(dry || !model.is_empty(), "LPA_EVAL_MODEL=<openrouter slug>");
    let run = std::env::var("LPA_APP_EVAL_RUN").unwrap_or_else(|_| "live".to_string());
    let repeat: u32 = std::env::var("LPA_APP_EVAL_REPEAT")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let max_usd: f64 = std::env::var("LPA_APP_EVAL_MAX_USD")
        .ok()
        .and_then(|usd| usd.parse().ok())
        .unwrap_or(2.0);
    let all = Scenario::all().expect("scenarios load");
    let mut picked = Vec::new();
    for scenario in &all {
        match selection.picks(scenario) {
            Ok(()) => picked.push(scenario.clone()),
            Err(why) => eprintln!(
                "app-agent-eval: skip {} {} — {why}",
                scenario.id, scenario.name
            ),
        }
    }
    assert!(
        !picked.is_empty(),
        "no scenario matches the selection {selection:?}"
    );
    let ceiling: f64 = picked.iter().map(|s| s.budget.usd).sum::<f64>() * f64::from(repeat);
    eprintln!(
        "app-agent-eval: {} scenario(s) × {repeat}, per-scenario budgets sum to ${ceiling:.2} \
         (the worst case); run cap ${max_usd:.2}",
        picked.len()
    );
    if dry {
        for scenario in &picked {
            eprintln!(
                "  {:>3} {:<32} {:?} seat, {} check(s), budget {} turns / ${}{}{}",
                scenario.id,
                scenario.name,
                scenario.seat(),
                scenario.checks.len(),
                scenario.budget.turns,
                scenario.budget.usd,
                match scenario.stage_b() {
                    Some(stage_b) => format!(", stage B pad {} × {}", stage_b.pad, stage_b.leds),
                    None => String::new(),
                },
                match scenario.waits_for.is_empty() {
                    true => String::new(),
                    false => format!(" (pending on {})", scenario.waits_for),
                }
            );
        }
        return;
    }
    let run_dir = eval_run_dir(&run);
    let mut lines = Vec::new();
    let mut spent = 0.0;
    'scenarios: for scenario in &picked {
        for n in 1..=repeat {
            let dir_name = if repeat == 1 {
                scenario.name.clone()
            } else {
                format!("{}-r{n}", scenario.name)
            };
            // The cap holds whatever the model does: a scenario starts only
            // with room left, and its own budget is cut to that room, so a
            // runaway stops at the cap (Stop lands between events, so at
            // most one model turn past it).
            let room = max_usd - spent;
            if room < MIN_SCENARIO_USD {
                let line = format!(
                    "{dir_name}: SKIPPED — ${spent:.4} spent of the ${max_usd:.2} cap \
                     (and every scenario after it)"
                );
                eprintln!("app-agent-eval [{model}] {line}");
                lines.push(line);
                break 'scenarios;
            }
            let mut capped = scenario.clone();
            capped.budget.usd = capped.budget.usd.min(room);
            let outcome = run_scenario(
                &capped,
                &EvalDriver::Live {
                    model: model.clone(),
                },
            );
            spent += outcome
                .usage
                .reported_cost_usd()
                .unwrap_or(scenario.budget.usd);
            let dir = write_outcome(&run_dir, &dir_name, &outcome).expect("written");
            let line = format!(
                "{dir_name}: {} — {} turns, {} in / {} out tokens, {}{}",
                if outcome.passed() { "PASS" } else { "FAIL" },
                outcome.turns,
                outcome.usage.input_tokens
                    + outcome.usage.cache_read_tokens
                    + outcome.usage.cache_write_tokens,
                outcome.usage.output_tokens,
                outcome
                    .usage
                    .reported_cost_usd()
                    .map(|usd| format!("${usd:.4}"))
                    .unwrap_or_else(|| "cost unreported".to_string()),
                if outcome.passed() {
                    String::new()
                } else {
                    format!("\n    {}", outcome.failures().replace('\n', "\n    "))
                }
            );
            eprintln!("app-agent-eval [{model}] {line}  ({})", dir.display());
            lines.push(line);
        }
    }
    eprintln!(
        "\napp-agent-eval {run} — {model} — ${spent:.4} reported\n{}",
        lines.join("\n")
    );
}

// --- helpers ---------------------------------------------------------------

/// One scenario send whose model calls `edit_project` with the committed
/// script `scripts/<name>.json`, then says it is done.
fn script_turns(name: &str) -> Vec<Vec<Vec<TurnEvent>>> {
    script_turns_with(name, |script| script)
}

/// [`script_turns`], with the script's text changed first.
fn script_turns_with(
    name: &str,
    change: impl FnOnce(String) -> String,
) -> Vec<Vec<Vec<TurnEvent>>> {
    let path = super::app_agent_scenario::fixtures_dir()
        .join("scripts")
        .join(format!("{name}.json"));
    let input = change(
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    );
    vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_script".into(),
                name: lpa_agent::EDIT_PROJECT_TOOL_NAME.into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_script".into(),
                json_fragment: input,
            },
            turn_done(StopReason::ToolUse),
        ],
        vec![
            TurnEvent::TextDelta("Done.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]]
}

/// The run's last tool result, parsed.
fn last_tool_result(outcome: &super::app_agent_eval_harness::EvalOutcome) -> serde_json::Value {
    outcome
        .transcript
        .steps
        .iter()
        .rev()
        .find_map(|step| match step {
            EvalStep::ToolResult { content, .. } => serde_json::from_str(content).ok(),
            _ => None,
        })
        .expect("a tool result")
}

/// Every tool result the run produced, for failure messages.
fn tool_results(outcome: &super::app_agent_eval_harness::EvalOutcome) -> String {
    outcome
        .transcript
        .steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::ToolResult { content, .. } => {
                serde_json::from_str::<serde_json::Value>(content)
                    .map(|value| serde_json::to_string_pretty(&value).unwrap_or_default())
                    .ok()
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn turn_done(stop_reason: StopReason) -> TurnEvent {
    TurnEvent::TurnDone {
        stop_reason,
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..TokenUsage::default()
        },
    }
}

fn colourful() -> Vec<String> {
    Scenario::load("e1-sean-from-empty")
        .expect("e1")
        .checks
        .into_iter()
        .find_map(|check| match check {
            CheckSpec::Playlist { from, .. } => Some(from),
            _ => None,
        })
        .expect("S1 has a playlist check")
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
