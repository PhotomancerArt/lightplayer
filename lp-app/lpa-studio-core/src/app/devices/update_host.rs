//! **The update host** (M7 P5): Studio's effects-layer half of an
//! over-the-air update — it holds one [`UpdateDriver`] per device, feeds it
//! the board's channel-3 bytes, performs its effects, tells the device model
//! how far it got, and ends each leg of the Update activity
//! (`lpa_devices::activity::update`) by its contract.
//!
//! ```text
//!   Update activity ── RunEffect(Update) ──► start_leg ─┐  one driver per device,
//!         ▲                                             │  kept across legs until
//!         │ UpdateStage / UpdateOutcome / Ended         ▼  the activity ends
//!         └──────────── sink ◄──────── UpdateRun { driver, leg, narration }
//!                                         ▲      │ Send → LinkCommand::SendUpdate
//!   link pump ── LinkEvent::Update ───────┘      │ LookUpCache / FetchFromStore /
//!   (never the fold, DS1)                        ▼ KeepInCache → spawned futures
//! ```
//!
//! **A leg** is one link session: [`UpdateHost::start_leg`] (the model's
//! `EffectRequest::Update`) brings the driver's link up; every routed
//! `LinkEvent::Update` goes to [`UpdateDriver::on_board`]; the link closing
//! (seen by [`UpdateHost::reconcile`] after every fold) brings it down and
//! ends the leg `Interrupted` with no outcome, which the activity reads as
//! "between legs". The driver's `Done` ends the leg with an
//! `UpdateOutcome` first, which ends the activity.
//!
//! **A board's reset need not close the port.** Over a transport that does
//! not re-enumerate (the emulator's door, a classic's UART) the board's
//! restart is an lp-link *session* reset on a port that stays open: the pump
//! reports it ([`UpdateHost::on_link_reset`]), the driver goes down there
//! and then — the old session's words are lost — and comes back up when the
//! board speaks on the new session: its `M` (core-only sends one unasked) or
//! a hello that announces channel 3 ([`UpdateHost::on_hello`]; a running
//! engine sends no `M` unasked). The leg itself carries on.
//!
//! **The driver goes when the activity goes** — on its own `Done`, on an
//! abandon, and on [`UpdateHost::reconcile`] finding the device no longer
//! running an Update (an activity that ends with no link attached raises no
//! abandon).
//!
//! **Which build** (DS7): `Install { version }` takes this Studio's own build
//! when it is that version, else the store's release of it; `Auto` and
//! `Reinstall` take this Studio's own build when it has one (the driver finds
//! another build's engine itself, through the engine-source effects). With
//! no build of its own, a restore needs none: the host finds the board's
//! engine first (cache, then store) and drives the board's own build
//! (`HostBuild::for_heal`). A build that cannot be had ends the activity
//! with an outcome, never a hang.
//!
//! **The backup is pinned** in the engine cache (`set_held`) from the moment
//! it is held — a cache hit, or the read-back kept — until the activity ends
//! on the new version; an update that ends anywhere else keeps it pinned,
//! since the board may still need it.
//!
//! **Speak channel 3 only to a board that announced it** (DS9): a leg on a
//! board whose evidence has no update facts, or a link that does not carry
//! the channel, ends at once with `NeedsUsb`.
//!
//! **Another device's transfer** (DS8): a run that meets one waits (the
//! card's `Waiting`), asking `Q` every 3 s, and starts over when the board
//! is free; a board with no activity is watched the same way
//! ([`UpdateHost::set_watches`]) so the controller can take over with no
//! click.
//!
//! **Misses are remembered** (E13): a run that did not reach its goal
//! blocks the controller's no-click start for that board's engine until the
//! board reconnects (a newer link window) or, when the store was the
//! reason, until the store is back.
//!
//! Sans-IO in the core sense: time is the controller's injected clock, waits
//! are the injected timer factory's, and every future is runtime-neutral.
//! Credentials pass through (`login_with`, `on_board`) and are never logged.

use core::time::Duration;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use lpa_devices::activity::{ActivityKind, ActivityOutcome};
use lpa_devices::event::{ActivityMarker, EffectId, Event, Input};
use lpa_devices::identity::DeviceId;
use lpa_devices::link::{Link, LinkCommand, LinkId, LinkInfo};
use lpa_devices::{Roster, UpdateFacts, UpdateIntentFacts, UpdateOutcomeFacts, UpdateStageFacts};
use lpa_firmware_store::{EngineCache, EngineCacheEntry, EngineSource as CachedFrom};
use lpa_update::decide::{EngineSource, SourceEffect, SourceResult, SourceStep, StoreAnswer};
use lpa_update::{
    BoardView, Credential, Decision, DriverConfig, DriverEffect, Finish, HostBuild, HostFacts,
    ServeConfig, StopReason, UpdateDriver, decide, decide_for_intent,
};
use lpc_access::Tier;
use lpc_update::{BoardManifest, BoardMessage, PROTO_V1, encode_query, sha256_to_hex};

use super::device_effects::{DeviceTaskFuture, DeviceTimerFuture};
use super::device_firmware_sources::StudioFirmwareStore;
use super::device_update_route::UpdateLink;
use super::device_update_version::UpdateVersion;
use super::own_build_source::{OwnBuildSource, verified_own_build};
use super::store_lookups::{StoreLookup, StoreLookups};
use super::update_auto_start::{board_build_facts, board_engine_sha};
use super::update_build_facts::{StoreLatest, StoreReleases};
use super::update_driver_mirror::{decision_facts, driver_intent, outcome_facts, stage_facts};
use super::update_narration::{NarrationNames, UpdateNarration};
use super::update_store_builds::{
    store_build, store_engine, store_latest, store_release_entry, store_releases,
};

/// How often a run's driver is ticked (a login backoff; the Waiting ask).
const TICK_INTERVAL: Duration = Duration::from_millis(250);

/// How often a board whose transfer another device owns is asked again (DS8).
const BUSY_ASK_MS: u64 = 3_000;

/// A link handle, as the effects layer holds it.
pub(crate) type LinkHandle = Weak<RefCell<Box<dyn Link>>>;

/// The platform seams the host performs effects through, refreshed from the
/// effects layer.
#[derive(Clone)]
pub(crate) struct UpdateSeams {
    pub spawn: Rc<dyn Fn(DeviceTaskFuture)>,
    pub timer: Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>,
    pub sink: Rc<dyn Fn(Input)>,
    /// The controller's clock, epoch seconds.
    pub clock: Rc<dyn Fn() -> f64>,
    pub cache: Rc<dyn EngineCache>,
    pub store: Option<Rc<StudioFirmwareStore>>,
}

impl UpdateSeams {
    fn now_ms(&self) -> u64 {
        ((self.clock)() * 1_000.0).max(0.0) as u64
    }

