//! The effects layer: everything the model asked for, performed outside it.
//!
//! # The shape (invariant I7)
//!
//! ```text
//!   Roster::handle ──► Vec<Command> ──► DeviceEffects::apply
//!                                          │
//!                            Link::submit ─┤ (never awaits: the browser link
//!                                          │  queues into its own spawned
//!                                          │  drain future)
//!                       spawn(timer) ──────┤
//!                       spawn(pump) ───────┘
//!                                          │
//!   Roster::handle ◄── StudioCommand::Device(Input) ◄──────────┘
//! ```
//!
//! The fold loop is synchronous. Every future this module spawns ends by
//! pushing an [`Input`] onto the actor's ordered command queue, so a slow port
//! delays a card and never the app — that is the whole point of the invariant,
//! and it is what the shipped system's wedged pages violated.
//!
//! # Ordering and generations
//!
//! Commands are applied in the order the model emitted them, and link commands
//! reach the link in that order (the browser adapter drains its own queue one
//! at a time so a `Close` cannot overtake the `Open` it follows). Timers carry
//! the model's generation stamp: a fire whose `seq` is no longer the scope's
//! armed one is dropped INSIDE the model, so this layer never cancels a timer
//! — it just lets superseded ones land.

use core::future::Future;
use core::pin::Pin;
use core::time::Duration;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use lpa_devices::activity::{ActivityKind, ActivityOutcome};
use lpa_devices::event::{ActivityMarker, Command, EffectId, EffectRequest, Event, Input};
use lpa_devices::identity::{DeviceId, EndpointKey, MacAddress, PeerIdentity};
use lpa_devices::link::{Link, LinkCommand, LinkId, LinkInfo};
use lpa_devices::record::DeviceRecord;
use lpa_devices::time::TimerId;

use super::device_transport::{DeviceEffectCall, DeviceTransport, GrantedLink};

/// A spawned task. `?Send` like everything else in the studio.
pub type DeviceTaskFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// One platform sleep.
pub type DeviceTimerFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// How often a link pump re-reads a quiet wire.
///
/// The adapters never block, so this is purely how promptly a boot line or a
/// hello reaches the fold. 20 ms is well inside every budget the model keeps
/// (the hello re-ask is 1 s, the identify deadline 5 s) and cheap enough to
/// leave running on an attached-but-idle port.
const LINK_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// How long after a `navigator.serial` disconnect the sweep looks for links
/// that died.
///
/// The event fires when the OS drops the device; the browser's own port list
/// catches up a beat later. Asking immediately would still see the departed
/// port, so the check waits out that beat.
///
/// ⚠️ Still a settle, not a per-port signal: `install_serial_events` hands
/// its callbacks no port, so "which port left?" is answered by asking the
/// browser which grants remain. That question is now the RIGHT one — see
/// [`DeviceEffects::sweep_departed_ports`].
const HOTPLUG_SETTLE: Duration = Duration::from_millis(250);

/// Links attached (or detached) per granted-port sweep.
///
/// Ids are minted before the sweep awaits (a spawned future cannot hold
/// `&mut self`), so the budget is stated rather than unbounded. A tab with
/// more boards than this attaches the rest on the next sweep, and the
/// departure sweep uses the same ceiling for the same reason.
const MAX_SWEEP_LINKS: usize = 8;

/// The exclusive-wire-borrow token a link carries.
///
/// The EFFECT'S OWN id, not a bare `bool`. An abandoned effect (its activity
/// ended while it was still running) has its borrow released early, and its
/// future still completes later — with a plain flag that late completion
/// would release a borrow belonging to whatever effect had started since.
/// Naming the holder makes every release a no-op unless it is the holder's.
type BorrowToken = Rc<Cell<Option<EffectId>>>;

/// The borrow holder the editor LENS signs as (round-2 M5).
///
/// A lens is not an activity — it has no reducer, no deadline, no end
/// marker — but it holds the wire exactly the way a coarse effect does:
/// exclusively, with the pump paused, announced to the fold as a
/// [`Event::LinkBorrow`]. Signing the token with an id the model never
/// mints keeps every guarded release honest: an activity's late completion
/// cannot hand away the lens's wire, and the lens cannot release an
/// activity's.
pub const LENS_EFFECT_ID: EffectId = EffectId(u64::MAX);

/// Records the effects layer was asked to write, drained by the roster
/// sub-controller — which owns the library host and can await it.
#[derive(Debug, Default)]
pub struct PendingWrites {
    pub persist: Vec<DeviceRecord>,
    pub delete: Vec<DeviceId>,
    /// Pushes that completed and verified, for the library to bank
    /// (`CatalogOp::RecordPush`: a history `Pushed` event + the device
    /// association).
    pub pushes: Vec<CompletedPush>,
}

impl PendingWrites {
    pub fn is_empty(&self) -> bool {
        self.persist.is_empty() && self.delete.is_empty() && self.pushes.is_empty()
    }
}

/// What the library needs to remember about one finished push.
///
/// It is reported from HERE rather than as a model command on purpose: the
/// model holds no project identity — what a board runs is evidence, and the
/// library is the historian. The effects layer is the only place that knows
/// both which project's bytes went down and that they arrived intact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedPush {
    pub device: DeviceId,
    /// The library project whose bytes were sent.
    pub project_uid: String,
    /// The canonical content hash that was verified on the device.
    pub version: String,
}

/// One project, resolved and waiting for the effect that will send it.
///
/// Staged rather than carried on the action because
/// [`Action::Push`](lpa_devices::Action::Push)'s journal record is written
/// verbatim and replayed from JSON — a project's bytes have no business in a
/// flight recorder. The app resolves the source (an example installed, a
/// library project read, a starter generated) and stages the RESULT here,
/// including the failure: a project that could not be prepared and a device
/// that refused the write then reach the card by the same road.
pub type StagedPush = Result<PushPayload, String>;

