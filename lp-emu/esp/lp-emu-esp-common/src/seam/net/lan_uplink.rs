//! A LAN's uplink (Wi-Fi relay plan P9): the one way out of the virtual LAN,
//! to a host address a run names — the mirror of
//! [`super::lan_port_forward`].
//!
//! A run names a host (`lightplayer.app`) and where it really is
//! (`127.0.0.1:<port>`, a cloud server on the host). The gateway then holds
//! a second address beyond the /24, the **uplink address** (TEST-NET-1,
//! `192.0.2.1`), answers DNS for the name with it
//! ([`super::lan_dns_server`]), and listens on `<uplink address>:<port>`
//! in its smoltcp stack. A board that dials the name reaches that listener
//! through its default route (the gateway); each connection it opens is
//! paired, there and then, with a host `TcpStream::connect` to the named
//! address, and bytes are copied both ways exactly as a forward copies them.
//! The board runs today's firmware: it dials `lightplayer.app` like a real
//! one.
//!
//! **The host side is wall-clock**, like a forward's; the LAN side stays in
//! guest time. A connection whose board no longer holds the address it
//! dialled from is closed at once (the W9 lesson the forward learnt: left
//! open, its ARP requests would hold the gateway's one-a-second ARP rate
//! limit for every other connection), and one that stops answering with
//! data waiting is bounded by the forward's
//! [`super::lan_port_forward::CONNECTION_TIMEOUT`].
//!
//! Generic TCP: nothing here knows what the board and the host say.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use smoltcp::iface::SocketHandle;
use smoltcp::socket::tcp;
use smoltcp::wire::{IpAddress, IpListenEndpoint};

use super::lan_port_forward::{CONNECTION_TIMEOUT, ForwardConn, ForwardCounters, SOCKET_BUFFER};
use super::lan_stack::LanStack;

/// The uplink address: TEST-NET-1 (RFC 5737), outside every LAN this
/// emulator makes, and never anybody's real address.
pub const UPLINK_IP: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);

/// How long the host side's connect may take (a host on loopback answers
/// at once, or refuses).
const HOST_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// One named host beyond the LAN.
pub struct LanUplink {
    /// The name a board resolves (`lightplayer.app`).
    pub name: String,
    /// The port a board dials (80, the relay's device leg).
    pub port: u16,
    /// Where the host side connects.
    pub to: SocketAddr,
    listener: Option<SocketHandle>,
    conns: Vec<ForwardConn>,
    counters: ForwardCounters,
}

impl LanUplink {
    /// `name`, dialled on `port`, carried to `to`.
    pub fn new(name: &str, port: u16, to: SocketAddr) -> Self {
        Self {
            name: name.trim_end_matches('.').to_ascii_lowercase(),
            port,
            to,
            listener: None,
            conns: Vec::new(),
            counters: ForwardCounters::default(),
        }
    }

    /// Whether a board asking for `name` means this uplink (case-blind).
    pub fn is_named(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name.trim_end_matches('.'))
    }

    /// What it has carried: `accepted` boards' connections paired with the
    /// host, `refused` ones the host would not take, the bytes each way,
    /// and `moved` connections closed because their board's address went.
    pub fn counters(&self) -> ForwardCounters {
        self.counters
    }

    /// Connections open now.
    pub fn open(&self) -> usize {
        self.conns.len()
    }

    /// Listen, pair what a board opened with the host, copy bytes both ways
    /// and close what has finished. `bound(ip)`: a board holds `ip` now.
    pub(super) fn pump(&mut self, stack: &mut LanStack, bound: &dyn Fn(Ipv4Addr) -> bool) {
        self.accept(stack);
        for conn in &mut self.conns {
            if bound(conn.board_ip()) {
                conn.pump(stack, &mut self.counters);
            } else {
                conn.close_moved(stack);
                self.counters.moved += 1;
            }
        }
        let (dead, live): (Vec<_>, Vec<_>) = std::mem::take(&mut self.conns)
            .into_iter()
            .partition(ForwardConn::is_dead);
        for conn in dead {
            stack.sockets.remove(conn.socket());
        }
        self.conns = live;
    }

    /// Keep one socket listening; when a board has opened it, pair it with
    /// the host and listen on a fresh one.
    fn accept(&mut self, stack: &mut LanStack) {
        let handle = match self.listener {
            Some(handle) => handle,
            None => {
                let mut socket = tcp::Socket::new(
                    tcp::SocketBuffer::new(vec![0u8; SOCKET_BUFFER]),
                    tcp::SocketBuffer::new(vec![0u8; SOCKET_BUFFER]),
                );
                socket.set_timeout(Some(CONNECTION_TIMEOUT));
                let handle = stack.sockets.add(socket);
                self.listener = Some(handle);
                handle
            }
        };
        let socket = stack.sockets.get_mut::<tcp::Socket>(handle);
        match socket.state() {
            // Waiting, or a board's handshake under way: the pair is made
            // once it is established (a connection that is not yet would
            // read to the copy as one the board already finished).
            tcp::State::Listen | tcp::State::SynReceived => {}
            // New, or the host refused the last board (aborted, its RST
            // sent): listen (again).
            tcp::State::Closed => {
                let _ = socket.listen(IpListenEndpoint {
                    addr: Some(IpAddress::Ipv4(UPLINK_IP)),
                    port: self.port,
                });
            }
            // A board has dialled in.
            _ => {
                let board_ip = match socket.remote_endpoint() {
                    Some(endpoint) => match endpoint.addr {
                        IpAddress::Ipv4(ip) => ip,
                    },
                    None => return,
                };
                match connect_host(self.to) {
                    Some(host) => {
                        self.counters.accepted += 1;
                        self.conns.push(ForwardConn::new(host, handle, board_ip));
                        self.listener = None;
                    }
                    None => {
                        self.counters.refused += 1;
                        socket.abort();
                    }
                }
            }
        }
    }
}

