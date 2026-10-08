//! The virtual LAN: boards, the gateway and probes on one Ethernet segment.
//!
//! [`VirtualLan`] is the [`SeamMedium`] under the network seam. Each board
//! is attached by its endpoint (`<board>/net`) and its station MAC; it joins
//! one of the LAN's [`VirtualAccessPoint`]s through its [`LanStation`], and
//! while joined, every frame its guest gives is carried on the segment:
//!
//! - **unicast by destination MAC**, learned from every frame's source;
//! - **broadcast, multicast and unknown unicast flooded** to every port but
//!   the sender (ARP, DHCP and mDNS are all flooded);
//! - after **one stated latency** in guest cycles ([`LanConfig`]), the same
//!   for every frame, never derived from its length;
//! - into a board's endpoint through the foundation's bounds: an endpoint at
//!   its queue bound refuses the frame, and the LAN counts it
//!   ([`LanCounters::refused`]) rather than hold it. The wake itself (one
//!   raise outstanding, the spacing) is the chip machine's
//!   ([`crate::seam::WakePacer`]), which raises when an endpoint has
//!   something waiting.
//!
//! **Deterministic**: frames are carried in order of their due cycle, then
//! of the order they were given; no map is iterated in hash order; every
//! stack's seed is fixed. Driven by the lockstep runner, two runs give the
//! same frame log ([`VirtualLan::log_frames`]). The one part that is not is
//! the host edge of a port forward, which is wall-clock by nature.
//!
//! Every LAN is a value: several can exist in one process.

use std::collections::BTreeMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use lp_emu_core::sched::Cycles;

use crate::seam::{EndpointEvent, EndpointId, PacerConfig, SeamEndpoint, SeamMedium};

use super::lan_frame::{self, MAX_FRAME_LEN};
use super::lan_gateway::LanGateway;
use super::lan_probe::{LanProbe, ProbeId};
use super::lan_station::{LanStation, StationEvent};
use super::virtual_access_point::{self, ScanRecord, VirtualAccessPoint};

/// The gateway's MAC: locally administered, unicast.
pub const GATEWAY_MAC: [u8; 6] = [0x02, 0x4c, 0x41, 0x4e, 0x00, 0x01];

/// A network seam endpoint's pacing: the foundation's spacing and queue
/// bound, and a take cap of one whole Ethernet frame (the foundation's
/// default, 512 bytes, would refuse a full-size frame outright).
pub fn net_pacer_config() -> PacerConfig {
    PacerConfig {
        take_cap: MAX_FRAME_LEN,
        ..PacerConfig::default()
    }
}

/// A LAN's stated constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanConfig {
    /// The chip's guest cycles per microsecond: the stacks' clock. The
    /// chip's number, never this crate's.
    pub cycles_per_us: u64,
    /// One frame's time on the segment, in guest cycles. Never zero.
    pub frame_latency: Cycles,
    /// From a board's join to its outcome.
    pub join_latency: Cycles,
    /// From a scan's start to its results.
    pub scan_latency: Cycles,
    /// The gateway's address; the LAN is its /24.
    pub gateway_ip: Ipv4Addr,
    /// The first host number the DHCP server hands out.
    pub first_lease_host: u8,
    /// The lease time the DHCP server states, in seconds.
    pub lease_secs: u32,
}

impl LanConfig {
    /// The defaults at a chip's clock: a frame takes **100 µs**, a join
    /// **10 ms**, a scan **100 ms**; the LAN is `192.168.4.0/24` with the
    /// gateway at `.1` and leases from `.100`, a day long. Stated numbers, not
    /// measured ones: no radio is modelled.
    pub fn new(cycles_per_us: u64) -> Self {
        assert!(cycles_per_us > 0, "a LAN needs the chip's clock rate");
        Self {
            cycles_per_us,
            frame_latency: 100 * cycles_per_us,
            join_latency: 10_000 * cycles_per_us,
            scan_latency: 100_000 * cycles_per_us,
            gateway_ip: Ipv4Addr::new(192, 168, 4, 1),
            first_lease_host: 100,
            lease_secs: 86_400,
        }
    }
}