/// The bytes and the identity of one prepared push.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushPayload {
    /// The library project this came from — bookkeeping only; the device is
    /// never told.
    pub project_uid: String,
    /// Display label for the progress line ("porch-sign").
    pub label: String,
    pub files: Vec<(String, Vec<u8>)>,
    pub content_hash: String,
    /// Storage dir to use when the board reports nothing loaded.
    pub fallback_storage_id: String,
}

/// One routed transport plus what the effects layer knows about it.
struct LinkSlot {
    info: LinkInfo,
    link: Rc<RefCell<Box<dyn Link>>>,
    /// A coarse effect holds the wire: the pump stops draining until the
    /// borrow ends, so the effect's own conversation is the only reader.
    /// This is the executor's half of the exclusive-borrow discipline.
    borrowed: BorrowToken,
}

/// A link that arrived from a spawned future, waiting to join the routing map.
struct Arrival {
    link: LinkId,
    info: LinkInfo,
    handle: Rc<RefCell<Box<dyn Link>>>,
    borrowed: BorrowToken,
}

/// Owns the links, the platform seams, and the spawning.
pub struct DeviceEffects {
    links: BTreeMap<LinkId, LinkSlot>,
    /// New links land here from spawned futures and are drained into
    /// [`Self::links`] by [`Self::settle`] before every fold.
    arrivals: Rc<RefCell<Vec<Arrival>>>,
    transport: Option<Rc<dyn DeviceTransport>>,
    spawn: Option<Rc<dyn Fn(DeviceTaskFuture)>>,
    timer: Option<Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>>,
    /// Puts model inputs back on the actor's queue.
    sink: Option<Rc<dyn Fn(Input)>>,
    writes: PendingWrites,
    /// Payloads staged for a push that has not run yet, by device. A
    /// one-shot handoff, not a store: it is written just before the gesture
    /// is folded, taken by the effect, and dropped with the device.
    staged_pushes: BTreeMap<DeviceId, StagedPush>,
    /// Verified pushes reported by spawned effect futures, drained into
    /// [`PendingWrites`] at the next take. Shared because a spawned future
    /// cannot hold `&mut self` across an await — the same reason
    /// [`Self::arrivals`] exists.
    completed_pushes: Rc<RefCell<Vec<CompletedPush>>>,
    next_link: u64,
}

impl Default for DeviceEffects {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceEffects {
    pub fn new() -> Self {
        Self {
            links: BTreeMap::new(),
            arrivals: Rc::new(RefCell::new(Vec::new())),
            transport: None,
            spawn: None,
            timer: None,
            sink: None,
            writes: PendingWrites::default(),
            staged_pushes: BTreeMap::new(),
            completed_pushes: Rc::new(RefCell::new(Vec::new())),
            next_link: 0,
        }
    }

    /// Stage what a [`Action::Push`](lpa_devices::Action::Push) gesture will
    /// send — or the reason it cannot be sent.
    ///
    /// Called immediately before the gesture is folded. A second stage for
    /// the same device replaces the first, which is what makes an in-place
    /// Retry after a failure send freshly resolved bytes rather than a
    /// stale snapshot.
    pub fn stage_push(&mut self, device: DeviceId, staged: StagedPush) {
        self.staged_pushes.insert(device, staged);
    }

    /// Install the platform transport (wasm: browser Web Serial; host tests:
    /// a fake). Without one the roster has no ports, and the page says so.
    pub fn set_transport(&mut self, transport: Rc<dyn DeviceTransport>) {
        self.transport = Some(transport);
    }

    /// Install the platform task spawner (`spawn_local` on wasm; a collecting
    /// closure in host tests).
    pub fn set_spawner(&mut self, spawner: impl Fn(DeviceTaskFuture) + 'static) {
        self.spawn = Some(Rc::new(spawner));
    }

    /// Install the platform timer factory — the SAME one the actor builds its
    /// pull deadlines from, so device waits and project waits cannot drift
    /// onto two different clocks.
    pub fn set_timer(&mut self, timer: impl FnMut(Duration) -> DeviceTimerFuture + 'static) {
        self.timer = Some(Rc::new(RefCell::new(timer)));
    }

    /// Install the sink that puts model inputs back on the actor's queue.
    pub fn set_input_sink(&mut self, sink: impl Fn(Input) + 'static) {
        self.sink = Some(Rc::new(sink));
    }

    /// The platform timer factory, shared: the lens session's wire client
    /// builds its request deadlines from the SAME clock device waits run
    /// on, so a lens pull and a device timer cannot drift onto two clocks.
    pub fn timer_factory(&self) -> Option<Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>> {
        self.timer.clone()
    }

    /// Whether the seams a real device needs are all installed.
    pub fn is_wired(&self) -> bool {
        self.transport.is_some() && self.spawn.is_some() && self.sink.is_some()
    }

    /// Hand a link's wire to the editor lens (round-2 M5): pause the pump
    /// by taking the borrow as [`LENS_EFFECT_ID`], tell the fold, and build
    /// the `lpa-client` io the lens session's wire client will speak
    /// through. Every line that io drains comes back through the tap as
    /// the very `LinkEvent` the pump would have produced, so the device's
    /// evidence keeps folding while the editor owns the port.
    ///
    /// Refused — honestly, with the reason — when the port is gone or an
    /// activity already holds the wire; the model never sees a half-borrow.
    pub fn attach_lens_wire(
        &mut self,
        link: LinkId,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let (Some(transport), Some(sink)) = (self.transport.clone(), self.sink.clone()) else {
            return Err("the device transport is not installed".to_string());
        };
        let Some(slot) = self.links.get(&link) else {
            return Err("the port is gone".to_string());
        };
        if let Some(holder) = slot.borrowed.get() {
            return Err(match holder == LENS_EFFECT_ID {
                true => "the editor already holds this board's wire".to_string(),
                false => {
                    "an activity is using this board's wire; wait for it to finish".to_string()
                }
            });
        }
        let info = slot.info.clone();
        let borrowed = Rc::clone(&slot.borrowed);
        let tap: super::device_transport::LensLineTap = {
            let sink = Rc::clone(&sink);
            Rc::new(move |event: super::device_transport::LensTapEvent| {
                use super::device_transport::LensTapEvent;
                match event {
                    LensTapEvent::Line(line) => sink(Input::link(
                        link,
                        lpa_link::device_link::demux::demux_line(&line),
                    )),
                    // The pump's mark-gone rule, replayed for the lens: a
                    // port error means the port died underneath us, so the
                    // fold hears the error AND the close.
                    LensTapEvent::PortError(message) => {
                        sink(Input::link(
                            link,
                            lpa_devices::link::LinkEvent::Error(message.clone()),
                        ));
                        sink(Input::link(
                            link,
                            lpa_devices::link::LinkEvent::Closed { reason: message },
                        ));
                    }
                }
            })
        };
        borrowed.set(Some(LENS_EFFECT_ID));
        sink(Input::Event(Event::LinkBorrow { link, held: true }));
        match transport.lens_client_io(info, tap) {
            Ok(io) => {
                log::debug!("the editor lens holds {link:?}; the pump is paused");
                Ok(io)
            }
            Err(message) => {
                // Nothing took the wire after all: give it straight back so
                // the fold's freshness resumes with no gap.
                if release_borrow(&borrowed, LENS_EFFECT_ID) {
                    sink(Input::Event(Event::LinkBorrow { link, held: false }));
                }
                Err(message)
            }
        }
    }

