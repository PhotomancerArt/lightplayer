//! The app agent's `act` against the real controller (plan P08): what can
//! be pressed is whatever the readout offers, checked again at the press.
//! The card legs need an offer only the user may press on a host build (the
//! USB add slot needs a serial transport); they arrive with the roadmap's
//! offers work.

use lpa_agent::{StopReason, TokenUsage, TurnEvent};

use super::app_agent_eval_driver::{AgentEvalStudio, ModelSource, RunLimits};
use super::app_agent_eval_harness::golden_tree;
use super::app_agent_scenario::Scenario;
use super::app_agent_transcript::EvalStep;

#[test]
fn an_id_the_readout_never_offered_is_refused_with_the_current_offers() {
    let scenario = Scenario::load("e2-make-it-300").expect("e2");
    let scripts = vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "act1".into(),
                name: lpa_agent::ACT_TOOL_NAME.into(),
            },
            TurnEvent::ToolInputDelta {
                id: "act1".into(),
                json_fragment: serde_json::json!({ "action": "a9", "why": "testing" }).to_string(),
            },
            turn_done(StopReason::ToolUse),
        ],
        vec![
            TurnEvent::TextDelta("Nothing to press.".into()),
            turn_done(StopReason::EndTurn),
        ],
    ]];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    studio.start(&scenario.start, golden_tree);
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
            .is_some_and(|reason| reason.contains("\"a9\"")),
        "{result:#}"
    );
    assert!(
        refused["offers"]
            .as_str()
            .is_some_and(|offers| offers.contains("actions")),
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
    let call = |id: &str, name: &str, input: serde_json::Value| {
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
    };
    let mut edit = call("e1", lpa_agent::EDIT_PROJECT_TOOL_NAME, script);
    edit.push(turn_done(StopReason::ToolUse));
    // After the edit's tool round the readout lists Save first (a1).
    let save = serde_json::json!({ "action": "a1", "why": "keep the 300" });
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
    studio.start(&scenario.start, golden_tree);
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
