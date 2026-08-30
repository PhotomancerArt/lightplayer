//! The project-shaped opening frame — and what it says while it waits.
//!
//! Shown while the route says a project and the actor's view hasn't
//! reached it yet (a card click, a boot reopen, a forward-button reopen):
//! the URL's intent picks the frame, so the gallery never flashes on a
//! project reload.
//!
//! # Honest states (P6, D4)
//!
//! Before P6 this was a skeleton and nothing else, which made a FAILED
//! open indistinguishable from a slow one: the route never matched the
//! view, so the skeleton pulsed forever and only a reload got out. The
//! frame now narrates the real pipeline and ends every open in one of
//! three places — open, [`OpeningState::Failed`] with a working Retry, or
//! superseded by a newer click.
//!
//! The narration is POLLED rather than pushed, because the studio actor
//! is parked inside the open for the whole of it: nothing is emitted
//! between "the click landed" and "the project is up". The three sources
//! are all page-thread signals ([`OpenProbe::read`]):
//!
//! - `lpa_link`'s engine cache — download bytes and compile;
//! - `lpa_link`'s boot wait — the studio worker's boot phase;
//! - `lpa_studio_core`'s open signals — the core's own milestone, whether
//!   an open is in flight at all, and the terminal failure with its Retry
//!   action; plus `lpa_fs_opfs`'s lock waits for the rare "blocked on a
//!   background sync" state.
//!
//! Labels are DEBOUNCED ([`OpeningLabel`]): a fast open passes through
//! three of these states in under a frame, and strobing them would read as
//! a glitch rather than as progress. A state has to hold for
//! [`LABEL_HOLD_TICKS`] polls before it takes the label, so a fast open
//! shows the calm skeleton and nothing else. A failure never waits.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::{ActionPriority, OpenStage, UiAction};

use crate::core::solid_action_class;
use crate::router::StudioRoute;

/// How often the frame re-reads the platform's open signals.
const POLL_INTERVAL_MS: u32 = 75;

/// How many consecutive polls a new state must survive before it replaces
/// the displayed label — the ~150 ms debounce.
const LABEL_HOLD_TICKS: u8 = 2;

/// What the frame is narrating right now.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum OpeningState {
    /// Nothing specific to say (yet): the calm skeleton. Where every fast
    /// open begins and ends.
    #[default]
    Opening,
    /// Fetching the engine binary. `total_bytes` is `None` when the
    /// response declared no usable length — then there is no percentage to
    /// show, only motion.
    DownloadingEngine {
        received_bytes: f64,
        total_bytes: Option<f64>,
    },
    /// The engine is in hand and coming up (compile, instantiate, GPU,
    /// runtime).
    StartingEngine { phase: EnginePhase },
    /// The engine is up; the project is being read and deployed onto it.
    PreparingProject,
    /// The project is momentarily locked by a background cloud sync trip.
    /// Rare, short, and worth naming — it used to surface as "this project
    /// is open in another tab" with one tab open.
    WaitingForSync,
    /// The open ended. `message` is the mapped `UiError` wording; `retry`
    /// runs the same open again.
    Failed { message: String, retry: UiAction },
}

/// A phase of bringing the engine up, in the boot protocol's own
/// vocabulary (`docs/adr/2026-08-14-browser-worker-boot-protocol-v2.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnginePhase {
    /// The downloaded binary is being compiled page-side.
    Compiling,
    /// The worker has started and is setting itself up.
    Booting,
    /// The worker is instantiating the engine module.
    Instantiating,
    /// The worker is asking the browser for a GPU device.
    GpuInit,
    /// The engine runtime itself is being created.
    RuntimeCreate,
}