/// A port on the segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LanPort {
    Board(EndpointId),
    Gateway,
    Probe(ProbeId),
}

impl std::fmt::Display for LanPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Board(id) => write!(f, "{id}"),
            Self::Gateway => f.write_str("gateway"),
            Self::Probe(p) => write!(f, "probe{}", p.0),
        }
    }
}

/// One delivery, for the frame log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameRecord {
    /// The guest cycle it arrived.
    pub at: Cycles,
    pub from: LanPort,
    pub to: LanPort,
    pub bytes: Vec<u8>,
}

/// What a LAN has carried, and what it has not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LanCounters {
    /// Frames put on the segment.
    pub carried: u64,
    /// Deliveries made (a flooded frame counts once per port it reached).
    pub delivered: u64,
    /// Frames flooded (group or unknown destination).
    pub flooded: u64,
    /// Deliveries a board's endpoint refused (at its bound, or too large).
    pub refused: u64,
    /// Frames a board gave while not joined, or addressed to a board no
    /// longer joined: dropped.
    pub unlinked: u64,
    /// Runts and oversize frames: dropped.
    pub malformed: u64,
    /// Deliveries to a board whose endpoint was not passed in.
    pub absent: u64,
}

/// A home network for several boards.
pub struct VirtualLan {
    config: LanConfig,
    access_points: Vec<VirtualAccessPoint>,
    stations: Vec<LanStation>,
    gateway: LanGateway,
    probes: Vec<LanProbe>,
    /// MAC → the port it was last seen on.
    learned: BTreeMap<[u8; 6], LanPort>,
    /// `(due, order) → (from, frame)`.
    in_flight: BTreeMap<(Cycles, u64), (LanPort, Vec<u8>)>,
    order: u64,
    now: Cycles,
    counters: LanCounters,
    log: Option<Vec<FrameRecord>>,
}

impl VirtualLan {
    pub fn new(config: LanConfig) -> Self {
        assert!(
            config.frame_latency > 0,
            "a frame cannot arrive in the cycle it was given"
        );
        Self {
            gateway: LanGateway::new(
                GATEWAY_MAC,
                config.gateway_ip,
                config.first_lease_host,
                config.lease_secs,
                config.cycles_per_us,
            ),
            config,
            access_points: Vec::new(),
            stations: Vec::new(),
            probes: Vec::new(),
            learned: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            order: 0,
            now: 0,
            counters: LanCounters::default(),
            log: None,
        }
    }

    pub fn config(&self) -> LanConfig {
        self.config
    }

    // --- The networks in range ---------------------------------------------

    pub fn with_access_point(mut self, ap: VirtualAccessPoint) -> Self {
        self.add_access_point(ap);
        self
    }

    pub fn add_access_point(&mut self, ap: VirtualAccessPoint) {
        self.access_points.push(ap);
    }

    /// Every access point named `name` leaves range; boards joined to it
    /// lose their link (a [`StationEvent::LinkLost`]).
    pub fn remove_access_point(&mut self, name: &str) {
        self.access_points.retain(|ap| ap.name != name);
        for s in &mut self.stations {
            s.network_left(name);
        }
    }

    pub fn access_points(&self) -> &[VirtualAccessPoint] {
        &self.access_points
    }

    // --- Boards --------------------------------------------------------------

    /// Put a board on the LAN: its endpoint and station MAC. Its lease is
    /// reserved now, in attach order, so its address is known before it
    /// boots. Attaching the same endpoint twice keeps the first.
    pub fn attach(&mut self, endpoint: EndpointId, mac: [u8; 6]) -> Option<Ipv4Addr> {
        if self.station(endpoint).is_none() {
            self.stations.push(LanStation::new(endpoint, mac));
        }
        self.gateway.dhcp.reserve(mac)
    }

    pub fn stations(&self) -> &[LanStation] {
        &self.stations
    }

    pub fn station(&self, board: EndpointId) -> Option<&LanStation> {
        self.stations.iter().find(|s| s.endpoint == board)
    }

