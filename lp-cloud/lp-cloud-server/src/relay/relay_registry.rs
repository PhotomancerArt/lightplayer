//! The hub plus the legs' outboxes: what the socket tasks share.
//!
//! [`RelayHub`] is sans-IO; this is where its [`HubAction`]s become
//! messages in the legs' bounded queues, and where a browser's route is put
//! to the access decision ([`RouteAdmissionPolicy`], which alone decides
//! who may reach a board). One `std::sync::Mutex` guards the hub, the
//! queues and the policy together — never held across an
//! await, and **never the store lock**: the relay's hot path (every frame)
//! touches nothing else. The only store calls on the relay's path are the
//! account lookup at a board's registration and the session lookup when a
//! browser leg opens, both through `AppState::with_service` before this
//! lock is taken.
//!
//! A leg whose queue is full is dropped from the table: its task sees its
//! queue end and closes with [`RelayCloseCode::Overloaded`]. A slow browser
//! cannot make the relay buffer without bound.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use lpc_cloud_api::{BoardPicture, BoardPresence, KnownPicture};
use lpc_history::PrefixedUid;
use lpc_relay::{RefuseReason, RelayBoardId, RelayCloseCode, RelayFrame};
use tokio::sync::{mpsc, watch};

use super::device_leg::epoch_seconds;
use super::relay_hub::{BoardRegistration, HubAction, LegId, OpenRefusal, RelayHub};
use super::route_admission::{
    Admission, InterimRouteAdmission, RouteAdmissionPolicy, RouteRequest,
};
use crate::config::RelayPictureSettings;

/// How many messages a leg's queue holds before the leg is dropped as
/// overloaded. A lp-link window is two frames; this is generous.
pub const LEG_QUEUE: usize = 64;

/// What a leg's task is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegCommand {
    /// Send one binary message.
    Send(Vec<u8>),
    /// Close the socket with this code and end.
    Close(RelayCloseCode),
}

/// See the module doc.
pub struct RelayRegistry {
    inner: Mutex<Inner>,
    next_leg: AtomicU64,
    live_legs: AtomicUsize,
    shutdown: watch::Sender<bool>,
}

struct Inner {
    hub: RelayHub,
    outboxes: HashMap<LegId, mpsc::Sender<LegCommand>>,
    admission: Box<dyn RouteAdmissionPolicy>,
}

/// A route that opened: its id on the board, and how it was admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmittedRoute {
    pub route: u16,
    pub admission: Admission,
}

/// One leg's identity and inbox, from [`RelayRegistry::open_leg`]. Dropping
/// it counts the leg as finished (for the shutdown drain).
pub struct LegHandle {
    pub leg: LegId,
    pub inbox: mpsc::Receiver<LegCommand>,
    /// The queue's sending half, until it is handed to the registry: the
    /// registry must hold the only one, so that dropping it ends the inbox.
    outbox: Option<mpsc::Sender<LegCommand>>,
    registry: Arc<RelayRegistry>,
}

impl LegHandle {
    /// The queue's sending half, for the registry. Once only.
    ///
    /// # Panics
    ///
    /// On a second call: a leg is routed once.
    pub fn take_outbox(&mut self) -> mpsc::Sender<LegCommand> {
        self.outbox
            .take()
            .expect("a leg's outbox is handed over once")
    }
}

impl Drop for LegHandle {
    fn drop(&mut self) {
        self.registry.live_legs.fetch_sub(1, Ordering::AcqRel);
    }
}