    /// Give a lens-held wire back: the pump resumes on its next tick and
    /// the fold hears the release. A no-op for a port that is gone (unplug
    /// mid-lens — the detach evidence is already on the queue) and for a
    /// wire the lens does not hold.
    pub fn release_lens_wire(&mut self, link: LinkId) {
        let Some(slot) = self.links.get(&link) else {
            return;
        };
        if !release_borrow(&slot.borrowed, LENS_EFFECT_ID) {
            return;
        }
        log::debug!("the editor lens released {link:?}; the pump resumes");
        if let Some(sink) = &self.sink {
            sink(Input::Event(Event::LinkBorrow { link, held: false }));
        }
    }

    /// Whether the editor lens currently holds this link's wire.
    pub fn lens_holds_wire(&self, link: LinkId) -> bool {
        self.links
            .get(&link)
            .is_some_and(|slot| slot.borrowed.get() == Some(LENS_EFFECT_ID))
    }

    /// Record writes to perform, taken by the roster sub-controller.
    pub fn take_writes(&mut self) -> PendingWrites {
        self.writes
            .pushes
            .extend(self.completed_pushes.borrow_mut().drain(..));
        core::mem::take(&mut self.writes)
    }

    /// Move links that arrived from spawned futures into the routing map.
    /// Called before every fold, so a link is routable by the time the
    /// `LinkAttached` it queued behind itself is handled.
    pub fn settle(&mut self) {
        let arrived: Vec<Arrival> = self.arrivals.borrow_mut().drain(..).collect();
        for arrival in arrived {
            self.links.insert(
                arrival.link,
                LinkSlot {
                    info: arrival.info,
                    link: arrival.handle,
                    borrowed: arrival.borrowed,
                },
            );
        }
    }

    /// Perform one batch of commands, in order.
    pub fn apply(&mut self, commands: Vec<Command>) {
        for command in commands {
            self.apply_one(command);
        }
    }

    fn apply_one(&mut self, command: Command) {
        match command {
            Command::Link { link, command } => self.submit(link, command),
            Command::StartTimer { timer, after_ms } => self.start_timer(timer, after_ms),
            Command::RequestUsbGrant => self.request_grant(),
            Command::PersistRecord(record) => self.writes.persist.push(record),
            Command::DeleteRecord(device) => {
                // A forgotten device takes its unsent payload with it.
                self.staged_pushes.remove(&device);
                self.writes.delete.push(device);
            }
            Command::RevokeGrant(info) => self.revoke_grant(info),
            Command::RunEffect {
                device,
                link,
                effect_id,
                effect,
            } => self.run_effect(device, link, effect_id, effect),
            // The device is named on the command for the journal's sake;
            // the release is addressed by link and guarded by effect id.
            Command::AbandonEffect {
                link, effect_id, ..
            } => self.abandon_effect(link, effect_id),
        }
    }

    /// Give a still-running effect's wire back.
    ///
    /// The effect itself cannot be stopped — it is a spawned future with no
    /// handle — but the pump can start reading again the moment the model
    /// says nothing is listening to it any more. The release is GUARDED by
    /// the effect id: an abandon for an effect that already finished (the
    /// ordinary settle, where the end marker is what ended the activity) is
    /// a no-op, and so is one that arrives after a newer effect has taken
    /// the same wire.
    fn abandon_effect(&mut self, link: LinkId, effect_id: EffectId) {
        let Some(slot) = self.links.get(&link) else {
            return;
        };
        if !release_borrow(&slot.borrowed, effect_id) {
            return;
        }
        log::debug!("abandoned effect {effect_id:?} on {link:?}; the pump resumes");
        if let Some(sink) = &self.sink {
            sink(Input::Event(Event::LinkBorrow { link, held: false }));
        }
    }

