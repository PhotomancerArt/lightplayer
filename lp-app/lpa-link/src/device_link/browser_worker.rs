//! [`Link`] over a `fw-browser` worker (wasm only): the sim as a transport.
//!
//! The browser worker was a `LinkProvider`/`LinkSession` and nothing else, so
//! the studio could run a simulator but could never fold one the way it folds
//! silicon — no link, no record, no card. This adapter closes that gap: it
//! turns the worker's envelope channel into the model's event-queue
//! contract, so `lpa-devices` sees one more `Link` and needs no new arm.
//!
//! # What a worker link is, and is not
//!
//! It is NOT a second provider. [`BrowserWorkerHandle`] still owns the
//! `Worker`, the boot handshake and the output buffer. All this does is
//! translate:
//!
//! | model | worker |
//! |---|---|
//! | [`LinkCommand::Open`] | spawn the worker + `Boot` it with the options this link was built with |
//! | [`LinkCommand::Close`] | `terminate()` — a sim's "port" is the worker itself |
//! | [`LinkCommand::SendFrame`] / [`LinkCommand::SendLine`] | `ProtocolIn` |
//! | [`LinkCommand::RunReset`] | destroy + recreate the runtime (see below) |
//! | `ProtocolOut` | [`LinkEvent::Frame`] / [`LinkEvent::Passthrough`], through the SAME demux a serial line takes |
//! | `Log` | [`LinkEvent::Line`] — a sim's console output is console output |
//! | `Status { fatal }` | [`LinkEvent::Error`] then [`LinkEvent::Closed`] |
//!
//! # Reset is destroy-and-recreate
//!
//! A worker has no DTR, no RTS and no boot ROM, so there is no sequence to
//! run and nothing to tell the [`ResetKind`]s apart. Every kind therefore
//! means the one thing a sim can honestly do — throw the runtime away and
//! start a fresh one — and reports `ok: true`. Under-claiming here would be
//! worse than the collapse: a caller that thinks it put a board into the ROM
//! downloader and did not misreads everything after, and a sim simply has no
//! downloader to be in.
//!
//! # The executor lives here, not in the model
//!
//! Booting a worker is promise-shaped, so commands run in a spawned future
//! that pushes [`LinkEvent`]s onto a shared queue (invariant I7: the fold
//! loop never awaits device IO). Commands drain **one at a time** so a
//! queued `Close` cannot overtake the `Open` it follows — the same rule
//! `browser_serial.rs` keeps, for the same reason.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo, ResetKind};
use wasm_bindgen_futures::spawn_local;

use crate::device_link::demux::demux_line;
use crate::device_link::wire::client_message;
use crate::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserRuntimeTier, BrowserWorkerHandle,
    BrowserWorkerOptions,
};

/// What the runtime behind a worker link wears when it boots.
///
/// ⚠️ **Seam pending P1.** The plan's PD1 puts exactly these three facts in
/// `BrowserRuntimeOptions`, carried by `create_runtime(label, options_json)`
/// and mirrored in `worker_envelope.rs`. Until that lands the worker's
/// `Boot` envelope has nowhere to put them, so a link carries them, hands
/// them to whoever asks, and passes nothing extra to the worker: the sim
/// boots as it does today. Reconciling is one edit — pass this struct's
/// fields into the boot envelope and delete the type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimRuntimeOptions {
    /// Requested shader-execution tier (PD12: sims ask for GPU, the worker
    /// answers with what it granted).
    pub tier: BrowserRuntimeTier,
    /// The target's runtime manifest, as JSON. Empty = whatever the worker
    /// defaults to.
    pub hardware_manifest_json: String,
    /// The synthetic base MAC this sim reports as its hardware identity
    /// (PD6: Studio-minted, locally administered).
    pub base_mac: String,
}

/// One [`Link`] over a `fw-browser` worker.
///
/// Nothing is spawned until the model sends [`LinkCommand::Open`]: building a
/// link is naming a sim, not starting one, and the model is the only thing
/// entitled to decide the difference — exactly the grant/connect split the
/// serial links keep.
pub struct BrowserWorkerLink {
    inner: Rc<WorkerLinkInner>,
}

