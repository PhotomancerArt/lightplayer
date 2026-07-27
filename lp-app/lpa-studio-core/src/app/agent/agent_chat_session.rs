//! [`AgentChatSession`]: per-shader agent conversation state.
//!
//! Holds the transcript MIRROR the view renders (the authoritative model
//! transcript lives inside the `lpa_agent::AgentSession`, which parks in
//! [`Self::runtime`] between runs and travels with the spawned run future
//! while one is in flight).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use lpa_agent::{AgentEvent, AgentSession, ModelProvider, TokenUsage};
use lpc_model::ArtifactLocation;

use crate::app::agent::agent_host_bridge::{AgentBridgeState, AgentHostBridge};
use crate::app::agent::agent_session_key::AgentSessionKey;
use crate::app::agent::ui_agent_view::{UiAgentStatus, UiAgentToolRow, UiAgentTurn, UiAgentUsage};

/// The concrete `lpa-agent` session type Studio drives: a runtime-chosen
/// provider behind a box, over the command-queue host bridge.
pub type AgentSessionRuntime = AgentSession<Box<dyn ModelProvider>, AgentHostBridge>;

/// One shader node's conversation: view mirror + parked session runtime.
pub struct AgentChatSession {
    pub key: AgentSessionKey,
    /// The shader source artifact the session operates on (the decoration
    /// lookup identity).
    pub artifact: ArtifactLocation,
    /// The transcript mirror the DTO clones from.
    pub turns: Vec<UiAgentTurn>,
    /// Live status projected into the DTO.
    pub status: UiAgentStatus,
    /// Cumulative usage (authoritatively reset by `SessionDone` totals).
    pub usage: TokenUsage,
    /// True from run start until `RunEnded` arrives.
    pub running: bool,
    /// The shared snapshot the host bridge serves (refreshed at run start).
    pub bridge: Rc<RefCell<AgentBridgeState>>,
    /// The parked session runtime (`None` while a run future owns it; the
    /// future puts it back before sending `RunEnded`).
    pub runtime: Rc<RefCell<Option<AgentSessionRuntime>>>,
    /// The running session's abort flag (the Stop button's target).
    pub abort: Arc<AtomicBool>,
}

