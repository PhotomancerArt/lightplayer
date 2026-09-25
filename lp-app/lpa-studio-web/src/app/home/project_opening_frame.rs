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
//! # A board (2026-09-24)
//!
//! An open onto a board has no engine to narrate; it has the board. The
//! core's [`OpenStage::WaitingForDevice`] and [`OpenStage::OnDevice`] say
//! which board and which wire step, the upload carries real byte counts,
//! and a step that has held for [`STALL_NOTE_SECS`] says how long. Every
//! board state has a way out on the page itself — Cancel, Reset the board,
//! and for a board this page has no port for, "Connect this board": the
//! `requestPort()` gesture a fresh page in Brave needs before it can reach
//! a board at all (Yona, 2026-09-24, JSON Pack sitting).
//!
//! Labels are DEBOUNCED ([`OpeningLabel`]): a fast open passes through
//! three of these states in under a frame, and strobing them would read as
//! a glitch rather than as progress. A state has to hold for
//! [`LABEL_HOLD_TICKS`] polls before it takes the label, so a fast open
//! shows the calm skeleton and nothing else. A failure never waits.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::{
    AccessCommand, ActionPriority, DeviceAction, DeviceOpenProgress, DeviceOpenStep, DeviceWait,
    DeviceWaitReason, DevicesOp, OpenDevice, OpenStage, RuntimeOp, UiAction,
};

use crate::app::home::access_ui_context::access_handler;
use crate::core::{quiet_action_class, solid_action_class};
use crate::router::StudioRoute;

/// How often the frame re-reads the platform's open signals.
const POLL_INTERVAL_MS: u32 = 75;

/// How many consecutive polls a new state must survive before it replaces
/// the displayed label — the ~150 ms debounce.
const LABEL_HOLD_TICKS: u8 = 2;

/// How long one board step may hold before the frame says how long it has
/// been. Shorter than the 20 s request deadline on purpose: the person
/// should read "still loading, 12 s" before they read a failure.
const STALL_NOTE_SECS: u64 = 8;

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
    /// The open is held on a board that is not ready for it.
    WaitingForDevice(DeviceWait),
    /// The project is going onto a board, one wire step at a time.
    OnDevice(DeviceOpenProgress),
    /// The open ended. `message` is the mapped `UiError` wording; `retry`
    /// runs the same open again; `device` is the board it was on, if any;
    /// `needs_unlock` when the board refused the link's tier, so the way
    /// on is Unlock rather than a Reset.
    Failed {
        message: String,
        retry: UiAction,
        device: Option<OpenDevice>,
        needs_unlock: bool,
    },
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
            Self::RuntimeCreate => "Starting the sim…",
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
            Self::WaitingForDevice(wait) => Some(wait_label(wait)),
            Self::OnDevice(progress) => Some(step_label(progress)),
            Self::Failed { .. } => Some("This project did not open".to_string()),
        }
    }

    /// The board this state is about, when it is about one.
    pub fn device(&self) -> Option<&OpenDevice> {
        match self {
            Self::WaitingForDevice(wait) => Some(&wait.device),
            Self::OnDevice(progress) => Some(&progress.device),
            Self::Failed { device, .. } => device.as_ref(),
            _ => None,
        }
    }

    /// The unit the stall clock runs in: the state AND, on a board, its
    /// step — an upload that is still moving is not a stall, but the byte
    /// count changing is not a new step either.
    fn stall_key(&self) -> (u8, u8) {
        let step = match self {
            Self::WaitingForDevice(wait) => wait.reason.clone() as u8,
            Self::OnDevice(progress) => match progress.step {
                DeviceOpenStep::Connecting => 0,
                DeviceOpenStep::Clearing => 1,
                DeviceOpenStep::Uploading { .. } => 2,
                DeviceOpenStep::Loading => 3,
                DeviceOpenStep::Reading => 4,
            },
            _ => 0,
        };
        (self.kind(), step)
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
            Self::OnDevice(DeviceOpenProgress {
                step:
                    DeviceOpenStep::Uploading {
                        sent_bytes,
                        total_bytes,
                    },
                ..
            }) if *total_bytes > 0 => {
                Some((*sent_bytes as f64 / *total_bytes as f64).clamp(0.0, 1.0))
            }
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
            Self::WaitingForDevice(_) => 6,
            Self::OnDevice(_) => 7,
        }
    }
}

