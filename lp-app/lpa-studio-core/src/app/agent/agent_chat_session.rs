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

use lpa_agent::{AgentEvent, AgentSession, EngineStatusKind, ModelProvider, TokenUsage};
use lpc_model::{ArtifactLocation, Revision};

use crate::UiProductPreview;
use crate::app::agent::agent_host_bridge::{AgentBridgeState, AgentHostBridge};
use crate::app::agent::agent_session_key::AgentSessionKey;
use crate::app::agent::ui_agent_view::{UiAgentStatus, UiAgentToolRow, UiAgentTurn, UiAgentUsage};

/// The concrete `lpa-agent` session type Studio drives: a runtime-chosen
/// provider behind a box, over the command-queue host bridge.
pub type AgentSessionRuntime = AgentSession<Box<dyn ModelProvider>, AgentHostBridge>;

/// Retention cap for [`AgentChatSession::edits`]: past it the oldest record
/// is dropped (and counted, so the UI can say so).
pub const MAX_EDIT_RECORDS: usize = 50;

/// One staged edit of this session, recorded when its `iterate` call
/// executed (the source is mirrored core-side at `ToolExecuted` — the
/// authoritative transcript travels with the run future, so the parked
/// runtime is never read).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentEditRecord {
    /// Session-scoped edit ordinal (1-based; monotonic across the cap).
    pub turn: u32,
    /// The call's one-line intent note.
    pub note: Option<String>,
    /// The staged shader source, verbatim (the revert payload).
    pub source: Rc<str>,
    /// Preview snapshot attached once the verdict resolved ok AND a
    /// post-edit preview landed (`None` until then, or on errors).
    pub thumb: Option<UiProductPreview>,
    /// Engine verdict for this edit (`None` while unresolved).
    pub engine_ok: Option<bool>,
    /// The record's anchor revision — the staleness guard: the engine
    /// status Revision observed when the record was pushed (normally the
    /// post-verdict frame, since `iterate` awaits the verdict in-call).
    /// Only previews with `revision >= at` may become the thumb; while
    /// `engine_ok` is unresolved, a status advance past `at` resolves it.
    pub at: Revision,
}

