//! [`AgentTranscriptMirror`]: the transcript a chat view renders, folded
//! from streamed [`AgentEvent`]s — shared by the shader chat
//! ([`crate::app::agent::agent_chat_session::AgentChatSession`]) and the app
//! chat ([`crate::app::agent::app_agent_session::AppAgentSession`]).
//!
//! The authoritative model transcript lives inside the `lpa_agent`
//! session; this is what the user sees: text, thinking, tool rows, notices,
//! status and usage.

use lpa_agent::{AgentEvent, StopReason, TokenUsage};

use crate::UiNoticeLevel;
use crate::app::agent::ui_agent_edit_batch::UiAgentEditBatch;
use crate::app::agent::ui_agent_view::{UiAgentStatus, UiAgentToolRow, UiAgentTurn, UiAgentUsage};

/// Per-turn cap on retained thinking text (bytes). Thinking can run long;
/// the mirror keeps the NEWEST text (the part the user is watching) and
/// trims the front. Session-scoped only — thinking is never persisted.
pub const MAX_THINKING_BYTES: usize = 20_000;

/// One model turn's outcome, mirrored for the debug export: how the turn
/// stopped and what it cost. The model-facing transcript records neither,
/// so the mirror keeps them (session-scoped, like the rest of the mirror).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTurnStat {
    pub stop_reason: StopReason,
    pub usage: TokenUsage,
}

/// A tool call the fold just settled — handed back so a session can hook
/// its own bookkeeping (the shader chat's edit records).
pub struct ExecutedTool {
    pub id: String,
    pub name: String,
    pub summary_json: serde_json::Value,
}

/// One chat's view-side transcript state.
#[derive(Clone, Debug)]
pub struct AgentTranscriptMirror {
    /// The transcript mirror the DTO clones from.
    pub turns: Vec<UiAgentTurn>,
    /// Live status projected into the DTO.
    pub status: UiAgentStatus,
    /// Cumulative usage (authoritatively reset by `SessionDone` totals).
    pub usage: TokenUsage,
    /// Per-turn stop reason + usage, in turn order (debug-export data).
    pub turn_stats: Vec<AgentTurnStat>,
    /// A run began since the last turn was pushed: the next text or
    /// thinking starts its own turn instead of continuing the previous
    /// run's (a run a card resumes shows no user turn between them).
    run_boundary: bool,
}

impl Default for AgentTranscriptMirror {
    fn default() -> Self {
        Self {
            turns: Vec::new(),
            status: UiAgentStatus::Idle,
            usage: TokenUsage::default(),
            turn_stats: Vec::new(),
            run_boundary: false,
        }
    }
}

impl AgentTranscriptMirror {
    /// A new run starts: whatever it streams is its own turn, even with no
    /// user turn in between (the run a settled card resumes).
    pub fn begin_run(&mut self) {
        self.run_boundary = true;
    }

