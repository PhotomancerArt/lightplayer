//! One virtual LAN shared by every board on it, and by the host.
//!
//! A [`VirtualLan`] is a value; [`SharedLan`] is a cloneable handle to one
//! (`Arc<Mutex<…>>`, `Send + Sync`), so a chip machine answering its
//! network seam, a host printing a board's forward, a test's probe and a
//! runner delivering frames all reach the same network — and `emu serve`'s
//! boards, each on its own OS thread, can share one.
//!
//! # Who drives delivery ([`LanDriver`])
//!
//! - **[`LanDriver::Runner`]**: a runner calls [`SeamMedium::deliver`] at its
//!   quantum boundaries with every machine's endpoints (the lockstep runner,
//!   `Lockstep::with_medium(Box::new(lan.clone()))`). Deterministic, and the
//!   only form a multi-board test or CI uses. Machines never drive it.
//! - **[`LanDriver::SelfDriven`]**: the board's machine drives it itself, at
//!   the top of its slices ([`SharedLan::pump`]), on its **guest clock**. For
//!   one board (`emu run`, the tab): deterministic too. A board's restart
//!   resets its guest clock to zero; the LAN's clock carries on from where it
//!   was (an offset is taken), so a lease's and a stack's timers never run
//!   backwards.
//! - **[`LanDriver::WallClock`]**: each machine drives it, on threads of its
//!   own, and the LAN's clock is the host's — guest cycles at the chip's rate
//!   since the LAN was made — because boards that each keep their own guest
//!   clock have no shared one (`emu serve`, plan MD19). **Not
//!   deterministic**, and never what a test that asserts runs on.
//!
//! In both self-driven forms each attached board has a **mailbox** inside the
//! handle: a proxy endpoint the LAN delivers into, which the board's machine
//! syncs with its own endpoint when it pumps (what it gave goes in, what
//! arrived comes out). That is what lets a board deliver to another board
//! whose endpoint is on another thread.
//!
//! # What is cheap
//!
//! A pump that has nothing to carry, and whose LAN has nothing due, does not
//! run the LAN at all: the handle keeps the LAN's next due cycle
//! ([`VirtualLan::next_due`]) and a host edge (a port forward's sockets,
//! wall-clock by nature) is polled at most once a millisecond of host time.
//!
//! # A board's pace ([`super::lan_pace`], [`super::lan_host_pace`])
//!
//! A board held to the host's clock waits at its next pump when it has run
//! ahead, and pumps at least every [`HOST_PACE_STEP_US`] of its guest time,
//! so an idle skip cannot leap past what the host has had time to say. Which
//! boards are held is each board's pace ([`SharedLan::set_pace`]):
//!
//! - **unset** (every board until a host sets one): held while a host is
//!   connected through any of the LAN's forwards. Without that an idle
//!   board's timers fire long before a host could have answered (an upload
//!   over the LAN resent 40–95 frames with nothing lost);
//! - [`Pace::Realtime`]: held for the whole run, host or no host;
//! - [`Pace::Max`]: never held.
//!
//! It is the one way the host's clock changes how fast a run goes. On a
//! [`LanDriver::Runner`] LAN nothing waits, and it refuses `realtime`.

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use lp_emu_core::sched::Cycles;

use crate::air::ParticipantId;
use crate::seam::{EndpointEvent, EndpointId, SeamEndpoint, SeamMedium};

use super::lan_frame::MAX_FRAME_LEN;
use super::lan_host_pace::HostPace;
use super::lan_pace::Pace;
use super::lan_station::StationEvent;
use super::virtual_access_point::ScanRecord;
use super::virtual_lan::{VirtualLan, net_pacer_config};

/// The network seam's endpoint label: a board's endpoint is `<board>/net`.
pub const NET_SEAM: &str = "net";

/// How often a port forward's host sockets are looked at, in host time.
const FORWARD_POLL: Duration = Duration::from_millis(1);

/// While a board is held to the host's clock, it pumps at least this often in
/// its guest time (µs), so a host's bytes reach it within about this much of
/// its time (the USB door's poll, `LIVE_POLL_CYCLES`, is the same
/// millisecond).
pub const HOST_PACE_STEP_US: u64 = 1_000;

/// Board `board`'s network endpoint, `<board>/net`.
pub fn net_endpoint(board: ParticipantId) -> EndpointId {
    EndpointId {
        board,
        seam: NET_SEAM,
    }
}

/// Who drives a shared LAN's delivery, and on whose clock. See [the module
/// docs](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanDriver {
    /// A runner's quantum boundaries ([`SeamMedium::deliver`]).
    Runner,
    /// The one board's own machine, on its guest clock.
    SelfDriven,
    /// Every board's machine, on the host's clock.
    WallClock,
}

