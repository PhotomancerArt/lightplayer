//! [`AgentController`]: owns the per-shader agent chat sessions.
//!
//! Lives on the [`StudioController`](crate::StudioController) like the
//! console/settings sub-state. Runs are spawned through an injected spawner
//! (the wasm shell passes `spawn_local`; tests collect and drive futures
//! manually) and report progress back as [`AgentFeedback`] commands.

use core::future::Future;
use core::pin::Pin;
use core::time::Duration;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use lpa_agent::{AgentError, AgentSession, ModelProvider, ShaderContext};
use lpc_model::ArtifactLocation;
use lps_probe::LedPoint;

use crate::app::agent::agent_chat_session::AgentChatSession;
use crate::app::agent::agent_feedback::AgentFeedback;
use crate::app::agent::agent_host_bridge::AgentHostBridge;
use crate::app::agent::agent_pricing::{AgentCostRates, format_cost_usd};
use crate::app::agent::agent_provider_config::AgentProviderConfig;
use crate::app::agent::agent_session_key::AgentSessionKey;
use crate::app::agent::ui_agent_view::{UiAgentAvailability, UiAgentUsage, UiAgentView};
use crate::app::settings::agent_provider::AgentProviderGuidance;
use crate::app::studio::studio_view_channel::CommandSender;
use crate::{
    ProjectEditorView, RuntimeId, StudioCommand, UiAssetEditorKind, UiConfigSlot, UiConfigSlotBody,
    UiNodeChild, UiNodeSection, UiNodeTabBody,
};

/// A spawned agent-run future (`!Send`; driven by the platform's
/// single-threaded executor).
pub type AgentTaskFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// One platform timer future for the host bridge's engine-verdict polls.
pub type AgentTimerFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// The timer factory the bridge polls on (the actor's `make_timer`, boxed;
/// installed by `StudioActor::new`).
pub type AgentTimerFactory = Rc<RefCell<dyn FnMut(Duration) -> AgentTimerFuture>>;

/// An instantly-resolving timer factory: the default before the actor
/// installs the platform one, and what most tests want (verdict polls spin
/// through their budget without wall-clock waits).
pub fn instant_agent_timer() -> AgentTimerFactory {
    Rc::new(RefCell::new(|_delay| {
        Box::pin(core::future::ready(())) as AgentTimerFuture
    }))
}

/// A platform model-list fetch (P8): resolved by the injected fetcher over
/// the crate's GET transport, driven by the same spawner as run futures.
pub type AgentModelsFetchFuture =
    Pin<Box<dyn Future<Output = Result<Vec<lpa_agent::ModelInfo>, lpa_agent::ListModelsError>>>>;

/// The settings-derived slice the view decoration needs, computed once per
/// view build by the studio controller.
#[derive(Clone, Debug)]
pub struct AgentViewContext {
    /// Whether the selected provider is sufficiently configured.
    pub availability: UiAgentAvailability,
    /// The selected provider's onboarding guidance, carried while
    /// `availability` is `NeedsKey` (the chat empty state renders it).
    pub setup: Option<AgentProviderGuidance>,
    /// Cost rates for the usage estimate (`None` ⇒ tokens only).
    pub cost_rates: Option<AgentCostRates>,
    /// The model-chip slice (effective model + discovered options) the
    /// chat footer renders.
    pub model: crate::UiAgentModelView,
}

/// Everything a run start needs beyond the message text: the refreshed host
/// bridge snapshot, gathered by the studio controller from project state.
pub struct AgentRunContext {
    /// Overlay-aware shader source at run start.
    pub source: String,
    /// Union of all fixtures' mapping points, labeled by fixture name.
    pub led_points: Vec<LedPoint>,
    /// System-prompt context (names, fixture summary, bindings).
    pub context: ShaderContext,
}