    fn now_secs(&self) -> f64 {
        (self.clock)()
    }

    fn marker(&self, device: DeviceId, effect: EffectId, marker: ActivityMarker) {
        (self.sink)(Input::Event(Event::ActivityMarker {
            device,
            effect: Some(effect),
            marker,
        }));
    }

    /// One terminal line, in Studio's voice.
    fn say(&self, device: DeviceId, effect: EffectId, line: String) {
        self.marker(
            device,
            effect,
            ActivityMarker::Progress {
                label: line,
                percent: None,
            },
        );
    }
}

/// What the fold knew about the board when the leg was asked for.
#[derive(Clone, Debug, Default)]
pub(crate) struct LegFacts {
    /// The board announced channel 3 and the link carries it (DS9).
    pub announced: bool,
    /// The board's update facts this window.
    pub facts: Option<UpdateFacts>,
}

/// One leg, as the effects layer starts it.
pub(crate) struct LegStart {
    pub device: DeviceId,
    pub link: LinkId,
    pub effect_id: EffectId,
    pub intent: UpdateIntentFacts,
    pub info: LinkInfo,
    pub handle: LinkHandle,
    pub facts: LegFacts,
}

/// The update host. Cheap to clone: one shared state.
#[derive(Clone)]
pub struct UpdateHost {
    state: Rc<RefCell<HostState>>,
}

impl Default for UpdateHost {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateHost {
    pub fn new() -> Self {
        Self {
            state: Rc::new_cyclic(|me| {
                RefCell::new(HostState {
                    me: me.clone(),
                    seams: None,
                    own: None,
                    credentials: Vec::new(),
                    tiers: BTreeMap::new(),
                    runs: BTreeMap::new(),
                    next_generation: 0,
                    blocks: BTreeMap::new(),
                    store_epoch: 0,
                    store_offline: false,
                    watches: BTreeMap::new(),
                    pins: BTreeMap::new(),
                    latest: None,
                    latest_asked: None,
                    releases: None,
                    releases_asked: None,
                    lookups: StoreLookups::default(),
                })
            }),
        }
    }

    pub(crate) fn set_seams(&self, seams: UpdateSeams) {
        self.state.borrow_mut().seams = Some(seams);
    }

    /// Install this Studio's own build (the shell's port).
    pub fn set_own_source(&self, source: Option<Rc<dyn OwnBuildSource>>) {
        self.state.borrow_mut().own = source;
    }

    /// This Studio's own build's facts, as its source says them now: a
    /// source may learn them after it is installed (the bundle's, which
    /// fetches its manifests), so the controller asks again after folds.
    pub(crate) fn own_facts(&self) -> Option<lpa_update::HostBuildFacts> {
        let own = self.state.borrow().own.clone();
        own.and_then(|own| own.facts())
    }

    /// The credentials the controller holds for a core-side login: this
    /// browser's and the account's keys. Replaced wholesale; never logged.
    pub(crate) fn set_credentials(&self, credentials: Vec<Credential>) {
        let mut state = self.state.borrow_mut();
        if state.credentials != credentials {
            state.credentials = credentials;
        }
    }

    /// The user's tier on `device`, when known (a Bluetooth link's grant).
    pub(crate) fn set_tier(&self, device: DeviceId, tier: Option<Tier>) {
        let mut state = self.state.borrow_mut();
        match tier {
            Some(tier) => state.tiers.insert(device, tier),
            None => state.tiers.remove(&device),
        };
    }

    /// Whether an update run holds a driver (or is preparing one) for
    /// `device`.
    pub fn is_running(&self, device: DeviceId) -> bool {
        self.state.borrow().runs.contains_key(&device)
    }

    /// Start one leg of `device`'s Update activity.
    pub(crate) fn start_leg(&self, start: LegStart) {
        self.state.borrow_mut().start_leg(start);
    }

    /// One channel-3 message from `link` (the pump's route, DS1).
    pub(crate) fn on_board(&self, link: LinkId, bytes: &[u8]) {
        self.state.borrow_mut().on_board(link, bytes);
    }

    /// `link`'s lp-link session reset while its port stayed open: the board
    /// restarted (an update's own reset, on a transport that does not
    /// re-enumerate) or the link gave up on a frame. Whatever the driver
    /// had in flight on the old session is lost: it goes down now, and up
    /// again when the board speaks on the new session ([`Self::on_hello`],
    /// or its first channel-3 message).
    pub(crate) fn on_link_reset(&self, link: LinkId) {
        self.state.borrow_mut().on_link_reset(link);
    }

    /// A hello on `link`: when it `announced` channel 3 (a split image's
    /// hello carries `firmware`), a driver waiting out a session reset
    /// there comes back up — a running engine says no `M` unasked.
    pub(crate) fn on_hello(&self, link: LinkId, announced: bool) {
        if announced {
            self.state.borrow_mut().session_back(link);
        }
    }

    /// The model abandoned the effect `effect_id` (a cancel while backing
    /// up): the run it belonged to goes.
    pub(crate) fn abandon(&self, effect_id: EffectId) {
        let mut state = self.state.borrow_mut();
        let device = state
            .runs
            .iter()
            .find(|(_, run)| run.last_effect == effect_id)
            .map(|(device, _)| *device);
        if let Some(device) = device {
            state.drop_run(device, None);
        }
    }

    /// After every fold: a leg whose link is no longer the device's open
    /// link ends `Interrupted`; a run whose device no longer runs an Update
    /// goes.
    pub(crate) fn reconcile(&self, roster: &Roster) {
        self.state.borrow_mut().reconcile(roster);
    }

    /// The boards to watch for another device's transfer (DS8): asked `Q`
    /// every 3 s while listed.
    pub(crate) fn set_watches(&self, watches: Vec<(DeviceId, LinkId, LinkHandle)>) {
        HostState::set_watches(&self.state, watches);
    }

    /// Whether the controller's no-click start is held off for `device`,
    /// whose core needs `engine_sha`, on its link `link` whose window
    /// started at `window_start_ms`: a run that did not reach its goal on
    /// this window, for this engine, and — if the store was the reason —
    /// before the store came back.
    pub(crate) fn auto_blocked(
        &self,
        device: DeviceId,
        engine_sha: Option<&str>,
        link: Option<LinkId>,
        window_start_ms: Option<u64>,
    ) -> bool {
        let state = self.state.borrow();
        let Some(block) = state.blocks.get(&device) else {
            return false;
        };
        if block.engine_sha.is_some() && block.engine_sha.as_deref() != engine_sha {
            return false;
        }
        if link != block.link {
            return false;
        }
        if window_start_ms.is_some_and(|started| started > block.at_ms) {
            return false;
        }
        !(block.offline && state.store_epoch > block.store_epoch)
    }