/// A cloneable, thread-safe handle to one [`VirtualLan`].
#[derive(Clone)]
pub struct SharedLan(Arc<Mutex<Shared>>);

struct Shared {
    lan: VirtualLan,
    driver: LanDriver,
    /// `SelfDriven`: LAN cycle = guest cycle + `offset`; `last` is the LAN
    /// cycle the last reading gave, so a guest clock that went back (a
    /// restart) moves the offset instead of the LAN's time.
    offset: Cycles,
    last: Cycles,
    /// `WallClock`: when the LAN was made.
    born: Instant,
    /// One per attached board, in attach order.
    mailboxes: Vec<SeamEndpoint>,
    /// The LAN's next due cycle, as of its last change.
    next_due: Option<Cycles>,
    /// The host changed the LAN in a way its due cycle cannot show (a probe
    /// queued a query): drive it at the next chance.
    dirty: bool,
    forwards_polled: Option<Instant>,
    /// Each board's pace against the host's clock, by endpoint.
    paces: Vec<BoardPace>,
}

/// One board's pace: what its host set, and where it stands.
struct BoardPace {
    board: EndpointId,
    /// `None`: held only while a host is connected ([`super::lan_pace`]).
    set: Option<Pace>,
    pace: HostPace,
}

impl SharedLan {
    /// Share `lan`, driven by `driver`.
    pub fn new(lan: VirtualLan, driver: LanDriver) -> Self {
        Self(Arc::new(Mutex::new(Shared {
            lan,
            driver,
            offset: 0,
            last: 0,
            born: Instant::now(),
            mailboxes: Vec::new(),
            next_due: None,
            dirty: true,
            forwards_polled: None,
            paces: Vec::new(),
        })))
    }

    pub fn driver(&self) -> LanDriver {
        self.inner().driver
    }

    /// Whether `self` and `other` are handles to the same LAN.
    pub fn same_lan(&self, other: &SharedLan) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    // --- The host's side -----------------------------------------------------

    /// Put a board on the LAN: its endpoint (`<board>/net`, [`net_endpoint`])
    /// and its station MAC. Returns the address its lease reserved.
    ///
    /// **Idempotent**, keyed by the pair: attaching the same endpoint with the
    /// same MAC again (a host that attached before the build, then the
    /// machine at its chip start; a board that restarted) changes nothing.
    /// The same endpoint with another MAC, or the same MAC on another
    /// endpoint, is refused — two boards with one MAC would share a lease and
    /// steal each other's frames.
    pub fn attach(&self, board: EndpointId, mac: [u8; 6]) -> Result<Option<Ipv4Addr>, String> {
        let mut g = self.inner();
        for s in g.lan.stations() {
            if s.endpoint == board && s.mac != mac {
                return Err(format!(
                    "board {board} is already on this LAN with MAC {}",
                    super::lan_frame::mac_to_string(&s.mac)
                ));
            }
            if s.endpoint != board && s.mac == mac {
                return Err(format!(
                    "MAC {} is already on this LAN as board {}: give each board its own",
                    super::lan_frame::mac_to_string(&mac),
                    s.endpoint
                ));
            }
        }
        let ip = g.lan.attach(board, mac);
        if !g.mailboxes.iter().any(|m| m.id == board) {
            g.mailboxes
                .push(SeamEndpoint::new(board, 1, net_pacer_config()));
        }
        g.changed();
        Ok(ip)
    }

    /// Forward a host TCP port (`127.0.0.1:0` for one the OS picks) to the
    /// board's `port` (80, its LAN endpoint); the board must be attached.
    /// Returns where a host connects (`lan:127.0.0.1:<port>`).
    pub fn forward(
        &self,
        board: EndpointId,
        host: SocketAddr,
        port: u16,
    ) -> io::Result<SocketAddr> {
        let mut g = self.inner();
        let at = g.lan.forward(board, host, port)?;
        g.changed();
        Ok(at)
    }

    /// The LAN's uplinks: each name, the port a board dials, where it goes.
    pub fn uplinks(&self) -> Vec<(String, u16, SocketAddr)> {
        self.inner()
            .lan
            .gateway()
            .uplinks()
            .iter()
            .map(|u| (u.name.clone(), u.port, u.to))
            .collect()
    }

    /// Carry `name` beyond the LAN to `to` on the host (`VirtualLan::uplink`).
    pub fn uplink(&self, name: &str, port: u16, to: SocketAddr) -> io::Result<Ipv4Addr> {
        let mut g = self.inner();
        let at = g.lan.uplink(name, port, to)?;
        g.changed();
        Ok(at)
    }

