//! Debug export of one agent chat: the raw model-facing transcript.
//!
//! Built on demand by [`crate::AgentOp::ExportDebug`] while the session is
//! parked (idle), embedded in the DTO as [`crate::UiAgentDebugDump`], and
//! downloaded by the web shell. The dump is what you hand to a developer
//! when a run misbehaves: the exact messages the model saw (tool inputs and
//! results verbatim, thinking blocks included), how every turn stopped, and
//! what each cost — never the API key.

use lpa_agent::{AgentTranscript, StopReason};
use serde_json::json;

use crate::app::agent::agent_chat_session::{AgentEditRecord, AgentTurnStat};
use crate::app::agent::agent_provider_config::AgentProviderConfig;

/// Dump format version, bumped when the shape changes.
const DUMP_VERSION: u32 = 1;

/// Assemble the pretty-printed JSON debug dump.
pub(crate) fn debug_dump_json(
    artifact: &str,
    config: Option<&AgentProviderConfig>,
    transcript: &AgentTranscript,
    turn_stats: &[AgentTurnStat],
    edits: &[AgentEditRecord],
) -> String {
    let (provider, model) = match config {
        Some(AgentProviderConfig::Anthropic(c)) => ("anthropic", c.model.clone()),
        Some(AgentProviderConfig::OpenAiCompat(c)) => ("openai-compat", c.model.clone()),
        None => ("unconfigured", String::new()),
    };
    let dump = json!({
        "format": DUMP_VERSION,
        "artifact": artifact,
        "provider": provider,
        "model": model,
        "usage_total": transcript.usage_total,
        "turns": turn_stats
            .iter()
            .map(|stat| {
                json!({
                    "stop_reason": stop_reason_str(&stat.stop_reason),
                    "usage": stat.usage,
                })
            })
            .collect::<Vec<_>>(),
        // Every staged source in full, first-class — the GLSL is also
        // buried in the messages' tool inputs, but debugging starts here.
        "edits": edits
            .iter()
            .map(|record| {
                json!({
                    "turn": record.turn,
                    "note": record.note,
                    "engine_ok": record.engine_ok,
                    "source": record.source.as_ref(),
                })
            })
            .collect::<Vec<_>>(),
        "messages": transcript.messages,
    });
    serde_json::to_string_pretty(&dump).unwrap_or_else(|error| {
        // Unreachable with these Serialize impls; still, a lossy dump beats
        // a panic inside a debugging affordance.
        format!("{{\"format\":{DUMP_VERSION},\"error\":\"serialize failed: {error}\"}}")
    })
}

/// Stable string form of a stop reason (`Other` keeps the provider's word).
fn stop_reason_str(reason: &StopReason) -> String {
    match reason {
        StopReason::EndTurn => "end_turn".to_string(),
        StopReason::ToolUse => "tool_use".to_string(),
        StopReason::MaxTokens => "max_tokens".to_string(),
        StopReason::Other(other) => format!("other:{other}"),
    }
}

#[cfg(test)]
mod tests {
    use lpa_agent::{ChatMessage, TokenUsage};
    use lpc_model::Revision;

    use super::*;

    fn transcript() -> AgentTranscript {
        let mut transcript = AgentTranscript::default();
        transcript.push_user_text("make it pulse");
        transcript.usage_total = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
            cache_write_tokens: 30,
            cache_read_tokens: 40,
        };
        transcript
    }

    #[test]
    fn dump_round_trips_as_json_with_the_expected_shape() {
        let stats = vec![AgentTurnStat {
            stop_reason: StopReason::MaxTokens,
            usage: TokenUsage::default(),
        }];
        let edits = vec![AgentEditRecord {
            turn: 1,
            note: Some("slow it down".into()),
            source: std::rc::Rc::from("uniform float speed;"),
            thumb: None,
            engine_ok: Some(true),
            at: Revision::default(),
        }];
        let json = debug_dump_json("nodes/a.glsl", None, &transcript(), &stats, &edits);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["format"], 1);
        assert_eq!(value["artifact"], "nodes/a.glsl");
        assert_eq!(value["provider"], "unconfigured");
        assert_eq!(value["turns"][0]["stop_reason"], "max_tokens");
        assert_eq!(value["usage_total"]["output_tokens"], 20);
        assert_eq!(value["edits"][0]["turn"], 1);
        assert_eq!(value["edits"][0]["source"], "uniform float speed;");
        // Messages round-trip through the provider-neutral serde shape.
        let messages: Vec<ChatMessage> =
            serde_json::from_value(value["messages"].clone()).expect("messages deserialize");
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn dump_names_the_provider_but_never_the_key() {
        let config = AgentProviderConfig::Anthropic(lpa_agent::AnthropicConfig {
            api_key: "sk-secret".to_string(),
            model: "claude-sonnet-5".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
        });
        let json = debug_dump_json("nodes/a.glsl", Some(&config), &transcript(), &[], &[]);
        assert!(json.contains("claude-sonnet-5"));
        assert!(!json.contains("sk-secret"));
    }
}