impl EnginePhase {
    /// The boot protocol's status word, mapped to a phase. Unknown words
    /// (a future protocol addition) read as the generic `Booting` rather
    /// than showing a raw wire token.
    fn from_status(status: &str) -> Option<Self> {
        match status {
            "ready" | "error" => None,
            "instantiating" => Some(Self::Instantiating),
            "gpu-init" => Some(Self::GpuInit),
            "runtime-create" => Some(Self::RuntimeCreate),
            _ => Some(Self::Booting),
        }
    }

    /// What the user reads. Plain english about the machine's actual work
    /// — never the wire word.
    pub fn label(self) -> &'static str {
        match self {
            Self::Compiling => "Preparing the engine…",
            Self::Booting => "Starting the engine…",
            Self::Instantiating => "Loading the engine…",
            Self::GpuInit => "Setting up graphics…",
            Self::RuntimeCreate => "Starting the simulator…",
        }
    }
}

impl OpeningState {
    /// The headline this state shows, or `None` for the calm skeleton.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::Opening => None,
            Self::DownloadingEngine {
                received_bytes,
                total_bytes,
            } => Some(match total_bytes {
                Some(total) if *total > 0.0 => format!(
                    "Downloading the engine… {}%",
                    ((received_bytes / total) * 100.0).clamp(0.0, 100.0).round()
                ),
                _ => format!(
                    "Downloading the engine… {:.1} MB",
                    received_bytes / 1_048_576.0
                ),
            }),
            Self::StartingEngine { phase } => Some(phase.label().to_string()),
            Self::PreparingProject => Some("Preparing the project…".to_string()),
            Self::WaitingForSync => Some("Waiting for a background sync to finish…".to_string()),
            Self::Failed { .. } => Some("This project did not open".to_string()),
        }
    }

    /// Completion, 0.0–1.0, when the state knows one. Only the engine
    /// download does; everything else is a phase, not a quantity, and a
    /// made-up bar is worse than none.
    pub fn fraction(&self) -> Option<f64> {
        match self {
            Self::DownloadingEngine {
                received_bytes,
                total_bytes: Some(total),
            } if *total > 0.0 => Some((received_bytes / total).clamp(0.0, 1.0)),
            _ => None,
        }
    }

    /// Which state this is, ignoring its payload — the unit the label
    /// debounce works in, so download bytes can tick freely without
    /// re-arming it.
    fn kind(&self) -> u8 {
        match self {
            Self::Opening => 0,
            Self::DownloadingEngine { .. } => 1,
            Self::StartingEngine { .. } => 2,
            Self::PreparingProject => 3,
            Self::WaitingForSync => 4,
            Self::Failed { .. } => 5,
        }
    }
}

/// One reading of every signal the open pipeline publishes.
///
/// A plain struct so the state machine below is a pure function of it —
/// testable on the host, where none of the browser signals exist.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct OpenProbe {
    /// A user-initiated open is in flight (`user_open_in_flight`).
    pub in_flight: bool,
    /// The core's own milestone for that open.
    pub stage: OpenStage,
    /// Engine download progress: `(received, total?)` while fetching.
    pub engine_download: Option<(f64, Option<f64>)>,
    /// The downloaded engine is being compiled page-side.
    pub engine_compiling: bool,
    /// The studio worker's boot-phase status word, while it is booting.
    pub boot_status: Option<String>,
    /// A project lock this tab wants is held by a sync trip right now.
    pub project_lock_contended: bool,
}