/// The agent sub-controller: session map + injected platform facilities.
#[derive(Default)]
pub struct AgentController {
    sessions: BTreeMap<AgentSessionKey, AgentChatSession>,
    /// Platform spawner for run futures (wasm: `spawn_local`).
    spawner: Option<Rc<dyn Fn(AgentTaskFuture)>>,
    /// Builds a fresh provider from the effective settings at each run
    /// start (so provider/key/model changes apply without restarting
    /// sessions).
    provider_factory: Option<Rc<dyn Fn(&AgentProviderConfig) -> Box<dyn ModelProvider>>>,
    /// Builds a model-list fetch from discovery credentials (P8; the web
    /// shell wraps `lpa_agent::list_*_models` over the fetch transport).
    models_fetcher: Option<Rc<dyn Fn(&AgentProviderConfig) -> AgentModelsFetchFuture>>,
    /// The actor's command sender, installed at actor construction; run
    /// futures report progress and stage edits through it.
    command_tx: Option<CommandSender>,
    /// The platform timer factory the host bridge's engine-verdict wait
    /// polls on (installed by `StudioActor::new`; `None` ⇒ instant timers).
    timer: Option<AgentTimerFactory>,
}

impl AgentController {
    /// The controller id agent chat actions target.
    pub const NODE_ID: &'static str = "studio|agent";

    pub fn new() -> Self {
        Self::default()
    }

    /// Install the platform spawner (before the actor takes ownership).
    pub fn set_spawner(&mut self, spawner: impl Fn(AgentTaskFuture) + 'static) {
        self.spawner = Some(Rc::new(spawner));
    }

    /// Install the provider factory (before the actor takes ownership).
    pub fn set_provider_factory(
        &mut self,
        factory: impl Fn(&AgentProviderConfig) -> Box<dyn ModelProvider> + 'static,
    ) {
        self.provider_factory = Some(Rc::new(factory));
    }

    /// Install the model-list fetcher (before the actor takes ownership).
    pub fn set_models_fetcher(
        &mut self,
        fetcher: impl Fn(&AgentProviderConfig) -> AgentModelsFetchFuture + 'static,
    ) {
        self.models_fetcher = Some(Rc::new(fetcher));
    }

    /// Spawn one model-list fetch for `config`, reporting back through the
    /// command queue as [`SettingsCommand::ModelsLoaded`]. Returns `false`
    /// when any platform facility (fetcher, spawner, sender) is missing —
    /// the caller then leaves no in-flight marker behind.
    pub(crate) fn spawn_models_fetch(
        &self,
        provider: crate::AgentProvider,
        fingerprint: String,
        config: &AgentProviderConfig,
    ) -> bool {
        let (Some(fetcher), Some(spawner), Some(tx)) = (
            self.models_fetcher.as_ref(),
            self.spawner.as_ref(),
            self.command_tx.clone(),
        ) else {
            return false;
        };
        let future = fetcher(config);
        let task: AgentTaskFuture = Box::pin(async move {
            let result = future.await;
            tx.send(StudioCommand::Settings(
                crate::SettingsCommand::ModelsLoaded {
                    provider,
                    fingerprint,
                    result,
                },
            ));
        });
        (spawner)(task);
        true
    }

    /// Install the actor's command sender (done by `StudioActor::new`).
    pub(crate) fn set_command_sender(&mut self, tx: CommandSender) {
        self.command_tx = Some(tx);
    }

    /// Install the platform timer factory (done by `StudioActor::new`,
    /// boxed from its `make_timer`).
    pub fn set_timer(&mut self, timer: impl FnMut(Duration) -> AgentTimerFuture + 'static) {
        self.timer = Some(Rc::new(RefCell::new(timer)));
    }

    /// Refresh every session's shared bridge cell from the project
    /// controller's per-artifact lookups: engine status (the verdict-wait
    /// seam) and def-side param records (the params-diff seam), then let
    /// the session settle its edit-history records against the fresh cell
    /// plus the node's cached visual preview (P4: verdict resolution +
    /// revision-guarded thumbnail attach). Called after every processed
    /// actor batch so a running agent observes status advances and def
    /// changes the pulls bring in. Returns true when any history record
    /// changed — the caller marks the view dirty, since preview advances
    /// alone need not move the sync revision the change gate watches.
    pub(crate) fn refresh_engine_status(
        &mut self,
        resolve: impl Fn(&ArtifactLocation) -> Option<crate::AgentEngineStatus>,
        params: impl Fn(&ArtifactLocation) -> Option<Vec<lpa_agent::ParamDefRecord>>,
        preview: impl Fn(&ArtifactLocation) -> Option<crate::UiProductPreview>,
    ) -> bool {
        let mut history_changed = false;
        for session in self.sessions.values_mut() {
            {
                let mut bridge = session.bridge.borrow_mut();
                if let Some(status) = resolve(&session.artifact) {
                    bridge.engine = Some(status);
                }
                if let Some(defs) = params(&session.artifact) {
                    bridge.params = Some(defs);
                }
            }
            history_changed |= session.resolve_edit_outcomes(preview(&session.artifact).as_ref());
        }
        history_changed
    }