impl RelayRegistry {
    /// A registry deciding admission by the interim rule, asking boards for
    /// pictures at the default cadence.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Self::with_pictures(RelayPictureSettings::default())
    }

    /// A registry deciding admission by the interim rule, asking boards for
    /// pictures at `pictures` (`LP_CLOUD_RELAY_PICTURE_*`).
    #[must_use]
    pub fn with_pictures(pictures: RelayPictureSettings) -> Arc<Self> {
        Self::with_parts(Box::new(InterimRouteAdmission::new()), pictures)
    }

    /// A registry deciding admission by `admission`.
    #[must_use]
    pub fn with_admission(admission: Box<dyn RouteAdmissionPolicy>) -> Arc<Self> {
        Self::with_parts(admission, RelayPictureSettings::default())
    }

    fn with_parts(
        admission: Box<dyn RouteAdmissionPolicy>,
        pictures: RelayPictureSettings,
    ) -> Arc<Self> {
        // Pictures are numbered from the start time in microseconds, so a
        // reader holding a `seq` from before a deploy never takes a new
        // picture for the one it has.
        let first_seq = (epoch_seconds() * 1e6) as u64;
        Arc::new(Self {
            inner: Mutex::new(Inner {
                hub: RelayHub::with_settings(pictures, first_seq),
                outboxes: HashMap::new(),
                admission,
            }),
            next_leg: AtomicU64::new(1),
            live_legs: AtomicUsize::new(0),
            shutdown: watch::Sender::new(false),
        })
    }

    /// A new leg: its id and its queue. Not routed anywhere until it
    /// registers (a board) or opens a route (a browser).
    pub fn open_leg(self: &Arc<Self>) -> LegHandle {
        let (outbox, inbox) = mpsc::channel(LEG_QUEUE);
        self.live_legs.fetch_add(1, Ordering::AcqRel);
        LegHandle {
            leg: self.next_leg.fetch_add(1, Ordering::Relaxed),
            outbox: Some(outbox),
            inbox,
            registry: Arc::clone(self),
        }
    }

    /// Put a board online on `leg`, whose queue is `outbox`.
    pub fn register_board(
        &self,
        registration: BoardRegistration,
        outbox: mpsc::Sender<LegCommand>,
    ) -> Result<(), RefuseReason> {
        let mut inner = self.lock();
        let leg = registration.leg;
        let actions = inner.hub.register(registration)?;
        inner.outboxes.insert(leg, outbox);
        inner.apply(actions);
        Ok(())
    }

    /// A frame from a registered board.
    pub fn board_message(&self, leg: LegId, frame: RelayFrame) {
        let now = epoch_seconds();
        let mut inner = self.lock();
        let actions = inner.hub.from_board(leg, frame, now);
        inner.apply(actions);
    }

    /// A device leg ended.
    pub fn board_gone(&self, leg: LegId) {
        let mut inner = self.lock();
        inner.outboxes.remove(&leg);
        let actions = inner.hub.board_gone(leg);
        inner.apply(actions);
    }

    /// Put a browser's route to the access decision
    /// ([`super::route_admission`]) and, if admitted, open it on the board
    /// (the hub's route limit permitting).
    pub fn open_route(
        &self,
        leg: LegId,
        outbox: mpsc::Sender<LegCommand>,
        user: Option<PrefixedUid>,
        ip: Option<IpAddr>,
        board: RelayBoardId,
    ) -> Result<AdmittedRoute, RelayCloseCode> {
        let mut inner = self.lock();
        let Inner { hub, admission, .. } = &mut *inner;
        let admission = admission.admit(&RouteRequest {
            user,
            ip,
            board_id: board,
            board: hub.board(board),
            now: Instant::now(),
        })?;
        let (route, actions) = hub
            .open_route(leg, board)
            .map_err(|refusal| match refusal {
                OpenRefusal::Offline => RelayCloseCode::BoardOffline,
                OpenRefusal::Busy => RelayCloseCode::Busy,
            })?;
        inner.outboxes.insert(leg, outbox);
        inner.apply(actions);
        Ok(AdmittedRoute { route, admission })
    }

    /// A message from a browser leg.
    pub fn browser_message(&self, leg: LegId, bytes: &[u8]) {
        let mut inner = self.lock();
        let actions = inner.hub.from_browser(leg, bytes);
        inner.apply(actions);
    }

    /// A browser leg ended.
    pub fn browser_gone(&self, leg: LegId) {
        let mut inner = self.lock();
        inner.outboxes.remove(&leg);
        let actions = inner.hub.browser_gone(leg);
        inner.apply(actions);
    }

    /// `ListBoards` for `user`, calling from `ip`.
    pub fn boards_for(&self, user: PrefixedUid, ip: Option<IpAddr>) -> Vec<BoardPresence> {
        self.lock().hub.boards_for(user, ip)
    }

    /// `BoardPictures` for `user`: the pictures of the boards asked about
    /// that `user` may read, each of them watched when `watch` (the boards
    /// told so through their queues).
    pub fn pictures_for(
        &self,
        user: PrefixedUid,
        asked: &[KnownPicture],
        watch: bool,
    ) -> Vec<BoardPicture> {
        let now = epoch_seconds();
        let mut inner = self.lock();
        let (pictures, actions) = inner.hub.pictures_for(user, asked, watch, now);
        inner.apply(actions);
        pictures
    }

    /// How many boards are online.
    pub fn board_count(&self) -> usize {
        self.lock().hub.board_count()
    }

    /// The process is stopping: close every leg "going away", including
    /// legs still registering (through [`Self::shutdown_signal`]).
    pub fn going_away(&self) {
        self.shutdown.send_replace(true);
        let mut inner = self.lock();
        let actions = inner.hub.shutdown();
        inner.apply(actions);
    }

    /// Resolves (changes) when [`Self::going_away`] is called.
    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    /// Wait until every leg task has ended, or `within` passes.
    pub async fn drained(&self, within: Duration) {
        let deadline = Instant::now() + within;
        while self.live_legs.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding this lock can only be a bug in the hub; the
        // relay's state is presence, and losing it costs every board one
        // reconnect. Recover rather than abort the whole service.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Inner {
    fn apply(&mut self, actions: Vec<HubAction>) {
        for action in actions {
            let (leg, command) = match action {
                HubAction::Send { leg, bytes } => (leg, LegCommand::Send(bytes)),
                HubAction::Close { leg, code } => (leg, LegCommand::Close(code)),
            };
            let Some(outbox) = self.outboxes.get(&leg) else {
                continue;
            };
            if outbox.try_send(command).is_err() {
                // Full (overloaded) or gone: either way the leg's task ends
                // when its queue does.
                self.outboxes.remove(&leg);
            }
        }
    }
}