    /// Execute one coarse effect: borrow the wire exclusively, run the
    /// operation through the transport, stream progress and the end marker
    /// back through the sink — the round-2 seam
    /// (`docs/adr/2026-08-25-event-fold-device-model.md`).
    ///
    /// ⚠️ Every marker goes through `self.sink` and nothing else — progress
    /// that is merely written somewhere a rebuild would read is exactly how
    /// a flash once ran for a minute with nothing on screen
    /// (`docs/defects/2026-07-28-flash-progress-never-reached-the-ui.md`).
    fn run_effect(
        &mut self,
        device: DeviceId,
        link: LinkId,
        effect_id: EffectId,
        effect: EffectRequest,
    ) {
        let kind = effect_kind(&effect);
        let (Some(transport), Some(spawn), Some(sink)) = (
            self.transport.clone(),
            self.spawn.clone(),
            self.sink.clone(),
        ) else {
            log::warn!("a coarse effect was requested before the platform seams were installed");
            return;
        };
        let Some(slot) = self.links.get(&link) else {
            // The port vanished between the fold and the effect — the same
            // race the detach event right behind it describes. The activity
            // still deserves an ending.
            sink(effect_ended(
                device,
                effect_id,
                kind,
                ActivityOutcome::Failed {
                    message: "the port is gone".to_string(),
                },
            ));
            return;
        };
        if slot.borrowed.get() == Some(LENS_EFFECT_ID) {
            // The editor owns this wire for as long as its lens is on the
            // board; a coarse effect cannot borrow it out from under the
            // session's client. The activity ends with the reason on the
            // card instead of two readers splitting the frames.
            sink(effect_ended(
                device,
                effect_id,
                kind,
                ActivityOutcome::Failed {
                    message: "the editor is open on this board — close it before running this"
                        .to_string(),
                },
            ));
            return;
        }
        let info = slot.info.clone();
        let borrowed = Rc::clone(&slot.borrowed);
        let payload = matches!(effect, EffectRequest::Push)
            .then(|| self.staged_pushes.remove(&device))
            .flatten();
        let pushed = payload
            .as_ref()
            .and_then(|staged| staged.as_ref().ok())
            .map(|payload: &PushPayload| {
                (
                    payload.project_uid.clone(),
                    payload.label.clone(),
                    payload.content_hash.clone(),
                )
            });
        let call = match resolve_effect_call(effect, payload) {
            Ok(call) => call,
            Err(message) => {
                sink(effect_ended(
                    device,
                    effect_id,
                    kind,
                    ActivityOutcome::Failed { message },
                ));
                return;
            }
        };
        let progress: super::device_transport::DeviceEffectProgress = {
            let sink = Rc::clone(&sink);
            Rc::new(move |label: String, percent: Option<u8>| {
                sink(Input::Event(Event::ActivityMarker {
                    device,
                    effect: Some(effect_id),
                    marker: ActivityMarker::Progress { label, percent },
                }));
            })
        };
        borrowed.set(Some(effect_id));
        // The borrow is a fact about the world, so the fold hears about it
        // (I6): while it holds, nothing is reading the port, and freshness
        // must not read that silence as the board going quiet.
        sink(Input::Event(Event::LinkBorrow { link, held: true }));
        let writes = Rc::clone(&self.completed_pushes);
        spawn(Box::pin(async move {
            let result = transport.run_effect(info, call, progress).await;
            // Give the wire back BEFORE the end marker folds: the reducer's
            // very next command may be the ladder's reopen, and a pump still
            // paused would eat the boot hello. Guarded, because this effect
            // may have been ABANDONED and the wire handed to a newer one.
            if release_borrow(&borrowed, effect_id) {
                sink(Input::Event(Event::LinkBorrow { link, held: false }));
            }
            match result {
                Ok(facts) => {
                    // A verified push is the library's to bank. Recorded
                    // here because this is the only place that knows both
                    // whose bytes went down and that they arrived intact.
                    if let Some((project_uid, label, version)) = pushed {
                        writes.borrow_mut().push(CompletedPush {
                            device,
                            project_uid,
                            version,
                        });
                        log::debug!("pushed {label} to device {device:?}");
                    }
                    // Normalized HERE, not trusted from the tool: the JS
                    // side is untestable, and an unnormalized (or garbage)
                    // MAC would mint a second identity for a board the
                    // hello later binds properly. Invalid reads are dropped,
                    // exactly like the old `record_probed_mac`.
                    let mac = facts
                        .probed_mac
                        .as_deref()
                        .and_then(lpa_link::normalize_base_mac);
                    if let Some(mac) = mac {
                        // Identity learned by the preflight enters as an
                        // event (I6): the join for a blank board.
                        sink(Input::Event(Event::IdentityObserved {
                            device,
                            identity: PeerIdentity {
                                mac: Some(MacAddress(mac)),
                                ..Default::default()
                            },
                        }));
                    }
                    sink(effect_ended(
                        device,
                        effect_id,
                        kind,
                        ActivityOutcome::Succeeded {
                            summary: facts.summary,
                        },
                    ));
                }
                Err(message) => {
                    sink(effect_ended(
                        device,
                        effect_id,
                        kind,
                        ActivityOutcome::Failed { message },
                    ));
                }
            }
        }));
    }

    fn submit(&mut self, link: LinkId, command: LinkCommand) {
        let Some(slot) = self.links.get(&link) else {
            // A command for a link that is gone is not a crash: the port
            // vanished between the fold and the effect, which is exactly the
            // race the detach event about to arrive describes.
            log::debug!("device link {link:?} is gone; dropping {command:?}");
            return;
        };
        slot.link.borrow_mut().submit(command);
    }

    fn start_timer(&mut self, timer: TimerId, after_ms: u64) {
        let (Some(spawn), Some(make_timer), Some(sink)) =
            (self.spawn.clone(), self.timer.clone(), self.sink.clone())
        else {
            log::warn!("a device timer was requested before the platform seams were installed");
            return;
        };
        let sleep = (make_timer.borrow_mut())(Duration::from_millis(after_ms));
        spawn(Box::pin(async move {
            sleep.await;
            // Superseded generations are dropped by the model, not here.
            sink(Input::Event(Event::TimerFired { timer }));
        }));
    }

    fn request_grant(&mut self) {
        let (Some(transport), Some(spawn), Some(sink)) = (
            self.transport.clone(),
            self.spawn.clone(),
            self.sink.clone(),
        ) else {
            log::warn!("a USB grant was requested with no device transport installed");
            return;
        };
        let link = self.mint_link_id();
        let register = self.registrar();
        spawn(Box::pin(async move {
            match transport.request_grant().await {
                // The chooser was dismissed: no port, no news, no error.
                Ok(None) => {}
                Ok(Some(granted)) => register(link, granted, sink),
                Err(error) => log::warn!("device grant request failed: {error}"),
            }
        }));
    }

    fn revoke_grant(&mut self, info: LinkInfo) {
        // Drop our handle first: a revoked grant must not leave a pump reading
        // a port that stopped being ours.
        self.drop_endpoint(&info.endpoint);
        let (Some(transport), Some(spawn)) = (self.transport.clone(), self.spawn.clone()) else {
            return;
        };
        spawn(Box::pin(async move {
            if let Err(error) = transport.revoke_grant(info).await {
                // Best effort by design: a grant the browser will not hand
                // back is a log line, never a card that cannot be dismissed.
                log::warn!("device grant not revoked: {error}");
            }
        }));
    }