    /// Record the outcome of one dispatched `UpsertParam` batch into the
    /// artifact's session bridge cell (`rejection` = joined rejection text
    /// when any command was refused). The awaiting run future polls it up
    /// by `seq`.
    pub(crate) fn record_write_ack(
        &mut self,
        artifact: &ArtifactLocation,
        seq: u64,
        rejection: Option<String>,
    ) {
        for session in self.sessions.values_mut() {
            if &session.artifact == artifact {
                session.bridge.borrow_mut().write_ack = Some((seq, rejection.clone()));
            }
        }
    }

    /// Build a provider from the current effective settings, when a factory
    /// is installed.
    pub(crate) fn build_provider(
        &self,
        config: &AgentProviderConfig,
    ) -> Option<Box<dyn ModelProvider>> {
        self.provider_factory
            .as_ref()
            .map(|factory| factory(config))
    }

    /// Fold one feedback message from a spawned run into its session.
    /// Unknown keys are ignored (a session map is never rebuilt mid-run, so
    /// this only covers feedback racing a future reset feature).
    pub fn apply_feedback(&mut self, feedback: AgentFeedback) {
        match feedback {
            AgentFeedback::Event { key, event } => {
                if let Some(session) = self.sessions.get_mut(&key) {
                    session.apply_event(event);
                }
            }
            AgentFeedback::RunEnded { key, error } => {
                if let Some(session) = self.sessions.get_mut(&key) {
                    session.run_ended(error);
                }
            }
        }
    }

    /// The recorded source for a revert: the lens runtime's session for
    /// `artifact`, its record with ordinal `turn`. Refused (with the
    /// user-facing reason) when the session is missing, a run is in
    /// flight (the agent would race the restage), or the record fell off
    /// the retention cap.
    pub(crate) fn revert_source(
        &self,
        runtime: Option<RuntimeId>,
        artifact: &ArtifactLocation,
        turn: u32,
    ) -> Result<std::rc::Rc<str>, String> {
        let session = runtime
            .and_then(|runtime| {
                self.sessions
                    .values()
                    .find(|session| session.key.runtime == runtime && &session.artifact == artifact)
            })
            .ok_or_else(|| "no agent session for this shader".to_string())?;
        if session.running {
            return Err("a run is in flight — stop it before reverting".to_string());
        }
        session
            .edit_record(turn)
            .map(|record| std::rc::Rc::clone(&record.source))
            .ok_or_else(|| format!("no edit record for turn {turn} (it may have been dropped)"))
    }

    /// Build the debug export for one session and embed it in the mirror
    /// (see [`crate::UiAgentDebugDump`]). Idle-only: the raw model-facing
    /// transcript lives in the parked runtime, which is moved into the run
    /// future while a run is in flight.
    pub(crate) fn export_debug(
        &mut self,
        runtime: Option<RuntimeId>,
        artifact: &ArtifactLocation,
        config: Option<&crate::AgentProviderConfig>,
    ) -> Result<(), String> {
        let session = self
            .session_for_mut(runtime, artifact)
            .ok_or_else(|| "no agent chat for this shader yet".to_string())?;
        if session.running {
            return Err("export waits for the run to finish".to_string());
        }
        let json = {
            let parked = session.runtime.borrow();
            let agent = parked
                .as_ref()
                .ok_or_else(|| "nothing to export yet — send a message first".to_string())?;
            crate::app::agent::agent_debug_export::debug_dump_json(
                artifact.file_path().as_str(),
                config,
                agent.transcript(),
                &session.turn_stats,
                &session.edits,
            )
        };
        session.set_debug_dump(json);
        Ok(())
    }

    /// Settle a successfully dispatched revert: mirror the restaged source
    /// into the session's bridge state (so the next run's `current_source`
    /// agrees before the async apply round-trips) and append the visible
    /// transcript notice.
    pub(crate) fn record_revert(
        &mut self,
        runtime: Option<RuntimeId>,
        artifact: &ArtifactLocation,
        turn: u32,
        source: &str,
    ) {
        if let Some(session) = self.session_for_mut(runtime, artifact) {
            session.bridge.borrow_mut().source = source.to_string();
            session.push_notice(format!(
                "Reverted to turn {turn} — that edit's source is staged again (Save keeps it)."
            ));
        }
    }

