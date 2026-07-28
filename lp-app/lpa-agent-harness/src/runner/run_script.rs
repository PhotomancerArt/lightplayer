//! Scripted-run script files: a serde-friendly mirror of
//! [`lpa_agent::TurnEvent`] (which is not serde), loaded from JSON and
//! played through the P1 [`crate::ScriptedProvider`].
//!
//! Shape: `runs[r][t]` is turn `t`'s event list for the `r`-th provider
//! build (one per Send; the runner sends once, so scripts usually carry a
//! single run). Enum variants use serde's default EXTERNAL tagging (the
//! repo bans tag/untagged/flatten machinery), so an event reads
//! `{"text": "..."}` / `{"tool_use": {...}}` / `{"done": {...}}`.

use std::path::Path;

use lpa_agent::provider::model_provider::{StopReason, TokenUsage, TurnEvent};
use serde::{Deserialize, Serialize};

/// A whole script file.
#[derive(Debug, Deserialize, Serialize)]
pub struct RunScript {
    pub runs: Vec<Vec<Vec<ScriptEvent>>>,
}

/// One scripted turn event (externally tagged JSON).
#[derive(Debug, Deserialize, Serialize)]
pub enum ScriptEvent {
    /// Assistant text.
    #[serde(rename = "text")]
    Text(String),
    /// Thinking text.
    #[serde(rename = "thinking")]
    Thinking(String),
    /// A tool call; `input` is delivered as one input-JSON delta.
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool call whose input stream was CUT mid-JSON (what a real
    /// `max_tokens` stop looks like on the wire): the raw fragment is
    /// delivered verbatim, deliberately not valid JSON. Pair it with a
    /// `done` carrying `max_tokens`.
    #[serde(rename = "tool_use_cut")]
    ToolUseCut {
        id: String,
        name: String,
        input_fragment: String,
    },
    /// End of turn. `stop_reason` is `end_turn` / `tool_use` /
    /// `max_tokens` / anything else verbatim (mapped to `Other`).
    #[serde(rename = "done")]
    Done {
        stop_reason: String,
        #[serde(default)]
        input_tokens: u32,
        #[serde(default)]
        output_tokens: u32,
    },
    /// A provider failure.
    #[serde(rename = "provider_error")]
    ProviderError { message: String, retryable: bool },
}

impl ScriptEvent {
    /// Expand into the wire-shaped [`TurnEvent`]s the provider streams.
    fn to_turn_events(&self) -> Vec<TurnEvent> {
        match self {
            Self::Text(text) => vec![TurnEvent::TextDelta(text.clone())],
            Self::Thinking(text) => vec![TurnEvent::ThinkingDelta(text.clone())],
            Self::ToolUse { id, name, input } => vec![
                TurnEvent::ToolUseStart {
                    id: id.clone(),
                    name: name.clone(),
                },
                TurnEvent::ToolInputDelta {
                    id: id.clone(),
                    json_fragment: input.to_string(),
                },
            ],
            Self::ToolUseCut {
                id,
                name,
                input_fragment,
            } => vec![
                TurnEvent::ToolUseStart {
                    id: id.clone(),
                    name: name.clone(),
                },
                TurnEvent::ToolInputDelta {
                    id: id.clone(),
                    json_fragment: input_fragment.clone(),
                },
            ],
            Self::Done {
                stop_reason,
                input_tokens,
                output_tokens,
            } => vec![TurnEvent::TurnDone {
                stop_reason: match stop_reason.as_str() {
                    "end_turn" => StopReason::EndTurn,
                    "tool_use" => StopReason::ToolUse,
                    "max_tokens" => StopReason::MaxTokens,
                    other => StopReason::Other(other.to_string()),
                },
                usage: TokenUsage {
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    ..TokenUsage::default()
                },
            }],
            Self::ProviderError { message, retryable } => vec![TurnEvent::ProviderError {
                message: message.clone(),
                retryable: *retryable,
            }],
        }
    }
}

impl RunScript {
    /// Parse a script from JSON text.
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| format!("invalid script JSON: {error}"))
    }

    /// Load a script file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::parse(&json)
    }

    /// The per-run turn scripts in [`TurnEvent`] form, ready for one
    /// `ScriptedProvider` per run.
    pub fn to_run_scripts(&self) -> Vec<Vec<Vec<TurnEvent>>> {
        self.runs
            .iter()
            .map(|run| {
                run.iter()
                    .map(|turn| turn.iter().flat_map(ScriptEvent::to_turn_events).collect())
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_json_round_trips_to_turn_events() {
        let script = RunScript::parse(
            r#"{
              "runs": [[
                [
                  {"text": "Trying green."},
                  {"tool_use": {"id": "tu_1", "name": "iterate",
                                "input": {"source": "vec4 render(vec2 p){return vec4(0.);}",
                                          "note": "go green"}}},
                  {"done": {"stop_reason": "tool_use", "input_tokens": 10, "output_tokens": 20}}
                ],
                [
                  {"text": "Done."},
                  {"done": {"stop_reason": "end_turn"}}
                ]
              ]]
            }"#,
        )
        .expect("parses");
        let runs = script.to_run_scripts();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].len(), 2, "two turns");
        // Turn 1: text + tool start + one input delta + done(tool_use).
        let turn = &runs[0][0];
        assert!(matches!(&turn[0], TurnEvent::TextDelta(t) if t == "Trying green."));
        assert!(
            matches!(&turn[1], TurnEvent::ToolUseStart { id, name } if id == "tu_1" && name == "iterate")
        );
        let TurnEvent::ToolInputDelta { json_fragment, .. } = &turn[2] else {
            panic!("expected input delta, got {:?}", turn[2]);
        };
        let input: serde_json::Value = serde_json::from_str(json_fragment).expect("input json");
        assert_eq!(input["note"], "go green");
        assert!(matches!(
            &turn[3],
            TurnEvent::TurnDone {
                stop_reason: StopReason::ToolUse,
                usage
            } if usage.input_tokens == 10 && usage.output_tokens == 20
        ));
        // Turn 2 ends the session.
        assert!(matches!(
            runs[0][1].last(),
            Some(TurnEvent::TurnDone {
                stop_reason: StopReason::EndTurn,
                ..
            })
        ));
    }

    #[test]
    fn tool_use_cut_delivers_the_raw_fragment_verbatim() {
        let script = RunScript::parse(
            r#"{"runs": [[[
                {"tool_use_cut": {"id": "tu_9", "name": "iterate",
                                  "input_fragment": "{\"source\":\"vec4 ren"}},
                {"done": {"stop_reason": "max_tokens"}}
            ]]]}"#,
        )
        .expect("parses");
        let turn = &script.to_run_scripts()[0][0];
        assert!(matches!(&turn[0], TurnEvent::ToolUseStart { id, .. } if id == "tu_9"));
        let TurnEvent::ToolInputDelta { json_fragment, .. } = &turn[1] else {
            panic!("expected input delta, got {:?}", turn[1]);
        };
        assert_eq!(json_fragment, "{\"source\":\"vec4 ren");
        assert!(
            serde_json::from_str::<serde_json::Value>(json_fragment).is_err(),
            "the fragment must be a genuine mid-JSON cut"
        );
        assert!(matches!(
            &turn[2],
            TurnEvent::TurnDone {
                stop_reason: StopReason::MaxTokens,
                ..
            }
        ));
    }

    #[test]
    fn unknown_stop_reason_maps_to_other_and_bad_json_errors() {
        let script =
            RunScript::parse(r#"{"runs": [[[{"done": {"stop_reason": "content_filter"}}]]]}"#)
                .expect("parses");
        assert!(matches!(
            &script.to_run_scripts()[0][0][0],
            TurnEvent::TurnDone { stop_reason: StopReason::Other(word), .. } if word == "content_filter"
        ));
        assert!(RunScript::parse("nope").is_err());
    }
}