fn wait_label(wait: &DeviceWait) -> String {
    let name = &wait.device.name;
    match wait.reason {
        DeviceWaitReason::NotConnected => format!("{name} is not connected to this page"),
        DeviceWaitReason::PortClosed => format!("{name}'s port is closed"),
        DeviceWaitReason::Identifying => format!("Waiting for {name} to answer…"),
        DeviceWaitReason::Busy => format!("Waiting for {name} to finish what it is doing…"),
        DeviceWaitReason::Unknown => format!("Looking for {name}…"),
    }
}

fn step_label(progress: &DeviceOpenProgress) -> String {
    let name = &progress.device.name;
    match progress.step {
        DeviceOpenStep::Connecting => format!("Connecting to {name}…"),
        DeviceOpenStep::Clearing => format!("Clearing {name}'s old project…"),
        DeviceOpenStep::Uploading {
            sent_bytes,
            total_bytes,
        } => format!(
            "Sending the project to {name}… {} of {}",
            human_bytes(sent_bytes),
            human_bytes(total_bytes)
        ),
        DeviceOpenStep::Loading => format!("Loading on {name} — compiling its shaders…"),
        DeviceOpenStep::Reading => format!("Reading the project back from {name}…"),
    }
}

/// What a board state explains under its label, when it has more to say.
fn device_detail(state: &OpeningState) -> Option<&'static str> {
    match state {
        OpeningState::WaitingForDevice(wait) => match wait.reason {
            DeviceWaitReason::NotConnected => Some(
                "A page may only use a USB board after you pick it, and some browsers (Brave) \
                 forget that answer on every reload. Connect it and this project opens on it.",
            ),
            DeviceWaitReason::PortClosed => {
                Some("The board is plugged in but this page closed its port.")
            }
            DeviceWaitReason::Identifying => Some(
                "It is connected but has not said hello yet. A board that just reset says it \
                 within a few seconds; one that stays quiet may need a reset.",
            ),
            DeviceWaitReason::Busy | DeviceWaitReason::Unknown => None,
        },
        OpeningState::OnDevice(DeviceOpenProgress {
            step: DeviceOpenStep::Loading,
            ..
        }) => Some("The board compiles every shader itself; a large project takes a while."),
        _ => None,
    }
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
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
            device: failure.device.clone(),
            needs_unlock: failure.needs_unlock,
        };
    }
    // A held open parks nothing — the actor is free while the board is
    // away — so it is read before `in_flight`, which a hold never sets.
    if let OpenStage::WaitingForDevice(wait) = &probe.stage {
        return OpeningState::WaitingForDevice(wait.clone());
    }
    if let OpenStage::OnDevice(progress) = &probe.stage {
        return OpeningState::OnDevice(progress.clone());
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
    /// How long the shown step has held, for a story to pose the stall
    /// note; live frames measure it themselves.
    #[props(default)]
    stalled_secs: Option<u64>,
    /// Where Retry dispatches. Absent in stories, where the button is
    /// present but inert.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let forced = state.clone();
    let mut polled = use_signal(OpeningState::default);
    let mut held_secs = use_signal(|| 0u64);
    // One poll loop per mount. It only runs where the frame does — a
    // project route whose project is not up — and stops with it.
    use_future(move || {
        let forced = forced.clone();
        async move {
            if forced.is_some() {
                return;
            }
            let mut label = OpeningLabel::default();
            let mut stall = StallClock::default();
            loop {
                let next = label.observe(opening_state(&OpenProbe::read()));
                let secs = stall.observe(next.stall_key(), now_ms());
                // The read's borrow is scoped to this `let` on purpose:
                // held across `set` it would be a runtime borrow panic.
                let changed = *polled.peek() != next;
                if changed {
                    polled.set(next);
                }
                if *held_secs.peek() != secs {
                    held_secs.set(secs);
                }
                TimeoutFuture::new(POLL_INTERVAL_MS).await;
            }
        }
    });
    let shown = state.unwrap_or_else(|| polled.read().clone());
    let held = stalled_secs.unwrap_or_else(|| *held_secs.read());

    if let OpeningState::Failed {
        message,
        retry,
        device,
        needs_unlock,
    } = &shown
    {
        return rsx! {
            OpenFailureNotice {
                message: message.clone(),
                retry: retry.clone(),
                device: device.clone(),
                needs_unlock: *needs_unlock,
                on_action,
            }
        };
    }

    let stall_note = match (&shown, held >= STALL_NOTE_SECS) {
        // Waiting on the PERSON (a click, a cable) is not a stall.
        (OpeningState::WaitingForDevice(wait), true)
            if !matches!(
                wait.reason,
                DeviceWaitReason::NotConnected | DeviceWaitReason::PortClosed
            ) =>
        {
            Some(format!("Waiting {held} s so far."))
        }
        (OpeningState::OnDevice(progress), true) => Some(format!(
            "Still {} — {held} s on this step.",
            progress.step.doing()
        )),
        _ => None,
    };

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
                // download, a board's upload). Every other phase gets the
                // pulsing dot above, which claims nothing it cannot know.
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
                if let Some(detail) = device_detail(&shown) {
                    p { class: "tw:m-0 tw:max-w-[560px] tw:text-xs tw:leading-normal tw:text-muted-foreground",
                        "{detail}"
                    }
                }
                if let Some(note) = stall_note {
                    p { class: "tw:m-0 tw:text-xs tw:text-status-warning-foreground", "{note}" }
                }
                if shown.device().is_some() {
                    DeviceOpenExits { state: shown.clone(), on_action }
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

/// The ways off a board open that is taking too long, or never started.
///
/// Its own component for the reason [`OpenFailureNotice`] is: the handlers
/// are built from props, not from a closure the poll loop rebuilds.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DeviceOpenExits(state: OpeningState, on_action: Option<EventHandler<UiAction>>) -> Element {
    let Some(device) = state.device().cloned() else {
        return rsx! {};
    };
    let connect = match &state {
        OpeningState::WaitingForDevice(wait) => match (&wait.reason, device.id) {
            // The chooser: `requestPort()` rides this click's activation.
            (DeviceWaitReason::NotConnected, Some(id)) => {
                Some(DevicesOp::action_for(DeviceAction::Reconnect {
                    device: id,
                }))
            }
            (DeviceWaitReason::NotConnected, None) => {
                Some(DevicesOp::action_for(DeviceAction::AddFromUsb))
            }
            (DeviceWaitReason::PortClosed, Some(id)) => {
                Some(DevicesOp::action_for(DeviceAction::Connect { device: id }))
            }
            _ => None,
        },
        _ => None,
    };
    // A reset needs a wire: a board with no port has nothing to pulse.
    let reset = match &state {
        OpeningState::WaitingForDevice(wait)
            if matches!(
                wait.reason,
                DeviceWaitReason::NotConnected | DeviceWaitReason::Unknown
            ) =>
        {
            None
        }
        _ => device
            .id
            .map(|id| DevicesOp::action_for(DeviceAction::ResetBoard { device: id })),
    };
    let cancel_and = move |then: Option<UiAction>| {
        // Wake the parked request first, so the actor is free for what the
        // click queues next.
        lpa_studio_core::cancel_open();
        if let Some(on_action) = on_action {
            on_action.call(UiAction::from_op(RuntimeOp::NODE_ID, RuntimeOp::CancelOpen));
            if let Some(then) = then {
                on_action.call(then);
            }
        }
        crate::router::navigate_push(&StudioRoute::Devices);
    };
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2.5 tw:pt-1",
            if let Some(connect) = connect {
                button {
                    r#type: "button",
                    class: solid_action_class(ActionPriority::Primary),
                    onclick: move |_| {
                        if let Some(on_action) = on_action {
                            on_action.call(connect.clone());
                        }
                    },
                    "Connect this board"
                }
            }
            if let Some(reset) = reset {
                button {
                    r#type: "button",
                    class: solid_action_class(ActionPriority::Secondary),
                    title: "Stop opening, reset the board's hardware, and go to Devices.",
                    onclick: move |_| cancel_and(Some(reset.clone())),
                    "Reset the board"
                }
            }
            button {
                r#type: "button",
                class: quiet_action_class(),
                title: "Stop opening this project and go to Devices.",
                onclick: move |_| cancel_and(None),
                "Cancel"
            }
        }
    }
}