    /// Flip the abort flag of the running session for `artifact` (Stop).
    /// The run settles asynchronously (Aborted → SessionDone → RunEnded).
    pub fn request_stop(&mut self, runtime: Option<RuntimeId>, artifact: &ArtifactLocation) {
        if let Some(session) = self.session_for_mut(runtime, artifact)
            && session.running
        {
            session.abort.store(true, Ordering::Relaxed);
        }
    }

    /// Start one run: refresh the bridge snapshot, push the user turn, and
    /// spawn the session future. Returns an error string (surfaced as a
    /// dispatch error) when the platform facilities are missing.
    pub(crate) fn start_run(
        &mut self,
        key: AgentSessionKey,
        artifact: ArtifactLocation,
        text: String,
        provider: Box<dyn ModelProvider>,
        ctx: AgentRunContext,
    ) -> Result<(), String> {
        let Some(tx) = self.command_tx.clone() else {
            return Err("agent command channel not installed".to_string());
        };
        let Some(spawner) = self.spawner.clone() else {
            return Err("agent spawner not installed".to_string());
        };

        let entry = self
            .sessions
            .entry(key.clone())
            .or_insert_with(|| AgentChatSession::new(key.clone(), artifact.clone()));
        if entry.running {
            entry.push_notice("A run is already in progress — stop it first.");
            return Ok(());
        }

        {
            let mut bridge = entry.bridge.borrow_mut();
            bridge.artifact = Some(artifact);
            bridge.source = ctx.source;
            bridge.led_points = ctx.led_points;
            bridge.context = ctx.context;
            // Drop stage/event correlation leftovers from an aborted run.
            bridge.staged_sources.clear();
        }

        let mut session = match entry.runtime.borrow_mut().take() {
            Some(mut session) => {
                // Fresh provider each run: settings changes (key, model)
                // apply without losing the conversation.
                session.set_provider(provider);
                session
            }
            None => AgentSession::new(
                provider,
                AgentHostBridge::new(
                    Rc::clone(&entry.bridge),
                    tx.clone(),
                    self.timer.clone().unwrap_or_else(instant_agent_timer),
                ),
            ),
        };
        entry.abort = session.abort_handle();
        entry.running = true;
        // A new run invalidates any embedded export (the raw transcript it
        // came from is about to grow); the web shell only downloads on a
        // fresh `seq`, so dropping it here just trims the DTO.
        entry.debug_dump = None;
        entry.status = crate::UiAgentStatus::Streaming;
        entry
            .turns
            .push(crate::UiAgentTurn::User { text: text.clone() });

        let slot = Rc::clone(&entry.runtime);
        let event_tx = tx.clone();
        let event_key = key.clone();
        let future: AgentTaskFuture = Box::pin(async move {
            let result = session
                .run(text, move |event| {
                    event_tx.send(StudioCommand::Agent(AgentFeedback::Event {
                        key: event_key.clone(),
                        event,
                    }));
                })
                .await;
            // Park the runtime BEFORE announcing the end, so a follow-up
            // Send in the same batch as `RunEnded` finds it.
            slot.borrow_mut().replace(session);
            let error = match result {
                Ok(()) => None,
                Err(AgentError::Provider { message }) => Some(message),
            };
            tx.send(StudioCommand::Agent(AgentFeedback::RunEnded { key, error }));
        });
        (spawner)(future);
        Ok(())
    }

    /// Decorate every GLSL inline editor in the project editor DTO with its
    /// agent chat view (existing session state, or the empty chat).
    pub fn decorate_editor_view(
        &self,
        editor: &mut ProjectEditorView,
        runtime: Option<RuntimeId>,
        ctx: &AgentViewContext,
    ) {
        for node in &mut editor.nodes {
            for tab in &mut node.tabs {
                if let UiNodeTabBody::Sections(sections) = &mut tab.body {
                    self.decorate_sections(sections, runtime, ctx);
                }
            }
            self.decorate_face(node.face.as_mut(), runtime, ctx);
            self.decorate_children(&mut node.children, runtime, ctx);
        }
    }

    fn decorate_children(
        &self,
        children: &mut [UiNodeChild],
        runtime: Option<RuntimeId>,
        ctx: &AgentViewContext,
    ) {
        for child in children {
            self.decorate_sections(&mut child.sections, runtime, ctx);
            self.decorate_face(child.face.as_mut(), runtime, ctx);
            self.decorate_children(&mut child.children, runtime, ctx);
        }
    }

