//! The `act` tool: press one action the readout offers (plan P08, D6).
//!
//! The agent names an action from the readout's `actions:` list; the host
//! checks it is still offered and enabled, then either presses it — the
//! same dispatch the user's own button makes — or, for an action only the
//! user may press (a confirmation, a browser picker, a flash), puts a card
//! in the chat whose click presses it. The model can never press a card.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::provider::model_provider::ToolDef;
use crate::tool::app::app_agent_host::AppAgentHost;
use crate::tool::app::app_tool_schema::app_tool_schema;
use crate::toolset::ToolOutcome;

pub const ACT_TOOL_NAME: &str = "act";

/// One press.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActInput {
    /// The action's id exactly as the readout lists it.
    pub action: String,
    /// One short line for the user: why this, now. Shown on the card when
    /// the user has to click it.
    pub why: String,
}

/// What pressing came to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActOutcome {
    /// Pressed. `notices` is what the app said back.
    Done { notices: Vec<String> },
    /// Only the user may press it: a card is in the chat. Stop, and tell
    /// the user which card to click.
    NeedsUser { card: String, says: String },
    /// Not pressed: no longer offered, disabled, or a card is still
    /// waiting. `offers` is the current action list when it helps.
    Refused {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        offers: Option<String>,
    },
}

pub fn act_tool_def() -> ToolDef {
    ToolDef {
        name: ACT_TOOL_NAME.into(),
        description: DESCRIPTION.into(),
        input_schema: app_tool_schema::<ActInput>(),
    }
}

const DESCRIPTION: &str = "\
Press one action from the readout's `actions:` list, by its id — the same \
button the user would press. An action marked [needs the user's click] is \
not pressed: a card appears in the chat and the user's click on it does it; \
when the result says `needs_user`, stop and tell the user which card to \
click. Content edits go through `edit_project`, not here.";

/// Run one `act` call against `host`.
pub async fn run_act(input_json: &Value, host: &mut dyn AppAgentHost) -> ToolOutcome {
    let input: ActInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(error) => {
            return ToolOutcome {
                content: json!({
                    "error": format!("invalid act input: {error}"),
                    "hint": "pass {\"action\": \"<id from the readout>\", \"why\": \"…\"}",
                })
                .to_string(),
                is_error: false,
                summary: json!({ "input_error": true }),
            };
        }
    };
    match host.act(&input).await {
        Ok(outcome) => {
            let summary = match &outcome {
                ActOutcome::Done { .. } => json!({ "action": input.action, "done": true }),
                ActOutcome::NeedsUser { card, .. } => {
                    json!({ "action": input.action, "card": card })
                }
                ActOutcome::Refused { reason, .. } => {
                    json!({ "action": input.action, "refused": reason })
                }
            };
            ToolOutcome {
                content: serde_json::to_string(&outcome).expect("an outcome serializes"),
                is_error: false,
                summary,
            }
        }
        Err(error) => ToolOutcome {
            content: json!({ "error": error.message }).to_string(),
            is_error: false,
            summary: json!({ "action": input.action, "error": true }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_asks_for_an_id_and_a_reason_and_nothing_else() {
        let schema = act_tool_def().input_schema;
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&json!("action")));
        assert!(required.contains(&json!("why")));
        assert!(
            serde_json::from_value::<ActInput>(
                json!({ "action": "a1", "why": "x", "force": true })
            )
            .is_err()
        );
    }

    #[test]
    fn outcomes_read_as_plain_json() {
        let needs = ActOutcome::NeedsUser {
            card: "c1".into(),
            says: "Connect your board".into(),
        };
        assert_eq!(
            serde_json::to_value(&needs).unwrap(),
            json!({ "needs_user": { "card": "c1", "says": "Connect your board" } })
        );
        let refused = ActOutcome::Refused {
            reason: "not offered right now".into(),
            offers: None,
        };
        assert_eq!(
            serde_json::to_value(&refused).unwrap(),
            json!({ "refused": { "reason": "not offered right now" } })
        );
    }
}
