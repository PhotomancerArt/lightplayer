//! What the agent did, shown where it lives (agentic-UI roadmap M8): the
//! controls its presses and edits touch are lit for a moment by offer path,
//! the chat rows say where, and Show — a core offer at `show/<target>` —
//! brings a control back into view. Against the real controller, with the
//! injected clock deciding when a light goes out.

use lpa_agent::{StopReason, TokenUsage, TurnEvent};

use super::app_agent_eval_driver::{AgentEvalStudio, ModelSource, RunLimits};
use super::app_agent_eval_harness::golden_tree;
use super::app_agent_scenario::Scenario;
use crate::app::studio::offer_press_test_api::OfferPressTestApi;
use crate::{
    AGENT_ACTIVITY_LIT_SECS, AgentActivityKind, OfferArgs, OfferPath, UiAgentTurn, UiStudioView,
};

#[test]
fn an_edit_and_a_press_light_their_controls_until_the_lights_expire() {
    let mut studio = edited_then_saved();
    let view = studio.view();
    let lit = &view.app_agent.activity.lit;
    let save = OfferPath::project().child("save");
    let fixture = fixture_prefix(&view);
    assert!(
        lit.iter()
            .any(|lit| lit.path == save && lit.kind == AgentActivityKind::Pressed),
        "the agent's Save is lit: {lit:#?}"
    );
    assert!(
        lit.iter()
            .any(|lit| lit.path == fixture && lit.kind == AgentActivityKind::Edited),
        "the fixture card it edited is lit: {lit:#?}"
    );
    assert_eq!(
        lit.iter().filter(|lit| lit.path == fixture).count(),
        1,
        "two edits on one node light its card once"
    );

    // Nothing moves before the lights are due out …
    assert!(
        studio
            .advance_clock(AGENT_ACTIVITY_LIT_SECS / 2.0)
            .is_none(),
        "a quiet batch while the lights are on publishes nothing"
    );
    // … and the batch after they are publishes the view without them.
    let dark = studio
        .advance_clock(AGENT_ACTIVITY_LIT_SECS)
        .expect("the lights going out republishes the view");
    assert!(dark.app_agent.activity.lit.is_empty(), "{dark:#?}");
}

#[test]
fn rows_say_where_and_show_brings_the_node_back_lit() {
    let mut studio = edited_then_saved();
    let view = studio.view();
    let fixture = fixture_prefix(&view);
    let (act_line, edit_places) = rows(&view);
    assert_eq!(act_line, "pressed Save in the project header");
    let show = OfferPath::show_of(&fixture);
    assert!(
        edit_places
            .iter()
            .all(|place| place.show.as_ref() == Some(&show)),
        "each fixture edit links to the fixture's Show: {edit_places:#?}"
    );
    assert!(
        edit_places
            .iter()
            .all(|place| place.place == "on the fixture card"),
        "{edit_places:#?}"
    );
    // Save went away when it saved: its row names it, with no Show.
    studio.not_offered(OfferPath::show_of(&OfferPath::project().child("save")));

    studio.advance_clock(AGENT_ACTIVITY_LIT_SECS + 1.0);
    studio.press(&show, OfferArgs::new());
    let shown = studio.view();
    let reveal = shown
        .app_agent
        .activity
        .reveal
        .clone()
        .expect("Show asks the page to reveal");
    assert_eq!(reveal.path, fixture);
    assert!(
        shown.app_agent.activity.lit_at(&fixture).is_some(),
        "Show lights it again"
    );
    assert_eq!(
        shown.offers.focus().node.as_ref(),
        Some(&fixture),
        "Show focuses the node's card, as a tree-row click does"
    );
}

#[test]
fn show_is_the_users_link_and_never_in_the_agents_readout() {
    let mut studio = edited_then_saved();
    let fixture = fixture_prefix(&studio.view());
    studio.offered(OfferPath::show_of(&fixture));
    let readout = studio.readout();
    // (`studio.show` is the golden's root node, so look for the path.)
    assert!(
        !readout.contains(&OfferPath::show_of(&fixture).to_string())
            && !readout.contains("Show fixture"),
        "the agent never sees Show: {readout}"
    );
}

/// The `make-it-300` edits without their save, then the agent's own press
/// of Save.
fn edited_then_saved() -> AgentEvalStudio {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let path = super::app_agent_scenario::fixtures_dir().join("scripts/make-it-300.json");
    let mut script: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("script")).expect("json");
    script["save"] = serde_json::Value::Bool(false);
    let mut edit = call("e1", lpa_agent::EDIT_PROJECT_TOOL_NAME, script);
    edit.push(turn_done(StopReason::ToolUse));
    let mut press = call(
        "p1",
        lpa_agent::ACT_TOOL_NAME,
        serde_json::json!({ "action": "project/save", "why": "keep the 300" }),
    );
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
    studio.start(&scenario.start, golden_tree);
    studio.send("make it 300 and save", limits());
    assert!(!studio.unsaved(), "the agent's press saved");
    studio
}

/// The fixture card's prefix, as the edit row's first line names it.
fn fixture_prefix(view: &UiStudioView) -> OfferPath {
    view.app_agent
        .activity
        .lit
        .iter()
        .find(|lit| lit.kind == AgentActivityKind::Edited)
        .map(|lit| lit.path.clone())
        .or_else(|| {
            view.app_agent
                .turns
                .iter()
                .find_map(|turn| match turn {
                    UiAgentTurn::Tool(row) => row.edits.as_ref()?.lines[0].node.clone(),
                    _ => None,
                })
                .and_then(|node| crate::ProjectNodeAddress::parse(&node).ok())
                .map(|address| OfferPath::project_node(&address))
        })
        .expect("the edit landed on the fixture")
}

/// The act row's line, and every edit line's place.
fn rows(view: &UiStudioView) -> (String, Vec<crate::UiAgentPlace>) {
    let mut act = String::new();
    let mut places = Vec::new();
    for turn in &view.app_agent.turns {
        let UiAgentTurn::Tool(row) = turn else {
            continue;
        };
        if row.tool == "act" {
            act = row.summary_line();
        }
        if let Some(edits) = &row.edits {
            places.extend(edits.lines.iter().filter_map(|line| line.place.clone()));
        }
    }
    assert!(!places.is_empty(), "the edit lines are placed");
    (act, places)
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