    /// Sweep the grants this origin already holds and attach anything new.
    ///
    /// Runs at startup and on every `navigator.serial` connect. Ports already
    /// attached are skipped by endpoint, so a hotplug storm cannot mint two
    /// links for one port.
    ///
    /// ⚠️ Brave revokes Web Serial grants on reload where Chrome persists
    /// them. An empty sweep is an ordinary answer, not a failure.
    pub fn sweep_granted_ports(&mut self) {
        let (Some(transport), Some(spawn), Some(sink)) = (
            self.transport.clone(),
            self.spawn.clone(),
            self.sink.clone(),
        ) else {
            return;
        };
        let held: Vec<EndpointKey> = self
            .links
            .values()
            .map(|slot| slot.info.endpoint.clone())
            .collect();
        let register = self.registrar();
        let ids: Vec<LinkId> = (0..MAX_SWEEP_LINKS).map(|_| self.mint_link_id()).collect();
        spawn(Box::pin(async move {
            let granted = match transport.discover_granted().await {
                Ok(granted) => granted,
                Err(error) => {
                    log::warn!("granted-port discovery failed: {error}");
                    return;
                }
            };
            let mut ids = ids.into_iter();
            for grant in granted {
                if held.contains(&grant.info.endpoint) {
                    continue;
                }
                let Some(link) = ids.next() else {
                    log::warn!("more granted ports than one sweep attaches; the rest wait");
                    return;
                };
                register(link, grant, Rc::clone(&sink));
            }
        }));
    }

    /// React to a `navigator.serial` disconnect: after a settle, ask the
    /// browser which grants remain and detach every link whose endpoint is
    /// no longer among them.
    ///
    /// The diff is the whole point. This used to detach every link that was
    /// not OPEN, which got both answers wrong: a board unplugged while its
    /// port was merely granted was **invisible** — the card sat at
    /// "Attached", no detach was ever raised, and so the replug raised no
    /// attach either and the board could only be recovered by hand (G1
    /// bench, 2026-08-31) — while a board that was simply never opened got
    /// detached because somebody unplugged a different device. An unplugged
    /// port disappears from `getPorts()` whatever its open state, so asking
    /// the browser answers both.
    ///
    /// See [`HOTPLUG_SETTLE`] for why the ask waits a beat, and
    /// [`MAX_SWEEP_LINKS`] for the bound. Re-attachment needs nothing new:
    /// the connect edge sweeps grants, and the model's endpoint supersede +
    /// identity merge put the board back on its own card.
    pub fn sweep_departed_ports(&mut self) {
        let (Some(transport), Some(spawn), Some(make_timer), Some(sink)) = (
            self.transport.clone(),
            self.spawn.clone(),
            self.timer.clone(),
            self.sink.clone(),
        ) else {
            return;
        };
        let held: Vec<(LinkId, EndpointKey)> = self
            .links
            .iter()
            .map(|(link, slot)| (*link, slot.info.endpoint.clone()))
            .collect();
        if held.is_empty() {
            return;
        }
        let sleep = (make_timer.borrow_mut())(HOTPLUG_SETTLE);
        spawn(Box::pin(async move {
            sleep.await;
            let remaining: Vec<EndpointKey> = match transport.discover_granted().await {
                Ok(granted) => granted
                    .into_iter()
                    .map(|grant| grant.info.endpoint)
                    .collect(),
                Err(error) => {
                    // A discovery that FAILED says nothing about which board
                    // left. Detaching on it would tear down every card over
                    // a transient browser error.
                    log::warn!("departure sweep could not list grants: {error}");
                    return;
                }
            };
            let mut budget = MAX_SWEEP_LINKS;
            for (link, endpoint) in held {
                if remaining.contains(&endpoint) {
                    continue;
                }
                if budget == 0 {
                    log::warn!("more departed ports than one sweep detaches; the rest wait");
                    return;
                }
                budget -= 1;
                sink(Input::Event(Event::LinkDetached { link }));
            }
        }));
    }

    /// Keep only the links `keep` still claims.
    ///
    /// Run after every fold against the roster's own link map: the model is
    /// the authority on what is routed, so a link it has let go stops being
    /// pumped rather than lingering as a second opinion.
    pub fn retain_links(&mut self, keep: impl Fn(LinkId) -> bool) {
        self.links.retain(|link, _| keep(*link));
    }

    fn drop_endpoint(&mut self, endpoint: &EndpointKey) {
        self.links.retain(|_, slot| slot.info.endpoint != *endpoint);
    }

    /// A callback that installs a granted link and tells the model about it.
    ///
    /// A closure rather than a method because the sweep and the chooser both
    /// run in spawned futures, which cannot hold `&mut self` across an await.
    fn registrar(&self) -> impl Fn(LinkId, GrantedLink, Rc<dyn Fn(Input)>) + 'static {
        let arrivals = Rc::clone(&self.arrivals);
        let spawn = self.spawn.clone();
        let timer = self.timer.clone();
        move |link, granted, sink| {
            let info = granted.info.clone();
            let handle = Rc::new(RefCell::new(granted.link));
            let borrowed: BorrowToken = Rc::new(Cell::new(None));
            arrivals.borrow_mut().push(Arrival {
                link,
                info: info.clone(),
                handle: Rc::clone(&handle),
                borrowed: Rc::clone(&borrowed),
            });
            if let (Some(spawn), Some(timer)) = (spawn.clone(), timer.clone()) {
                spawn_pump(
                    &spawn,
                    &timer,
                    link,
                    Rc::downgrade(&handle),
                    borrowed,
                    Rc::clone(&sink),
                );
            }
            sink(Input::Event(Event::LinkAttached { link, info }));
        }
    }

    fn mint_link_id(&mut self) -> LinkId {
        self.next_link += 1;
        LinkId(self.next_link)
    }
}