    /// The board attached as `from` is now `to`, with its MAC, its lease and
    /// its forwards (a runner renumbered the machine after a host attached
    /// it). Nothing happens when `to` is already attached.
    pub fn rename_board(&self, from: EndpointId, to: EndpointId) {
        let mut g = self.inner();
        if from == to || g.lan.station(to).is_some() {
            return;
        }
        g.lan.rename_station(from, to);
        if let Some(m) = g.mailboxes.iter_mut().find(|m| m.id == from) {
            m.id = to;
        }
        if let Some(p) = g.paces.iter_mut().find(|p| p.board == from) {
            p.board = to;
        }
    }

    /// Set board `board`'s pace ([`super::lan_pace`]); `None` is the unset
    /// pace, held only while a host is connected. A [`LanDriver::Runner`]
    /// LAN refuses [`Pace::Realtime`]: its runner drives it on guest time
    /// alone, and nothing there could wait.
    pub fn set_pace(&self, board: EndpointId, pace: Option<Pace>) -> Result<(), String> {
        let mut g = self.inner();
        if g.driver == LanDriver::Runner && pace == Some(Pace::Realtime) {
            return Err(format!(
                "board {board}: pace realtime needs a LAN its boards drive themselves, and this \
                 one is a runner's (driven at its quantum boundaries, on guest time)"
            ));
        }
        match g.paces.iter_mut().find(|p| p.board == board) {
            Some(p) => {
                if p.set != pace {
                    p.pace.release();
                }
                p.set = pace;
            }
            None => g.paces.push(BoardPace {
                board,
                set: pace,
                pace: HostPace::default(),
            }),
        }
        Ok(())
    }

    /// Board `board`'s pace, as its host set it (`None`: unset).
    pub fn pace(&self, board: EndpointId) -> Option<Pace> {
        self.inner().pace_set(board)
    }

    /// The board's next DHCP lease gets a different address than its last.
    pub fn renumber_next_lease(&self, board: EndpointId) {
        self.inner().lan.renumber_next_lease(board);
    }

    /// The board's address, once its DHCP exchange has finished.
    pub fn address(&self, board: EndpointId) -> Option<Ipv4Addr> {
        self.inner().lan.address(board)
    }

    /// Anything else, with the LAN in hand: add or remove an access point,
    /// add a probe and ask it things, read the frame log or the counters.
    /// The LAN is driven at the next chance afterwards.
    pub fn with<R>(&self, f: impl FnOnce(&mut VirtualLan) -> R) -> R {
        let mut g = self.inner();
        let out = f(&mut g.lan);
        g.changed();
        g.dirty = true;
        out
    }

    // --- A board's calls (a chip machine answering its network seam) --------
    //
    // `guest_now` is the calling board's guest cycle; the handle converts it
    // to the LAN's clock.

    pub fn link_up(&self, board: EndpointId) -> bool {
        self.inner().lan.link_up(board)
    }

    pub fn scan_start(&self, board: EndpointId, guest_now: Cycles) -> bool {
        let mut g = self.inner();
        let now = g.lan_now(guest_now);
        let started = g.lan.scan_start(board, now);
        g.changed();
        started
    }

    /// The board's last finished scan, strongest first, hidden ones left out.
    pub fn scan_results(&self, board: EndpointId) -> Vec<ScanRecord> {
        self.inner().lan.scan_results(board).to_vec()
    }

    pub fn connect(
        &self,
        board: EndpointId,
        guest_now: Cycles,
        name: &[u8],
        password: &[u8],
    ) -> bool {
        let mut g = self.inner();
        let now = g.lan_now(guest_now);
        let started = g.lan.connect(board, now, name, password);
        g.changed();
        started
    }

    pub fn disconnect(&self, board: EndpointId) {
        let mut g = self.inner();
        g.lan.disconnect(board);
        g.changed();
    }

    pub fn take_event(&self, board: EndpointId) -> Option<StationEvent> {
        self.inner().lan.take_event(board)
    }

    pub fn has_event(&self, board: EndpointId) -> bool {
        self.inner().lan.has_event(board)
    }

    /// The board restarted: its station forgets its link and its events, and
    /// its mailbox what was waiting for the guest that was. It stays attached.
    pub fn reset_board(&self, board: EndpointId) {
        let mut g = self.inner();
        g.lan.reset_station(board);
        if let Some(m) = g.mailboxes.iter_mut().find(|m| m.id == board) {
            while m.take_one(MAX_FRAME_LEN).is_some() {}
            m.drain_outbound();
        }
        g.changed();
    }