    /// Fold one streamed event into the mirror.
    pub fn apply_event(&mut self, event: AgentEvent) -> Option<ExecutedTool> {
        match event {
            AgentEvent::TextDelta(text) => {
                self.status = UiAgentStatus::Streaming;
                let fresh = core::mem::take(&mut self.run_boundary);
                match self.turns.last_mut() {
                    Some(UiAgentTurn::Assistant { text: existing }) if !fresh => {
                        existing.push_str(&text)
                    }
                    _ => self.turns.push(UiAgentTurn::Assistant { text }),
                }
            }
            AgentEvent::ThinkingDelta(text) => {
                self.status = UiAgentStatus::Streaming;
                let fresh = core::mem::take(&mut self.run_boundary);
                match self.turns.last_mut() {
                    Some(UiAgentTurn::Thinking {
                        text: existing,
                        done: false,
                    }) if !fresh => {
                        existing.push_str(&text);
                        cap_thinking_text(existing);
                    }
                    _ => self.turns.push(UiAgentTurn::Thinking { text, done: false }),
                }
            }
            AgentEvent::ThinkingDone => {
                if let Some(UiAgentTurn::Thinking { done, .. }) = self.turns.last_mut() {
                    *done = true;
                }
            }
            AgentEvent::ToolUseStart { id, name } => {
                self.status = UiAgentStatus::RunningTool;
                self.run_boundary = false;
                self.turns.push(UiAgentTurn::Tool(
                    UiAgentToolRow::started(id).for_tool(name),
                ));
            }
            // The raw input JSON stays in core/debug; the row renders the
            // executed summary instead.
            AgentEvent::ToolInputDelta { .. } => {}
            // The accumulated input's note lands pre-execution so the
            // running row reads "{note} — running" while the tool works.
            AgentEvent::ToolInputReady { id, note } => {
                if let Some(row) = self.tool_row_mut(&id) {
                    row.note = note;
                }
            }
            // Live phase for the running row ("compiling", "probe 2/5", …).
            AgentEvent::ToolProgress { id, phase } => {
                if let Some(row) = self.tool_row_mut(&id) {
                    row.phase = Some(phase.to_string());
                }
            }
            AgentEvent::ToolExecuted {
                id,
                name,
                summary_json,
            } => {
                self.status = UiAgentStatus::Streaming;
                let row = self.turns.iter_mut().rev().find_map(|turn| match turn {
                    UiAgentTurn::Tool(row) if row.id == id => Some(row),
                    _ => None,
                });
                if let Some(row) = row {
                    row.done = true;
                    row.phase = None;
                    row.note = summary_json["note"].as_str().map(str::to_string);
                    row.staged = summary_json["staged"].as_bool().unwrap_or(false);
                    row.shader_ok = summary_json["shader_ok"].as_bool();
                    row.probes = summary_json["probes"].as_u64().unwrap_or(0) as u32;
                    row.warnings = summary_json["warnings"].as_u64().unwrap_or(0) as u32;
                    row.error = summary_json["error"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| {
                            summary_json["input_error"]
                                .as_bool()
                                .unwrap_or(false)
                                .then(|| "invalid tool input".to_string())
                        });
                    row.detail = serde_json::to_string_pretty(&summary_json)
                        .unwrap_or_else(|_| summary_json.to_string());
                    row.tool = name.clone();
                    row.edits = (name == "edit_project")
                        .then(|| UiAgentEditBatch::from_summary(&summary_json))
                        .flatten();
                    row.headline = app_tool_headline(&name, &summary_json);
                }
                return Some(ExecutedTool {
                    id,
                    name,
                    summary_json,
                });
            }
            AgentEvent::TurnDone { stop_reason, usage } => {
                self.usage.add(usage);
                self.turn_stats.push(AgentTurnStat { stop_reason, usage });
            }
            AgentEvent::MaxTurnsReached { turns } => {
                self.push_notice(format!(
                    "Turn limit reached ({turns} model turns) — the agent stopped to wait for you."
                ));
            }
            AgentEvent::Truncated {
                stop_reason,
                dropped_tool_call,
            } => {
                // The cut usually lands mid-tool-call: that row will never
                // execute, so it must not keep pulsing "running".
                self.resolve_unfinished_tool_rows("cut off by the output-token limit");
                self.push_warning_notice(truncation_notice(&stop_reason, dropped_tool_call));
            }
            AgentEvent::Aborted => {
                self.push_notice("Stopped.");
            }
            AgentEvent::ProviderError { message, retryable } => {
                self.push_notice(format!("Provider error: {message}"));
                self.status = UiAgentStatus::Error { message, retryable };
            }
            AgentEvent::SessionDone { usage_total } => {
                // The session's own total is authoritative (it survives
                // event loss and covers every turn of this session).
                self.usage = usage_total;
            }
        }
        None
    }