    /// The store is (back) online: a miss it caused may be tried again.
    pub(crate) fn note_store_online(&self) {
        let mut state = self.state.borrow_mut();
        state.store_epoch += 1;
        state.store_offline = false;
    }

    /// Ask the store for its latest release of `target`, once per target
    /// and store epoch.
    pub(crate) fn want_store_latest(&self, target: &str) {
        HostState::want_store_latest(&self.state, target);
    }

    /// The store's latest release, once it answered.
    pub(crate) fn store_latest(&self) -> Option<StoreLatest> {
        self.state.borrow().latest.clone()
    }

    /// Ask the store for its release index of `target`, once per target
    /// and store epoch (a miss, a 404 or a refused index is "no list" until
    /// the epoch moves).
    pub(crate) fn want_store_releases(&self, target: &str) {
        HostState::want_store_releases(&self.state, target);
    }

    /// The store's release index, once it answered with one.
    pub(crate) fn store_releases(&self) -> Option<StoreReleases> {
        self.state.borrow().releases.clone()
    }

    /// Ask the store for release `version` of `target` by its exact
    /// version ("Other version…"'s box), once — again only after an
    /// offline answer.
    pub(crate) fn want_store_lookup(&self, target: &str, version: &str) {
        HostState::want_store_lookup(&self.state, target, version);
    }

    /// Every look-up asked, and where each stands.
    pub(crate) fn store_lookups(&self) -> StoreLookups {
        self.state.borrow().lookups.clone()
    }
}

/// A run's remembered failure (see [`UpdateHost::auto_blocked`]).
#[derive(Clone, Debug)]
struct AutoBlock {
    engine_sha: Option<String>,
    link: Option<LinkId>,
    at_ms: u64,
    offline: bool,
    store_epoch: u64,
}

/// A board watched for another device's transfer.
struct Watch {
    generation: u64,
    handle: LinkHandle,
}

/// One device's update across its legs.
struct UpdateRun {
    generation: u64,
    intent: UpdateIntentFacts,
    serve: ServeConfig,
    link_word: UpdateLink,
    phase: RunPhase,
    /// The leg running now; `None` between legs.
    leg: Option<Leg>,
    /// The latest leg's effect stamp: what an ending between legs carries.
    last_effect: EffectId,
    /// The device's link, for a remembered miss.
    link: LinkId,
    /// The board's update facts at the latest leg's start.
    facts: Option<UpdateFacts>,
    names: Option<NarrationNames>,
    narration: UpdateNarration,
    /// The driver decided an offered update: the next engine source is the
    /// backup of this engine.
    backup_sha: Option<[u8; 32]>,
    /// A read-back ran (what a kept engine came from).
    read_back: bool,
    /// The leg's lp-link session reset (the port stayed open); the driver
    /// is down until the board speaks on the new session.
    session_reset: bool,
}

enum RunPhase {
    /// Not begun: the first leg begins it.
    New,
    /// Loading the build, or finding a restore's engine before a driver
    /// exists.
    Preparing {
        source: Option<(EngineSource, BoardManifest)>,
    },
    Driving(Box<UpdateDriver>),
    /// Another device holds the transfer; ask again at `next_ask_ms`.
    Waiting {
        next_ask_ms: u64,
    },
}

struct Leg {
    link: LinkId,
    effect_id: EffectId,
    handle: LinkHandle,
}

struct HostState {
    me: Weak<RefCell<HostState>>,
    seams: Option<UpdateSeams>,
    own: Option<Rc<dyn OwnBuildSource>>,
    credentials: Vec<Credential>,
    tiers: BTreeMap<DeviceId, Tier>,
    runs: BTreeMap<DeviceId, UpdateRun>,
    next_generation: u64,
    blocks: BTreeMap<DeviceId, AutoBlock>,
    store_epoch: u64,
    store_offline: bool,
    watches: BTreeMap<DeviceId, Watch>,
    /// The backup each device's update pinned, by engine SHA-256 (hex).
    pins: BTreeMap<DeviceId, String>,
    latest: Option<StoreLatest>,
    latest_asked: Option<(String, u64)>,
    releases: Option<StoreReleases>,
    releases_asked: Option<(String, u64)>,
    lookups: StoreLookups,
}

impl HostState {
    // ---- Legs ------------------------------------------------------------------

    fn start_leg(&mut self, start: LegStart) {
        let Some(seams) = self.seams.clone() else {
            log::warn!("an update leg was asked for before the platform seams were installed");
            return;
        };
        let device = start.device;
        let now = seams.now_ms();
        if !start.facts.announced {
            // DS9: nothing on channel 3 to a board that never offered it.
            self.runs.remove(&device);
            seams.say(
                device,
                start.effect_id,
                "this board did not offer the update channel · USB once".to_string(),
            );
            end_leg_with(
                &seams,
                device,
                start.effect_id,
                UpdateOutcomeFacts::NeedsUsb,
            );
            return;
        }
        if !self.runs.contains_key(&device) {
            self.next_generation += 1;
            let generation = self.next_generation;
            let bluetooth = start.info.endpoint.is_bluetooth();
            self.runs.insert(
                device,
                UpdateRun {
                    generation,
                    intent: start.intent.clone(),
                    serve: match bluetooth {
                        true => ServeConfig::BLE,
                        false => ServeConfig::USB,
                    },
                    link_word: match bluetooth {
                        true => UpdateLink::Bluetooth,
                        false => UpdateLink::Usb,
                    },
                    phase: RunPhase::New,
                    leg: None,
                    last_effect: start.effect_id,
                    link: start.link,
                    facts: None,
                    names: None,
                    narration: UpdateNarration::default(),
                    backup_sha: None,
                    read_back: false,
                    session_reset: false,
                },
            );
            spawn_ticks(&seams, self.me.clone(), device, generation);
        }
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        run.last_effect = start.effect_id;
        run.link = start.link;
        run.session_reset = false;
        run.facts = start.facts.facts.clone();
        let replaced = run.leg.replace(Leg {
            link: start.link,
            effect_id: start.effect_id,
            handle: start.handle,
        });
        if let Some(line) = run.narration.link_up(now) {
            seams.say(device, start.effect_id, line);
        }
        match &mut run.phase {
            RunPhase::New => self.begin(device),
            RunPhase::Driving(driver) => {
                // A leg the model moved past before this host saw its link
                // go (a reconnect that beat the close): down, then up.
                if replaced.is_some() {
                    driver.link_down(now);
                    driver.take_effects();
                }
                driver.link_up(now);
                self.process(device);
            }
            RunPhase::Waiting { next_ask_ms } => {
                *next_ask_ms = now + BUSY_ASK_MS;
                self.send(device, encode_query(PROTO_V1));
            }
            // The build (or the restore's engine) is on its way; the driver
            // brings this leg's link up when it is made.
            RunPhase::Preparing { .. } => {}
        }
    }