/// The host side of one connection, non-blocking; `None` if it refused.
fn connect_host(to: SocketAddr) -> Option<TcpStream> {
    if cfg!(target_family = "wasm") {
        return None;
    }
    let host = TcpStream::connect_timeout(&to, HOST_CONNECT_TIMEOUT).ok()?;
    host.set_nonblocking(true).ok()?;
    let _ = host.set_nodelay(true);
    Some(host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use lp_emu_core::sched::Cycles;

    use crate::seam::net::lan_dns::{DnsData, parse_reply};
    use crate::seam::net::lan_test_board::{QUANTUM, TestBoard, run};
    use crate::seam::net::{LanConfig, VirtualLan};

    const MS: Cycles = 160_000;

    /// The whole way out: DHCP names the gateway as the DNS server, the
    /// name resolves to the uplink address and nothing else resolves, and a
    /// connection the board dials there reaches the host and back.
    #[test]
    fn a_board_resolves_the_named_host_and_reaches_it_through_the_uplink() {
        let host = TcpListener::bind("127.0.0.1:0").unwrap();
        let to = host.local_addr().unwrap();
        let echo = std::thread::spawn(move || {
            let (mut conn, _) = host.accept().unwrap();
            let mut buf = [0u8; 64];
            let n = conn.read(&mut buf).unwrap();
            conn.write_all(&buf[..n]).unwrap();
            n
        });

        let mut lan = VirtualLan::new(LanConfig::new(160));
        for ap in crate::seam::net::virtual_access_point::tests::fixture_access_points() {
            lan.add_access_point(ap);
        }
        assert_eq!(lan.uplink("lightplayer.app", 80, to).unwrap(), UPLINK_IP);
        let mut board = TestBoard::new(0, "lp-aaaa");
        board.join(&mut lan);
        run(&mut lan, &mut [&mut board], 0, 20 * MS);
        assert!(board.ip.is_some());
        assert_eq!(board.dns_server, Some(lan.config().gateway_ip));

        board.ask_dns("lightplayer.app");
        board.ask_dns("example.com");
        let mut now = 20 * MS;
        while board.dns_replies.len() < 2 && now < 200 * MS {
            run(&mut lan, &mut [&mut board], now, now + QUANTUM);
            now += QUANTUM;
        }
        let answers: Vec<_> = board
            .dns_replies
            .iter()
            .map(|reply| parse_reply(reply).unwrap())
            .collect();
        assert_eq!(answers[0].len(), 1);
        assert_eq!(answers[0][0].data, DnsData::A(UPLINK_IP));
        assert!(answers[1].is_empty(), "nothing else resolves");
        assert_eq!(board.dns_replies[1][3] & 0x0f, 3, "NXDOMAIN");

        board.dial(UPLINK_IP, 80);
        let mut sent = false;
        let mut got = Vec::new();
        while got.len() < 9 && now < 2_000 * MS {
            if !sent {
                sent = board.client_send(b"via cloud") == 9;
            }
            run(&mut lan, &mut [&mut board], now, now + QUANTUM);
            now += QUANTUM;
            got.extend(board.client_recv());
            std::thread::yield_now();
        }
        assert_eq!(got, b"via cloud");
        assert_eq!(echo.join().unwrap(), 9);
        let counters = lan.gateway().uplinks()[0].counters();
        assert_eq!(counters.accepted, 1);
        assert_eq!((counters.bytes_to_host, counters.bytes_to_board), (9, 9));
    }

    #[test]
    fn a_lan_without_an_uplink_names_no_dns_server() {
        let mut lan = VirtualLan::new(LanConfig::new(160));
        for ap in crate::seam::net::virtual_access_point::tests::fixture_access_points() {
            lan.add_access_point(ap);
        }
        let mut board = TestBoard::new(0, "lp-aaaa");
        board.join(&mut lan);
        run(&mut lan, &mut [&mut board], 0, 20 * MS);
        assert!(board.ip.is_some());
        assert_eq!(board.dns_server, None, "today's LAN, byte for byte");
    }

    #[test]
    fn a_name_is_matched_case_blind_without_its_trailing_dot() {
        let uplink = LanUplink::new("LightPlayer.app.", 80, "127.0.0.1:1".parse().unwrap());
        assert!(uplink.is_named("lightplayer.app"));
        assert!(uplink.is_named("LIGHTPLAYER.APP."));
        assert!(!uplink.is_named("example.com"));
        assert_eq!(uplink.open(), 0);
    }
}