    /// The run future finished; settle the terminal status.
    pub fn run_ended(&mut self, error: Option<String>) {
        // A run that ends mid-thought (abort, provider failure) never sent
        // the boundary event — collapse the trailing thinking strip anyway.
        if let Some(UiAgentTurn::Thinking { done, .. }) = self.turns.last_mut() {
            *done = true;
        }
        // General invariant: however the run ended (abort, provider error,
        // truncation), no tool row may stay pulsing "running" forever.
        self.resolve_unfinished_tool_rows("interrupted — the run ended before this call finished");
        match error {
            // `ProviderError` events usually set the error status already;
            // this covers failure paths that end the run without one.
            Some(message) => {
                if !matches!(self.status, UiAgentStatus::Error { .. }) {
                    self.status = UiAgentStatus::Error {
                        message,
                        retryable: true,
                    };
                }
            }
            None => self.status = UiAgentStatus::Idle,
        }
    }

    /// Append a session-level notice to the transcript.
    pub fn push_notice(&mut self, text: impl Into<String>) {
        self.turns.push(UiAgentTurn::Notice {
            text: text.into(),
            level: UiNoticeLevel::Info,
        });
    }

    /// Append a warning-toned notice (the run ended incomplete).
    pub fn push_warning_notice(&mut self, text: impl Into<String>) {
        self.turns.push(UiAgentTurn::Notice {
            text: text.into(),
            level: UiNoticeLevel::Warning,
        });
    }

    /// Settle every not-yet-done tool row as failed with `reason`, so a
    /// run that ends for any reason leaves no dangling "running" row.
    fn resolve_unfinished_tool_rows(&mut self, reason: &str) {
        for turn in &mut self.turns {
            if let UiAgentTurn::Tool(row) = turn
                && !row.done
            {
                row.done = true;
                row.phase = None;
                row.error = Some(reason.to_string());
            }
        }
    }

    /// The most recent tool row with `id` (updates target the newest call).
    pub fn tool_row_mut(&mut self, id: &str) -> Option<&mut UiAgentToolRow> {
        self.turns.iter_mut().rev().find_map(|turn| match turn {
            UiAgentTurn::Tool(row) if row.id == id => Some(row),
            _ => None,
        })
    }

    /// Snapshot the mirror as DTO fields (turns + usage).
    pub fn ui_usage(&self) -> UiAgentUsage {
        UiAgentUsage {
            input_tokens: self.usage.input_tokens,
            output_tokens: self.usage.output_tokens,
            cache_write_tokens: self.usage.cache_write_tokens,
            cache_read_tokens: self.usage.cache_read_tokens,
            cost_micro_usd: self.usage.cost_micro_usd,
        }
    }
}

/// A finished app-agent `act` or `read`, in words: what was pressed (or
/// handed to the user on a card, or refused), what was read. `None` for
/// every other tool.
fn app_tool_headline(name: &str, summary: &serde_json::Value) -> Option<String> {
    let text = |key: &str| summary[key].as_str().unwrap_or("").to_string();
    match name {
        "act" => {
            let action = text("action");
            Some(if summary["done"].as_bool() == Some(true) {
                format!("pressed {action}")
            } else if let Some(card) = summary["card"].as_str() {
                format!("asked you to click card {card}: {action}")
            } else if let Some(reason) = summary["refused"].as_str() {
                format!("{action} — refused: {reason}")
            } else if summary["input_error"].as_bool() == Some(true) {
                "invalid act input".to_string()
            } else {
                format!("{action} — failed")
            })
        }
        "read" => {
            if summary["input_error"].as_bool() == Some(true) {
                return Some("invalid read input".to_string());
            }
            let what = format!("read {} {}", text("read"), text("name"));
            Some(match summary["error"].as_str() {
                Some(error) => format!("{} — {error}", what.trim_end()),
                None => what.trim_end().to_string(),
            })
        }
        _ => None,
    }
}

