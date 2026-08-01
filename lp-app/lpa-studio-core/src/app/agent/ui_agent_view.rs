//! [`UiAgentView`]: the agent chat DTO decorated onto the shader editor.
//!
//! Carried on [`crate::UiAssetEditor::agent`] for GLSL inline editors, so
//! the web tab strip (Agent | Code) renders where the editor renders. Like
//! [`crate::UiAssetEditor`], the DTO prebuilds its actions so the view never
//! constructs domain ops.

use std::rc::Rc;

use lpc_model::ArtifactLocation;

use crate::app::agent::agent_controller::AgentController;
use crate::app::agent::agent_op::AgentOp;
use crate::app::settings::agent_provider::AgentProviderGuidance;
use crate::app::settings::ui_settings_view::UiModelOption;
use crate::{ControllerId, UiAction, UiNoticeLevel, UiProductPreview};

/// The agent chat state for one shader node's editor region.
#[derive(Clone, Debug, PartialEq)]
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
    /// The session's staged-edit history (oldest first): one entry per
    /// `iterate` call that staged source, with its preview thumbnail once
    /// the engine verdict resolved ok. The filmstrip above the composer.
    pub history: Vec<UiAgentHistoryEntry>,
    /// How many oldest history entries fell off the retention cap.
    pub history_dropped: u32,
    /// The session's model context for the footer chip (settings-derived).
    pub model: UiAgentModelView,
    /// The latest requested debug export (see [`UiAgentDebugDump`]);
    /// `None` until an export is requested.
    pub debug: Option<UiAgentDebugDump>,
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
            history: Vec::new(),
            history_dropped: 0,
            model: UiAgentModelView::default(),
            debug: None,
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

    /// The Revert action for one history entry: restage that edit's source
    /// through the normal overlay flow (confirm-less — Save-gated like any
    /// staged edit).
    pub fn revert_action(&self, turn: u32) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::RevertToTurn {
                artifact: self.artifact.clone(),
                turn,
            },
        )
    }

    /// The debug-export action: ask core to dump the model-facing
    /// transcript; the result lands on [`Self::debug`] with a fresh `seq`
    /// and the web shell downloads it. Idle-only (the raw transcript is
    /// parked in the controller only between runs).
    pub fn export_debug_action(&self) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::ExportDebug {
                artifact: self.artifact.clone(),
            },
        )
    }
}

/// One requested debug export: the raw model-facing transcript dump (wire
/// shapes, per-turn stop reasons and usage, provider + model — never the
/// API key), pretty-printed JSON. `seq` is session-monotonic; the web
/// shell downloads the dump exactly when it observes `seq` advance, so a
/// re-rendered stale DTO never re-downloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAgentDebugDump {
    pub seq: u64,
    pub json: Rc<str>,
}

/// One staged edit in the session's history filmstrip: a compact projection
/// of the core-side edit record (the full source stays in core — the view
/// reverts by turn number through [`UiAgentView::revert_action`]).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAgentHistoryEntry {
    /// Session-scoped edit ordinal (1-based, monotonic across the cap).
    pub turn: u32,
    /// The model's one-line intent for the call that staged this edit.
    pub note: Option<String>,
    /// 32×32 preview snapshot taken after the engine verdict resolved ok
    /// (`None` until a post-edit preview lands, or when the edit errored).
    pub thumb: Option<UiProductPreview>,
    /// Engine verdict for this edit (`None` while unresolved).
    pub engine_ok: Option<bool>,
}

/// The chat footer's model-chip slice (P10 item 4): the effective model
/// and the provider's discovered options, so the session's model shows —
/// and can switch — without opening the settings popover. A chip
/// selection dispatches the SAME settings mutation the popover's model
/// field uses ([`crate::SettingsCommand::SetAgentModel`]) and applies
/// from the NEXT run (providers are rebuilt at each run start); custom
/// free-text ids stay a settings-popover affair.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiAgentModelView {
    /// The effective model id (`None` while the provider requires one and
    /// none is configured — the chip then points at Settings).
    pub effective: Option<String>,
    /// The provider's fetched `/models` list (P8 store; empty until a
    /// fetch resolves — the chip requests one when the selector opens).
    pub options: Vec<UiModelOption>,
    /// A `/models` fetch is in flight for the selected provider.
    pub loading: bool,
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
    /// A streamed thinking/reasoning segment: visible while `done` is
    /// false (the model is working), collapsed to a one-line expander once
    /// it completes. Session-scoped only — never persisted.
    Thinking { text: String, done: bool },
    /// A tool call row (compact one-liner, expandable detail).
    Tool(UiAgentToolRow),
    /// A session-level notice (stopped, turn limit, provider error,
    /// truncated run). `level` picks the presentation: `Info` renders dim,
    /// `Warning` warning-toned (a run that ended incomplete).
    Notice { text: String, level: UiNoticeLevel },
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
    /// The staged edit's history ordinal ([`UiAgentHistoryEntry::turn`]),
    /// stamped when the call's edit record is pushed — the transcript's
    /// inline snapshot renders through this correlation (`None` for
    /// non-staging calls).
    pub edit_turn: Option<u32>,
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
            edit_turn: None,
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
        // Compile-less calls (e.g. `upsert_param`) contribute no compile
        // segment; their line is the note plus what actually happened.
        let mut segments: Vec<String> = Vec::new();
        match self.shader_ok {
            Some(true) => segments.push("compile ok".to_string()),
            Some(false) => segments.push("compile error".to_string()),
            None => {}
        }
        if self.staged {
            segments.push("staged edit".to_string());
        }
        if self.probes > 0 {
            segments.push(format!("{} probes", self.probes));
        }
        if self.warnings > 0 {
            segments.push(format!("{} warnings", self.warnings));
        }
        let outcome = if segments.is_empty() {
            "done".to_string()
        } else {
            segments.join(", ")
        };
        match &self.note {
            Some(note) => format!("{note} — {outcome}"),
            None => outcome,
        }
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
    fn revert_action_targets_the_record_by_turn() {
        let revert = view().revert_action(3);
        assert!(revert.is_for_node(AgentController::NODE_ID));
        assert_eq!(
            revert.op_as::<AgentOp>(),
            Some(&AgentOp::RevertToTurn {
                artifact: ArtifactLocation::file("/shader.glsl"),
                turn: 3,
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