    /// The leg's link is gone: down for the driver, `Interrupted` for the
    /// activity.
    fn leg_lost(&mut self, device: DeviceId, reason: &str) {
        log::info!("update: {device:?}'s leg ended: {reason}");
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        let Some(leg) = run.leg.take() else {
            return;
        };
        let now = seams.now_ms();
        if let RunPhase::Driving(driver) = &mut run.phase {
            driver.link_down(now);
            // Whatever the driver says on a dead link goes nowhere.
            for effect in driver.take_effects() {
                log::debug!(
                    "update: a {} after the link went down (dropped)",
                    DriverEffectName(&effect)
                );
            }
        }
        if let Some(line) = run.narration.link_down(now) {
            seams.say(device, leg.effect_id, line);
        }
        seams.marker(
            device,
            leg.effect_id,
            ActivityMarker::Ended {
                kind: ActivityKind::Update,
                outcome: ActivityOutcome::Interrupted {
                    reason: reason.to_string(),
                },
            },
        );
    }

    fn reconcile(&mut self, roster: &Roster) {
        let devices: Vec<DeviceId> = self.runs.keys().copied().collect();
        for device in devices {
            let (kind, evidence) = match roster.device(device) {
                Some(found) => (found.activity_kind(), Some(&found.evidence)),
                None => match roster.pending().iter().find(|p| p.device_id() == device) {
                    Some(pending) => (pending.activity_kind(), Some(pending.evidence())),
                    None => {
                        if self.follow_merge(roster, device) {
                            continue;
                        }
                        (None, None)
                    }
                },
            };
            if kind != Some(ActivityKind::Update) {
                // The activity ended without the driver's word (a gap that
                // ran out, a cancel, an eviction, a forgotten board).
                let engine = evidence
                    .and_then(|e| e.update_facts())
                    .and_then(board_engine_sha);
                log::info!("update: {device:?}'s run dropped: its activity is now {kind:?}");
                self.drop_run(device, Some((engine, false)));
                continue;
            }
            let Some(leg_link) = self
                .runs
                .get(&device)
                .and_then(|run| run.leg.as_ref())
                .map(|leg| leg.link)
            else {
                continue;
            };
            let open_here =
                evidence.is_some_and(|e| e.presence.is_open() && e.link() == Some(leg_link));
            if !open_here {
                self.leg_lost(device, "the board's link closed");
            }
        }
    }

    /// `device` is gone from the roster because it was merged into another
    /// entry — an anonymous card kept with "Set up this device" whose hello
    /// then names a board Studio remembers (E13 on a known board, found in
    /// the emulator walk). The roster moved the running Update, effect stamps
    /// and all, to the surviving entry (and routes markers still addressed
    /// to the old id there, `Roster::merged_into`); the run follows it
    /// instead of being dropped, which left the card on "Finishing the
    /// update…" with nothing driving it. Answers whether it moved.
    fn follow_merge(&mut self, roster: &Roster, device: DeviceId) -> bool {
        let Some(into) = roster.merged_into(device).filter(|into| {
            roster
                .device(*into)
                .is_some_and(|d| d.activity_kind() == Some(ActivityKind::Update))
        }) else {
            return false;
        };
        if self.runs.contains_key(&into) {
            return false;
        }
        let Some(run) = self.runs.remove(&device) else {
            return false;
        };
        let generation = run.generation;
        self.runs.insert(into, run);
        if let Some(pin) = self.pins.remove(&device) {
            self.pins.insert(into, pin);
        }
        log::info!("update: {device:?} was merged into {into:?}; its update follows");
        // The run's ticks were keyed by the old id: tick the new one.
        if let Some(seams) = self.seams.clone() {
            spawn_ticks(&seams, self.me.clone(), into, generation);
        }
        true
    }

    /// Forget `device`'s run. `block`: remember a miss (the board's engine,
    /// and whether the store was offline) so no click restarts it on this
    /// link window.
    fn drop_run(&mut self, device: DeviceId, block: Option<(Option<String>, bool)>) {
        let Some(run) = self.runs.remove(&device) else {
            return;
        };
        if let (Some((engine_sha, offline)), Some(seams)) = (block, &self.seams) {
            self.blocks.insert(
                device,
                AutoBlock {
                    engine_sha,
                    link: Some(run.link),
                    at_ms: seams.now_ms(),
                    offline,
                    store_epoch: self.store_epoch,
                },
            );
        }
    }

    // ---- Beginning: which build ---------------------------------------------------

    fn begin(&mut self, device: DeviceId) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let own = self.own.clone();
        let own_facts = own.as_ref().and_then(|own| own.facts());
        let me = self.me.clone();
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        let generation = run.generation;
        let facts = run.facts.clone();
        let wants_own = match &run.intent {
            UpdateIntentFacts::Install { version, .. } => own_facts
                .as_ref()
                .is_some_and(|own| own.identity.version == *version),
            UpdateIntentFacts::Auto | UpdateIntentFacts::Reinstall => own_facts.is_some(),
        };
        run.phase = RunPhase::Preparing { source: None };
        if wants_own && let Some(own) = own {
            (seams.spawn)(Box::pin(async move {
                let loaded = verified_own_build(own.load().await, own_facts.as_ref());
                if let Some(cell) = me.upgrade() {
                    cell.borrow_mut()
                        .build_loaded(device, generation, loaded, false);
                }
            }));
            return;
        }
        if let UpdateIntentFacts::Install { version, .. } = &run.intent {
            let version = version.clone();
            let target = facts
                .as_ref()
                .and_then(|f| f.target.clone())
                .unwrap_or_default();
            let store = seams.store.clone();
            (seams.spawn)(Box::pin(async move {
                let built = store_build(store, target, version).await;
                if let Some(cell) = me.upgrade() {
                    let mut state = cell.borrow_mut();
                    let offline = built.as_ref().err().is_some_and(|miss| miss.offline);
                    state.note_store_answer(offline);
                    state.build_loaded(device, generation, built.map_err(|miss| miss.why), offline);
                }
            }));
            return;
        }
        // No build of our own: a restore drives the board's own build, once
        // its engine is found.
        self.begin_without_own(device);
    }

