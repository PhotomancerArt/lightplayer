//! A host-side participant on the LAN, for tests: ask the boards their names
//! over mDNS and DNS-SD, and open a TCP connection to one, with no board in
//! the middle.
//!
//! A probe is a port on the segment with its own MAC, an address the
//! gateway reserves for it (so it needs no DHCP exchange) and a smoltcp
//! stack, run in guest time by the [`super::VirtualLan`] that holds it. It
//! listens on the mDNS group and keeps every record any reply carries, so a
//! test can check "both boards answered their names" after a run.

use std::net::Ipv4Addr;

use lp_emu_core::sched::Cycles;
use smoltcp::iface::SocketHandle;
use smoltcp::socket::{tcp, udp};
use smoltcp::wire::{IpAddress, IpEndpoint};

use super::lan_dns::{self, DnsAnswer, MDNS_GROUP, MDNS_PORT};
use super::lan_stack::LanStack;

const MDNS_BUFFER: usize = 8 * 1024;
const TCP_BUFFER: usize = 16 * 1024;
const FIRST_LOCAL_PORT: u16 = 40_000;

/// Which probe on a LAN.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProbeId(pub usize);

/// One of a probe's TCP connections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeConn(SocketHandle);

/// A test's participant on the LAN.
pub struct LanProbe {
    pub mac: [u8; 6],
    pub ip: Ipv4Addr,
    pub(super) stack: LanStack,
    mdns: SocketHandle,
    queries: Vec<Vec<u8>>,
    answers: Vec<DnsAnswer>,
    next_local_port: u16,
}

