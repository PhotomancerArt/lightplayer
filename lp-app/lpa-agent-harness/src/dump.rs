//! [`parse_dump`]: a typed reader over the agent debug-export JSON.
//!
//! The export (lpa-studio-core's `agent_debug_export`, format 1) is the
//! artifact a session hands to a developer: provider identity, per-turn
//! stop reasons and usage, every staged edit in full, and the verbatim
//! model-facing messages. Tests and the (P2) headless runner parse it here
//! to assert on stop reasons, edits, and cache buckets instead of poking
//! raw JSON paths.

use lpa_agent::{ChatMessage, TokenUsage};
use serde::Deserialize;

/// The dump format version this parser understands.
pub const DUMP_FORMAT: u32 = 1;

/// One parsed debug dump (format 1).
#[derive(Debug, Deserialize)]
pub struct Dump {
    /// Dump format version (validated by [`parse_dump`]).
    pub format: u32,
    /// The shader artifact path the session edited.
    pub artifact: String,
    /// Provider kind (`anthropic` / `openai-compat` / `unconfigured`).
    pub provider: String,
    /// Model id (empty when unconfigured).
    pub model: String,
    /// Session-total usage, cache buckets included.
    pub usage_total: TokenUsage,
    /// Per-model-turn stop reason + usage, in turn order.
    pub turns: Vec<DumpTurn>,
    /// Every staged edit in full, in order.
    pub edits: Vec<DumpEdit>,
    /// The verbatim model-facing transcript.
    pub messages: Vec<ChatMessage>,
}

/// One model turn's outcome.
#[derive(Debug, Deserialize)]
pub struct DumpTurn {
    /// Stable stop-reason string (`end_turn` / `tool_use` / `max_tokens` /
    /// `other:<provider word>`).
    pub stop_reason: String,
    pub usage: TokenUsage,
}

/// One staged edit record.
#[derive(Debug, Deserialize)]
pub struct DumpEdit {
    /// Session-scoped edit ordinal (1-based).
    pub turn: u32,
    /// The call's one-line intent note.
    pub note: Option<String>,
    /// Engine verdict for this edit (`None` while unresolved).
    pub engine_ok: Option<bool>,
    /// The staged shader source, verbatim.
    pub source: String,
}

/// Parse a debug-export dump, rejecting unknown format versions.
pub fn parse_dump(json: &str) -> Result<Dump, String> {
    let dump: Dump = serde_json::from_str(json).map_err(|e| format!("invalid dump JSON: {e}"))?;
    if dump.format != DUMP_FORMAT {
        return Err(format!(
            "unsupported dump format {} (this parser understands format {DUMP_FORMAT})",
            dump.format
        ));
    }
    Ok(dump)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A representative format-1 dump, shaped like
    /// `agent_debug_export::debug_dump_json` output (the studio-core export
    /// tests parse REAL exporter output through `parse_dump`).
    fn sample() -> String {
        json!({
            "format": 1,
            "artifact": "nodes/a.glsl",
            "provider": "anthropic",
            "model": "claude-sonnet-5",
            "usage_total": {
                "input_tokens": 10, "output_tokens": 20,
                "cache_write_tokens": 30, "cache_read_tokens": 40,
            },
            "turns": [
                { "stop_reason": "tool_use",
                  "usage": { "input_tokens": 10, "output_tokens": 20,
                             "cache_write_tokens": 30, "cache_read_tokens": 40 } },
                { "stop_reason": "max_tokens",
                  "usage": { "input_tokens": 0, "output_tokens": 0,
                             "cache_write_tokens": 0, "cache_read_tokens": 0 } },
            ],
            "edits": [
                { "turn": 1, "note": "go green", "engine_ok": true,
                  "source": "vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.0, 1.0); }" },
                { "turn": 2, "note": null, "engine_ok": null, "source": "" },
            ],
            "messages": [
                { "role": "user", "content": [ { "type": "text", "text": "make it green" } ] },
            ],
        })
        .to_string()
    }

    #[test]
    fn dump_parses_stop_reasons_edits_and_cache_buckets() {
        let dump = parse_dump(&sample()).expect("parses");
        assert_eq!(dump.format, 1);
        assert_eq!(dump.artifact, "nodes/a.glsl");
        assert_eq!(dump.provider, "anthropic");
        assert_eq!(dump.model, "claude-sonnet-5");
        assert_eq!(dump.usage_total.cache_write_tokens, 30);
        assert_eq!(dump.usage_total.cache_read_tokens, 40);
        let stops: Vec<&str> = dump.turns.iter().map(|t| t.stop_reason.as_str()).collect();
        assert_eq!(stops, ["tool_use", "max_tokens"]);
        assert_eq!(dump.edits[0].turn, 1);
        assert_eq!(dump.edits[0].note.as_deref(), Some("go green"));
        assert_eq!(dump.edits[0].engine_ok, Some(true));
        assert!(dump.edits[0].source.contains("render"));
        assert_eq!(dump.edits[1].engine_ok, None);
        assert_eq!(dump.messages.len(), 1);
    }

    #[test]
    fn wrong_format_and_bad_json_are_rejected() {
        let future = sample().replace("\"format\":1", "\"format\":2");
        let err = parse_dump(&future).expect_err("format 2 rejected");
        assert!(err.contains("unsupported dump format 2"), "{err}");

        let err = parse_dump("not json").expect_err("garbage rejected");
        assert!(err.contains("invalid dump JSON"), "{err}");
    }
}