/// One pump future per link: drain everything the wire has, then breathe.
///
/// It holds a WEAK handle, so dropping the link (forget, revoke, detach) ends
/// the pump on its next tick without a cancellation channel. While a coarse
/// effect holds the `borrowed` flag the pump does not touch the wire at all —
/// the effect's own conversation is the only reader (exclusive borrow).
fn spawn_pump(
    spawn: &Rc<dyn Fn(DeviceTaskFuture)>,
    timer: &Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>,
    link: LinkId,
    handle: Weak<RefCell<Box<dyn Link>>>,
    borrowed: BorrowToken,
    sink: Rc<dyn Fn(Input)>,
) {
    let timer = Rc::clone(timer);
    spawn(Box::pin(async move {
        loop {
            let Some(strong) = handle.upgrade() else {
                return;
            };
            // Drain synchronously: `poll_event` never blocks, by contract.
            while borrowed.get().is_none() {
                let next = strong.borrow_mut().poll_event();
                let Some(event) = next else { break };
                sink(Input::link(link, event));
            }
            drop(strong);
            let sleep = (timer.borrow_mut())(LINK_POLL_INTERVAL);
            sleep.await;
        }
    }));
}

/// Release a wire borrow, but only if `effect_id` is the one holding it.
///
/// Returns whether anything was actually released — which is what the
/// caller announces to the fold. Both release paths go through here: the
/// effect completing, and the model abandoning it. An unguarded release
/// would let a straggler hand away the borrow of whatever effect took the
/// wire after it.
fn release_borrow(borrowed: &BorrowToken, effect_id: EffectId) -> bool {
    if borrowed.get() != Some(effect_id) {
        return false;
    }
    borrowed.set(None);
    true
}

/// The Ended marker a coarse effect reports through the sink, stamped with
/// the generation the model handed out. The stamp is what lets the fold drop
/// this marker if the activity that asked for it has since been evicted.
fn effect_ended(
    device: DeviceId,
    effect_id: EffectId,
    kind: ActivityKind,
    outcome: ActivityOutcome,
) -> Input {
    Input::Event(Event::ActivityMarker {
        device,
        effect: Some(effect_id),
        marker: ActivityMarker::Ended { kind, outcome },
    })
}

/// Which activity a coarse effect belongs to.
fn effect_kind(effect: &EffectRequest) -> ActivityKind {
    match effect {
        EffectRequest::Flash { .. } | EffectRequest::WriteBoardManifest { .. } => {
            ActivityKind::Flash
        }
        EffectRequest::Push => ActivityKind::Push,
        EffectRequest::Erase => ActivityKind::Erase,
        EffectRequest::RemoveProject => ActivityKind::RemoveProject,
    }
}