    fn station_mut(&mut self, board: EndpointId) -> Option<&mut LanStation> {
        self.stations.iter_mut().find(|s| s.endpoint == board)
    }

    /// Start a scan; its results land a stated time after `now`. `false`:
    /// no such board.
    pub fn scan_start(&mut self, board: EndpointId, now: Cycles) -> bool {
        let due = now + self.config.scan_latency;
        self.station_mut(board).map(|s| s.scan_start(due)).is_some()
    }

    /// The board's last finished scan, strongest first, hidden networks
    /// left out.
    pub fn scan_results(&self, board: EndpointId) -> &[ScanRecord] {
        self.station(board).map_or(&[], |s| s.scan_results())
    }

    /// Start joining `name` with `password`; the outcome (decided against
    /// the networks in range now) lands a stated time after `now`.
    pub fn connect(
        &mut self,
        board: EndpointId,
        now: Cycles,
        name: &[u8],
        password: &[u8],
    ) -> bool {
        let outcome = virtual_access_point::join(&self.access_points, name, password);
        let due = now + self.config.join_latency;
        self.station_mut(board)
            .map(|s| s.connect(due, outcome, name))
            .is_some()
    }

    pub fn disconnect(&mut self, board: EndpointId) {
        if let Some(s) = self.station_mut(board) {
            s.disconnect();
        }
    }

    /// The board attached as `from` is now `to` (a runner renumbered its
    /// machines): its station, and the segment's memory of where its MAC is,
    /// move with it. Nothing happens when `to` is already attached.
    pub fn rename_station(&mut self, from: EndpointId, to: EndpointId) {
        if from == to || self.station(to).is_some() {
            return;
        }
        if let Some(s) = self.station_mut(from) {
            s.endpoint = to;
        }
        for port in self.learned.values_mut() {
            if *port == LanPort::Board(from) {
                *port = LanPort::Board(to);
            }
        }
    }

    /// The board restarted: its station forgets its link, its pending join
    /// or scan and its untaken events ([`LanStation::reset`]). It stays
    /// attached, with its MAC and its lease.
    pub fn reset_station(&mut self, board: EndpointId) {
        if let Some(s) = self.station_mut(board) {
            s.reset();
        }
    }

    pub fn link_up(&self, board: EndpointId) -> bool {
        self.station(board).is_some_and(|s| s.link_up())
    }

    pub fn take_event(&mut self, board: EndpointId) -> Option<StationEvent> {
        self.station_mut(board).and_then(|s| s.take_event())
    }

    pub fn has_event(&self, board: EndpointId) -> bool {
        self.station(board).is_some_and(|s| s.has_event())
    }

    /// The board's address, once its DHCP exchange has finished.
    pub fn address(&self, board: EndpointId) -> Option<Ipv4Addr> {
        let mac = self.station(board)?.mac;
        self.gateway.dhcp.bound_ip(mac)
    }

    /// The board's next DISCOVER gets a different address than its last
    /// (to test a board whose address changes across a reset).
    pub fn renumber_next_lease(&mut self, board: EndpointId) {
        if let Some(mac) = self.station(board).map(|s| s.mac) {
            self.gateway.dhcp.renumber_next(mac);
        }
    }

    // --- The host's doors ----------------------------------------------------

    /// Forward a host TCP port (`127.0.0.1:0` for one the OS picks) to the
    /// board's `port` (80, its LAN endpoint). Returns where a host connects.
    pub fn forward(
        &mut self,
        board: EndpointId,
        host: SocketAddr,
        port: u16,
    ) -> io::Result<SocketAddr> {
        let mac = self
            .station(board)
            .map(|s| s.mac)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such board on this LAN"))?;
        self.gateway.forward(host, mac, port)
    }

    /// Carry `name` beyond the LAN to `to` on the host (a board dials it
    /// on `port`): its uplink ([`super::LanUplink`]). Returns the address
    /// the name resolves to on the LAN.
    pub fn uplink(&mut self, name: &str, port: u16, to: SocketAddr) -> io::Result<Ipv4Addr> {
        self.gateway.uplink(name, port, to)
    }

