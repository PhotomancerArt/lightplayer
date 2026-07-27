//! [`UiAgentView`]: the agent chat DTO decorated onto the shader editor.
//!
//! Carried on [`crate::UiAssetEditor::agent`] for GLSL inline editors, so
//! the web tab strip (Agent | Code) renders where the editor renders. Like
//! [`crate::UiAssetEditor`], the DTO prebuilds its actions so the view never
//! constructs domain ops.

use lpc_model::ArtifactLocation;

use crate::app::agent::agent_controller::AgentController;
use crate::app::agent::agent_op::AgentOp;
use crate::app::settings::agent_provider::AgentProviderGuidance;
use crate::{ControllerId, UiAction};

/// The agent chat state for one shader node's editor region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentView {
    /// The shader source artifact this chat operates on (the send/stop
    /// target identity).
    pub artifact: ArtifactLocation,
    /// Whether the agent can run at all (the selected provider is
    /// sufficiently configured).
    pub availability: UiAgentAvailability,
    /// Onboarding guidance for the selected provider, populated while
    /// [`Self::availability`] is `NeedsKey` (the empty state renders it —
    /// same copy the settings popover shows, sourced once in core).
    pub setup: Option<AgentProviderGuidance>,
    /// Live run status.
    pub status: UiAgentStatus,
    /// The rendered conversation.
    pub turns: Vec<UiAgentTurn>,
    /// Cumulative token usage for this session.
    pub usage: UiAgentUsage,
    /// Display-ready cost estimate for [`Self::usage`] (e.g. `~$0.0042`),
    /// when rates are known (pricing table or settings overrides).
    pub estimated_cost: Option<String>,
}

impl UiAgentView {
    /// The no-conversation view for a shader without a session yet.
    pub fn empty(artifact: ArtifactLocation, availability: UiAgentAvailability) -> Self {
        Self {
            artifact,
            availability,
            setup: None,
            status: UiAgentStatus::Idle,
            turns: Vec::new(),
            usage: UiAgentUsage::default(),
            estimated_cost: None,
        }
    }

    /// True while a run is in flight (streaming text or executing a tool).
    pub fn busy(&self) -> bool {
        matches!(
            self.status,
            UiAgentStatus::Streaming | UiAgentStatus::RunningTool
        )
    }

    /// The Send action for one composed message.
    pub fn send_action(&self, text: &str) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::Send {
                artifact: self.artifact.clone(),
                text: text.to_string(),
            },
        )
    }

    /// The Stop action for the running turn.
    pub fn stop_action(&self) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::Stop {
                artifact: self.artifact.clone(),
            },
        )
    }
}

/// Whether the agent can run (settings-derived; Ready ⇔ the SELECTED
/// provider is sufficiently configured — Anthropic: key; OpenAI: key +
/// model; Custom: base URL + model).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAgentAvailability {
    /// The provider is configured; the Agent tab is the default tab.
    Ready,
    /// Setup incomplete; the chat shows the provider's setup guidance
    /// (carried on [`UiAgentView::setup`]).
    NeedsKey,
}

/// Live status of the session's current (or last) run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAgentStatus {
    /// No run in flight; sending is possible.
    Idle,
    /// A model turn is streaming.
    Streaming,
    /// A tool call is executing (probe evaluation runs synchronously).
    RunningTool,
    /// The last run failed; `retryable` distinguishes transient failures.
    Error { message: String, retryable: bool },
}

/// One rendered transcript entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAgentTurn {
    /// A user message.
    User { text: String },
    /// Assistant text (one contiguous streamed block).
    Assistant { text: String },
    /// A tool call row (compact one-liner, expandable detail).
    Tool(UiAgentToolRow),
    /// A session-level notice (stopped, turn limit, provider error).
    Notice { text: String },
}

/// Compact projection of one `iterate` call for the tool row. Derived from
/// the tool's summary JSON; the raw experiment JSON stays in core/debug.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentToolRow {
    /// Provider tool-use id (row identity for updates and expansion).
    pub id: String,
    /// The model's one-line intent for this call.
    pub note: Option<String>,
    /// Live activity label while executing ("compiling", "probe 2/5", …);
    /// cleared when the call finishes.
    pub phase: Option<String>,
    /// False while the call is still executing.
    pub done: bool,
    /// Whether the call staged new source (an overlay edit landed).
    pub staged: bool,
    /// Compile outcome, once done (`None` until then or on tool errors).
    pub shader_ok: Option<bool>,
    /// Number of probes evaluated.
    pub probes: u32,
    /// Number of warnings returned.
    pub warnings: u32,
    /// Tool/host failure message, when the call failed.
    pub error: Option<String>,
    /// Pretty-printed summary JSON for the expanded detail view.
    pub detail: String,
}