    /// Attach the agent chat DTO to a shader card's face (node-card P3):
    /// the face's `agent` section hosts the SAME chat the inline editor's
    /// Agent tab carries, keyed by the code drawer's artifact. The code
    /// drawer itself stays undecorated — inside the face, the chat lives in
    /// its own section, not on a tab strip.
    fn decorate_face(
        &self,
        face: Option<&mut crate::UiNodeFace>,
        runtime: Option<RuntimeId>,
        ctx: &AgentViewContext,
    ) {
        let Some(crate::UiNodeFace::Shader(shader)) = face else {
            return;
        };
        let Some(editor) = &shader.code_drawer else {
            return;
        };
        if editor.kind != UiAssetEditorKind::Glsl {
            return;
        }
        shader.agent = Some(self.view_for(runtime, &editor.artifact, ctx));
    }

    fn decorate_sections(
        &self,
        sections: &mut [UiNodeSection],
        runtime: Option<RuntimeId>,
        ctx: &AgentViewContext,
    ) {
        for section in sections {
            match section {
                UiNodeSection::AssetSlots(slots)
                | UiNodeSection::ConfigSlots(slots)
                | UiNodeSection::DebugSlots(slots) => {
                    self.decorate_slots(slots, runtime, ctx);
                }
                _ => {}
            }
        }
    }

    fn decorate_slots(
        &self,
        slots: &mut [UiConfigSlot],
        runtime: Option<RuntimeId>,
        ctx: &AgentViewContext,
    ) {
        for slot in slots {
            match &mut slot.body {
                UiConfigSlotBody::Asset(asset) => {
                    if let Some(editor) = &mut asset.inline_editor
                        && editor.kind == UiAssetEditorKind::Glsl
                    {
                        editor.agent = Some(self.view_for(runtime, &editor.artifact, ctx));
                    }
                }
                UiConfigSlotBody::Record(record) => {
                    self.decorate_slots(&mut record.fields, runtime, ctx);
                }
                _ => {}
            }
        }
    }

    /// The chat DTO for one shader editor: the lens runtime's session for
    /// this artifact when one exists, the empty chat otherwise.
    fn view_for(
        &self,
        runtime: Option<RuntimeId>,
        artifact: &ArtifactLocation,
        ctx: &AgentViewContext,
    ) -> UiAgentView {
        let session = runtime.and_then(|runtime| {
            self.sessions
                .values()
                .find(|session| session.key.runtime == runtime && &session.artifact == artifact)
        });
        let (status, turns, usage, history, history_dropped, debug) = match session {
            Some(session) => (
                session.status.clone(),
                session.turns.clone(),
                session.ui_usage(),
                session
                    .edits
                    .iter()
                    .map(|record| crate::UiAgentHistoryEntry {
                        turn: record.turn,
                        note: record.note.clone(),
                        thumb: record.thumb.clone(),
                        engine_ok: record.engine_ok,
                    })
                    .collect(),
                session.dropped_edits,
                session.debug_dump.clone(),
            ),
            None => (
                crate::UiAgentStatus::Idle,
                Vec::new(),
                UiAgentUsage::default(),
                Vec::new(),
                0,
                None,
            ),
        };
        UiAgentView {
            artifact: artifact.clone(),
            availability: ctx.availability,
            setup: ctx.setup,
            status,
            turns,
            usage,
            estimated_cost: estimated_cost(usage, ctx.cost_rates),
            history,
            history_dropped,
            model: ctx.model.clone(),
            debug,
        }
    }

    fn session_for_mut(
        &mut self,
        runtime: Option<RuntimeId>,
        artifact: &ArtifactLocation,
    ) -> Option<&mut AgentChatSession> {
        let runtime = runtime?;
        self.sessions
            .values_mut()
            .find(|session| session.key.runtime == runtime && &session.artifact == artifact)
    }
}

/// Display-ready cost estimate for a session's cumulative usage: `None`
/// without rates (unknown model, no overrides) or before any usage.
fn estimated_cost(usage: UiAgentUsage, rates: Option<AgentCostRates>) -> Option<String> {
    let rates = rates?;
    if usage.is_zero() {
        return None;
    }
    Some(format!("~{}", format_cost_usd(rates.estimate_usd(usage))))
}
