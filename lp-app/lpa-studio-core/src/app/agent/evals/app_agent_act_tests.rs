//! The app agent's `act` against the real controller (plan P08). What can
//! be pressed is whatever the readout offers; the card flow's end-to-end
//! legs wait on core publishing the view's offers (the "offers" design,
//! planning dir `design-offers-as-a-core-concept.md`).

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
