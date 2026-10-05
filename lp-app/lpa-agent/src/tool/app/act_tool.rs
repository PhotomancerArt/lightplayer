//! The `act` tool: press one action the readout offers (plan P08, D6).
//!
//! The agent names an action from the readout's `actions:` list by its
//! offer path (`project/save`), with the values it takes when the readout
//! lists any (`args`: flash *which board*); the host looks the path up in
//! the offer tree as it is at the press, binds the values the way the
//! user's own picker does, then either presses it — the same dispatch the
//! user's own button makes — or, for an action only the user may press (it
//! loses something for good, or the browser needs a real click), puts a
//! card in the chat whose click presses it, its controls pre-filled with
//! the agent's values. The model can never press a card.

use std::collections::BTreeMap;

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
    /// The action's path exactly as the readout lists it, e.g.
    /// `project/save` or `project/demo.module/orbit.shader/remove`.
    pub action: String,
    /// The values the action takes, by the names the readout lists under
    /// it (`takes board: one of …`), e.g. `{"board":
    /// "seeed/xiao-esp32-c6"}`. Leave out what has a default; omit it
    /// entirely for an action that takes nothing.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, ActArgValue>,
    /// One short line for the user: why this, now. Shown on the card when
    /// the user has to click it.
    pub why: String,
}

/// One value an action takes: a choice's value or text as a string, a
/// switch as `true`/`false`; a number is read as its text. Held as the text
/// a press carries.
///
/// Read by hand from the JSON value rather than as an untagged enum: serde's
/// untagged machinery is fenced out of the tree
/// (`scripts/check-serde-content.sh`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActArgValue(String);

impl ActArgValue {
    /// The value as the press carries it.
    pub fn as_text(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ActArgValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(Self(text)),
            Value::Bool(on) => Ok(Self(on.to_string())),
            Value::Number(number) => Ok(Self(number.to_string())),
            other => Err(serde::de::Error::custom(format!(
                "an arg is one value — a string, true/false or a number — not {other}"
            ))),
        }
    }
}

impl Serialize for ActArgValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl JsonSchema for ActArgValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ActArgValue".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A choice's value or text as a string; a switch as true or false.",
            "type": ["string", "boolean", "number"],
        })
    }
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
    /// Not pressed: no action at that path (or no longer), disabled, or a
    /// card is still waiting. `offers` is the current action list when it
    /// helps.
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
Press one action from the readout's `actions:` list, by its path exactly as \
listed (`project/save`) — the same button the user would press. When the \
readout lists what an action `takes`, pass those values in `args` by name \
(`{\"board\": \"seeed/xiao-esp32-c6\"}`); one left out takes its default, \
and a value that is not allowed is refused with the choices. An action \
marked [undoable] takes something away that Revert brings back: press it \
only when it is what the user asked for, and say what it removed. An action \
marked [needs the user's click] is not pressed: a card appears in the chat \
and the user's click on it does it; when the result says `needs_user`, stop \
and tell the user which card to click. Content edits go through \
`edit_project`, not here.";

/// Run one `act` call against `host`.
pub async fn run_act(input_json: &Value, host: &mut dyn AppAgentHost) -> ToolOutcome {
    let input: ActInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(error) => {
            return ToolOutcome {
                content: json!({
                    "error": format!("invalid act input: {error}"),
                    "hint": "pass {\"action\": \"<path from the readout, e.g. project/save>\", \"args\": {\"<name>\": \"<value>\"} (only when it takes values), \"why\": \"…\"}",
                })
                .to_string(),
                is_error: false,
                summary: json!({ "input_error": true }),
            };
        }
    };
    match host.act(&input).await {
        Ok(outcome) => {
            let mut summary = match &outcome {
                ActOutcome::Done { .. } => json!({ "action": input.action, "done": true }),
                ActOutcome::NeedsUser { card, .. } => {
                    json!({ "action": input.action, "card": card })
                }
                ActOutcome::Refused { reason, .. } => {
                    json!({ "action": input.action, "refused": reason })
                }
            };
            if !input.args.is_empty() {
                summary["args"] = json!(input.args);
            }
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
    fn the_schema_asks_for_a_path_and_a_reason_and_nothing_else() {
        let schema = act_tool_def().input_schema;
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&json!("action")));
        assert!(required.contains(&json!("why")));
        assert!(
            !required.contains(&json!("args")),
            "args are optional: most actions take none"
        );
        assert!(schema["properties"]["args"].is_object(), "{schema:#}");
        assert!(
            serde_json::from_value::<ActInput>(
                json!({ "action": "project/save", "why": "x", "force": true })
            )
            .is_err()
        );
    }

    #[test]
    fn args_carry_text_switches_and_numbers_as_text() {
        let input: ActInput = serde_json::from_value(json!({
            "action": "devices/mac-a0f26287b48c/flash",
            "args": { "board": "seeed/xiao-esp32-c6", "all_boards": true, "count": 3 },
            "why": "x",
        }))
        .expect("scalars are values");
        let values: Vec<(&str, String)> = input
            .args
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_text().to_string()))
            .collect();
        assert_eq!(
            values,
            [
                ("all_boards", "true".to_string()),
                ("board", "seeed/xiao-esp32-c6".to_string()),
                ("count", "3".to_string()),
            ]
        );
        let bare: ActInput =
            serde_json::from_value(json!({ "action": "project/save", "why": "x" })).unwrap();
        assert!(bare.args.is_empty(), "no args is the common case");
        assert!(
            serde_json::from_value::<ActInput>(
                json!({ "action": "a", "why": "x", "args": { "board": ["a", "b"] } })
            )
            .is_err(),
            "a list is not one value"
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