    pub fn gateway(&self) -> &LanGateway {
        &self.gateway
    }

    /// Put a probe on the segment, its address reserved from the gateway.
    pub fn add_probe(&mut self) -> ProbeId {
        let id = ProbeId(self.probes.len());
        let mac = [0x02, 0x4c, 0x41, 0x4e, 0xfe, id.0 as u8];
        let ip = self
            .gateway
            .dhcp
            .reserve(mac)
            .expect("the /24 has room for a probe");
        self.probes.push(LanProbe::new(
            mac,
            ip,
            self.config.gateway_ip,
            self.config.cycles_per_us,
            self.now,
        ));
        id
    }

    pub fn probe(&self, id: ProbeId) -> &LanProbe {
        &self.probes[id.0]
    }

    pub fn probe_mut(&mut self, id: ProbeId) -> &mut LanProbe {
        &mut self.probes[id.0]
    }

    // --- What happened -------------------------------------------------------

    pub fn counters(&self) -> LanCounters {
        self.counters
    }

    /// Keep (or stop keeping) a record of every delivery.
    pub fn log_frames(&mut self, on: bool) {
        self.log = on.then(Vec::new);
    }

    pub fn frame_log(&self) -> &[FrameRecord] {
        self.log.as_deref().unwrap_or(&[])
    }

    /// The earliest guest cycle the LAN has something to do: a frame due, a
    /// join or scan landing, a stack's timer. A host's idle skip is bounded
    /// by it. A port forward's host edge is not in it (it is wall-clock).
    pub fn next_due(&mut self) -> Option<Cycles> {
        let now = self.now;
        let frames = self.in_flight.keys().next().map(|(due, _)| *due);
        let stations = self.stations.iter().filter_map(|s| s.next_due()).min();
        let gateway = self.gateway.poll_at(now);
        let probes = self
            .probes
            .iter_mut()
            .filter_map(|p| p.stack.poll_at(now))
            .min();
        [frames, stations, gateway, probes]
            .into_iter()
            .flatten()
            .min()
    }

    // --- The segment ---------------------------------------------------------

    fn launch(&mut self, at: Cycles, from: LanPort, frame: Vec<u8>) {
        if frame.len() < lan_frame::ETHERNET_HEADER_LEN || frame.len() > MAX_FRAME_LEN {
            self.counters.malformed += 1;
            return;
        }
        if let Some(src) = lan_frame::frame_src(&frame)
            && !lan_frame::is_group_mac(&src)
        {
            self.learned.insert(src, from);
        }
        self.counters.carried += 1;
        let key = (at + self.config.frame_latency, self.order);
        self.order += 1;
        self.in_flight.insert(key, (from, frame));
    }

    /// Where a frame from `from` goes.
    fn destinations(&mut self, from: LanPort, frame: &[u8]) -> Vec<LanPort> {
        let dst = lan_frame::frame_dst(frame).expect("launched frames have a header");
        if !lan_frame::is_group_mac(&dst)
            && let Some(port) = self.learned.get(&dst).copied()
        {
            return if port == from { Vec::new() } else { vec![port] };
        }
        self.counters.flooded += 1;
        let boards = self
            .stations
            .iter()
            .filter(|s| s.link_up())
            .map(|s| LanPort::Board(s.endpoint));
        let probes = (0..self.probes.len()).map(|i| LanPort::Probe(ProbeId(i)));
        boards
            .chain(std::iter::once(LanPort::Gateway))
            .chain(probes)
            .filter(|p| *p != from)
            .collect()
    }