    // --- Self-driven delivery -------------------------------------------------

    /// One board's turn at the LAN, at the top of its machine's slice: hand
    /// over what its guest gave, drive the LAN if anything is due, and move
    /// what arrived for it into `endpoint`. A no-op on a
    /// [`LanDriver::Runner`] LAN, which its runner drives.
    ///
    /// A board held to the host's clock whose guest clock has run ahead
    /// waits here first, outside the lock (the module docs, "A board's
    /// pace").
    pub fn pump(&self, endpoint: &mut SeamEndpoint, guest_now: Cycles) {
        let wait = {
            let mut g = self.inner();
            if g.driver == LanDriver::Runner {
                return;
            }
            g.host_wait(endpoint.id, guest_now)
        };
        if !wait.is_zero() {
            sleep(wait);
        }
        let mut g = self.inner();
        let now = g.lan_now(guest_now);
        let Some(i) = g.mailboxes.iter().position(|m| m.id == endpoint.id) else {
            return;
        };
        let gave = endpoint.drain_outbound();
        let outbound = !gave.is_empty();
        for event in gave {
            let at = g.given_at(event.at, guest_now, now);
            g.mailboxes[i].push_outbound(EndpointEvent {
                at,
                bytes: event.bytes,
            });
        }
        if g.drive_due(now, outbound) {
            {
                let Shared { lan, mailboxes, .. } = &mut *g;
                let mut eps: Vec<&mut SeamEndpoint> = mailboxes.iter_mut().collect();
                lan.deliver(now, &mut eps);
            }
            g.dirty = false;
            g.changed();
        }
        while let Some(bytes) = g.mailboxes[i].take_one(MAX_FRAME_LEN) {
            // Past the endpoint's bound it refuses and counts, as for any
            // producer.
            let _ = endpoint.push_inbound(EndpointEvent {
                at: guest_now,
                bytes,
            });
        }
    }

    /// The guest cycle by which self-driven board `board` must pump again,
    /// for its machine's slice and idle skip: the LAN's next due cycle, on
    /// the board's clock, and at most a [`HOST_PACE_STEP_US`] away while the
    /// board is held to the host's clock. `None` on a [`LanDriver::Runner`]
    /// LAN, or when nothing is due.
    pub fn deadline(&self, board: EndpointId, guest_now: Cycles) -> Option<Cycles> {
        let mut g = self.inner();
        if g.driver == LanDriver::Runner {
            return None;
        }
        let now = g.lan_now(guest_now);
        let mut due = if g.dirty { Some(now) } else { g.next_due };
        if g.held(board) {
            let step = now.saturating_add(HOST_PACE_STEP_US * g.lan.config().cycles_per_us);
            due = Some(due.map_or(step, |at| at.min(step)));
        }
        due.map(|at| guest_now.saturating_add(at.saturating_sub(now)))
    }

    /// Whether a host is connected through any of this LAN's forwards.
    pub fn hosted(&self) -> bool {
        self.inner().hosted()
    }

    fn inner(&self) -> MutexGuard<'_, Shared> {
        // A board thread that panicked while holding the LAN leaves it as it
        // was; the other boards carry on with it.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Shared {
    /// Whether a host is connected through any forward, or any uplink (a
    /// board's connection out to a host is a wall-clock peer too: the
    /// relay's timers and the board's must agree).
    fn hosted(&self) -> bool {
        let gateway = self.lan.gateway();
        gateway.forwards().iter().any(|f| f.open() > 0)
            || gateway.uplinks().iter().any(|u| u.open() > 0)
    }

    /// Board `board`'s pace as its host set it.
    fn pace_set(&self, board: EndpointId) -> Option<Pace> {
        self.paces
            .iter()
            .find(|p| p.board == board)
            .and_then(|p| p.set)
    }

    /// Whether board `board` is held to the host's clock now: always at
    /// `realtime`, never at `max`, and while a host is connected unset.
    fn held(&self, board: EndpointId) -> bool {
        match self.pace_set(board) {
            Some(Pace::Realtime) => true,
            Some(Pace::Max) => false,
            None => self.hosted(),
        }
    }

    /// How long board `board`, its guest clock at `guest`, must wait for the
    /// host's clock to catch up; zero, and its pace released, when it is not
    /// held.
    fn host_wait(&mut self, board: EndpointId, guest: Cycles) -> Duration {
        let held = self.held(board);
        let cycles_per_us = self.lan.config().cycles_per_us;
        let i = match self.paces.iter().position(|p| p.board == board) {
            Some(i) => i,
            None if held => {
                self.paces.push(BoardPace {
                    board,
                    set: None,
                    pace: HostPace::default(),
                });
                self.paces.len() - 1
            }
            None => return Duration::ZERO,
        };
        let pace = &mut self.paces[i].pace;
        if held {
            pace.wait(guest, cycles_per_us, Instant::now())
        } else {
            pace.release();
            Duration::ZERO
        }
    }