impl LanProbe {
    pub(super) fn new(
        mac: [u8; 6],
        ip: Ipv4Addr,
        gateway: Ipv4Addr,
        cycles_per_us: u64,
        now: Cycles,
    ) -> Self {
        let mut stack = LanStack::new(mac, cycles_per_us, now);
        stack.set_address(ip, 24, Some(gateway));
        stack
            .iface
            .join_multicast_group(MDNS_GROUP)
            .expect("one group fits");
        let mut socket = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 16], vec![0; MDNS_BUFFER]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 16], vec![0; MDNS_BUFFER]),
        );
        socket.bind(MDNS_PORT).expect("a fresh socket binds");
        let mdns = stack.sockets.add(socket);
        Self {
            mac,
            ip,
            stack,
            mdns,
            queries: Vec::new(),
            answers: Vec::new(),
            next_local_port: FIRST_LOCAL_PORT,
        }
    }

    /// Ask the LAN for `name`'s records of type `rtype` (an mDNS query to the
    /// group, from port 5353). Sent the next time the LAN is driven.
    pub fn query(&mut self, name: &str, rtype: u16) {
        self.queries.push(lan_dns::encode_query(name, rtype));
    }

    /// Every record every reply has carried so far, in arrival order.
    pub fn answers(&self) -> &[DnsAnswer] {
        &self.answers
    }

    /// The address some reply gave `name` (an A record), if any has.
    pub fn resolved(&self, name: &str) -> Option<Ipv4Addr> {
        self.answers.iter().find_map(|a| match a.data {
            lan_dns::DnsData::A(ip) if a.is_named(name) => Some(ip),
            _ => None,
        })
    }

    pub fn clear_answers(&mut self) {
        self.answers.clear();
    }

    /// Open a TCP connection to `ip:port`; it connects as the LAN runs.
    pub fn connect(&mut self, ip: Ipv4Addr, port: u16) -> ProbeConn {
        let mut socket = tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0; TCP_BUFFER]),
            tcp::SocketBuffer::new(vec![0; TCP_BUFFER]),
        );
        let local = self.next_local_port;
        self.next_local_port = self.next_local_port.wrapping_add(1).max(FIRST_LOCAL_PORT);
        socket
            .connect(
                self.stack.iface.context(),
                IpEndpoint::new(IpAddress::Ipv4(ip), port),
                local,
            )
            .expect("a fresh socket connects");
        ProbeConn(self.stack.sockets.add(socket))
    }

    /// Queue bytes on a connection; how many it took.
    pub fn send(&mut self, conn: ProbeConn, bytes: &[u8]) -> usize {
        self.stack
            .sockets
            .get_mut::<tcp::Socket>(conn.0)
            .send_slice(bytes)
            .unwrap_or(0)
    }

    /// Everything received on a connection since the last call.
    pub fn recv(&mut self, conn: ProbeConn) -> Vec<u8> {
        let socket = self.stack.sockets.get_mut::<tcp::Socket>(conn.0);
        let mut out = Vec::new();
        while socket.can_recv() {
            let mut buf = [0u8; 4096];
            match socket.recv_slice(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
            }
        }
        out
    }

    pub fn is_established(&self, conn: ProbeConn) -> bool {
        self.stack.sockets.get::<tcp::Socket>(conn.0).state() == tcp::State::Established
    }

    pub fn state(&self, conn: ProbeConn) -> tcp::State {
        self.stack.sockets.get::<tcp::Socket>(conn.0).state()
    }

    pub fn close(&mut self, conn: ProbeConn) {
        self.stack.sockets.get_mut::<tcp::Socket>(conn.0).close();
    }

    /// One delivered frame, at its arrival cycle: what the stack sends back.
    pub(super) fn receive(&mut self, at: Cycles, frame: Vec<u8>) -> Vec<Vec<u8>> {
        self.stack.push_frame(frame);
        self.run(at)
    }

    /// Run at `now`: send queued queries, keep every reply, run the timers.
    pub(super) fn run(&mut self, now: Cycles) -> Vec<Vec<u8>> {
        let to = IpEndpoint::new(IpAddress::Ipv4(MDNS_GROUP), MDNS_PORT);
        let socket = self.stack.sockets.get_mut::<udp::Socket>(self.mdns);
        let queries = std::mem::take(&mut self.queries);
        let mut unsent = Vec::new();
        for q in queries {
            if !socket.can_send() || socket.send_slice(&q, to).is_err() {
                unsent.push(q);
            }
        }
        self.queries = unsent;
        let mut out = self.stack.poll(now);
        let socket = self.stack.sockets.get_mut::<udp::Socket>(self.mdns);
        let mut buf = vec![0u8; MDNS_BUFFER];
        while let Ok((n, _)) = socket.recv_slice(&mut buf) {
            if let Some(records) = lan_dns::parse_reply(&buf[..n]) {
                self.answers.extend(records);
            }
        }
        // A reply read just now may have freed a timer; one more run sends
        // anything it made (an ACK) at the same cycle.
        out.extend(self.stack.poll(now));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::net::lan_dns::{TYPE_A, TYPE_PTR};
    use crate::seam::net::lan_test_board::{TestBoard, run};
    use crate::seam::net::{LanConfig, VirtualLan};

    #[test]
    fn a_probe_talks_tcp_to_a_board_with_no_board_in_the_middle() {
        let mut lan = lan();
        let mut a = TestBoard::new(0, "lp-aaaa");
        a.join(&mut lan);
        let probe = lan.add_probe();
        assert_eq!(
            lan.probe(probe).ip,
            Ipv4Addr::new(192, 168, 4, 101),
            "reserved after a"
        );
        run(&mut lan, &mut [&mut a], 0, 20 * MS);

        let conn = lan.probe_mut(probe).connect(a.ip.unwrap(), 80);
        run(&mut lan, &mut [&mut a], 20 * MS, 22 * MS);
        assert!(lan.probe(probe).is_established(conn));
        assert_eq!(lan.probe_mut(probe).send(conn, b"two ways"), 8);
        run(&mut lan, &mut [&mut a], 22 * MS, 24 * MS);
        assert_eq!(lan.probe_mut(probe).recv(conn), b"two ways");
        lan.probe_mut(probe).close(conn);
        run(&mut lan, &mut [&mut a], 24 * MS, 26 * MS);
        assert!(matches!(
            lan.probe(probe).state(conn),
            tcp::State::TimeWait | tcp::State::Closed
        ));
    }

    #[test]
    fn a_query_nobody_answers_collects_nothing() {
        let mut lan = lan();
        let mut a = TestBoard::new(0, "lp-aaaa");
        a.join(&mut lan);
        let probe = lan.add_probe();
        run(&mut lan, &mut [&mut a], 0, 20 * MS);
        lan.probe_mut(probe)
            .query("_lightplayer._tcp.local", TYPE_PTR);
        lan.probe_mut(probe).query("lp-zzzz.local", TYPE_A);
        run(&mut lan, &mut [&mut a], 20 * MS, 25 * MS);
        assert!(lan.probe(probe).answers().is_empty());
        lan.probe_mut(probe).query("lp-aaaa.local", TYPE_A);
        run(&mut lan, &mut [&mut a], 25 * MS, 30 * MS);
        assert_eq!(lan.probe(probe).answers().len(), 1);
        lan.probe_mut(probe).clear_answers();
        assert_eq!(lan.probe(probe).resolved("lp-aaaa.local"), None);
    }

    const MS: Cycles = 160_000;

    fn lan() -> VirtualLan {
        let mut lan = VirtualLan::new(LanConfig::new(160));
        for ap in crate::seam::net::virtual_access_point::tests::fixture_access_points() {
            lan.add_access_point(ap);
        }
        lan
    }
}
