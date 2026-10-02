//! The `read` tool: the app agent's drill-down (plan P06). The readout says
//! what is there; `read` says everything about one thing — a node's whole
//! definition, a catalog pattern, a board's pins, a device. Read-only.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::provider::model_provider::ToolDef;
use crate::tool::app::app_agent_host::AppAgentHost;
use crate::tool::app::app_tool_schema::app_tool_schema;
use crate::toolset::ToolOutcome;

pub const READ_TOOL_NAME: &str = "read";

/// What to read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadInput {
    pub what: ReadWhat,
    /// The node's name or path (`fixture`, `playlist/palette_waves`), the
    /// pattern's slug (`palette-waves`), the board id
    /// (`seeed/xiao-esp32-c6`), or the device's name as the readout shows it.
    pub name: String,
}

/// The kinds of thing `read` knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadWhat {
    /// A node of the open project: its whole definition (the values `set`
    /// writes, by the same paths), status and issues.
    Node,
    /// A catalog pattern: what it looks like, its knobs, 1D or 2D.
    Pattern,
    /// A board: its LED-capable pin labels and the GPIO each one is.
    Board,
    /// A device on the roster: its card.
    Device,
}

pub fn read_tool_def() -> ToolDef {
    ToolDef {
        name: READ_TOOL_NAME.into(),
        description: DESCRIPTION.into(),
        input_schema: app_tool_schema::<ReadInput>(),
    }
}

const DESCRIPTION: &str = "\
Read one thing in full: a node's whole definition (its JSON, status and \
issues — the paths you `set`), a catalog pattern (description, knobs, 1D or \
2D), a board (its LED pin labels and GPIOs), or a device on the roster. \
Read-only; changes nothing.";

/// Run one `read` call against `host`.
pub async fn run_read(input_json: &Value, host: &mut dyn AppAgentHost) -> ToolOutcome {
    let input: ReadInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(error) => {
            return ToolOutcome {
                content: json!({
                    "error": format!("invalid read input: {error}"),
                    "what": ["node", "pattern", "board", "device"],
                })
                .to_string(),
                is_error: false,
                summary: json!({ "input_error": true }),
            };
        }
    };
    match host.read(&input).await {
        Ok(value) => ToolOutcome {
            content: value.to_string(),
            is_error: false,
            summary: json!({ "read": input.what, "name": input.name }),
        },
        Err(error) => ToolOutcome {
            content: json!({ "error": error.message }).to_string(),
            is_error: false,
            summary: json!({ "read": input.what, "name": input.name, "error": "not found" }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_names_the_four_kinds_and_refuses_others() {
        let text = read_tool_def().input_schema.to_string();
        for what in ["node", "pattern", "board", "device"] {
            assert!(text.contains(&format!("\"{what}\"")), "{text}");
        }
        assert!(
            serde_json::from_value::<ReadInput>(json!({ "what": "weather", "name": "x" })).is_err()
        );
    }
}