    /// The LAN's clock when the calling board's guest clock reads `guest`.
    fn lan_now(&mut self, guest: Cycles) -> Cycles {
        match self.driver {
            LanDriver::Runner => guest,
            LanDriver::SelfDriven => {
                if guest.saturating_add(self.offset) < self.last {
                    self.offset = self.last - guest;
                }
                self.last = guest.saturating_add(self.offset);
                self.last
            }
            LanDriver::WallClock => {
                let us = u64::try_from(self.born.elapsed().as_micros()).unwrap_or(u64::MAX);
                us.saturating_mul(self.lan.config().cycles_per_us)
            }
        }
    }

    /// When an event the guest gave at its cycle `at` left, on the LAN's
    /// clock (`now` is the LAN's reading of the guest's `guest_now`).
    fn given_at(&self, at: Cycles, guest_now: Cycles, now: Cycles) -> Cycles {
        match self.driver {
            LanDriver::WallClock => now,
            _ => now.saturating_sub(guest_now.saturating_sub(at)),
        }
    }

    /// Whether a pump at LAN cycle `now` should run the LAN.
    fn drive_due(&mut self, now: Cycles, outbound: bool) -> bool {
        if outbound || self.dirty || self.next_due.is_some_and(|d| d <= now) {
            return true;
        }
        if self.lan.gateway().forwards().is_empty() {
            return false;
        }
        let t = Instant::now();
        match self.forwards_polled {
            Some(at) if t.duration_since(at) < FORWARD_POLL => false,
            _ => {
                self.forwards_polled = Some(t);
                true
            }
        }
    }

    /// The LAN changed: its next due cycle may have.
    fn changed(&mut self) {
        self.next_due = self.lan.next_due();
    }
}

impl SeamMedium for SharedLan {
    /// A runner's boundary. Only a [`LanDriver::Runner`] LAN is driven here;
    /// a self-driven one is its machines' to drive, and two drivers would
    /// carry a frame twice.
    fn deliver(&mut self, now: Cycles, endpoints: &mut [&mut SeamEndpoint]) {
        let mut g = self.inner();
        if g.driver != LanDriver::Runner {
            return;
        }
        let outbound = endpoints
            .iter()
            .any(|e| e.id.seam == NET_SEAM && e.has_outbound());
        if g.drive_due(now, outbound) {
            g.lan.deliver(now, endpoints);
            g.dirty = false;
            g.changed();
        }
    }
}

/// Wait `d` of host time. A wasm build (the Studio tab's module) never has a
/// host connected — it binds no forward — and no host there sets a pace, so
/// it imports no sleep for it.
fn sleep(d: Duration) {
    #[cfg(not(target_family = "wasm"))]
    std::thread::sleep(d);
    #[cfg(target_family = "wasm")]
    let _ = d;
}