impl BrowserWorkerLink {
    /// A link over a not-yet-spawned worker.
    ///
    /// `info` is fabricated by the caller (the studio's sim transport owns
    /// the `sim:<uid>` endpoint scheme and the `Sim · <name>` label); the
    /// worker `options` say where the engine assets live; `runtime` is what
    /// the sim will wear once P1's boot options exist.
    pub fn new(info: LinkInfo, options: BrowserWorkerOptions, runtime: SimRuntimeOptions) -> Self {
        Self {
            inner: Rc::new(WorkerLinkInner {
                label: info.label.clone(),
                info,
                options,
                runtime: RefCell::new(runtime),
                handle: RefCell::new(None),
                events: RefCell::new(VecDeque::new()),
                queue: RefCell::new(VecDeque::new()),
                open: Cell::new(false),
                draining: Cell::new(false),
            }),
        }
    }

    /// Whether the worker is spawned and booted right now.
    pub fn is_open(&self) -> bool {
        self.inner.open.get()
    }

    /// A handle for the two things an EFFECT does to a sim's runtime, as
    /// opposed to the two things the model does to its link: restart it, and
    /// change the manifest the next one wears.
    ///
    /// Shared with the link (an `Rc` of the same inner), so a restart is the
    /// same destroy-and-recreate a [`LinkCommand::RunReset`] performs and the
    /// link stays the link across it — a sim's reset must not hand the
    /// effects layer a different port.
    pub fn control(&self) -> BrowserWorkerControl {
        BrowserWorkerControl {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl Link for BrowserWorkerLink {
    fn info(&self) -> &LinkInfo {
        &self.inner.info
    }

    fn submit(&mut self, command: LinkCommand) {
        self.inner.queue.borrow_mut().push_back(command);
        WorkerLinkInner::drain(&self.inner);
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if self.inner.events.borrow().is_empty() {
            self.inner.pump_outputs();
        }
        self.inner.events.borrow_mut().pop_front()
    }
}

/// The effect-side handle from [`BrowserWorkerLink::control`].
#[derive(Clone)]
pub struct BrowserWorkerControl {
    inner: Rc<WorkerLinkInner>,
}

impl BrowserWorkerControl {
    /// Throw the runtime away and start a fresh one, wearing whatever
    /// [`Self::set_hardware_manifest`] last left in place.
    ///
    /// This is what a sim's "reset" is, and what a flash and an erase both
    /// end with: there is no flash to write and no filesystem to wipe (lpfs
    /// is memory), so a restart is the whole of the honest effect.
    pub async fn restart(&self) -> Result<(), String> {
        self.inner.restart().await
    }

    /// Replace the runtime manifest the NEXT runtime wears.
    ///
    /// Effective on the next [`Self::restart`], which is the same "effective
    /// next boot" a `/hardware.json` write has on silicon (board-selection
    /// D4) — the sim keeps the promise the verb already makes.
    pub fn set_hardware_manifest(&self, manifest_json: String) {
        self.inner.runtime.borrow_mut().hardware_manifest_json = manifest_json;
    }

    /// The options the next runtime boots with.
    pub fn runtime_options(&self) -> SimRuntimeOptions {
        self.inner.runtime.borrow().clone()
    }

    /// Post one envelope at the worker, for the conversations that speak the
    /// protocol channel directly (`browser_worker_io`).
    pub(crate) fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), String> {
        self.inner.post(envelope)
    }

    /// Take everything the worker has posted since the last drain.
    pub(crate) fn take_outputs(&self) -> Vec<BrowserOutputEnvelope> {
        self.inner.take_outputs()
    }
}

/// Shared state: what the spawned futures, the polling side and the control
/// handle all touch.
struct WorkerLinkInner {
    info: LinkInfo,
    /// The worker's boot label, taken from the link's own label so worker
    /// logs and the card name the same thing.
    label: String,
    options: BrowserWorkerOptions,
    runtime: RefCell<SimRuntimeOptions>,
    /// `None` until `Open` spawns one; dropped (which terminates it) by
    /// `Close`.
    handle: RefCell<Option<BrowserWorkerHandle>>,
    events: RefCell<VecDeque<LinkEvent>>,
    queue: RefCell<VecDeque<LinkCommand>>,
    open: Cell<bool>,
    /// A future is already draining [`Self::queue`]. Keeps commands ordered
    /// without a channel.
    draining: Cell<bool>,
}

impl WorkerLinkInner {
    /// Start draining the command queue, unless a future already is.
    fn drain(inner: &Rc<Self>) {
        if inner.draining.get() {
            return;
        }
        inner.draining.set(true);
        let inner = Rc::clone(inner);
        spawn_local(async move {
            loop {
                // The borrow ends before the await: no RefCell borrow may
                // span a suspension point.
                let next = inner.queue.borrow_mut().pop_front();
                let Some(command) = next else {
                    break;
                };
                inner.execute(command).await;
            }
            inner.draining.set(false);
        });
    }

    async fn execute(&self, command: LinkCommand) {
        match command {
            // The baud is meaningless to a worker and deliberately ignored
            // rather than faked into a field: a sim has no wire rate, and
            // recording one would invite somebody to trust it.
            LinkCommand::Open { .. } => self.open_worker().await,
            LinkCommand::Close => self.close_worker("closed by request"),
            LinkCommand::RunReset(kind) => self.run_reset(kind).await,
            LinkCommand::SendFrame(frame) => match client_message(&frame) {
                Ok(message) => match lpc_wire::json::to_string(&message) {
                    Ok(json) => self.send_protocol(json),
                    Err(error) => self.push(LinkEvent::Error(format!(
                        "failed to encode {:?}: {error}",
                        frame.body
                    ))),
                },
                Err(error) => self.push(LinkEvent::Error(error)),
            },
            // A raw line only reaches a worker as a frame: the `M!` marker is
            // a serial-wire convention, and the envelope carries the body.
            // A line that is not a frame has no worker-side meaning at all,
            // so it is reported rather than silently dropped.
            LinkCommand::SendLine(line) => match line.trim_end().strip_prefix("M!") {
                Some(frame) => self.send_protocol(frame.to_string()),
                None => self.push(LinkEvent::Error(format!(
                    "a sim has no console to type at; dropped {line:?}"
                ))),
            },
        }
    }

    /// Spawn the worker and boot it. Failure is an error AND a close: a
    /// worker that did not boot is not a port that is merely quiet.
    async fn open_worker(&self) {
        if self.handle.borrow().is_some() {
            // Already spawned: re-announce so a second Open is answered the
            // way the serial link answers one.
            self.push(LinkEvent::Opened {
                info: self.info.clone(),
            });
            return;
        }
        match self.spawn_and_boot().await {
            Ok(()) => {
                self.open.set(true);
                self.push(LinkEvent::Opened {
                    info: self.info.clone(),
                });
            }
            Err(error) => {
                self.push(LinkEvent::Error(error));
                self.push(LinkEvent::Closed {
                    reason: "the sim did not start".to_string(),
                });
            }
        }
    }

    /// The spawn + boot half, without the eventing: shared by `Open` and the
    /// destroy-and-recreate a reset performs.
    async fn spawn_and_boot(&self) -> Result<(), String> {
        let script = self.options.worker_script_path();
        let mut handle = BrowserWorkerHandle::new(&script).map_err(|error| error.to_string())?;
        // P1 reconciliation point: the boot envelope grows the runtime
        // options (`self.runtime`) and this call passes them.
        let outputs = handle
            .boot(&self.label, &self.options)
            .await
            .map_err(|error| error.to_string())?;
        *self.handle.borrow_mut() = Some(handle);
        // Boot output is evidence like any other: the studio's log envelopes
        // become lines so a sim that complained on the way up says so on its
        // card instead of only in the console.
        for output in outputs {
            for event in worker_events(output) {
                self.push(event);
            }
        }
        Ok(())
    }

    /// Terminate the worker and SAY so, even if there was nothing running —
    /// the same rule the serial link keeps, because a link that stays silent
    /// makes the model wait out its whole cancel grace.
    fn close_worker(&self, reason: &str) {
        self.open.set(false);
        // Dropping the handle terminates the worker (its `Drop` does), so
        // there is nothing to await and no cleanup to lose.
        let handle = self.handle.borrow_mut().take();
        drop(handle);
        self.push(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    /// Destroy and recreate the runtime. Every [`ResetKind`] lands here.
    async fn run_reset(&self, kind: ResetKind) {
        let was_open = self.open.get();
        let handle = self.handle.borrow_mut().take();
        drop(handle);
        self.open.set(false);
        if !was_open {
            // Nothing was running: the reset is vacuously done, and the
            // model gets its outcome rather than waiting for one.
            self.push(LinkEvent::ResetOutcome { kind, ok: true });
            return;
        }
        match self.spawn_and_boot().await {
            Ok(()) => {
                self.open.set(true);
                self.push(LinkEvent::ResetOutcome { kind, ok: true });
                self.push(LinkEvent::Opened {
                    info: self.info.clone(),
                });
            }
            Err(error) => {
                self.push(LinkEvent::Error(error));
                self.push(LinkEvent::ResetOutcome { kind, ok: false });
                self.push(LinkEvent::Closed {
                    reason: "the sim did not come back".to_string(),
                });
            }
        }
    }

    /// The restart an effect asks for: the same destroy-and-recreate, with
    /// the outcome returned instead of raised (an effect reports through its
    /// activity, not through the link's event queue).
    async fn restart(&self) -> Result<(), String> {
        let handle = self.handle.borrow_mut().take();
        drop(handle);
        self.open.set(false);
        self.spawn_and_boot().await?;
        self.open.set(true);
        self.push(LinkEvent::Opened {
            info: self.info.clone(),
        });
        Ok(())
    }

    fn send_protocol(&self, frame: String) {
        if let Err(error) = self.post(&BrowserInputEnvelope::ProtocolIn {
            runtime_id: None,
            frame,
        }) {
            self.push(LinkEvent::Error(error));
        }
    }

    fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), String> {
        let handle = self.handle.borrow();
        let Some(handle) = handle.as_ref() else {
            return Err("write on a sim that is not running".to_string());
        };
        handle.post(envelope).map_err(|error| error.to_string())
    }

    fn take_outputs(&self) -> Vec<BrowserOutputEnvelope> {
        let mut handle = self.handle.borrow_mut();
        match handle.as_mut() {
            Some(handle) => handle.take_outputs(),
            None => Vec::new(),
        }
    }

    /// Drain the worker's output buffer onto the event queue.
    fn pump_outputs(&self) {
        let mut fatal = false;
        for output in self.take_outputs() {
            fatal |= is_fatal(&output);
            for event in worker_events(output) {
                self.push(event);
            }
        }
        // A condemned wasm instance never answers again (the instance-fatal
        // rule), so it is the sim's equivalent of a port dying underneath
        // us: the fold hears the error AND the close, or the card would sit
        // at "Attached" forever.
        if fatal && self.open.replace(false) {
            let handle = self.handle.borrow_mut().take();
            drop(handle);
            self.push(LinkEvent::Closed {
                reason: "the sim stopped answering".to_string(),
            });
        }
    }

    fn push(&self, event: LinkEvent) {
        self.events.borrow_mut().push_back(event);
    }
}

/// Whether an envelope condemns the worker instance.
fn is_fatal(output: &BrowserOutputEnvelope) -> bool {
    matches!(output, BrowserOutputEnvelope::Status { status, .. } if status == "fatal")
}

/// One worker output as the model's events.
///
/// Protocol frames go through the SAME [`demux_line`] a serial line takes,
/// so an app conversation's reply is classified as
/// [`LinkEvent::Passthrough`] here exactly as it is on a wire — the pump and
/// the lens tap cannot disagree about which frames the fold hears, whatever
/// the transport underneath.
fn worker_events(output: BrowserOutputEnvelope) -> Vec<LinkEvent> {
    match output {
        BrowserOutputEnvelope::ProtocolOut { frame, .. } => vec![demux_line(&format!("M!{frame}"))],
        BrowserOutputEnvelope::Log {
            level,
            target,
            message,
            ..
        } => vec![LinkEvent::Line(format!("{level} {target}: {message}"))],
        BrowserOutputEnvelope::Status {
            status, message, ..
        } if status == "error" || status == "fatal" => vec![LinkEvent::Error(
            message.unwrap_or_else(|| format!("the sim reported {status}")),
        )],
        // Lifecycle chatter the model has no use for: boot phases (the
        // opening frame reads those), runtime bookkeeping, and everything
        // the preview path owns. Frames are the evidence; these are not.
        _ => Vec::new(),
    }
}