impl OpenProbe {
    /// Read every signal, now. Browser-only sources read as absent on the
    /// host, where the pure state machine is what the tests exercise.
    pub fn read() -> Self {
        Self {
            in_flight: lpa_studio_core::user_open_in_flight(),
            stage: lpa_studio_core::open_stage(),
            ..Self::read_platform()
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn read_platform() -> Self {
        use lpa_link::providers::browser_worker::{
            EngineAssetPhase, STUDIO_RUNTIME_WORKER_LABEL, engine_asset_phase, worker_boot_phase,
        };

        let engine = engine_asset_phase();
        Self {
            engine_download: match &engine {
                EngineAssetPhase::Fetching {
                    received_bytes,
                    total_bytes,
                } => Some((*received_bytes, *total_bytes)),
                _ => None,
            },
            engine_compiling: matches!(engine, EngineAssetPhase::Compiling),
            // The studio's OWN worker only: a preview-pool member booting
            // beside the click is the gallery's business, not the frame's.
            boot_status: worker_boot_phase()
                .filter(|phase| phase.label == STUDIO_RUNTIME_WORKER_LABEL)
                .map(|phase| phase.status),
            project_lock_contended: !lpa_fs_opfs::projects_awaiting_lock().is_empty(),
            ..Self::default()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_platform() -> Self {
        Self::default()
    }
}

/// The open pipeline's state machine: one probe in, one honest state out.
///
/// Ordered most-specific first, and deliberately never guesses. A phase is
/// reported only while something is actually doing it; anything else falls
/// through to the calm skeleton rather than inventing a stage.
pub fn opening_state(probe: &OpenProbe) -> OpeningState {
    // A failure outlives its open: nothing else is in flight by then, and
    // this is the state that replaced the eternal skeleton.
    if let OpenStage::Failed(failure) = &probe.stage {
        return OpeningState::Failed {
            message: failure.message.clone(),
            retry: failure.retry.clone(),
        };
    }
    if !probe.in_flight {
        // A project route with no open running: a boot reopen whose action
        // has not been dispatched yet, or the moment after a supersede.
        return OpeningState::Opening;
    }
    if let Some((received_bytes, total_bytes)) = probe.engine_download {
        return OpeningState::DownloadingEngine {
            received_bytes,
            total_bytes,
        };
    }
    if probe.engine_compiling {
        return OpeningState::StartingEngine {
            phase: EnginePhase::Compiling,
        };
    }
    if let Some(phase) = probe
        .boot_status
        .as_deref()
        .and_then(EnginePhase::from_status)
    {
        return OpeningState::StartingEngine { phase };
    }
    if probe.stage == OpenStage::PreparingProject {
        // The lock wait only means something once the project IS the work:
        // a sync trip polling while the engine downloads is not what this
        // open is waiting on.
        if probe.project_lock_contended {
            return OpeningState::WaitingForSync;
        }
        return OpeningState::PreparingProject;
    }
    OpeningState::Opening
}

/// The label debounce: a state has to hold for [`LABEL_HOLD_TICKS`] polls
/// before it replaces what is on screen.
///
/// Payload changes within the displayed state (download bytes) apply
/// immediately — the debounce is about the LABEL, and a percentage that
/// refused to move would be its own kind of lie.
#[derive(Debug, Default)]
pub struct OpeningLabel {
    shown: OpeningState,
    pending: Option<(OpeningState, u8)>,
}

impl OpeningLabel {
    /// Fold one observation in and return what to render.
    pub fn observe(&mut self, next: OpeningState) -> OpeningState {
        if next.kind() == self.shown.kind() {
            self.shown = next;
            self.pending = None;
            return self.shown.clone();
        }
        // An error is never held back: the user is already waiting.
        if matches!(next, OpeningState::Failed { .. }) {
            self.shown = next;
            self.pending = None;
            return self.shown.clone();
        }
        let ticks = match self.pending.take() {
            Some((pending, ticks)) if pending.kind() == next.kind() => ticks.saturating_add(1),
            _ => 1,
        };
        if ticks >= LABEL_HOLD_TICKS {
            self.shown = next;
        } else {
            self.pending = Some((next, ticks));
        }
        self.shown.clone()
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectOpeningFrame(
    /// Render this state instead of polling the live signals — the story
    /// page's seam, and what makes every state reviewable.
    #[props(default)]
    state: Option<OpeningState>,
    /// Where Retry dispatches. Absent in stories, where the button is
    /// present but inert.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let forced = state.clone();
    let mut polled = use_signal(OpeningState::default);
    // One poll loop per mount. It only runs where the frame does — a
    // project route whose project is not up — and stops with it.
    use_future(move || {
        let forced = forced.clone();
        async move {
            if forced.is_some() {
                return;
            }
            let mut label = OpeningLabel::default();
            loop {
                let next = label.observe(opening_state(&OpenProbe::read()));
                // The read's borrow is scoped to this `let` on purpose:
                // held across `set` it would be a runtime borrow panic.
                let changed = *polled.peek() != next;
                if changed {
                    polled.set(next);
                }
                TimeoutFuture::new(POLL_INTERVAL_MS).await;
            }
        }
    });
    let shown = state.unwrap_or_else(|| polled.read().clone());

    if let OpeningState::Failed { message, retry } = &shown {
        return rsx! {
            OpenFailureNotice { message: message.clone(), retry: retry.clone(), on_action }
        };
    }

    rsx! {
        section { class: "tw:grid tw:gap-3.5",
            div { class: "tw:grid tw:gap-2",
                div { class: "tw:flex tw:items-center tw:gap-3",
                    span { class: "tw:h-2.5 tw:w-2.5 tw:animate-pulse tw:rounded-full tw:bg-status-working-foreground" }
                    p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-muted-foreground",
                        {shown.label().unwrap_or_else(|| "Opening project…".to_string())}
                    }
                }
                // A bar only where a real quantity exists (the engine
                // download). Every other phase gets the pulsing dot above,
                // which claims nothing it cannot know.
                if let Some(fraction) = shown.fraction() {
                    div {
                        class: "tw:h-1 tw:w-full tw:max-w-[420px] tw:overflow-hidden tw:rounded-pill tw:bg-card-subtle",
                        role: "progressbar",
                        aria_valuemin: "0",
                        aria_valuemax: "100",
                        aria_valuenow: "{(fraction * 100.0).round()}",
                        div {
                            class: "tw:h-full tw:rounded-pill tw:transition-[width] ux-iri-fill-static",
                            style: "width: {(fraction * 100.0).round()}%;",
                        }
                    }
                }
            }
            // a rough silhouette of the editor's three-column layout
            div { class: "tw:grid tw:animate-pulse tw:grid-cols-[minmax(220px,280px)_minmax(0,1fr)_minmax(300px,360px)] tw:gap-3.5 tw:max-[960px]:grid-cols-1",
                div { class: skeleton_class(), style: "height: 180px;" }
                div { class: "tw:grid tw:content-start tw:gap-3.5",
                    div { class: skeleton_class(), style: "height: 120px;" }
                    div { class: skeleton_class(), style: "height: 220px;" }
                }
                div { class: skeleton_class(), style: "height: 180px;" }
            }
        }
    }
}

/// The opening pipeline at card size: the state label plus the engine
/// download's bar, for the card whose own open is running.
///
/// An example opened from Explore never reaches a `/p/` route until the
/// open completes, so the full [`ProjectOpeningFrame`] never shows for
/// it — on a slow connection the whole engine download would pass behind
/// a static "Opening…" (the G1 finding). This line is the same probe,
/// state machine, and debounce, rendered where that open actually lives:
/// on the card. Failed states render as the calm fallback here — the
/// grid-level [`OpenFailureNotice`] owns failure, and the card's
/// `opening` flag clears with it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OpeningProgressLine() -> Element {
    let mut polled = use_signal(OpeningState::default);
    // One poll loop per mount — only the single opening card mounts one,
    // and it stops when the open settles and the card re-renders idle.
    use_future(move || async move {
        let mut label = OpeningLabel::default();
        loop {
            let next = label.observe(opening_state(&OpenProbe::read()));
            let changed = *polled.peek() != next;
            if changed {
                polled.set(next);
            }
            TimeoutFuture::new(POLL_INTERVAL_MS).await;
        }
    });
    let shown = polled.read().clone();
    let label = match &shown {
        OpeningState::Failed { .. } => None,
        state => state.label(),
    };

    rsx! {
        div { class: "tw:grid tw:gap-1",
            p { class: "tw:m-0 tw:text-xs tw:text-status-working-foreground",
                {label.unwrap_or_else(|| "Opening…".to_string())}
            }
            if let Some(fraction) = shown.fraction() {
                div {
                    class: "tw:h-0.5 tw:w-full tw:overflow-hidden tw:rounded-pill tw:bg-card-subtle",
                    role: "progressbar",
                    aria_valuemin: "0",
                    aria_valuemax: "100",
                    aria_valuenow: "{(fraction * 100.0).round()}",
                    div {
                        class: "tw:h-full tw:rounded-pill tw:transition-[width] ux-iri-fill-static",
                        style: "width: {(fraction * 100.0).round()}%;",
                    }
                }
            }
        }
    }
}

/// The dead end, with both ways out of it.
///
/// Split into its own component for two reasons. Retry's handler is then
/// built from props that do NOT change while the frame re-renders at poll
/// cadence — a closure rebuilt every 75 ms is a click waiting to be
/// dropped. And an example opened from Explore never reaches a `/p/`
/// route, so it has no opening frame to fail inside: the grid renders
/// this same notice instead, and no open ends in silence.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OpenFailureNotice(
    message: String,
    retry: UiAction,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let explore = StudioRoute::Explore.path();
    rsx! {
        section { class: "tw:grid tw:max-w-[560px] tw:gap-3.5",
            div { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-4",
                p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-status-error-foreground",
                    "This project did not open"
                }
                p { class: "tw:m-0 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                    "{message}"
                }
            }
            div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2.5",
                button {
                    r#type: "button",
                    class: solid_action_class(ActionPriority::Secondary),
                    onclick: move |_| {
                        if let Some(on_action) = on_action {
                            on_action.call(retry.clone());
                        }
                    },
                    "Retry"
                }
                a {
                    class: "tw:text-sm tw:text-muted-foreground tw:no-underline tw:hover:text-strong-foreground",
                    href: "{explore}",
                    "Back to Explore"
                }
            }
        }
    }
}

fn skeleton_class() -> &'static str {
    "tw:rounded-md tw:border tw:border-border tw:bg-card"
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{ControllerId, HOME_NODE_ID, HomeOp, OpenFailure};

