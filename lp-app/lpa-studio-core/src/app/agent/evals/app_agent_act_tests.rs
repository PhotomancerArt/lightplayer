//! The app agent's `act` against the real controller (plan P08, M1 P3):
//! the agent names an offer by its path, and the host looks it up in the
//! offer tree as it is at the press — pressing it, refusing it, or putting
//! it on a card when only the user may press it.

use lpa_agent::{StopReason, TokenUsage, TurnEvent};

use super::app_agent_eval_driver::{AgentEvalStudio, ModelSource};
use super::app_agent_scenario::Scenario;
use super::app_agent_scenario_seat::{RunLimits, ScenarioSeat};
use super::app_agent_transcript::EvalStep;
use crate::app::studio::offer_press_test_api::OfferPressTestApi;

#[test]
fn a_path_the_readout_never_offered_is_refused_with_the_current_offers() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let scripts = vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "act1".into(),
                name: lpa_agent::ACT_TOOL_NAME.into(),
            },
            TurnEvent::ToolInputDelta {
                id: "act1".into(),
                json_fragment: serde_json::json!({ "action": "project/nope", "why": "testing" })
                    .to_string(),
            },
            turn_done(StopReason::ToolUse),
        ],
        vec![
            TurnEvent::TextDelta("Nothing to press.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    studio.send("press it", limits());

    let result = studio
        .transcript_steps()
        .into_iter()
        .find_map(|step| match step {
            EvalStep::ToolResult { content, .. } => {
                serde_json::from_str::<serde_json::Value>(&content).ok()
            }
            _ => None,
        })
        .expect("the act result");
    let refused = &result["refused"];
    assert!(
        refused["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("\"project/nope\"")),
        "{result:#}"
    );
    assert!(
        refused["offers"]
            .as_str()
            .is_some_and(|offers| offers.contains("- project/")),
        "a refusal carries what IS offered: {result:#}"
    );
    assert!(studio.cards().is_empty());
}