    /// `Auto` / `Reinstall` with no build of this Studio's own: decide on
    /// the board's own build; a restore (or a reinstall) finds its engine
    /// first, anything else ends here.
    fn begin_without_own(&mut self, device: DeviceId) {
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        let facts = run.facts.clone();
        let manifest = facts
            .as_ref()
            .and_then(|f| BoardManifest::from_json(f.manifest_json.as_bytes()).ok());
        let derived = facts.as_ref().and_then(board_build_facts);
        let (Some(manifest), Some(derived)) = (manifest, derived) else {
            return self.finish(device, UpdateOutcomeFacts::NeedsUsb, None);
        };
        let board = BoardView::from_manifest(manifest.clone());
        let host = HostFacts {
            build: &derived,
            user_tier: self.tiers.get(&device).copied(),
            allow_downgrade: false,
        };
        let intent = driver_intent(&run.intent);
        let decision = decide_for_intent(decide(&board, &host), &board, &host, intent);
        match decision {
            Decision::Heal { engine_sha, .. } | Decision::Reinstall { engine_sha, .. } => {
                let source = EngineSource::new(
                    engine_sha,
                    manifest.target.clone(),
                    manifest.build_id.clone(),
                    None,
                );
                let step = source.start();
                run.phase = RunPhase::Preparing {
                    source: Some((source, manifest)),
                };
                self.on_prep_step(device, step);
            }
            Decision::Busy { done, total } => self.enter_waiting(device, &decision, done, total),
            other => {
                let outcome = decision_facts(&other);
                self.finish(device, outcome, None);
            }
        }
    }

    fn build_loaded(
        &mut self,
        device: DeviceId,
        generation: u64,
        loaded: Result<HostBuild, String>,
        offline: bool,
    ) {
        let Some(run) = self
            .runs
            .get(&device)
            .filter(|run| run.generation == generation)
        else {
            return;
        };
        match loaded {
            Ok(build) => self.start_driver(device, build),
            Err(why) => {
                let to = match &run.intent {
                    UpdateIntentFacts::Install { version, .. } => version.clone(),
                    _ => "this Studio's firmware".to_string(),
                };
                let line = format!("could not load {to}: {why}");
                self.finish(
                    device,
                    UpdateOutcomeFacts::MissingEngine { offline },
                    Some(line),
                );
            }
        }
    }