    fn retry_action() -> UiAction {
        UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenExample {
                id: "examples/fyeah-sign".to_string(),
            },
        )
    }

    fn opening() -> OpenProbe {
        OpenProbe {
            in_flight: true,
            stage: OpenStage::Starting,
            ..OpenProbe::default()
        }
    }

    #[test]
    fn a_route_with_no_open_running_stays_a_calm_skeleton() {
        assert_eq!(
            opening_state(&OpenProbe::default()),
            OpeningState::Opening,
            "a boot reopen has nothing to narrate yet"
        );
        assert_eq!(OpeningState::Opening.label(), None);
    }

    #[test]
    fn the_engine_download_is_the_one_state_with_a_percentage() {
        let state = opening_state(&OpenProbe {
            engine_download: Some((5_242_880.0, Some(10_485_760.0))),
            ..opening()
        });
        assert_eq!(state.fraction(), Some(0.5));
        assert_eq!(state.label().unwrap(), "Downloading the engine… 50%");

        // A content-encoded response declares no length: motion, no bar.
        let indeterminate = opening_state(&OpenProbe {
            engine_download: Some((5_242_880.0, None)),
            ..opening()
        });
        assert_eq!(indeterminate.fraction(), None);
        assert_eq!(
            indeterminate.label().unwrap(),
            "Downloading the engine… 5.0 MB"
        );
    }

    #[test]
    fn boot_phases_read_as_work_not_as_wire_words() {
        for (status, expected) in [
            ("booting", "Starting the engine…"),
            ("instantiating", "Loading the engine…"),
            ("gpu-init", "Setting up graphics…"),
            ("runtime-create", "Starting the simulator…"),
            // an unknown future phase must never leak the raw token
            ("warming-caches", "Starting the engine…"),
        ] {
            let state = opening_state(&OpenProbe {
                boot_status: Some(status.to_string()),
                ..opening()
            });
            assert_eq!(state.label().as_deref(), Some(expected), "{status}");
        }
        // `ready` is not a wait — the boot is over.
        assert_eq!(
            opening_state(&OpenProbe {
                boot_status: Some("ready".to_string()),
                ..opening()
            }),
            OpeningState::Opening
        );
    }

    #[test]
    fn waiting_for_sync_needs_the_project_to_actually_be_the_work() {
        // Contention while the engine still downloads is somebody else's
        // sync, not what this click is blocked on.
        assert!(matches!(
            opening_state(&OpenProbe {
                engine_download: Some((1.0, Some(2.0))),
                project_lock_contended: true,
                ..opening()
            }),
            OpeningState::DownloadingEngine { .. }
        ));
        assert_eq!(
            opening_state(&OpenProbe {
                stage: OpenStage::PreparingProject,
                project_lock_contended: true,
                ..opening()
            }),
            OpeningState::WaitingForSync
        );
        assert_eq!(
            opening_state(&OpenProbe {
                stage: OpenStage::PreparingProject,
                ..opening()
            }),
            OpeningState::PreparingProject
        );
    }

    #[test]
    fn a_failure_outlives_its_open_and_carries_its_retry() {
        // The eternal-skeleton case: the open is over, nothing is in
        // flight, and the route still does not match the view.
        let state = opening_state(&OpenProbe {
            in_flight: false,
            stage: OpenStage::Failed(OpenFailure {
                message: "the simulator did not connect".to_string(),
                retry: retry_action(),
            }),
            ..OpenProbe::default()
        });
        let OpeningState::Failed { message, retry } = state else {
            panic!("a finished failure must not fall back to the skeleton");
        };
        assert_eq!(message, "the simulator did not connect");
        assert_eq!(retry, retry_action());
    }

    #[test]
    fn a_fast_open_never_strobes_its_labels() {
        // Each phase lasts one poll — the fast path. Nothing but the calm
        // skeleton should ever reach the screen.
        let mut label = OpeningLabel::default();
        let shown = [
            OpeningState::DownloadingEngine {
                received_bytes: 1.0,
                total_bytes: Some(2.0),
            },
            OpeningState::StartingEngine {
                phase: EnginePhase::Booting,
            },
            OpeningState::PreparingProject,
        ]
        .map(|state| label.observe(state));
        assert!(
            shown.iter().all(|state| *state == OpeningState::Opening),
            "{shown:?}"
        );
    }

    #[test]
    fn a_state_that_holds_takes_the_label_and_then_updates_freely() {
        let mut label = OpeningLabel::default();
        let downloading = |received: f64| OpeningState::DownloadingEngine {
            received_bytes: received,
            total_bytes: Some(100.0),
        };
        assert_eq!(label.observe(downloading(10.0)), OpeningState::Opening);
        assert_eq!(label.observe(downloading(20.0)), downloading(20.0));
        // Payload movement is not a label change: the percentage ticks
        // every poll without re-arming the debounce.
        assert_eq!(label.observe(downloading(30.0)), downloading(30.0));
        assert_eq!(label.observe(downloading(40.0)), downloading(40.0));
    }

    #[test]
    fn a_failure_is_never_held_back_by_the_debounce() {
        let mut label = OpeningLabel::default();
        let failed = OpeningState::Failed {
            message: "engine wasm fetch/compile failed".to_string(),
            retry: retry_action(),
        };
        assert_eq!(label.observe(failed.clone()), failed);
    }
}