impl AgentChatSession {
    pub fn new(key: AgentSessionKey, artifact: ArtifactLocation) -> Self {
        Self {
            key,
            artifact,
            turns: Vec::new(),
            status: UiAgentStatus::Idle,
            usage: TokenUsage::default(),
            running: false,
            bridge: Rc::new(RefCell::new(AgentBridgeState::default())),
            runtime: Rc::new(RefCell::new(None)),
            abort: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Fold one streamed event into the mirror.
    pub fn apply_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta(text) => {
                self.status = UiAgentStatus::Streaming;
                match self.turns.last_mut() {
                    Some(UiAgentTurn::Assistant { text: existing }) => existing.push_str(&text),
                    _ => self.turns.push(UiAgentTurn::Assistant { text }),
                }
            }
            AgentEvent::ToolUseStart { id, .. } => {
                self.status = UiAgentStatus::RunningTool;
                self.turns
                    .push(UiAgentTurn::Tool(UiAgentToolRow::started(id)));
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
                id, summary_json, ..
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
                }
            }
            AgentEvent::TurnDone { usage, .. } => {
                self.usage.add(usage);
            }
            AgentEvent::MaxTurnsReached { turns } => {
                self.push_notice(format!(
                    "Turn limit reached ({turns} model turns) — the agent stopped to wait for you."
                ));
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
    }

    /// The run future finished; settle the terminal status.
    pub fn run_ended(&mut self, error: Option<String>) {
        self.running = false;
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
        self.turns.push(UiAgentTurn::Notice { text: text.into() });
    }

    /// The most recent tool row with `id` (updates target the newest call).
    fn tool_row_mut(&mut self, id: &str) -> Option<&mut UiAgentToolRow> {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn session() -> AgentChatSession {
        AgentChatSession::new(
            AgentSessionKey::new(crate::RuntimeId::new(1), "/p/shader"),
            ArtifactLocation::file("/shader.glsl"),
        )
    }

    #[test]
    fn text_deltas_accumulate_into_one_assistant_turn() {
        let mut session = session();
        session.apply_event(AgentEvent::TextDelta("Hel".into()));
        session.apply_event(AgentEvent::TextDelta("lo.".into()));

        assert_eq!(
            session.turns,
            vec![UiAgentTurn::Assistant {
                text: "Hello.".into()
            }]
        );
        assert_eq!(session.status, UiAgentStatus::Streaming);
    }

    #[test]
    fn tool_rows_fill_from_the_executed_summary() {
        let mut session = session();
        session.apply_event(AgentEvent::ToolUseStart {
            id: "tu_1".into(),
            name: "iterate".into(),
        });
        assert_eq!(session.status, UiAgentStatus::RunningTool);

        session.apply_event(AgentEvent::ToolExecuted {
            id: "tu_1".into(),
            name: "iterate".into(),
            summary_json: json!({
                "note": "go green", "staged": true, "shader_ok": true,
                "probes": 2, "warnings": 1,
            }),
        });
        let UiAgentTurn::Tool(row) = &session.turns[0] else {
            panic!("expected tool row");
        };
        assert!(row.done);
        assert_eq!(row.note.as_deref(), Some("go green"));
        assert!(row.staged);
        assert_eq!(row.shader_ok, Some(true));
        assert_eq!((row.probes, row.warnings), (2, 1));
        assert!(row.detail.contains("go green"));
        assert_eq!(session.status, UiAgentStatus::Streaming);
    }

    #[test]
    fn input_ready_and_progress_shape_the_running_row() {
        let mut session = session();
        session.apply_event(AgentEvent::ToolUseStart {
            id: "tu_1".into(),
            name: "iterate".into(),
        });
        session.apply_event(AgentEvent::ToolInputReady {
            id: "tu_1".into(),
            note: Some("go green".into()),
        });
        session.apply_event(AgentEvent::ToolProgress {
            id: "tu_1".into(),
            phase: lpa_agent::ToolPhase::Probing { i: 2, of: 5 },
        });
        let UiAgentTurn::Tool(row) = &session.turns[0] else {
            panic!("expected tool row");
        };
        assert!(!row.done);
        assert_eq!(row.summary_line(), "go green — probe 2/5");

        // Execution completes: the phase clears, the outcome takes over.
        session.apply_event(AgentEvent::ToolExecuted {
            id: "tu_1".into(),
            name: "iterate".into(),
            summary_json: json!({ "note": "go green", "staged": true, "shader_ok": true }),
        });
        let UiAgentTurn::Tool(row) = &session.turns[0] else {
            panic!("expected tool row");
        };
        assert!(row.done);
        assert_eq!(row.phase, None);
        assert!(row.summary_line().contains("compile ok"));
    }

    #[test]
    fn usage_accumulates_and_session_done_total_is_authoritative() {
        let mut session = session();
        session.apply_event(AgentEvent::TurnDone {
            stop_reason: lpa_agent::StopReason::ToolUse,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_write_tokens: 100,
                cache_read_tokens: 0,
            },
        });
        assert_eq!(session.usage.input_tokens, 10);
        session.apply_event(AgentEvent::SessionDone {
            usage_total: TokenUsage {
                input_tokens: 60,
                output_tokens: 40,
                cache_write_tokens: 120,
                cache_read_tokens: 90,
            },
        });
        assert_eq!(
            session.ui_usage(),
            UiAgentUsage {
                input_tokens: 60,
                output_tokens: 40,
                cache_write_tokens: 120,
                cache_read_tokens: 90,
            }
        );
    }

    #[test]
    fn provider_error_sets_status_and_run_ended_keeps_it() {
        let mut session = session();
        session.running = true;
        session.apply_event(AgentEvent::ProviderError {
            message: "401 unauthorized".into(),
            retryable: false,
        });
        session.run_ended(Some("401 unauthorized".into()));
        assert!(!session.running);
        assert_eq!(
            session.status,
            UiAgentStatus::Error {
                message: "401 unauthorized".into(),
                retryable: false,
            }
        );
        assert!(matches!(
            session.turns.last(),
            Some(UiAgentTurn::Notice { text }) if text.contains("401")
        ));
    }

    #[test]
    fn clean_run_end_returns_to_idle() {
        let mut session = session();
        session.running = true;
        session.apply_event(AgentEvent::TextDelta("done".into()));
        session.run_ended(None);
        assert_eq!(session.status, UiAgentStatus::Idle);
    }
}