    fn start_driver(&mut self, device: DeviceId, build: HostBuild) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let tier = self.tiers.get(&device).copied();
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        let board = run
            .facts
            .as_ref()
            .and_then(|f| f.version.clone().map(|v| (v, f.build_id.clone())))
            .map(|(version, build_id)| UpdateVersion { version, build_id })
            .unwrap_or_else(|| UpdateVersion::new(""));
        let finishing = run
            .facts
            .as_ref()
            .and_then(|f| BoardView::from_json(f.manifest_json.as_bytes()))
            .is_some_and(|board| {
                board.core_sha256() == Some(build.core.sha256)
                    && matches!(
                        board.state(),
                        Some(lpc_update::BoardState::OnTrial | lpc_update::BoardState::Updating)
                    )
            });
        run.names = Some(NarrationNames {
            board,
            to: UpdateVersion::with_build_id(&build.identity.version, &build.identity.build_id),
            link: run.link_word,
            finishing,
        });
        let config = DriverConfig {
            serve: run.serve,
            user_tier: tier,
            intent: driver_intent(&run.intent),
            ..DriverConfig::default()
        };
        let mut driver = Box::new(UpdateDriver::new(build, config));
        if run.leg.is_some() {
            driver.link_up(seams.now_ms());
        }
        run.phase = RunPhase::Driving(driver);
        self.process(device);
    }

    // ---- The driver's effects ------------------------------------------------------

    /// Perform the driver's effects until it asks for nothing more.
    fn process(&mut self, device: DeviceId) {
        loop {
            let Some(run) = self.runs.get_mut(&device) else {
                return;
            };
            let RunPhase::Driving(driver) = &mut run.phase else {
                return;
            };
            let effects = driver.take_effects();
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                if !self.runs.contains_key(&device) {
                    return;
                }
                self.perform(device, effect);
            }
        }
    }

    fn perform(&mut self, device: DeviceId, effect: DriverEffect) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let now = seams.now_ms();
        match effect {
            DriverEffect::Send(bytes) => {
                if bytes.first() == Some(&b'O')
                    && let Some(run) = self.runs.get_mut(&device)
                {
                    run.narration.offered();
                }
                self.send(device, bytes);
            }
            DriverEffect::NeedCredentials => {
                // The browser's and the account's keys; the login client
                // answers only an offer whose salt it holds. None held: the
                // driver ends `NoCredentials` ("needs the author password").
                let held = self.credentials.clone();
                if let Some(RunPhase::Driving(driver)) =
                    self.runs.get_mut(&device).map(|run| &mut run.phase)
                {
                    driver.login_with(&held);
                }
            }
            DriverEffect::Source(source) => self.source_effect(device, source),
            DriverEffect::Progress { stage, done, total } => {
                let Some(run) = self.runs.get_mut(&device) else {
                    return;
                };
                let stage = stage_facts(stage);
                run.read_back |= stage == UpdateStageFacts::BackingUp;
                if let Some(line) = run.narration.progress(now, stage, done, total) {
                    seams.say(device, run.last_effect, line);
                }
                seams.marker(
                    device,
                    run.last_effect,
                    ActivityMarker::UpdateStage { stage, done, total },
                );
            }
            DriverEffect::Decided(decision) => {
                log::info!("update: {device:?} decided {decision:?}");
                self.decided(device, decision)
            }
            DriverEffect::Done(finish) => {
                if matches!(
                    finish,
                    Finish::Stopped(StopReason::Decision(Decision::Busy { .. }))
                ) {
                    // Waiting already: the decision came first.
                    return;
                }
                let outcome = outcome_facts(&finish);
                self.finish(device, outcome, None);
            }
        }
    }

    /// The driver's decision on the board's latest manifest.
    fn decided(&mut self, device: DeviceId, decision: Decision) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        if !matches!(decision, Decision::Busy { .. })
            && let Some(names) = &run.names
            && let Some(line) = run.narration.decided(&decision, names)
        {
            seams.say(device, run.last_effect, line);
        }
        match decision {
            Decision::OfferUpdate { .. } => {
                if let RunPhase::Driving(driver) = &run.phase {
                    run.backup_sha = driver.board().engine_sha256();
                }
                // An offered update waits for a go only an Install gives: a
                // run that started for anything else has done its part (a
                // restore whose board runs its own build again).
                if !matches!(run.intent, UpdateIntentFacts::Install { .. }) {
                    self.finish(device, UpdateOutcomeFacts::UpToDate, None);
                }
            }
            Decision::Busy { done, total } => {
                self.enter_waiting(device, &decision, done, total);
            }
            _ => {}
        }
    }

    /// One engine-source effect, for the driver or a restore being prepared.
    fn source_effect(&mut self, device: DeviceId, effect: SourceEffect) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(run) = self.runs.get(&device) else {
            return;
        };
        let generation = run.generation;
        let me = self.me.clone();
        match effect {
            SourceEffect::LookUpCache { sha } => {
                let cache = Rc::clone(&seams.cache);
                let now = seams.now_secs();
                (seams.spawn)(Box::pin(async move {
                    let hit = cache.get(&sha256_to_hex(&sha), now).await.ok();
                    if let Some(cell) = me.upgrade() {
                        cell.borrow_mut()
                            .cache_answered(device, generation, sha, hit);
                    }
                }));
            }
            SourceEffect::FetchFromStore {
                target,
                build_id,
                sha,
            } => {
                let store = seams.store.clone();
                (seams.spawn)(Box::pin(async move {
                    let answer = store_engine(store, target, build_id, sha).await;
                    if let Some(cell) = me.upgrade() {
                        let mut state = cell.borrow_mut();
                        state.note_store_answer(answer == StoreAnswer::Offline);
                        state.source_answered(device, generation, SourceResult::Store(answer));
                    }
                }));
            }
            SourceEffect::KeepInCache { sha, bytes } => {
                let backup = run.backup_sha == Some(sha);
                let from = match run.read_back {
                    true => CachedFrom::ReadBack,
                    false => CachedFrom::Fetched,
                };
                let mut entry = EngineCacheEntry::new(
                    sha256_to_hex(&sha),
                    bytes.len() as u64,
                    from,
                    seams.now_secs(),
                );
                if let Some(facts) = &run.facts {
                    entry.target = facts.target.clone();
                    entry.build_id = facts.build_id.clone();
                    entry.version = facts.version.clone();
                }
                let cache = Rc::clone(&seams.cache);
                (seams.spawn)(Box::pin(async move {
                    let hex = entry.sha256.clone();
                    if let Err(error) = cache.put(entry, bytes).await {
                        log::warn!("engine cache: {hex} not kept: {error}");
                        return;
                    }
                    if backup {
                        pin(&cache, &hex, &me, device).await;
                    }
                }));
            }
            // The driver reads back itself, over its link; a restore never
            // reads back (there is no engine to read).
            SourceEffect::ReadBack { .. } => {
                log::warn!("update: a read-back asked of the host; ignored");
            }
        }
    }

    fn cache_answered(
        &mut self,
        device: DeviceId,
        generation: u64,
        sha: [u8; 32],
        hit: Option<Vec<u8>>,
    ) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(run) = self
            .runs
            .get(&device)
            .filter(|run| run.generation == generation)
        else {
            return;
        };
        if hit.is_some() && run.backup_sha == Some(sha) {
            // The backup is already here: pinned while the update runs.
            if let Some(names) = &run.names {
                seams.say(
                    device,
                    run.last_effect,
                    run.narration.backup_cached(&names.board),
                );
            }
            let cache = Rc::clone(&seams.cache);
            let me = self.me.clone();
            let hex = sha256_to_hex(&sha);
            (seams.spawn)(Box::pin(async move {
                pin(&cache, &hex, &me, device).await;
            }));
        }
        self.source_answered(device, generation, SourceResult::Cache(hit));
    }

    fn source_answered(&mut self, device: DeviceId, generation: u64, result: SourceResult) {
        let Some(run) = self
            .runs
            .get_mut(&device)
            .filter(|run| run.generation == generation)
        else {
            return;
        };
        match &mut run.phase {
            RunPhase::Driving(driver) => {
                driver.source_result(result);
                self.process(device);
            }
            RunPhase::Preparing {
                source: Some((source, _)),
            } => {
                let step = source.on_result(result);
                self.on_prep_step(device, step);
            }
            _ => {}
        }
    }

    /// A restore being prepared (no build of our own): the engine source's
    /// next step.
    fn on_prep_step(&mut self, device: DeviceId, step: SourceStep) {
        match step {
            SourceStep::Ask(effect) => self.source_effect(device, effect),
            SourceStep::Held { bytes, keep } => {
                if let Some(keep) = keep {
                    self.source_effect(device, keep);
                }
                let manifest = match self.runs.get(&device).map(|run| &run.phase) {
                    Some(RunPhase::Preparing {
                        source: Some((_, manifest)),
                    }) => manifest.clone(),
                    _ => return,
                };
                match HostBuild::for_heal(&manifest, bytes) {
                    Ok(build) => self.start_driver(device, build),
                    Err(error) => {
                        log::warn!("update: the board's engine does not make its build: {error:?}");
                        self.finish(
                            device,
                            UpdateOutcomeFacts::MissingEngine { offline: false },
                            None,
                        );
                    }
                }
            }
            SourceStep::Missing { offline } => {
                self.finish(device, UpdateOutcomeFacts::MissingEngine { offline }, None);
            }
        }
    }

    // ---- Waiting on another device (DS8) -----------------------------------------

    fn enter_waiting(&mut self, device: DeviceId, decision: &Decision, done: u32, total: u32) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        let names = run.names.clone().unwrap_or_else(|| NarrationNames {
            board: UpdateVersion::new(""),
            to: UpdateVersion::new(""),
            link: run.link_word,
            finishing: false,
        });
        let was_waiting = matches!(run.phase, RunPhase::Waiting { .. });
        run.phase = RunPhase::Waiting {
            next_ask_ms: seams.now_ms() + BUSY_ASK_MS,
        };
        if !was_waiting && let Some(line) = run.narration.decided(decision, &names) {
            seams.say(device, run.last_effect, line);
        }
        seams.marker(
            device,
            run.last_effect,
            ActivityMarker::UpdateStage {
                stage: UpdateStageFacts::Waiting,
                done,
                total,
            },
        );
    }

    /// A manifest while waiting: still another device's, or free — then
    /// the run starts over on this link.
    fn waiting_heard(&mut self, device: DeviceId, bytes: &[u8]) {
        let Ok(BoardMessage::Manifest(json)) = BoardMessage::decode(bytes) else {
            return;
        };
        let Ok(manifest) = BoardManifest::from_json(json) else {
            return;
        };
        match manifest.transfer {
            Some(t) if t.busy => {
                let Some(seams) = self.seams.clone() else {
                    return;
                };
                if let Some(run) = self.runs.get(&device) {
                    seams.marker(
                        device,
                        run.last_effect,
                        ActivityMarker::UpdateStage {
                            stage: UpdateStageFacts::Waiting,
                            done: t.done,
                            total: t.total,
                        },
                    );
                }
            }
            _ => {
                let Some(run) = self.runs.get_mut(&device) else {
                    return;
                };
                if let Some(facts) = &mut run.facts {
                    facts.manifest_json = String::from_utf8_lossy(json).into_owned();
                }
                // Free: start over on this link, from the build up (the
                // board's manifest decides afresh, DM9).
                run.phase = RunPhase::New;
                self.begin(device);
            }
        }
    }

    fn set_watches(cell: &Rc<RefCell<Self>>, watches: Vec<(DeviceId, LinkId, LinkHandle)>) {
        let mut state = cell.borrow_mut();
        let wanted: Vec<DeviceId> = watches.iter().map(|(device, ..)| *device).collect();
        state.watches.retain(|device, _| wanted.contains(device));
        let Some(seams) = state.seams.clone() else {
            return;
        };
        for (device, _link, handle) in watches {
            if state.watches.contains_key(&device) {
                continue;
            }
            state.next_generation += 1;
            let generation = state.next_generation;
            state.watches.insert(device, Watch { generation, handle });
            let me = state.me.clone();
            let timer = Rc::clone(&seams.timer);
            (seams.spawn)(Box::pin(async move {
                loop {
                    let sleep = (timer.borrow_mut())(Duration::from_millis(BUSY_ASK_MS));
                    sleep.await;
                    let Some(cell) = me.upgrade() else {
                        return;
                    };
                    let state = cell.borrow();
                    let Some(watch) = state
                        .watches
                        .get(&device)
                        .filter(|watch| watch.generation == generation)
                    else {
                        return;
                    };
                    if let Some(link) = watch.handle.upgrade() {
                        link.borrow_mut()
                            .submit(LinkCommand::SendUpdate(encode_query(PROTO_V1)));
                    }
                }
            }));
        }
    }

    // ---- Routing (DS1) ---------------------------------------------------------------

    /// The device whose leg runs on `link`.
    fn leg_on(&self, link: LinkId) -> Option<DeviceId> {
        self.runs
            .iter()
            .find(|(_, run)| run.leg.as_ref().is_some_and(|leg| leg.link == link))
            .map(|(device, _)| *device)
    }

    /// See [`UpdateHost::on_link_reset`].
    fn on_link_reset(&mut self, link: LinkId) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(device) = self.leg_on(link) else {
            return;
        };
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        let RunPhase::Driving(driver) = &mut run.phase else {
            return;
        };
        let now = seams.now_ms();
        log::info!("update: {device:?}'s link session reset; the driver waits for the board");
        driver.link_down(now);
        // Whatever the driver says on a session that is gone goes nowhere.
        for effect in driver.take_effects() {
            log::debug!(
                "update: a {} after the session reset (dropped)",
                DriverEffectName(&effect)
            );
        }
        run.session_reset = true;
        let effect = run.last_effect;
        if let Some(line) = run.narration.link_down(now) {
            seams.say(device, effect, line);
        }
    }

    /// The board spoke on the new session of `link`: a driver down since a
    /// session reset comes back up, and the reconnect is narrated.
    fn session_back(&mut self, link: LinkId) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(device) = self.leg_on(link) else {
            return;
        };
        let Some(run) = self.runs.get_mut(&device) else {
            return;
        };
        if !core::mem::replace(&mut run.session_reset, false) {
            return;
        }
        log::info!("update: {device:?}'s board is back on a new link session");
        let now = seams.now_ms();
        let effect = run.last_effect;
        if let Some(line) = run.narration.link_up(now) {
            seams.say(device, effect, line);
        }
        if let RunPhase::Driving(driver) = &mut run.phase {
            driver.link_up(now);
            self.process(device);
        }
    }

    fn on_board(&mut self, link: LinkId, bytes: &[u8]) {
        // The board's first word on a new session ends a reset's gap.
        self.session_back(link);
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(device) = self.leg_on(link) else {
            // No leg on this link (a board's own `M` on link-up, a watched
            // board's answer): its facts reach the fold decoded.
            if !self.runs.is_empty() {
                log::info!(
                    "update: a {} on {link:?}, where no update leg runs",
                    lpc_wire_type(bytes)
                );
            }
            return;
        };
        let now = seams.now_ms();
        if bytes.first() == Some(&b'M') {
            log::debug!("update: an M for {device:?}");
        }
        match self.runs.get_mut(&device).map(|run| &mut run.phase) {
            Some(RunPhase::Driving(driver)) => {
                driver.on_board(now, bytes, &self.credentials);
                self.process(device);
            }
            Some(RunPhase::Waiting { .. }) => self.waiting_heard(device, bytes),
            Some(RunPhase::New | RunPhase::Preparing { .. }) | None => {}
        }
    }

    fn send(&self, device: DeviceId, bytes: Vec<u8>) {
        let Some(link) = self
            .runs
            .get(&device)
            .and_then(|run| run.leg.as_ref())
            .and_then(|leg| leg.handle.upgrade())
        else {
            return;
        };
        link.borrow_mut().submit(LinkCommand::SendUpdate(bytes));
    }

    // ---- Time ------------------------------------------------------------------------

    fn tick(&mut self, device: DeviceId, generation: u64) -> bool {
        let Some(seams) = self.seams.clone() else {
            return true;
        };
        let now = seams.now_ms();
        let Some(run) = self
            .runs
            .get_mut(&device)
            .filter(|run| run.generation == generation)
        else {
            return false;
        };
        match &mut run.phase {
            RunPhase::Driving(driver) => {
                driver.tick(now);
                self.process(device);
            }
            RunPhase::Waiting { next_ask_ms } if now >= *next_ask_ms => {
                *next_ask_ms = now + BUSY_ASK_MS;
                if run.leg.is_some() {
                    self.send(device, encode_query(PROTO_V1));
                }
            }
            _ => {}
        }
        true
    }

    // ---- Ending ----------------------------------------------------------------------

    /// End the run on `outcome`: the narration's last lines, the outcome, the
    /// leg's end. The backup is let go on the new version; anything else is
    /// remembered as a miss for the no-click start.
    fn finish(&mut self, device: DeviceId, outcome: UpdateOutcomeFacts, line: Option<String>) {
        let Some(seams) = self.seams.clone() else {
            return;
        };
        let Some(mut run) = self.runs.remove(&device) else {
            return;
        };
        log::info!("update: {device:?} ended: {outcome:?}");
        let effect_id = run
            .leg
            .as_ref()
            .map_or(run.last_effect, |leg| leg.effect_id);
        if let Some(line) = line {
            seams.say(device, effect_id, line);
        }
        let board = run
            .names
            .as_ref()
            .map(|names| names.board.short())
            .or_else(|| run.facts.as_ref().and_then(|f| f.version.clone()))
            .unwrap_or_else(|| "its firmware".to_string());
        match outcome {
            UpdateOutcomeFacts::UpToDate => {
                if let Some(rates) = run.narration.rates() {
                    seams.say(device, effect_id, rates);
                }
            }
            UpdateOutcomeFacts::MissingEngine { offline: false } => {
                seams.say(
                    device,
                    effect_id,
                    format!("no copy of {board} here or online"),
                );
            }
            UpdateOutcomeFacts::MissingEngine { offline: true } => seams.say(
                device,
                effect_id,
                format!("no copy of {board} here, and the release store is unreachable"),
            ),
            _ => {}
        }
        end_leg_with(&seams, device, effect_id, outcome);
        if outcome.is_up_to_date() {
            if let Some(hex) = self.pins.remove(&device) {
                let cache = Rc::clone(&seams.cache);
                (seams.spawn)(Box::pin(async move {
                    if let Err(error) = cache.set_held(&hex, false).await {
                        log::debug!("engine cache: {hex} not released: {error}");
                    }
                }));
            }
            self.blocks.remove(&device);
        } else {
            self.blocks.insert(
                device,
                AutoBlock {
                    engine_sha: run.facts.as_ref().and_then(board_engine_sha),
                    link: Some(run.link),
                    at_ms: seams.now_ms(),
                    offline: matches!(outcome, UpdateOutcomeFacts::MissingEngine { offline: true }),
                    store_epoch: self.store_epoch,
                },
            );
        }
    }

    // ---- The store ---------------------------------------------------------------------

    /// A store answer: an offline one is remembered; an answer after one is
    /// the store back.
    fn note_store_answer(&mut self, offline: bool) {
        if offline {
            self.store_offline = true;
        } else if self.store_offline {
            self.store_offline = false;
            self.store_epoch += 1;
        }
    }

    fn want_store_latest(cell: &Rc<RefCell<Self>>, target: &str) {
        let mut state = cell.borrow_mut();
        let Some(seams) = state.seams.clone() else {
            return;
        };
        let Some(store) = seams.store.clone() else {
            return;
        };
        let asked = (target.to_string(), state.store_epoch);
        if state.latest_asked.as_ref() == Some(&asked) {
            return;
        }
        state.latest_asked = Some(asked);
        let me = state.me.clone();
        let target = target.to_string();
        (seams.spawn)(Box::pin(async move {
            let answer = store_latest(store, target).await;
            let Some(cell) = me.upgrade() else {
                return;
            };
            let mut state = cell.borrow_mut();
            match answer {
                Ok(facts) => {
                    state.note_store_answer(false);
                    state.latest = facts.map(|facts| StoreLatest { facts });
                }
                Err(miss) => {
                    log::debug!("firmware store: no latest: {}", miss.why);
                    if miss.offline {
                        state.note_store_answer(true);
                        // Asked again when the store is back.
                        state.latest_asked = None;
                    }
                }
            }
        }));
    }

    fn want_store_releases(cell: &Rc<RefCell<Self>>, target: &str) {
        let mut state = cell.borrow_mut();
        let Some(seams) = state.seams.clone() else {
            return;
        };
        let Some(store) = seams.store.clone() else {
            return;
        };
        let asked = (target.to_string(), state.store_epoch);
        if state.releases_asked.as_ref() == Some(&asked) {
            return;
        }
        state.releases_asked = Some(asked);
        let me = state.me.clone();
        let target = target.to_string();
        (seams.spawn)(Box::pin(async move {
            let answer = store_releases(store, target).await;
            let Some(cell) = me.upgrade() else {
                return;
            };
            let mut state = cell.borrow_mut();
            match answer {
                Ok(index) => {
                    state.note_store_answer(false);
                    state.releases = index.map(|index| StoreReleases { index });
                }
                Err(miss) => {
                    log::debug!("firmware store: no release index: {}", miss.why);
                    if miss.offline {
                        state.note_store_answer(true);
                        // Asked again when the store is back.
                        state.releases_asked = None;
                    }
                }
            }
        }));
    }

    fn want_store_lookup(cell: &Rc<RefCell<Self>>, target: &str, version: &str) {
        let mut state = cell.borrow_mut();
        if !state.lookups.wants(target, version) {
            return;
        }
        let Some(seams) = state.seams.clone() else {
            return;
        };
        let Some(store) = seams.store.clone() else {
            state.lookups.set(target, version, StoreLookup::Offline);
            return;
        };
        state.lookups.set(target, version, StoreLookup::Looking);
        let me = state.me.clone();
        let (target, version) = (target.to_string(), version.to_string());
        (seams.spawn)(Box::pin(async move {
            let answer = store_release_entry(store, target.clone(), version.clone()).await;
            let Some(cell) = me.upgrade() else {
                return;
            };
            let mut state = cell.borrow_mut();
            let lookup = match answer {
                Ok(Some(entry)) => StoreLookup::Found(entry),
                Ok(None) => StoreLookup::Missing,
                Err(miss) if miss.offline => StoreLookup::Offline,
                Err(miss) => {
                    log::debug!("firmware store: {version} refused: {}", miss.why);
                    StoreLookup::Missing
                }
            };
            state.note_store_answer(lookup == StoreLookup::Offline);
            state.lookups.set(&target, &version, lookup);
        }));
    }
}