/// How long the shown step has held, in whole seconds.
#[derive(Debug, Default)]
struct StallClock {
    key: Option<(u8, u8)>,
    since_ms: f64,
}

impl StallClock {
    fn observe(&mut self, key: (u8, u8), now_ms: f64) -> u64 {
        if self.key != Some(key) {
            self.key = Some(key);
            self.since_ms = now_ms;
        }
        ((now_ms - self.since_ms).max(0.0) / 1000.0) as u64
    }
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    0.0
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
    /// The board the open failed on: the notice then offers to reset it,
    /// and the way back is Devices rather than Explore.
    #[props(default)]
    device: Option<OpenDevice>,
    /// The board refused the link's tier (a Bluetooth link unlocked for
    /// play): the notice offers Unlock instead of a Reset, because the
    /// board is fine.
    #[props(default)]
    needs_unlock: bool,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let (back_href, back_label) = match device {
        Some(_) => (StudioRoute::Devices.path(), "Back to devices"),
        None => (StudioRoute::Explore.path(), "Back to Explore"),
    };
    let board = device.as_ref().and_then(|device| device.id);
    let unlock = board.filter(|_| needs_unlock);
    let reset = board
        .filter(|_| !needs_unlock)
        .map(|id| DevicesOp::action_for(DeviceAction::ResetBoard { device: id }));
    let on_access = access_handler();
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
                if let Some(device) = unlock {
                    button {
                        r#type: "button",
                        class: solid_action_class(ActionPriority::Secondary),
                        title: "Unlock with an edit password; Retry once it is unlocked.",
                        onclick: move |_| on_access.call(AccessCommand::LogIn { device }),
                        "Unlock"
                    }
                }
                if let Some(reset) = reset {
                    button {
                        r#type: "button",
                        class: solid_action_class(ActionPriority::Secondary),
                        title: "Reset the board's hardware; Retry once it says hello again.",
                        onclick: move |_| {
                            if let Some(on_action) = on_action {
                                on_action.call(reset.clone());
                            }
                        },
                        "Reset the board"
                    }
                }
                a {
                    class: "tw:text-sm tw:text-muted-foreground tw:no-underline tw:hover:text-strong-foreground",
                    href: "{back_href}",
                    "{back_label}"
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
                id: "catalog/fyeah-sign".to_string(),
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
            ("runtime-create", "Starting the sim…"),
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
                message: "the device did not start".to_string(),
                retry: retry_action(),
                device: None,
                needs_unlock: false,
            }),
            ..OpenProbe::default()
        });
        let OpeningState::Failed { message, retry, .. } = state else {
            panic!("a finished failure must not fall back to the skeleton");
        };
        assert_eq!(message, "the device did not start");
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
            device: None,
            needs_unlock: false,
        };
        assert_eq!(label.observe(failed.clone()), failed);
    }

    /// A board's tier refusal reaches the page as one Unlock answers.
    #[test]
    fn a_refused_open_carries_its_unlock() {
        let state = opening_state(&OpenProbe {
            in_flight: false,
            stage: OpenStage::Failed(OpenFailure {
                message: "This needs an edit device password — unlock again with one.".to_string(),
                retry: retry_action(),
                device: None,
                needs_unlock: true,
            }),
            ..OpenProbe::default()
        });
        assert!(matches!(
            state,
            OpeningState::Failed {
                needs_unlock: true,
                ..
            }
        ));
    }
}