/// Resolve a model effect into platform terms. The board manifest is looked
/// up HERE (the app layer owns `lpa-boards`), so a display-only board id
/// degrades honestly instead of writing junk to the device; a push's payload
/// is the staged one, and its absence is an honest failure rather than a
/// silent no-op.
fn resolve_effect_call(
    effect: EffectRequest,
    payload: Option<StagedPush>,
) -> Result<DeviceEffectCall, String> {
    match effect {
        EffectRequest::Flash { build_id, .. } => Ok(DeviceEffectCall::FlashFirmware { build_id }),
        EffectRequest::WriteBoardManifest { board_id } => {
            let manifest_json = lpa_boards::runtime_manifest_json(&board_id)
                .ok_or_else(|| format!("board {board_id} has no checked-in runtime manifest"))?;
            Ok(DeviceEffectCall::WriteHardwareManifest {
                manifest_json: manifest_json.to_string(),
            })
        }
        EffectRequest::Push => match payload {
            // The app could not prepare a project (a generate that refused,
            // a library read that failed): its reason IS the outcome, so the
            // card says what actually went wrong.
            Some(Err(message)) => Err(message),
            Some(Ok(payload)) => Ok(DeviceEffectCall::PushProject {
                files: payload.files,
                expected_hash: payload.content_hash,
                fallback_storage_id: payload.fallback_storage_id,
            }),
            // Unreachable through the app (every Push gesture stages first),
            // and honest rather than silent if it ever is not.
            None => Err("nothing was prepared to send to this board".to_string()),
        },
        EffectRequest::Erase => Ok(DeviceEffectCall::EraseFlash),
        // Nothing is staged for a removal and nothing needs to be: the dir
        // that goes is the board's own report, read inside the
        // conversation. The fallback is only for the race where the board
        // has stopped reporting one by the time the effect runs.
        EffectRequest::RemoveProject => Ok(DeviceEffectCall::RemoveProject {
            fallback_storage_id: crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID
                .to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::device_transport::{
        DeviceEffectFacts, DeviceEffectProgress, DeviceTransportFuture, LensLineTap, LensTapEvent,
    };
    use super::*;
    use lpa_devices::identity::DeviceId;
    use lpa_devices::record::DeviceRecord;

    /// A link command for a port that is already gone is a no-op, not a
    /// panic. The window is real: the fold decides, and by the time the
    /// effect runs the port may have been revoked or unplugged — the detach
    /// event describing that is right behind it on the queue.
    #[test]
    fn a_command_for_a_vanished_link_is_dropped_rather_than_fatal() {
        let mut effects = DeviceEffects::new();

        effects.apply(vec![Command::Link {
            link: LinkId(9),
            command: LinkCommand::Close,
        }]);
        // Nothing to assert but survival: the alternative shape (panicking on
        // an unknown link, as the `lpa-link` bench does deliberately) would
        // take the whole page down on an ordinary race.
    }

    /// Record writes accumulate for the caller and are taken exactly once —
    /// the roster performs them outside the fold.
    #[test]
    fn record_writes_are_collected_and_taken_once() {
        let mut effects = DeviceEffects::new();
        let record = DeviceRecord::new(DeviceId(1), Default::default());

        effects.apply(vec![
            Command::PersistRecord(record.clone()),
            Command::DeleteRecord(DeviceId(2)),
        ]);

        let writes = effects.take_writes();
        assert_eq!(writes.persist, vec![record]);
        assert_eq!(writes.delete, vec![DeviceId(2)]);
        assert!(effects.take_writes().is_empty(), "taken means taken");
    }

    /// A staged payload becomes the platform call verbatim; nothing about
    /// the project reaches the model on the way.
    #[test]
    fn a_staged_push_resolves_into_the_conversations_own_arguments() {
        let staged = Ok(PushPayload {
            project_uid: "prj000000daqf6dvvqz".to_string(),
            label: "porch-sign".to_string(),
            files: vec![("project.json".to_string(), b"{}".to_vec())],
            content_hash: "lph1:abc".to_string(),
            fallback_storage_id: "studio".to_string(),
        });

        let call = resolve_effect_call(EffectRequest::Push, Some(staged)).expect("resolved");

        assert_eq!(
            call,
            DeviceEffectCall::PushProject {
                files: vec![("project.json".to_string(), b"{}".to_vec())],
                expected_hash: "lph1:abc".to_string(),
                fallback_storage_id: "studio".to_string(),
            }
        );
    }

    /// A project the app could not prepare fails with ITS reason, so the
    /// card says what actually went wrong rather than "the push failed".
    #[test]
    fn a_preparation_failure_becomes_the_effects_own_message() {
        let staged = Err("this board has no catalog entry".to_string());

        let error = resolve_effect_call(EffectRequest::Push, Some(staged)).expect_err("refused");

        assert_eq!(error, "this board has no catalog entry");
    }

    /// Unreachable through the app — every Push gesture stages first — and
    /// honest rather than silent if it ever is not.
    #[test]
    fn a_push_with_nothing_staged_refuses_out_loud() {
        let error = resolve_effect_call(EffectRequest::Push, None).expect_err("refused");

        assert!(error.contains("nothing was prepared"), "{error}");
    }

    /// Each effect belongs to the activity that asked for it — the kind the
    /// end marker wears, and the one the fold brackets against.
    #[test]
    fn effects_name_the_activity_they_belong_to() {
        assert_eq!(
            effect_kind(&EffectRequest::Flash {
                build_id: "esp32c6-4mb".to_string(),
                board_id: "seeed-xiao-esp32c6".to_string(),
            }),
            ActivityKind::Flash
        );
        assert_eq!(
            effect_kind(&EffectRequest::WriteBoardManifest {
                board_id: "seeed-xiao-esp32c6".to_string(),
            }),
            ActivityKind::Flash,
            "the manifest stamp is the flash's second half, not its own flow"
        );
        assert_eq!(effect_kind(&EffectRequest::Push), ActivityKind::Push);
    }

    /// Forgetting a device drops whatever was staged for it: a payload
    /// waiting for a card that no longer exists is nobody's.
    #[test]
    fn forgetting_a_device_drops_its_staged_payload() {
        let mut effects = DeviceEffects::new();
        effects.stage_push(DeviceId(1), Err("never mind".to_string()));

        effects.apply(vec![Command::DeleteRecord(DeviceId(1))]);

        assert!(effects.staged_pushes.is_empty());
    }

    // -----------------------------------------------------------------
    // The lens wire handover (round-2 M5)
    // -----------------------------------------------------------------

    /// A link that answers nothing: the handover tests care about the
    /// borrow and the fold, not the wire.
    struct QuietLink(LinkInfo);

    impl Link for QuietLink {
        fn info(&self) -> &LinkInfo {
            &self.0
        }
        fn submit(&mut self, _command: LinkCommand) {}
        fn poll_event(&mut self) -> Option<lpa_devices::link::LinkEvent> {
            None
        }
    }

    /// A transport whose lens io is a recording stub; `refuse` makes the
    /// build fail so the give-back-on-failure path is exercised.
    struct LensStubTransport {
        refuse: bool,
        taps: Rc<RefCell<Vec<LensLineTap>>>,
    }

    struct NoIo;

    #[async_trait::async_trait(?Send)]
    impl lpa_client::ClientIo for NoIo {
        async fn send(
            &mut self,
            _msg: lpc_wire::ClientMessage,
        ) -> Result<(), lpc_wire::TransportError> {
            Ok(())
        }
        async fn receive(
            &mut self,
        ) -> Result<lpc_wire::WireServerMessage, lpc_wire::TransportError> {
            Err(lpc_wire::TransportError::Other("stub".to_string()))
        }
        async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
            Ok(())
        }
    }

    impl DeviceTransport for LensStubTransport {
        fn label(&self) -> &'static str {
            "lens stub"
        }
        fn run_effect(
            &self,
            _info: LinkInfo,
            _call: DeviceEffectCall,
            _progress: DeviceEffectProgress,
        ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
            Box::pin(core::future::ready(Ok(DeviceEffectFacts::default())))
        }
        fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
            Box::pin(core::future::ready(Ok(Vec::new())))
        }
        fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
            Box::pin(core::future::ready(Ok(None)))
        }
        fn revoke_grant(&self, _info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
            Box::pin(core::future::ready(Ok(())))
        }
        fn lens_client_io(
            &self,
            _info: LinkInfo,
            tap: LensLineTap,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            if self.refuse {
                return Err("no port for you".to_string());
            }
            self.taps.borrow_mut().push(tap);
            Ok(Box::new(NoIo))
        }
    }

    /// An effects layer with one routable link and a recording sink.
    fn lens_bench(
        refuse: bool,
    ) -> (
        DeviceEffects,
        Rc<RefCell<Vec<Input>>>,
        Rc<RefCell<Vec<LensLineTap>>>,
    ) {
        let mut effects = DeviceEffects::new();
        let taps = Rc::new(RefCell::new(Vec::new()));
        effects.set_transport(Rc::new(LensStubTransport {
            refuse,
            taps: Rc::clone(&taps),
        }));
        let inputs = Rc::new(RefCell::new(Vec::new()));
        let sink_inputs = Rc::clone(&inputs);
        effects.set_input_sink(move |input| sink_inputs.borrow_mut().push(input));
        effects.set_spawner(|_future| {});
        effects.links.insert(
            LinkId(1),
            LinkSlot {
                info: LinkInfo::default(),
                link: Rc::new(RefCell::new(Box::new(QuietLink(LinkInfo::default())))),
                borrowed: Rc::new(Cell::new(None)),
            },
        );
        (effects, inputs, taps)
    }

    fn borrow_events(inputs: &Rc<RefCell<Vec<Input>>>) -> Vec<bool> {
        inputs
            .borrow()
            .iter()
            .filter_map(|input| match input {
                Input::Event(Event::LinkBorrow { held, .. }) => Some(*held),
                _ => None,
            })
            .collect()
    }

    /// Attaching the lens pauses the pump (the borrow), tells the fold, and
    /// hands back an io; releasing does the reverse. The token is the
    /// lens's own, so an activity's guarded release cannot touch it.
    #[test]
    fn the_lens_borrows_the_wire_and_gives_it_back_through_the_fold() {
        let (mut effects, inputs, _taps) = lens_bench(false);

        effects
            .attach_lens_wire(LinkId(1))
            .expect("the lens gets its io");
        assert!(effects.lens_holds_wire(LinkId(1)));
        assert_eq!(borrow_events(&inputs), vec![true]);

        // A second attach refuses honestly; the borrow is untouched.
        let Err(again) = effects.attach_lens_wire(LinkId(1)) else {
            panic!("a second attach must refuse");
        };
        assert!(again.contains("already holds"), "{again}");
        assert_eq!(borrow_events(&inputs), vec![true]);

        // An activity's release with its OWN id is a no-op on the lens's
        // wire (the guard that keeps stragglers honest).
        effects.abandon_effect(LinkId(1), EffectId(7));
        assert!(effects.lens_holds_wire(LinkId(1)));

        effects.release_lens_wire(LinkId(1));
        assert!(!effects.lens_holds_wire(LinkId(1)));
        assert_eq!(borrow_events(&inputs), vec![true, false]);

        // Releasing twice is quiet.
        effects.release_lens_wire(LinkId(1));
        assert_eq!(borrow_events(&inputs), vec![true, false]);
    }

    /// Every line the lens io drains reaches the fold as the event the
    /// pump would have produced — a frame stays a frame, console output
    /// stays a line — so the card never notices the handover.
    #[test]
    fn tapped_lines_reach_the_fold_as_link_events() {
        use lpa_devices::link::LinkEvent;
        use lpa_devices::wire::ServerFrameBody;
        let (mut effects, inputs, taps) = lens_bench(false);
        effects.attach_lens_wire(LinkId(1)).expect("io");
        let tap = Rc::clone(&taps.borrow()[0]);

        tap(LensTapEvent::Line("[INFO] boot line".to_string()));
        tap(LensTapEvent::Line(
            "M!{\"id\":0,\"msg\":\"unloadProject\"}".to_string(),
        ));
        tap(LensTapEvent::PortError(
            "Serial port disconnected.".to_string(),
        ));

        let events: Vec<Input> = inputs
            .borrow()
            .iter()
            .filter(|input| matches!(input, Input::Event(Event::Link { .. })))
            .cloned()
            .collect();
        assert_eq!(events.len(), 4, "{events:?}");
        assert!(matches!(
            &events[0],
            Input::Event(Event::Link { link: LinkId(1), event: LinkEvent::Line(line) }) if line == "[INFO] boot line"
        ));
        assert!(matches!(
            &events[1],
            Input::Event(Event::Link { link: LinkId(1), event: LinkEvent::Frame(frame) })
                if matches!(frame.body, ServerFrameBody::Other { .. })
        ));
        // A port error is the pump's mark-gone rule: error, then close.
        assert!(matches!(
            &events[2],
            Input::Event(Event::Link { link: LinkId(1), event: LinkEvent::Error(message) })
                if message.contains("disconnected")
        ));
        assert!(matches!(
            &events[3],
            Input::Event(Event::Link { link: LinkId(1), event: LinkEvent::Closed { reason } })
                if reason.contains("disconnected")
        ));
    }

    /// A transport that cannot build the io leaves no half-borrow behind:
    /// the pump resumes and the fold hears both edges.
    #[test]
    fn a_refused_lens_io_gives_the_wire_straight_back() {
        let (mut effects, inputs, _taps) = lens_bench(true);

        let Err(error) = effects.attach_lens_wire(LinkId(1)) else {
            panic!("a refused io must surface");
        };
        assert!(error.contains("no port"), "{error}");
        assert!(!effects.lens_holds_wire(LinkId(1)));
        assert_eq!(borrow_events(&inputs), vec![true, false]);
    }

    /// A coarse effect cannot take the wire out from under the lens: the
    /// activity ends with the reason, and the borrow stays the lens's.
    #[test]
    fn an_effect_on_a_lens_held_wire_ends_honestly() {
        let (mut effects, inputs, _taps) = lens_bench(false);
        effects.attach_lens_wire(LinkId(1)).expect("io");

        effects.run_effect(
            DeviceId(1),
            LinkId(1),
            EffectId(3),
            EffectRequest::Flash {
                build_id: "esp32c6-4mb".to_string(),
                board_id: "seeed-xiao-esp32c6".to_string(),
            },
        );

        assert!(effects.lens_holds_wire(LinkId(1)));
        let ended = inputs.borrow().iter().any(|input| matches!(
            input,
            Input::Event(Event::ActivityMarker {
                effect: Some(EffectId(3)),
                marker: ActivityMarker::Ended { outcome: ActivityOutcome::Failed { message }, .. },
                ..
            }) if message.contains("editor is open")
        ));
        assert!(ended, "{:?}", inputs.borrow());
    }

    /// Without a transport nothing is attempted — a host build and a browser
    /// with no Web Serial both land here, and both must be quiet.
    #[test]
    fn an_unwired_effects_layer_sweeps_nothing() {
        let mut effects = DeviceEffects::new();

        assert!(!effects.is_wired());
        effects.sweep_granted_ports();
        effects.sweep_departed_ports();
        effects.settle();

        assert!(effects.take_writes().is_empty());
    }
}