/// The leg's end after a typed outcome (the activity ends on it).
fn end_leg_with(
    seams: &UpdateSeams,
    device: DeviceId,
    effect_id: EffectId,
    outcome: UpdateOutcomeFacts,
) {
    seams.marker(device, effect_id, ActivityMarker::UpdateOutcome(outcome));
    seams.marker(
        device,
        effect_id,
        ActivityMarker::Ended {
            kind: ActivityKind::Update,
            outcome: outcome.activity_outcome(),
        },
    );
}

/// Pin `hex` in the cache and remember it as `device`'s backup.
async fn pin(
    cache: &Rc<dyn EngineCache>,
    hex: &str,
    me: &Weak<RefCell<HostState>>,
    device: DeviceId,
) {
    match cache.set_held(hex, true).await {
        Ok(()) => {
            if let Some(cell) = me.upgrade() {
                cell.borrow_mut().pins.insert(device, hex.to_string());
            }
        }
        Err(error) => log::warn!("engine cache: backup {hex} not pinned: {error}"),
    }
}

/// A run's tick loop: ends when the run does.
fn spawn_ticks(
    seams: &UpdateSeams,
    me: Weak<RefCell<HostState>>,
    device: DeviceId,
    generation: u64,
) {
    let timer = Rc::clone(&seams.timer);
    (seams.spawn)(Box::pin(async move {
        loop {
            let sleep = (timer.borrow_mut())(TICK_INTERVAL);
            sleep.await;
            let Some(cell) = me.upgrade() else {
                return;
            };
            if !cell.borrow_mut().tick(device, generation) {
                return;
            }
        }
    }));
}

/// A driver effect's name for a debug line — never its bytes, never a
/// credential.
struct DriverEffectName<'a>(&'a DriverEffect);

impl core::fmt::Display for DriverEffectName<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self.0 {
            DriverEffect::Send(_) => "send",
            DriverEffect::NeedCredentials => "credentials",
            DriverEffect::Source(_) => "source",
            DriverEffect::Progress { .. } => "progress",
            DriverEffect::Decided(_) => "decision",
            DriverEffect::Done(_) => "done",
        };
        f.write_str(name)
    }
}

/// A channel-3 message's type, for a log line.
fn lpc_wire_type(bytes: &[u8]) -> String {
    match bytes.first() {
        Some(ty) if ty.is_ascii_graphic() => char::from(*ty).to_string(),
        Some(ty) => format!("{ty:#04x}"),
        None => "empty message".to_string(),
    }
}