/// The user-facing copy for a truncated run. `MaxTokens` gets the
/// actionable phrasing (retry, or ask for something smaller); an unknown
/// `Other` stop reason is surfaced verbatim.
fn truncation_notice(stop_reason: &StopReason, dropped_tool_call: bool) -> String {
    match (stop_reason, dropped_tool_call) {
        (StopReason::MaxTokens, true) => "Run stopped: the response hit the output-token limit \
             while writing the edit — try again or ask for something smaller."
            .to_string(),
        (StopReason::MaxTokens, false) => "Run stopped: the response hit the output-token limit \
             — try again or ask for something smaller."
            .to_string(),
        (StopReason::Other(reason), true) => format!(
            "Run stopped early (provider reported {reason:?}) — the unfinished edit was discarded."
        ),
        (StopReason::Other(reason), false) => {
            format!("Run stopped early (provider reported {reason:?}).")
        }
        // Unreachable today (the session only emits Truncated for the two
        // arms above); a safe fallback beats a panic.
        _ => "Run stopped early.".to_string(),
    }
}

/// Trim one thinking turn's text to [`MAX_THINKING_BYTES`], dropping the
/// OLDEST text on a char boundary and marking the cut with an ellipsis.
fn cap_thinking_text(text: &mut String) {
    if text.len() <= MAX_THINKING_BYTES {
        return;
    }
    let cut = text.len() - MAX_THINKING_BYTES;
    let boundary = (cut..text.len())
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(text.len());
    text.replace_range(..boundary, "…");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_edit_project_call_folds_into_an_edit_row() {
        let mut mirror = AgentTranscriptMirror::default();
        mirror.apply_event(AgentEvent::ToolUseStart {
            id: "tu_1".into(),
            name: "edit_project".into(),
        });
        let Some(UiAgentTurn::Tool(row)) = mirror.turns.last() else {
            panic!("a tool row");
        };
        assert_eq!(row.summary_line(), "Editing the project…");
        mirror.apply_event(AgentEvent::ToolExecuted {
            id: "tu_1".into(),
            name: "edit_project".into(),
            summary_json: json!({
                "note": "build it", "edits": 1, "applied": 1, "saved": true,
                "rows": [{ "edit": "create_node", "target": "Playlist", "ok": true }]
            }),
        });
        let Some(UiAgentTurn::Tool(row)) = mirror.turns.last() else {
            panic!("a tool row");
        };
        assert_eq!(row.summary_line(), "added Playlist, saved");
        assert_eq!(row.note.as_deref(), Some("build it"));
    }

    /// A run a card resumes shows no user turn: its text is still its own
    /// assistant turn, never glued onto the previous run's.
    #[test]
    fn a_new_run_starts_a_new_assistant_turn() {
        let mut mirror = AgentTranscriptMirror::default();
        mirror.begin_run();
        mirror.apply_event(AgentEvent::TextDelta("Click Connect ".into()));
        mirror.apply_event(AgentEvent::TextDelta("on the card.".into()));
        mirror.begin_run();
        mirror.apply_event(AgentEvent::ThinkingDelta("it worked".into()));
        mirror.apply_event(AgentEvent::ThinkingDone);
        mirror.apply_event(AgentEvent::TextDelta("Connected.".into()));
        mirror.begin_run();
        mirror.apply_event(AgentEvent::TextDelta("Again.".into()));
        let texts: Vec<&str> = mirror
            .turns
            .iter()
            .filter_map(|turn| match turn {
                UiAgentTurn::Assistant { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            ["Click Connect on the card.", "Connected.", "Again."]
        );
    }

    #[test]
    fn act_and_read_calls_say_what_they_did() {
        assert_eq!(
            app_tool_headline("act", &json!({ "action": "project/save", "done": true })),
            Some("pressed project/save".into())
        );
        assert_eq!(
            app_tool_headline(
                "act",
                &json!({ "action": "devices/connect-usb", "card": "c1" })
            ),
            Some("asked you to click card c1: devices/connect-usb".into())
        );
        assert_eq!(
            app_tool_headline("read", &json!({ "read": "node", "name": "fixture" })),
            Some("read node fixture".into())
        );
        assert_eq!(app_tool_headline("iterate", &json!({})), None);
    }
}