    fn arrive(
        &mut self,
        due: Cycles,
        from: LanPort,
        frame: Vec<u8>,
        endpoints: &mut [&mut SeamEndpoint],
    ) {
        for to in self.destinations(from, &frame) {
            let answers = match to {
                LanPort::Board(id) => {
                    if !self.link_up(id) {
                        self.counters.unlinked += 1;
                        continue;
                    }
                    let Some(ep) = endpoints.iter_mut().find(|e| e.id == id) else {
                        self.counters.absent += 1;
                        continue;
                    };
                    let event = EndpointEvent {
                        at: due,
                        bytes: frame.clone(),
                    };
                    if ep.push_inbound(event).is_err() {
                        self.counters.refused += 1;
                        continue;
                    }
                    Vec::new()
                }
                LanPort::Gateway => self.gateway.receive(due, frame.clone()),
                LanPort::Probe(p) => self.probes[p.0].receive(due, frame.clone()),
            };
            self.counters.delivered += 1;
            if let Some(log) = &mut self.log {
                log.push(FrameRecord {
                    at: due,
                    from,
                    to,
                    bytes: frame.clone(),
                });
            }
            for answer in answers {
                self.launch(due, to, answer);
            }
        }
    }
}

impl SeamMedium for VirtualLan {
    fn deliver(&mut self, now: Cycles, endpoints: &mut [&mut SeamEndpoint]) {
        let now = now.max(self.now);
        self.now = now;

        // 1. Joins and scans whose time has come.
        for s in &mut self.stations {
            s.advance(now, &self.access_points);
        }

        // 2. What each attached board gave. Endpoints of other seams are
        //    another medium's, and left alone.
        for ep in endpoints.iter_mut() {
            let Some(linked) = self.station(ep.id).map(|s| s.link_up()) else {
                continue;
            };
            let from = LanPort::Board(ep.id);
            for event in ep.drain_outbound() {
                if linked {
                    self.launch(event.at, from, event.bytes);
                } else {
                    self.counters.unlinked += 1;
                }
            }
        }

        // 3. The segment, in due order; what a delivery causes may itself
        //    fall due by `now` and is carried in the same pass.
        while let Some(entry) = self.in_flight.first_entry() {
            if entry.key().0 > now {
                break;
            }
            let ((due, _), (from, frame)) = entry.remove_entry();
            self.arrive(due, from, frame, endpoints);
        }

        // 4. The timers and host edges at `now`: their frames leave now and
        //    arrive a latency later, at a later boundary.
        for frame in self.gateway.run(now) {
            self.launch(now, LanPort::Gateway, frame);
        }
        for i in 0..self.probes.len() {
            for frame in self.probes[i].run(now) {
                self.launch(now, LanPort::Probe(ProbeId(i)), frame);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::ParticipantId;
    use crate::seam::net::lan_dns::TYPE_A;
    use crate::seam::net::lan_frame::{BROADCAST_MAC, UdpDatagram};
    use crate::seam::net::lan_test_board::{TestBoard, run};

    #[test]
    fn two_boards_each_get_their_reserved_address_over_dhcp() {
        let mut lan = lan();
        let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
        a.join(&mut lan);
        b.join(&mut lan);
        run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
        assert_eq!(a.ip, Some(Ipv4Addr::new(192, 168, 4, 100)));
        assert_eq!(b.ip, Some(Ipv4Addr::new(192, 168, 4, 101)));
        assert_eq!(lan.address(a.id()), a.ip);
        assert_eq!(lan.address(b.id()), b.ip);
        let (offers, acks, naks, _) = lan.gateway().dhcp.counts();
        assert_eq!((offers, acks, naks), (2, 2, 0));
    }

    #[test]
    fn a_board_not_joined_neither_sends_nor_hears() {
        let mut lan = lan();
        let mut a = TestBoard::new(0, "lp-aaaa");
        let mut b = TestBoard::new(1, "lp-bbbb");
        lan.attach(a.id(), a.mac);
        b.join(&mut lan);
        run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
        assert_eq!(a.ip, None);
        // a gives a frame while not joined: dropped. b's broadcasts (its
        // DHCP) never reached a.
        a.endpoint.push_outbound(EndpointEvent {
            at: 20 * MS,
            bytes: vec![0xff; 60],
        });
        let carried = lan.counters().carried;
        lan.deliver(20 * MS, &mut [&mut a.endpoint, &mut b.endpoint]);
        assert_eq!(lan.counters().carried, carried);
        assert_eq!(lan.counters().unlinked, 1);
        assert!(!a.endpoint.has_inbound());
        assert!(b.ip.is_some());

        // A wrong password keeps it off too, and says why.
        assert!(lan.connect(a.id(), 20 * MS, b"home", b"wrong"));
        run(&mut lan, &mut [&mut a, &mut b], 20 * MS, 40 * MS);
        assert_eq!(lan.take_event(a.id()), Some(StationEvent::AuthFailed));
        assert!(!lan.link_up(a.id()));
        assert_eq!(a.ip, None);
    }

    #[test]
    fn unicast_follows_the_learned_mac_and_broadcast_floods_all_but_the_sender() {
        let mut lan = lan();
        let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
        a.join(&mut lan);
        b.join(&mut lan);
        run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
        lan.log_frames(true);
        let t = 20 * MS;

        // A broadcast from a: b and the gateway hear it, a does not.
        let hello = UdpDatagram {
            src_mac: a.mac,
            dst_mac: BROADCAST_MAC,
            src_ip: a.ip.unwrap(),
            dst_ip: Ipv4Addr::BROADCAST,
            src_port: 9,
            dst_port: 9,
            payload: b"to everyone",
        }
        .emit();
        a.endpoint.push_outbound(EndpointEvent {
            at: t,
            bytes: hello.clone(),
        });
        // A unicast from b to a's learned MAC: only a.
        let direct = UdpDatagram {
            src_mac: b.mac,
            dst_mac: a.mac,
            src_ip: b.ip.unwrap(),
            dst_ip: a.ip.unwrap(),
            src_port: 9,
            dst_port: 9,
            payload: b"to a",
        }
        .emit();
        b.endpoint.push_outbound(EndpointEvent {
            at: t,
            bytes: direct.clone(),
        });
        let arrives = t + lan.config().frame_latency;
        lan.deliver(arrives, &mut [&mut a.endpoint, &mut b.endpoint]);

        let got = |bytes: &[u8]| -> Vec<LanPort> {
            lan.frame_log()
                .iter()
                .filter(|r| r.bytes == bytes)
                .map(|r| r.to)
                .collect()
        };
        assert_eq!(got(&hello), [LanPort::Board(b.id()), LanPort::Gateway]);
        assert_eq!(got(&direct), [LanPort::Board(a.id())]);
        assert!(
            lan.frame_log()
                .iter()
                .filter(|r| r.bytes == hello || r.bytes == direct)
                .all(|r| r.at == arrives),
            "one stated latency for every frame"
        );
    }

    #[test]
    fn multicast_carries_each_boards_name_to_a_probe() {
        let mut lan = lan();
        let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
        a.join(&mut lan);
        b.join(&mut lan);
        let probe = lan.add_probe();
        run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
        lan.probe_mut(probe).query("lp-aaaa.local", TYPE_A);
        lan.probe_mut(probe).query("lp-bbbb.local", TYPE_A);
        run(&mut lan, &mut [&mut a, &mut b], 20 * MS, 25 * MS);
        assert_eq!(lan.probe(probe).resolved("lp-aaaa.local"), a.ip);
        assert_eq!(lan.probe(probe).resolved("lp-bbbb.local"), b.ip);
        assert_eq!(lan.probe(probe).resolved("lp-cccc.local"), None);
    }

    #[test]
    fn a_full_endpoint_refuses_and_the_lan_counts_it() {
        let mut lan = lan();
        let mut a = TestBoard::with_config(
            0,
            "lp-aaaa",
            PacerConfig {
                queue_bound: 2,
                ..net_pacer_config()
            },
        );
        let mut b = TestBoard::new(1, "lp-bbbb");
        a.join(&mut lan);
        b.join(&mut lan);
        run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
        let refused_before = lan.counters().refused;

        // Five frames for a in one quantum, and a takes none of them.
        let t = 20 * MS;
        for i in 0..5u8 {
            let f = UdpDatagram {
                src_mac: b.mac,
                dst_mac: a.mac,
                src_ip: b.ip.unwrap(),
                dst_ip: a.ip.unwrap(),
                src_port: 9,
                dst_port: 9,
                payload: &[i],
            }
            .emit();
            b.endpoint.push_outbound(EndpointEvent { at: t, bytes: f });
        }
        lan.deliver(t + MS, &mut [&mut a.endpoint, &mut b.endpoint]);
        assert_eq!(a.endpoint.inbound_len(), 2, "the bound holds");
        assert_eq!(lan.counters().refused - refused_before, 3);
        assert_eq!(a.endpoint.refused(), 3);
    }

    #[test]
    fn the_same_script_gives_the_same_frame_log_twice() {
        let script = || {
            let mut lan = lan();
            lan.log_frames(true);
            let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
            a.join(&mut lan);
            b.join(&mut lan);
            let probe = lan.add_probe();
            run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
            lan.probe_mut(probe).query("lp-bbbb.local", TYPE_A);
            let conn = lan.probe_mut(probe).connect(a.ip.unwrap(), 80);
            run(&mut lan, &mut [&mut a, &mut b], 20 * MS, 25 * MS);
            lan.probe_mut(probe).send(conn, b"ping");
            run(&mut lan, &mut [&mut a, &mut b], 25 * MS, 30 * MS);
            assert_eq!(lan.probe_mut(probe).recv(conn), b"ping");
            (lan.frame_log().to_vec(), lan.counters())
        };
        let (first, counters) = script();
        let (second, again) = script();
        assert!(
            first.len() > 20,
            "the script carried something: {}",
            first.len()
        );
        assert_eq!(first, second);
        assert_eq!(counters, again);
    }

    #[test]
    fn a_lost_network_drops_the_link_and_a_scan_hears_whats_left() {
        let mut lan = lan();
        let mut a = TestBoard::new(0, "lp-aaaa");
        a.join(&mut lan);
        run(&mut lan, &mut [&mut a], 0, 20 * MS);
        assert_eq!(lan.take_event(a.id()), Some(StationEvent::Associated));
        lan.remove_access_point("home");
        assert!(!lan.link_up(a.id()));
        assert_eq!(lan.take_event(a.id()), Some(StationEvent::LinkLost));
        assert!(lan.scan_start(a.id(), 20 * MS));
        assert!(lan.next_due().unwrap() <= 20 * MS + lan.config().scan_latency);
        run(&mut lan, &mut [&mut a], 20 * MS, 130 * MS);
        assert_eq!(lan.take_event(a.id()), Some(StationEvent::ScanDone));
        let heard: Vec<&str> = lan
            .scan_results(a.id())
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(heard, ["cafe"], "the hidden attic is left out");
    }

    #[test]
    fn endpoints_of_other_seams_are_left_alone() {
        let mut lan = lan();
        let other = EndpointId {
            board: ParticipantId(0),
            seam: "ble",
        };
        let mut ep = SeamEndpoint::new(other, 1, PacerConfig::default());
        ep.push_outbound(EndpointEvent {
            at: 0,
            bytes: vec![0; 60],
        });
        lan.deliver(MS, &mut [&mut ep]);
        assert_eq!(
            ep.drain_outbound().len(),
            1,
            "not drained: another medium's"
        );
        assert_eq!(lan.counters(), LanCounters::default());
    }

    #[test]
    fn several_lans_in_one_process_share_nothing() {
        let (mut one, mut two) = (lan(), lan());
        let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
        a.join(&mut one);
        b.join(&mut two);
        run(&mut one, &mut [&mut a], 0, 20 * MS);
        run(&mut two, &mut [&mut b], 0, 20 * MS);
        // Each LAN's first lease: the same address on two separate networks.
        assert_eq!(a.ip, Some(Ipv4Addr::new(192, 168, 4, 100)));
        assert_eq!(b.ip, a.ip);
        assert!(one.station(b.id()).is_none());
    }

    /// One millisecond at the tests' 160 MHz.
    const MS: Cycles = 160_000;

    fn lan() -> VirtualLan {
        let mut lan = VirtualLan::new(LanConfig::new(160));
        for ap in crate::seam::net::virtual_access_point::tests::fixture_access_points() {
            lan.add_access_point(ap);
        }
        lan
    }
}
