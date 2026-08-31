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
use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkId, LinkInfo};
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
/// The event fires when the OS drops the device; the JS read pump learns of it
/// a beat later, when its pending read rejects. Checking immediately would see
/// a port that still looks open, so the check waits out that beat.
///
/// ⚠️ This is a settle, not a fix. `install_serial_events` hands its callbacks
/// no port, so "which port left?" is answered by "which of ours stopped being
/// open?". A per-port disconnect signal means widening the transport JS, which
/// is the bench slice's call, not this one's.
const HOTPLUG_SETTLE: Duration = Duration::from_millis(250);

/// Links attached per granted-port sweep.
///
/// Ids are minted before the sweep awaits (a spawned future cannot hold
/// `&mut self`), so the budget is stated rather than unbounded. A tab with
/// more boards than this attaches the rest on the next sweep.
const MAX_SWEEP_LINKS: usize = 8;

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
    /// Whether the port is open for traffic, folded from the events the pump
    /// carries. Not a second state machine: it is used for exactly one
    /// question — "did this port die?" — on a hotplug disconnect.
    open: Rc<Cell<bool>>,
    /// A coarse effect holds the wire: the pump stops draining until the
    /// borrow ends, so the effect's own conversation is the only reader.
    /// This is the executor's half of the exclusive-borrow discipline.
    borrowed: Rc<Cell<bool>>,
}

/// A link that arrived from a spawned future, waiting to join the routing map.
struct Arrival {
    link: LinkId,
    info: LinkInfo,
    handle: Rc<RefCell<Box<dyn Link>>>,
    open: Rc<Cell<bool>>,
    borrowed: Rc<Cell<bool>>,
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

    /// Whether the seams a real device needs are all installed.
    pub fn is_wired(&self) -> bool {
        self.transport.is_some() && self.spawn.is_some() && self.sink.is_some()
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
                    open: arrival.open,
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
        borrowed.set(true);
        let writes = Rc::clone(&self.completed_pushes);
        spawn(Box::pin(async move {
            let result = transport.run_effect(info, call, progress).await;
            // Give the wire back BEFORE the end marker folds: the reducer's
            // very next command may be the ladder's reopen, and a pump still
            // paused would eat the boot hello.
            borrowed.set(false);
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

    /// React to a `navigator.serial` disconnect: after a settle, tell the
    /// model about every link whose port stopped being open.
    ///
    /// See [`HOTPLUG_SETTLE`] for why this is a sweep rather than a targeted
    /// detach.
    pub fn sweep_departed_ports(&mut self) {
        let (Some(spawn), Some(make_timer), Some(sink)) =
            (self.spawn.clone(), self.timer.clone(), self.sink.clone())
        else {
            return;
        };
        let candidates: Vec<(LinkId, Rc<Cell<bool>>)> = self
            .links
            .iter()
            .map(|(link, slot)| (*link, Rc::clone(&slot.open)))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let sleep = (make_timer.borrow_mut())(HOTPLUG_SETTLE);
        spawn(Box::pin(async move {
            sleep.await;
            for (link, open) in candidates {
                if open.get() {
                    continue;
                }
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
            let open = Rc::new(Cell::new(false));
            let borrowed = Rc::new(Cell::new(false));
            arrivals.borrow_mut().push(Arrival {
                link,
                info: info.clone(),
                handle: Rc::clone(&handle),
                open: Rc::clone(&open),
                borrowed: Rc::clone(&borrowed),
            });
            if let (Some(spawn), Some(timer)) = (spawn.clone(), timer.clone()) {
                spawn_pump(
                    &spawn,
                    &timer,
                    link,
                    Rc::downgrade(&handle),
                    open,
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
    open: Rc<Cell<bool>>,
    borrowed: Rc<Cell<bool>>,
    sink: Rc<dyn Fn(Input)>,
) {
    let timer = Rc::clone(timer);
    spawn(Box::pin(async move {
        loop {
            let Some(strong) = handle.upgrade() else {
                return;
            };
            // Drain synchronously: `poll_event` never blocks, by contract.
            while !borrowed.get() {
                let next = strong.borrow_mut().poll_event();
                let Some(event) = next else { break };
                match &event {
                    LinkEvent::Opened { .. } => open.set(true),
                    LinkEvent::Closed { .. } => open.set(false),
                    _ => {}
                }
                sink(Input::link(link, event));
            }
            drop(strong);
            let sleep = (timer.borrow_mut())(LINK_POLL_INTERVAL);
            sleep.await;
        }
    }));
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
    }
}

#[cfg(test)]
mod tests {
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