impl UiAgentToolRow {
    /// A just-started row (input still streaming, tool not yet executed).
    pub fn started(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            note: None,
            phase: None,
            done: false,
            staged: false,
            shader_ok: None,
            probes: 0,
            warnings: 0,
            error: None,
            detail: String::new(),
        }
    }

    /// The compact one-line summary the collapsed row shows.
    pub fn summary_line(&self) -> String {
        if !self.done {
            // Live activity: prefer the current phase ("compiling",
            // "probe 2/5", …) over the generic "running".
            let activity = self.phase.as_deref().unwrap_or("running");
            return match &self.note {
                Some(note) => format!("{note} — {activity}"),
                None if self.phase.is_some() => format!("Experiment — {activity}"),
                None => "Running experiment".to_string(),
            };
        }
        if let Some(error) = &self.error {
            return format!("Tool failed: {error}");
        }
        let compile = match self.shader_ok {
            Some(true) => "compile ok",
            Some(false) => "compile error",
            None => "no compile",
        };
        let mut line = match &self.note {
            Some(note) => format!("{note} — {compile}"),
            None => compile.to_string(),
        };
        if self.staged {
            line.push_str(", staged edit");
        }
        if self.probes > 0 {
            line.push_str(&format!(", {} probes", self.probes));
        }
        if self.warnings > 0 {
            line.push_str(&format!(", {} warnings", self.warnings));
        }
        line
    }
}

/// Cumulative token usage (mirrors `lpa_agent::TokenUsage`, kept `Eq`).
/// The buckets are disjoint: `input_tokens` is the *uncached* prompt
/// remainder; the cache buckets are prompt tokens written to / served
/// from the provider prompt cache (a breakdown the footnote tooltip can
/// surface).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiAgentUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_write_tokens: u32,
    pub cache_read_tokens: u32,
}

impl UiAgentUsage {
    /// Total prompt tokens processed (fresh + cache writes + cache reads)
    /// — the "in" number a user expects, since the buckets are disjoint.
    pub fn total_input_tokens(&self) -> u32 {
        self.input_tokens + self.cache_write_tokens + self.cache_read_tokens
    }

    /// True when no tokens have been counted at all.
    pub fn is_zero(&self) -> bool {
        self.total_input_tokens() == 0 && self.output_tokens == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> UiAgentView {
        UiAgentView::empty(
            ArtifactLocation::file("/shader.glsl"),
            UiAgentAvailability::Ready,
        )
    }

    #[test]
    fn send_and_stop_actions_target_the_agent_controller() {
        let send = view().send_action("make it green");
        assert!(send.is_for_node(AgentController::NODE_ID));
        assert_eq!(
            send.op_as::<AgentOp>(),
            Some(&AgentOp::Send {
                artifact: ArtifactLocation::file("/shader.glsl"),
                text: "make it green".to_string(),
            })
        );

        let stop = view().stop_action();
        assert_eq!(
            stop.op_as::<AgentOp>(),
            Some(&AgentOp::Stop {
                artifact: ArtifactLocation::file("/shader.glsl"),
            })
        );
    }

    #[test]
    fn busy_tracks_run_states_only() {
        let mut view = view();
        assert!(!view.busy());
        view.status = UiAgentStatus::Streaming;
        assert!(view.busy());
        view.status = UiAgentStatus::RunningTool;
        assert!(view.busy());
        view.status = UiAgentStatus::Error {
            message: "boom".into(),
            retryable: true,
        };
        assert!(!view.busy());
    }

    #[test]
    fn running_row_prefers_the_live_phase() {
        let mut row = UiAgentToolRow::started("tu_1");
        assert_eq!(row.summary_line(), "Running experiment");
        row.phase = Some("probe 2/5".into());
        assert_eq!(row.summary_line(), "Experiment — probe 2/5");
        row.note = Some("go green".into());
        assert_eq!(row.summary_line(), "go green — probe 2/5");
    }

    #[test]
    fn tool_row_summary_line_compacts_the_outcome() {
        let mut row = UiAgentToolRow::started("tu_1");
        row.note = Some("go green".into());
        assert_eq!(row.summary_line(), "go green — running");

        row.done = true;
        row.staged = true;
        row.shader_ok = Some(true);
        row.probes = 2;
        assert_eq!(
            row.summary_line(),
            "go green — compile ok, staged edit, 2 probes"
        );

        row.error = Some("host refused".into());
        assert_eq!(row.summary_line(), "Tool failed: host refused");
    }
}