/// Edits leave Save on offer; the agent presses it the way the header's
/// button does, and a second press in the same turn — against the same
/// readout — is refused because Save is no longer offered.
#[test]
fn the_agent_presses_save_once_and_a_stale_press_is_refused() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let path = super::app_agent_scenario::fixtures_dir().join("scripts/make-it-300.json");
    let mut script: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("script")).expect("json");
    script["save"] = serde_json::Value::Bool(false);
    let mut edit = call("e1", lpa_agent::EDIT_PROJECT_TOOL_NAME, script);
    edit.push(turn_done(StopReason::ToolUse));
    // After the edit's tool round the readout lists Save.
    let save = serde_json::json!({ "action": "project/save", "why": "keep the 300" });
    let mut press = call("p1", lpa_agent::ACT_TOOL_NAME, save.clone());
    press.extend(call("p2", lpa_agent::ACT_TOOL_NAME, save));
    press.push(turn_done(StopReason::ToolUse));
    let scripts = vec![vec![
        edit,
        press,
        vec![
            TurnEvent::TextDelta("Saved.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    studio.send("make it 300 and save", limits());

    let results: Vec<serde_json::Value> = studio
        .transcript_steps()
        .into_iter()
        .filter_map(|step| match step {
            EvalStep::ToolResult { content, .. } => serde_json::from_str(&content).ok(),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 3, "{results:#?}");
    assert!(
        results[1].get("done").is_some(),
        "Save pressed: {:#}",
        results[1]
    );
    assert!(
        results[2]["refused"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("not offered any more")),
        "{:#}",
        results[2]
    );
    assert!(!studio.unsaved(), "the press saved the project");
    studio.not_offered("project/save");
}

/// Q6, as M7 bounds it: the readout counts a nested node's verbs when the
/// user is not looking at that node, `read` on the node lists them in full,
/// and the agent presses Remove by its path. Remove is undoable (Revert
/// brings the node back until a save), so it is pressed, not carded.
#[test]
fn the_agent_reads_a_nested_nodes_remove_and_presses_it() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let remove = format!("project/{ROOT}/clock.clock/remove");
    let mut read = call(
        "r0",
        lpa_agent::READ_TOOL_NAME,
        serde_json::json!({ "what": "node", "name": "clock" }),
    );
    read.push(turn_done(StopReason::ToolUse));
    let mut press = call(
        "r1",
        lpa_agent::ACT_TOOL_NAME,
        serde_json::json!({ "action": remove, "why": "you asked to drop the clock" }),
    );
    press.push(turn_done(StopReason::ToolUse));
    let scripts = vec![vec![
        read,
        press,
        vec![
            TurnEvent::TextDelta("I removed the clock.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    assert!(
        has_kind(&mut studio, "Clock"),
        "the golden has a clock to remove"
    );
    studio.send("remove the clock", limits());

    let steps = studio.transcript_steps();
    let state = steps
        .iter()
        .find_map(|step| match step {
            EvalStep::State { text } => Some(text.clone()),
            _ => None,
        })
        .expect("the readout the model saw");
    assert!(
        !state.contains(&format!("- {remove}: ")),
        "the clock is not in focus, so its verbs are counted: {state}"
    );
    assert!(state.contains("remove ×"), "{state}");
    let mut results = tool_results(&steps);
    let read = results.remove(0);
    let actions: Vec<&str> = read["actions"]
        .as_array()
        .expect("read lists the node's actions")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert!(
        actions.contains(&format!("- {remove}: Remove node [undoable]").as_str()),
        "{read:#}"
    );
    let result = results.remove(0);
    assert!(result.get("done").is_some(), "{result:#}");
    assert!(studio.cards().is_empty(), "an undoable press is no card");
    assert!(!has_kind(&mut studio, "Clock"), "the clock is gone");
    assert!(
        studio.unsaved(),
        "the removal waits to be saved or reverted"
    );
}

/// D7: Revert to saved loses every unsaved edit for good, so the agent's
/// press becomes a card, and the user's press of the card's button reverts.
#[test]
fn revert_to_saved_becomes_a_card() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let path = super::app_agent_scenario::fixtures_dir().join("scripts/make-it-300.json");
    let mut script: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("script")).expect("json");
    script["save"] = serde_json::Value::Bool(false);
    let mut edit = call("e1", lpa_agent::EDIT_PROJECT_TOOL_NAME, script);
    edit.push(turn_done(StopReason::ToolUse));
    let mut revert = call(
        "v1",
        lpa_agent::ACT_TOOL_NAME,
        serde_json::json!({ "action": "project/revert", "why": "you asked to undo it all" }),
    );
    revert.push(turn_done(StopReason::ToolUse));
    let scripts = vec![
        vec![
            edit,
            revert,
            vec![
                TurnEvent::TextDelta("Click Revert on the card.".into()),
                turn_done(StopReason::EndTurn),
            ],
        ],
        // The run the card's press resumes.
        vec![vec![
            TurnEvent::TextDelta("Reverted.".into()),
            turn_done(StopReason::EndTurn),
        ]],
    ];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    studio.send("make it 300, then undo all of it", limits());

    let results = tool_results(&studio.transcript_steps());
    assert_eq!(results.len(), 2, "{results:#?}");
    let needs = &results[1]["needs_user"];
    assert_eq!(needs["card"], "c1", "{:#}", results[1]);
    assert!(
        studio.unsaved(),
        "nothing is reverted before the user's click"
    );
    let cards = studio.cards();
    assert_eq!(cards.len(), 1, "{cards:#?}");
    let card = &cards[0];
    assert!(card.is_pending());
    assert_eq!(needs["says"], card.title.as_str());
    assert!(card.destructive, "a lasting card wears the error tint");

    studio.press_card(card.press.clone(), limits());

    assert!(!studio.unsaved(), "the card's press reverted the edits");
    assert!(
        matches!(
            studio.cards()[0].state,
            crate::UiAgentCardState::Done { .. }
        ),
        "{:#?}",
        studio.cards()
    );
    // The run the press resumed is its own assistant turn, not glued onto
    // the one that asked for the click.
    assert_eq!(studio.last_assistant_text(), "Reverted.");
    assert_eq!(
        studio.assistant_texts(),
        ["Click Revert on the card.", "Reverted."]
    );
}

/// QF: `edit_project`'s `remove_node` goes through the node's own Remove
/// offer. A node carrying unsaved edits is a Lasting removal (it sweeps
/// them for good), so the edit is refused, nothing changes, and the reason
/// names the offer; `act`ing that path hands the user the button as a
/// card, and the user's click removes it.
#[test]
fn a_lasting_removal_by_edit_project_is_refused_and_points_at_the_card() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let path = super::app_agent_scenario::fixtures_dir().join("scripts/make-it-300.json");
    let mut script: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("script")).expect("json");
    script["save"] = serde_json::Value::Bool(false);
    let mut edit = call("e1", lpa_agent::EDIT_PROJECT_TOOL_NAME, script);
    edit.push(turn_done(StopReason::ToolUse));
    let mut remove = call(
        "e2",
        lpa_agent::EDIT_PROJECT_TOOL_NAME,
        serde_json::json!({ "edits": [{ "remove_node": { "node": "fixture" } }] }),
    );
    remove.push(turn_done(StopReason::ToolUse));
    let scripts = vec![vec![
        edit,
        remove,
        vec![
            TurnEvent::TextDelta("That one is yours to click.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    studio.send("make it 300, then drop the fixture", limits());

    let results = tool_results(&studio.transcript_steps());
    assert_eq!(results.len(), 2, "{results:#?}");
    let status = &results[1]["results"][0];
    assert_eq!(status["ok"], false, "{:#}", results[1]);
    let reason = status["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("the removal is refused: {:#}", results[1]));
    assert!(
        reason.contains("unsaved edits") && reason.contains("`act` project/"),
        "the refusal names the offer to act: {reason}"
    );
    let offer = reason
        .split_whitespace()
        .find(|word| word.starts_with("project/") && word.ends_with("/remove"))
        .unwrap_or_else(|| panic!("an offer path: {reason}"))
        .to_string();
    assert!(has_kind(&mut studio, "Fixture"), "nothing was removed");
    assert!(studio.unsaved(), "the edits it would sweep are still there");
    assert!(studio.cards().is_empty(), "a refused edit makes no card");
    assert!(
        studio.offered(&offer).consequence().arms(),
        "the path it names is the Lasting Remove"
    );
}

/// QF's other half: a removal Revert can undo (the clock has no edits to
/// sweep) is still the agent's to make through `edit_project`.
#[test]
fn an_undoable_removal_by_edit_project_still_removes() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let mut remove = call(
        "e1",
        lpa_agent::EDIT_PROJECT_TOOL_NAME,
        serde_json::json!({ "edits": [{ "remove_node": { "node": "clock" } }] }),
    );
    remove.push(turn_done(StopReason::ToolUse));
    let scripts = vec![vec![
        remove,
        vec![
            TurnEvent::TextDelta("I removed the clock.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    assert!(has_kind(&mut studio, "Clock"));
    studio.send("remove the clock", limits());

    let result = tool_results(&studio.transcript_steps()).remove(0);
    assert_eq!(
        result["results"][0]["ok"], true,
        "the removal applied: {result:#}"
    );
    assert!(!has_kind(&mut studio, "Clock"), "the clock is gone");
    assert!(studio.unsaved(), "until Save or Revert");
    assert!(studio.cards().is_empty());
}

/// M6a: the readout lists the Add node picker's offers, the long kind
/// list named in part, and the agent presses `project/add-node` by path
/// with a `kind`, exactly as a picker row does.
#[test]
fn the_agent_reads_and_presses_the_add_node_offer() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let mut press = call(
        "a1",
        lpa_agent::ACT_TOOL_NAME,
        serde_json::json!({
            "action": "project/add-node",
            "args": { "kind": "clock" },
            "why": "you asked for a second clock",
        }),
    );
    press.push(turn_done(StopReason::ToolUse));
    let scripts = vec![vec![
        press,
        vec![
            TurnEvent::TextDelta("I added a clock.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario);
    let clocks = |studio: &mut AgentEvalStudio| {
        studio
            .node_statuses()
            .iter()
            .filter(|row| row.kind == "Clock")
            .count()
    };
    let before = clocks(&mut studio);
    studio.send("add another clock", limits());

    let steps = studio.transcript_steps();
    let state = steps
        .iter()
        .find_map(|step| match step {
            EvalStep::State { text } => Some(text.clone()),
            _ => None,
        })
        .expect("the readout the model saw");
    for line in [
        "- project/add-node: Add node [choose a kind in args]\n",
        "  takes kind: one of shader (Shader), texture (Texture), ",
        "and 4 more\n",
        "- project/import-pattern: Import pattern [choose a pattern in args]\n",
        "- project/paste-node: Paste node [choose a copied node in args]\n",
        "  takes clipboard: text\n",
    ] {
        assert!(state.contains(line), "{line:?} in:\n{state}");
    }
    // Every node publishes Copy; the readout lists the focused node's in
    // full and counts the rest (M7).
    assert!(
        state.contains(&format!(
            "- project/{ROOT}/fixture.fixture/copy: Copy JSON\n"
        )),
        "{state}"
    );
    assert!(
        state.contains("copy ×6"),
        "the other nodes' Copy, counted: {state}"
    );
    let result = tool_results(&steps).remove(0);
    assert!(result.get("done").is_some(), "{result:#}");
    assert_eq!(clocks(&mut studio), before + 1, "a clock was added");
}

/// The golden's tree root, as its node segment in an offer path.
const ROOT: &str = "studio.show";

fn has_kind(studio: &mut AgentEvalStudio, kind: &str) -> bool {
    studio.node_statuses().iter().any(|row| row.kind == kind)
}

fn tool_results(steps: &[EvalStep]) -> Vec<serde_json::Value> {
    steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::ToolResult { content, .. } => serde_json::from_str(content).ok(),
            _ => None,
        })
        .collect()
}

fn call(id: &str, name: &str, input: serde_json::Value) -> Vec<TurnEvent> {
    vec![
        TurnEvent::ToolUseStart {
            id: id.into(),
            name: name.into(),
        },
        TurnEvent::ToolInputDelta {
            id: id.into(),
            json_fragment: input.to_string(),
        },
    ]
}

fn limits() -> RunLimits {
    RunLimits {
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        usd: 1.0,
        turns: 8,
        tokens: None,
    }
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