impl fmt::Debug for SharedLan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g = self.inner();
        f.debug_struct("SharedLan")
            .field("driver", &g.driver)
            .field("boards", &g.lan.stations().len())
            .field("access_points", &g.lan.access_points().len())
            .field("counters", &g.lan.counters())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::net::lan_dns::TYPE_A;
    use crate::seam::net::lan_test_board::{QUANTUM, TestBoard};
    use crate::seam::net::virtual_lan::LanConfig;

    #[test]
    fn a_handle_is_send_and_sync_so_board_threads_can_share_one() {
        fn shareable<T: Send + Sync + Clone>() {}
        shareable::<SharedLan>();
    }

    #[test]
    fn attach_is_idempotent_by_endpoint_and_mac_and_refuses_a_clash() {
        let lan = SharedLan::new(lan(), LanDriver::Runner);
        let (a, b) = (
            net_endpoint(ParticipantId(0)),
            net_endpoint(ParticipantId(1)),
        );
        let ip = lan.attach(a, MAC_A).unwrap();
        assert_eq!(ip, Some(Ipv4Addr::new(192, 168, 4, 100)));
        assert_eq!(lan.attach(a, MAC_A).unwrap(), ip, "again: nothing changes");
        assert!(
            lan.attach(a, MAC_B)
                .unwrap_err()
                .contains("already on this LAN")
        );
        assert!(lan.attach(b, MAC_A).unwrap_err().contains("its own"));
        assert_eq!(lan.with(|l| l.stations().len()), 1);
        assert!(lan.forward(a, "127.0.0.1:0".parse().unwrap(), 80).is_ok());
        assert!(lan.forward(b, "127.0.0.1:0".parse().unwrap(), 80).is_err());
    }

    #[test]
    fn a_renamed_board_keeps_its_mac_lease_and_mailbox() {
        let lan = SharedLan::new(lan(), LanDriver::SelfDriven);
        let (old, new) = (
            net_endpoint(ParticipantId(5)),
            net_endpoint(ParticipantId(0)),
        );
        let ip = lan.attach(old, MAC_A).unwrap();
        lan.rename_board(old, new);
        assert!(lan.with(|l| l.station(old).is_none() && l.station(new).is_some()));
        assert_eq!(lan.attach(new, MAC_A).unwrap(), ip, "the same lease");
        assert_eq!(lan.inner().mailboxes.len(), 1);
        assert_eq!(lan.inner().mailboxes[0].id, new);
        // Renaming onto a board already there changes nothing.
        lan.attach(old, MAC_B).unwrap();
        lan.rename_board(old, new);
        assert!(lan.with(|l| l.station(old).is_some()));
    }

    #[test]
    fn a_runner_drives_a_runner_lan_and_a_pump_does_not() {
        let lan = SharedLan::new(lan(), LanDriver::Runner);
        let mut board = TestBoard::new(0, "lp-aaaa");
        lan.attach(board.id(), board.mac).unwrap();
        assert!(lan.connect(board.id(), 0, b"home", b"test-password-1"));
        // Pumping a runner's LAN carries nothing.
        lan.pump(&mut board.endpoint, 20 * MS);
        assert!(!lan.link_up(board.id()));
        assert_eq!(lan.deadline(board.id(), 20 * MS), None);
        let mut medium = lan.clone();
        drive(&mut medium, &mut [&mut board], 0, 20 * MS);
        assert!(lan.link_up(board.id()));
        assert_eq!(lan.take_event(board.id()), Some(StationEvent::Associated));
        assert_eq!(board.ip, Some(Ipv4Addr::new(192, 168, 4, 100)));
        assert_eq!(lan.address(board.id()), board.ip);
    }

    #[test]
    fn a_self_driven_board_joins_through_its_pumps_alone() {
        let lan = SharedLan::new(lan(), LanDriver::SelfDriven);
        let mut board = TestBoard::new(0, "lp-aaaa");
        lan.attach(board.id(), board.mac).unwrap();
        // A runner's boundary is a no-op on a self-driven LAN.
        let mut medium = lan.clone();
        medium.deliver(MS, &mut [&mut board.endpoint]);
        assert!(lan.connect(board.id(), 0, b"home", b"test-password-1"));
        assert_eq!(lan.deadline(board.id(), 0), Some(0), "never driven yet");
        lan.pump(&mut board.endpoint, 0);
        let join = lan.deadline(board.id(), 0).expect("the join lands later");
        assert!(join > 0 && join <= 10 * MS, "{join}");
        let mut now = 0;
        while now < 20 * MS {
            lan.pump(&mut board.endpoint, now);
            let linked = lan.link_up(board.id());
            board.step(now, linked);
            now += QUANTUM;
        }
        assert_eq!(board.ip, Some(Ipv4Addr::new(192, 168, 4, 100)));
        if let Some(d) = lan.deadline(board.id(), now) {
            assert!(d >= now, "{d} < {now}");
        }
    }

    #[test]
    fn a_restart_moves_the_offset_not_the_lans_time() {
        let lan = SharedLan::new(lan(), LanDriver::SelfDriven);
        let mut g = lan.inner();
        assert_eq!(g.lan_now(1_000), 1_000);
        // The board restarts: its clock reads 10 again.
        assert_eq!(g.lan_now(10), 1_000);
        assert_eq!(g.lan_now(110), 1_100);
        // A frame given 50 cycles ago, on the restarted clock.
        assert_eq!(g.given_at(60, 110, 1_100), 1_050);
    }

    #[test]
    fn a_board_reset_forgets_its_link_and_events_but_keeps_its_lease() {
        let lan = SharedLan::new(lan(), LanDriver::Runner);
        let mut board = TestBoard::new(0, "lp-aaaa");
        lan.attach(board.id(), board.mac).unwrap();
        assert!(lan.connect(board.id(), 0, b"home", b"test-password-1"));
        let mut medium = lan.clone();
        drive(&mut medium, &mut [&mut board], 0, 20 * MS);
        assert!(lan.link_up(board.id()));
        let ip = lan.address(board.id());
        lan.reset_board(board.id());
        assert!(!lan.link_up(board.id()));
        assert!(!lan.has_event(board.id()));
        assert_eq!(lan.attach(board.id(), board.mac).unwrap(), ip);
    }

    #[test]
    fn two_self_driven_boards_on_wall_clock_reach_each_other_through_mailboxes() {
        // Wall clock: nothing here asserts a time, only that both boards
        // resolve each other's names while each pumps on its own.
        let lan = SharedLan::new(lan(), LanDriver::WallClock);
        let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
        for x in [&a, &b] {
            lan.attach(x.id(), x.mac).unwrap();
            assert!(lan.connect(x.id(), 0, b"home", b"test-password-1"));
        }
        let probe = lan.with(|l| l.add_probe());
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut asked = false;
        let mut guest = 0;
        loop {
            for x in [&mut a, &mut b] {
                lan.pump(&mut x.endpoint, guest);
                let linked = lan.link_up(x.id());
                let now = lan.inner().lan_now(guest);
                x.step(now, linked);
            }
            guest += QUANTUM;
            if a.ip.is_some() && b.ip.is_some() && !asked {
                lan.with(|l| {
                    l.probe_mut(probe).query("lp-aaaa.local", TYPE_A);
                    l.probe_mut(probe).query("lp-bbbb.local", TYPE_A);
                });
                asked = true;
            }
            let both = lan.with(|l| {
                l.probe(probe).resolved("lp-aaaa.local").is_some()
                    && l.probe(probe).resolved("lp-bbbb.local").is_some()
            });
            if both {
                break;
            }
            assert!(Instant::now() < deadline, "the two boards never answered");
            std::thread::sleep(Duration::from_micros(200));
        }
        assert_ne!(a.ip, b.ip);
    }

    /// While a host is connected through a forward, the board's pace is
    /// engaged and its next pump is at most a pace step away; once the host
    /// has gone, neither. The pace's own rule, against host time it is
    /// handed, is `lan_host_pace`'s tests: nothing here asserts a time.
    #[test]
    fn a_connected_host_paces_the_lan_until_it_leaves() {
        let lan = SharedLan::new(lan(), LanDriver::SelfDriven);
        let mut board = TestBoard::new(0, "lp-aaaa");
        lan.attach(board.id(), board.mac).unwrap();
        assert!(lan.connect(board.id(), 0, b"home", b"test-password-1"));
        let at = lan
            .forward(board.id(), "127.0.0.1:0".parse().unwrap(), 80)
            .unwrap();
        let mut now = 0;
        while board.ip.is_none() {
            pump_and_step(&lan, &mut board, &mut now);
            assert!(now < 100 * MS, "the board joins");
        }
        assert!(!lan.hosted());
        assert!(!paced(&lan, board.id()), "with no host, nothing waits");

        // A safety net, never an input: the forward's host edge is a socket.
        let net = Instant::now() + Duration::from_secs(20);
        let host = std::net::TcpStream::connect(at).unwrap();
        while !lan.hosted() {
            pump_and_step(&lan, &mut board, &mut now);
            std::thread::sleep(Duration::from_micros(200));
            assert!(Instant::now() < net, "the forward never took the host");
        }
        pump_and_step(&lan, &mut board, &mut now);
        assert!(
            paced(&lan, board.id()),
            "a pump with a host there paces the board"
        );
        let next = lan
            .deadline(board.id(), now)
            .expect("a host keeps the board pumping");
        assert!(next <= now + HOST_PACE_STEP_US * 160, "{next} vs {now}");

        drop(host);
        while lan.hosted() {
            pump_and_step(&lan, &mut board, &mut now);
            std::thread::sleep(Duration::from_micros(200));
            assert!(Instant::now() < net, "the forward never let the host go");
        }
        pump_and_step(&lan, &mut board, &mut now);
        assert!(!paced(&lan, board.id()), "the host left: nothing waits");
    }

    /// `max` is never paced, even with a host connected through a forward.
    #[test]
    fn a_max_pace_never_waits_even_with_a_host_connected() {
        let (lan, mut board, mut now) = joined_board();
        lan.set_pace(board.id(), Some(Pace::Max)).unwrap();
        assert_eq!(lan.pace(board.id()), Some(Pace::Max));
        let at = lan
            .forward(board.id(), "127.0.0.1:0".parse().unwrap(), 80)
            .unwrap();
        let host = std::net::TcpStream::connect(at).unwrap();
        until_hosted(&lan, &mut board, &mut now, true);
        pump_and_step(&lan, &mut board, &mut now);
        assert!(
            !paced(&lan, board.id()),
            "a host is there, and nothing waits"
        );
        drop(host);
    }

    /// `realtime` holds the board with no host and no forward at all, and
    /// bounds its next pump to a pace step; set back to unset, it lets go.
    #[test]
    fn a_realtime_pace_holds_the_board_with_no_host() {
        let (lan, mut board, mut now) = joined_board();
        assert!(!paced(&lan, board.id()), "unset, with no host: not held");
        lan.set_pace(board.id(), Some(Pace::Realtime)).unwrap();
        pump_and_step(&lan, &mut board, &mut now);
        assert!(!lan.hosted());
        assert!(paced(&lan, board.id()), "realtime holds it host or no host");
        let next = lan
            .deadline(board.id(), now)
            .expect("a held board keeps pumping");
        assert!(next <= now + HOST_PACE_STEP_US * 160, "{next} vs {now}");

        lan.set_pace(board.id(), None).unwrap();
        pump_and_step(&lan, &mut board, &mut now);
        assert!(!paced(&lan, board.id()), "unset again, with no host");
    }

    /// A runner drives its LAN on guest time alone: nothing there could wait.
    #[test]
    fn a_runners_lan_refuses_a_realtime_pace() {
        let lan = SharedLan::new(lan(), LanDriver::Runner);
        let board = TestBoard::new(0, "lp-aaaa");
        lan.attach(board.id(), board.mac).unwrap();
        let err = lan.set_pace(board.id(), Some(Pace::Realtime)).unwrap_err();
        assert!(err.contains("runner"), "{err}");
        assert_eq!(lan.pace(board.id()), None, "a refusal sets nothing");
        lan.set_pace(board.id(), Some(Pace::Max)).unwrap();
        lan.set_pace(board.id(), None).unwrap();
    }

    /// One millisecond at the tests' 160 MHz.
    const MS: Cycles = 160_000;
    const MAC_A: [u8; 6] = [2, 0, 0, 0, 0xb0, 0];
    const MAC_B: [u8; 6] = [2, 0, 0, 0, 0xb0, 1];

    fn lan() -> VirtualLan {
        let mut lan = VirtualLan::new(LanConfig::new(160));
        for ap in crate::seam::net::virtual_access_point::tests::fixture_access_points() {
            lan.add_access_point(ap);
        }
        lan
    }

    /// One self-driven board's turn: pump, then step its guest a quantum.
    fn pump_and_step(lan: &SharedLan, board: &mut TestBoard, now: &mut Cycles) {
        lan.pump(&mut board.endpoint, *now);
        let linked = lan.link_up(board.id());
        board.step(*now, linked);
        *now += QUANTUM;
    }

    /// Whether `board` is being held to the host's clock.
    fn paced(lan: &SharedLan, board: EndpointId) -> bool {
        lan.inner()
            .paces
            .iter()
            .any(|p| p.board == board && p.pace.engaged())
    }

    /// A self-driven LAN with one board on it, joined and given its address.
    fn joined_board() -> (SharedLan, TestBoard, Cycles) {
        let lan = SharedLan::new(lan(), LanDriver::SelfDriven);
        let mut board = TestBoard::new(0, "lp-aaaa");
        lan.attach(board.id(), board.mac).unwrap();
        assert!(lan.connect(board.id(), 0, b"home", b"test-password-1"));
        let mut now = 0;
        while board.ip.is_none() {
            pump_and_step(&lan, &mut board, &mut now);
            assert!(now < 100 * MS, "the board joins");
        }
        (lan, board, now)
    }

    /// Pump until the forward has taken (`true`) or let go of a host.
    fn until_hosted(lan: &SharedLan, board: &mut TestBoard, now: &mut Cycles, hosted: bool) {
        // A safety net, never an input: the forward's host edge is a socket.
        let net = Instant::now() + Duration::from_secs(20);
        while lan.hosted() != hosted {
            pump_and_step(lan, board, now);
            std::thread::sleep(Duration::from_micros(200));
            assert!(Instant::now() < net, "the forward never saw the host move");
        }
    }

    /// The runner's loop: step every board, then deliver at the boundary.
    fn drive(medium: &mut SharedLan, boards: &mut [&mut TestBoard], from: Cycles, to: Cycles) {
        let mut now = from;
        while now < to {
            for b in boards.iter_mut() {
                let linked = medium.link_up(b.id());
                b.step(now, linked);
            }
            let mut eps: Vec<&mut SeamEndpoint> =
                boards.iter_mut().map(|b| &mut b.endpoint).collect();
            medium.deliver(now, &mut eps);
            now += QUANTUM;
        }
    }
}