/// One shader node's conversation: view mirror + parked session runtime.
pub struct AgentChatSession {
    pub key: AgentSessionKey,
    /// The shader source artifact the session operates on (the decoration
    /// lookup identity).
    pub artifact: ArtifactLocation,
    /// The transcript mirror the DTO clones from.
    pub turns: Vec<UiAgentTurn>,
    /// Session edit history (oldest first, capped at
    /// [`MAX_EDIT_RECORDS`]) — the revert store and the filmstrip source.
    pub edits: Vec<AgentEditRecord>,
    /// How many oldest records fell off the cap.
    pub dropped_edits: u32,
    /// The next edit record's ordinal (1-based, never reused).
    next_edit_turn: u32,
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
            edits: Vec::new(),
            dropped_edits: 0,
            next_edit_turn: 1,
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
                }
                // Source-staging calls become edit records. `iterate` is
                // the ONLY source-staging tool (`upsert_param` also reports
                // `staged: true`, but for a def edit) — the name gate keeps
                // the bridge's staged-source queue correlated per call.
                if name == "iterate" && summary_json["staged"].as_bool().unwrap_or(false) {
                    self.push_edit_record(&summary_json);
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

    /// Record one staged edit at `ToolExecuted` time. The source comes off
    /// the bridge's staged-source queue (pushed by `stage_source`, popped
    /// here in call order — the authoritative transcript travels with the
    /// run future, so the bridge mirror is the only readable copy);
    /// `engine_ok` seeds from the summary's engine section when the
    /// in-call verdict already resolved. Past [`MAX_EDIT_RECORDS`] the
    /// oldest record drops (counted for the UI).
    fn push_edit_record(&mut self, summary_json: &serde_json::Value) {
        let (source, at) = {
            let mut bridge = self.bridge.borrow_mut();
            let staged = bridge.staged_sources.pop_front();
            let source = staged.unwrap_or_else(|| Rc::from(bridge.source.as_str()));
            let at = bridge
                .engine
                .as_ref()
                .map(|status| status.revision)
                .unwrap_or_default();
            (source, at)
        };
        let engine_ok = match summary_json["engine"]["status"].as_str() {
            Some("ok") => Some(true),
            Some("error") => Some(false),
            _ => None,
        };
        if self.edits.len() >= MAX_EDIT_RECORDS {
            self.edits.remove(0);
            self.dropped_edits += 1;
        }
        let turn = self.next_edit_turn;
        self.next_edit_turn += 1;
        self.edits.push(AgentEditRecord {
            turn,
            note: summary_json["note"].as_str().map(str::to_string),
            source,
            thumb: None,
            engine_ok,
            at,
        });
    }

    /// Advance the edit records from the shared engine cell plus the
    /// node's cached preview: unresolved verdicts resolve once the status
    /// Revision moves past the record's anchor; the NEWEST ok record
    /// without a thumb then adopts a preview rendered at-or-after its
    /// anchor (older records missed their window — backfilling them with
    /// a later preview would show the wrong look). Returns true when any
    /// record changed, so the caller re-emits the view.
    pub fn resolve_edit_outcomes(&mut self, preview: Option<&UiProductPreview>) -> bool {
        let mut changed = false;
        let engine = self.bridge.borrow().engine.clone();
        if let Some(status) = engine {
            for record in &mut self.edits {
                if record.engine_ok.is_none() && status.revision > record.at {
                    record.engine_ok = Some(status.verdict.status == EngineStatusKind::Ok);
                    record.at = status.revision;
                    changed = true;
                }
            }
        }
        if let Some(preview @ UiProductPreview::VisualSrgb8 { revision, .. }) = preview
            && let Some(record) = self
                .edits
                .iter_mut()
                .rev()
                .find(|record| record.engine_ok == Some(true))
            && record.thumb.is_none()
            && *revision >= record.at.as_i64()
        {
            record.thumb = Some(preview.clone());
            changed = true;
        }
        changed
    }

    /// The edit record with ordinal `turn`, when it is still retained.
    pub fn edit_record(&self, turn: u32) -> Option<&AgentEditRecord> {
        self.edits.iter().find(|record| record.turn == turn)
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

    /// Stage `source` on the bridge (queue + mirror, exactly what
    /// `stage_source` does) and apply the matching executed-iterate pair.
    fn stage_and_execute(
        session: &mut AgentChatSession,
        id: &str,
        source: &str,
        engine_status: Option<&str>,
    ) {
        {
            let mut bridge = session.bridge.borrow_mut();
            bridge.source = source.to_string();
            bridge.staged_sources.push_back(Rc::from(source));
        }
        let mut summary = json!({ "note": format!("edit {id}"), "staged": true });
        if let Some(status) = engine_status {
            summary["engine"] = json!({ "status": status });
        }
        session.apply_event(AgentEvent::ToolUseStart {
            id: id.into(),
            name: "iterate".into(),
        });
        session.apply_event(AgentEvent::ToolExecuted {
            id: id.into(),
            name: "iterate".into(),
            summary_json: summary,
        });
    }

    fn preview(revision: i64) -> UiProductPreview {
        UiProductPreview::VisualSrgb8 {
            width: 2,
            height: 2,
            revision,
            bytes: Rc::from([0u8; 12].as_slice()),
        }
    }

    fn engine_ok_at(session: &AgentChatSession, revision: i64) {
        session.bridge.borrow_mut().engine = Some(crate::AgentEngineStatus {
            revision: Revision::new(revision),
            verdict: lpa_agent::EngineVerdict {
                status: EngineStatusKind::Ok,
                message: None,
                line_col: None,
            },
        });
    }

    #[test]
    fn staged_iterate_calls_mirror_edit_records_in_call_order() {
        let mut session = session();
        // Both stages land on the bridge BEFORE either ToolExecuted is
        // applied — the batched-feedback shape the queue exists for.
        {
            let mut bridge = session.bridge.borrow_mut();
            bridge.source = "v2".to_string();
            bridge.staged_sources.push_back(Rc::from("v1"));
            bridge.staged_sources.push_back(Rc::from("v2"));
        }
        for (id, status) in [("tu_1", "ok"), ("tu_2", "error")] {
            session.apply_event(AgentEvent::ToolUseStart {
                id: id.into(),
                name: "iterate".into(),
            });
            session.apply_event(AgentEvent::ToolExecuted {
                id: id.into(),
                name: "iterate".into(),
                summary_json: json!({
                    "note": format!("note {id}"), "staged": true,
                    "engine": { "status": status },
                }),
            });
        }

        assert_eq!(session.edits.len(), 2);
        assert_eq!(session.edits[0].turn, 1);
        assert_eq!(&*session.edits[0].source, "v1");
        assert_eq!(session.edits[0].note.as_deref(), Some("note tu_1"));
        assert_eq!(session.edits[0].engine_ok, Some(true));
        assert_eq!(session.edits[1].turn, 2);
        assert_eq!(&*session.edits[1].source, "v2");
        assert_eq!(session.edits[1].engine_ok, Some(false));
        assert!(session.bridge.borrow().staged_sources.is_empty());
    }

    #[test]
    fn only_source_staging_iterate_calls_become_records() {
        let mut session = session();
        // `upsert_param` reports `staged: true` too (a def edit) — it must
        // neither record nor consume the staged-source queue.
        session
            .bridge
            .borrow_mut()
            .staged_sources
            .push_back(Rc::from("v1"));
        session.apply_event(AgentEvent::ToolExecuted {
            id: "tu_1".into(),
            name: "upsert_param".into(),
            summary_json: json!({ "staged": true, "engine": { "status": "ok" } }),
        });
        // An unstaged iterate (probe-only call) records nothing either.
        session.apply_event(AgentEvent::ToolExecuted {
            id: "tu_2".into(),
            name: "iterate".into(),
            summary_json: json!({ "staged": false }),
        });
        assert!(session.edits.is_empty());
        assert_eq!(session.bridge.borrow().staged_sources.len(), 1);
    }

    #[test]
    fn unresolved_verdicts_resolve_when_the_status_revision_advances() {
        let mut session = session();
        stage_and_execute(&mut session, "tu_1", "v1", None);
        assert_eq!(session.edits[0].engine_ok, None);

        // Same anchor revision: nothing resolves yet.
        assert!(!session.resolve_edit_outcomes(None));

        engine_ok_at(&session, 5);
        assert!(session.resolve_edit_outcomes(None));
        assert_eq!(session.edits[0].engine_ok, Some(true));
        assert_eq!(session.edits[0].at, Revision::new(5), "anchor advances");
        // Settled records stay settled.
        assert!(!session.resolve_edit_outcomes(None));
    }

    #[test]
    fn thumbs_attach_revision_guarded_on_the_newest_ok_record() {
        let mut session = session();
        engine_ok_at(&session, 10);
        stage_and_execute(&mut session, "tu_1", "v1", Some("ok"));
        stage_and_execute(&mut session, "tu_2", "v2", Some("ok"));
        assert_eq!(session.edits[0].at, Revision::new(10));

        // A preview older than the anchor is pre-edit — never the thumb.
        assert!(!session.resolve_edit_outcomes(Some(&preview(9))));
        assert!(session.edits.iter().all(|record| record.thumb.is_none()));

        // A fresh preview lands on the NEWEST ok record only (the older
        // record's engine state was never rendered on its own).
        assert!(session.resolve_edit_outcomes(Some(&preview(10))));
        assert!(session.edits[0].thumb.is_none());
        assert_eq!(session.edits[1].thumb, Some(preview(10)));
        // Attached thumbs are not overwritten by later frames.
        assert!(!session.resolve_edit_outcomes(Some(&preview(11))));
        assert_eq!(session.edits[1].thumb, Some(preview(10)));
    }

    #[test]
    fn errored_edits_never_take_a_thumb() {
        let mut session = session();
        engine_ok_at(&session, 10);
        stage_and_execute(&mut session, "tu_1", "v1", Some("error"));
        assert!(!session.resolve_edit_outcomes(Some(&preview(20))));
        assert!(session.edits[0].thumb.is_none());
    }

    #[test]
    fn edit_records_cap_drops_the_oldest_and_keeps_numbering() {
        let mut session = session();
        for index in 0..(MAX_EDIT_RECORDS + 2) {
            stage_and_execute(&mut session, &format!("tu_{index}"), "src", Some("ok"));
        }
        assert_eq!(session.edits.len(), MAX_EDIT_RECORDS);
        assert_eq!(session.dropped_edits, 2);
        assert_eq!(session.edits.first().map(|record| record.turn), Some(3));
        assert_eq!(
            session.edits.last().map(|record| record.turn),
            Some(MAX_EDIT_RECORDS as u32 + 2)
        );
        assert!(session.edit_record(1).is_none(), "dropped records are gone");
        assert!(session.edit_record(3).is_some());
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
