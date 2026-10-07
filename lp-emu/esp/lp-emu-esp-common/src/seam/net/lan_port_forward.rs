//! A host TCP port forwarded to one board's port over the LAN.
//!
//! This is how a host tool reaches an emulated board "over Wi-Fi": it
//! connects to `127.0.0.1:<port>`, and the gateway opens a TCP connection
//! across the segment to `<board's address>:<board port>` (80, the board's
//! LAN endpoint) from its own smoltcp stack, and copies bytes both ways.
//!
//! **The host side is wall-clock**, like the USB door: the listener and its
//! connections are non-blocking host sockets, read and written whenever the
//! medium is driven. The LAN side stays in guest time: what crosses the
//! segment does so at guest cycles, through the gateway's stack.
//!
//! A host connection made before the board has an address is closed at
//! once (counted as refused): there is nobody to forward it to. A board that
//! refuses the connection (nothing listening) closes the host's too.
//!
//! **A connection follows the address it was opened to.** When the board's
//! lease moves (a renumbered board restarting onto a new address) or goes,
//! every connection opened to the old address is closed at once, host side
//! and all (counted as moved): nobody answers there any more, and the host
//! reconnects to reach the board where it is now. Left open it would not
//! just hang: the gateway's stack is smoltcp, whose ARP rate limit is **one
//! request a second for the whole stack**, so a connection asking for a dead
//! address starves every other connection's ARP — the board's new address,
//! and every other board's forward (walk step W9). A peer that stops
//! answering while it still holds its address (its link dropped, its board
//! stopped) is bounded the same way by [`CONNECTION_TIMEOUT`].

use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};

use smoltcp::iface::SocketHandle;
use smoltcp::socket::tcp;
use smoltcp::time::Duration;
use smoltcp::wire::IpAddress;

use super::lan_stack::LanStack;

/// Each forwarded connection's buffer, each way, in the gateway's stack.
pub(super) const SOCKET_BUFFER: usize = 16 * 1024;

/// The first of the gateway's own ports for forwarded connections.
const FIRST_LOCAL_PORT: u16 = 49_152;

/// A forwarded connection whose board answers nothing for this long while
/// data waits for it (or never answers its SYN) is reset, host side too, in
/// the LAN's time: a long stall, well past a busy board's, and the bound on
/// how long a vanished peer's ARP requests can hold the gateway's rate limit
/// (see the module docs). smoltcp applies it only with data outstanding, so
/// an idle connection stays open however quiet it is.
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

/// What one forward has done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForwardCounters {
    /// Host connections accepted and forwarded.
    pub accepted: u64,
    /// Host connections closed at once: the board had no address.
    pub refused: u64,
    pub bytes_to_board: u64,
    pub bytes_to_host: u64,
    /// Connections closed because the board's address moved (or its lease
    /// went) after they were opened.
    pub moved: u64,
}

/// One board's forward.
pub struct LanPortForward {
    /// The board's MAC, which names its lease.
    pub board_mac: [u8; 6],
    pub board_port: u16,
    listener: TcpListener,
    local: SocketAddr,
    conns: Vec<ForwardConn>,
    next_local_port: u16,
    counters: ForwardCounters,
}

/// One host connection paired with one socket in the gateway's stack, bytes
/// copied both ways: a forward's (the host dialled the board) and an
/// uplink's (`super::lan_uplink`: the board dialled the host).
pub(super) struct ForwardConn {
    host: TcpStream,
    socket: SocketHandle,
    /// The board's address when this connection was opened.
    board_ip: Ipv4Addr,
    to_board: Vec<u8>,
    to_host: Vec<u8>,
    host_eof: bool,
    fin_sent: bool,
    host_shut: bool,
    dead: bool,
}

impl LanPortForward {
    /// Listen on `host` (port 0 for one the OS picks) and forward to the
    /// board with `board_mac`, at `board_port`.
    ///
    /// A wasm build (the Studio tab's module) has no host sockets — the page
    /// has none to give, and no forward is ever asked of it — so there this
    /// is refused, and the module links no socket call it would need.
    pub fn bind(host: SocketAddr, board_mac: [u8; 6], board_port: u16) -> io::Result<Self> {
        if cfg!(target_family = "wasm") {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "a LAN port forward needs host sockets, and a wasm build has none",
            ));
        }
        let listener = TcpListener::bind(host)?;
        listener.set_nonblocking(true)?;
        let local = listener.local_addr()?;
        Ok(Self {
            board_mac,
            board_port,
            listener,
            local,
            conns: Vec::new(),
            next_local_port: FIRST_LOCAL_PORT,
            counters: ForwardCounters::default(),
        })
    }

    /// Where a host connects.
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    pub fn counters(&self) -> ForwardCounters {
        self.counters
    }

    /// Connections open now.
    pub fn open(&self) -> usize {
        self.conns.len()
    }

    /// Move bytes both ways: accept new host connections, copy what each
    /// side has into the other as far as it will take, and close what has
    /// finished. `board_ip`: the board's address, when it holds one.
    pub fn pump(&mut self, stack: &mut LanStack, board_ip: Option<Ipv4Addr>) {
        self.accept(stack, board_ip);
        for conn in &mut self.conns {
            if board_ip == Some(conn.board_ip) {
                conn.pump(stack, &mut self.counters);
            } else {
                conn.close_moved(stack);
                self.counters.moved += 1;
            }
        }
        let (dead, live): (Vec<_>, Vec<_>) = std::mem::take(&mut self.conns)
            .into_iter()
            .partition(|c| c.dead);
        for conn in dead {
            stack.sockets.remove(conn.socket);
        }
        self.conns = live;
    }

    fn accept(&mut self, stack: &mut LanStack, board_ip: Option<Ipv4Addr>) {
        loop {
            let host = match self.listener.accept() {
                Ok((host, _)) => host,
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(_) => return,
            };
            let Some(ip) = board_ip else {
                self.counters.refused += 1;
                shut(&host, Shutdown::Both);
                continue;
            };
            if host.set_nonblocking(true).is_err() {
                continue;
            }
            let _ = host.set_nodelay(true);
            let mut socket = tcp::Socket::new(
                tcp::SocketBuffer::new(vec![0u8; SOCKET_BUFFER]),
                tcp::SocketBuffer::new(vec![0u8; SOCKET_BUFFER]),
            );
            socket.set_timeout(Some(CONNECTION_TIMEOUT));
            let local_port = self.next_local_port;
            self.next_local_port = self
                .next_local_port
                .checked_add(1)
                .unwrap_or(FIRST_LOCAL_PORT);
            if socket
                .connect(
                    stack.iface.context(),
                    (IpAddress::Ipv4(ip), self.board_port),
                    local_port,
                )
                .is_err()
            {
                self.counters.refused += 1;
                shut(&host, Shutdown::Both);
                continue;
            }
            let handle = stack.sockets.add(socket);
            self.counters.accepted += 1;
            self.conns.push(ForwardConn::new(host, handle, ip));
        }
    }
}

impl ForwardConn {
    /// `host` (non-blocking) paired with `socket` in the gateway's stack, to
    /// or from the board at `board_ip`.
    pub(super) fn new(host: TcpStream, socket: SocketHandle, board_ip: Ipv4Addr) -> Self {
        Self {
            host,
            socket,
            board_ip,
            to_board: Vec::new(),
            to_host: Vec::new(),
            host_eof: false,
            fin_sent: false,
            host_shut: false,
            dead: false,
        }
    }

    /// The board's address this connection belongs to.
    pub(super) fn board_ip(&self) -> Ipv4Addr {
        self.board_ip
    }

    /// Finished: drop it (its socket from the stack too).
    pub(super) fn is_dead(&self) -> bool {
        self.dead
    }

    /// The gateway stack's socket.
    pub(super) fn socket(&self) -> SocketHandle {
        self.socket
    }

    /// The board no longer holds the address this connection was opened to:
    /// close the host's side and drop the socket. It is removed from the
    /// stack before it can dispatch anything, so no RST goes out — sending
    /// one would ask ARP for the dead address one last time.
    pub(super) fn close_moved(&mut self, stack: &mut LanStack) {
        stack.sockets.get_mut::<tcp::Socket>(self.socket).abort();
        shut(&self.host, Shutdown::Both);
        self.dead = true;
    }

    pub(super) fn pump(&mut self, stack: &mut LanStack, counters: &mut ForwardCounters) {
        let socket = stack.sockets.get_mut::<tcp::Socket>(self.socket);

        // Host → board: read only once what was read last time is handed on.
        if self.to_board.is_empty() && !self.host_eof {
            let mut buf = [0u8; 4096];
            match self.host.read(&mut buf) {
                Ok(0) => self.host_eof = true,
                Ok(n) => self.to_board.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => {
                    socket.abort();
                    self.dead = true;
                    return;
                }
            }
        }
        if !self.to_board.is_empty() && socket.can_send() {
            if let Ok(n) = socket.send_slice(&self.to_board) {
                counters.bytes_to_board += n as u64;
                self.to_board.drain(..n);
            }
        }
        if self.host_eof && self.to_board.is_empty() && !self.fin_sent {
            socket.close();
            self.fin_sent = true;
        }

        // Board → host.
        while socket.can_recv() && self.to_host.len() < SOCKET_BUFFER {
            let mut buf = [0u8; 4096];
            match socket.recv_slice(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => self.to_host.extend_from_slice(&buf[..n]),
            }
        }
        while !self.to_host.is_empty() {
            match self.host.write(&self.to_host) {
                Ok(0) => break,
                Ok(n) => {
                    counters.bytes_to_host += n as u64;
                    self.to_host.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => {
                    socket.abort();
                    self.dead = true;
                    return;
                }
            }
        }

        // The board has finished sending (or refused, or reset): pass it on.
        let board_done =
            !socket.may_recv() && !socket.can_recv() && socket.state() != tcp::State::SynSent;
        if board_done && self.to_host.is_empty() && !self.host_shut {
            shut(&self.host, Shutdown::Write);
            self.host_shut = true;
        }
        let finished = matches!(socket.state(), tcp::State::Closed | tcp::State::TimeWait);
        if finished && self.to_host.is_empty() {
            shut(&self.host, Shutdown::Both);
            self.dead = true;
        }
    }
}

/// Close one direction of a host connection. On a wasm build no forward is
/// ever bound ([`LanPortForward::bind`]), so this is never reached there, and
/// it calls nothing: the tab's WASI shim answers no `sock_shutdown`, and a
/// module that imported it would not instantiate.
pub(super) fn shut(stream: &TcpStream, how: Shutdown) {
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = stream.shutdown(how);
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (stream, how);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    use lp_emu_core::sched::Cycles;

    use crate::seam::net::lan_test_board::{QUANTUM, TestBoard, run};
    use crate::seam::net::{LanConfig, VirtualLan};

    #[test]
    fn the_forward_carries_bytes_both_ways_to_a_board_and_closes_with_it() {
        let mut lan = lan();
        let mut board = TestBoard::new(0, "lp-aaaa");
        board.join(&mut lan);
        let at = lan
            .forward(board.id(), "127.0.0.1:0".parse().unwrap(), 80)
            .unwrap();
        run(&mut lan, &mut [&mut board], 0, 20 * MS);
        assert!(board.ip.is_some());

        let mut host = TcpStream::connect(at).unwrap();
        host.set_nonblocking(true).unwrap();
        host.write_all(b"over the lan").unwrap();
        let mut now = 20 * MS;
        let echoed = drive_until(&mut lan, &mut board, &mut host, &mut now, |got| {
            got.len() >= 12
        });
        assert_eq!(
            echoed, b"over the lan",
            "the board's echo, back at the host"
        );

        // The host is done: the board sees the close, closes its side, and
        // the host reads the end.
        host.shutdown(std::net::Shutdown::Write).unwrap();
        let mut ended = false;
        for _ in 0..2_000 {
            run(&mut lan, &mut [&mut board], now, now + QUANTUM);
            now += QUANTUM;
            let mut buf = [0u8; 64];
            match host.read(&mut buf) {
                Ok(0) => {
                    ended = true;
                    break;
                }
                Ok(_) => panic!("nothing more was sent"),
                Err(_) => std::thread::yield_now(),
            }
        }
        assert!(ended, "the host reads the board's close");
        let f = lan.gateway().forwards()[0].counters();
        assert_eq!((f.accepted, f.refused), (1, 0));
        assert_eq!((f.bytes_to_board, f.bytes_to_host), (12, 12));
    }

    #[test]
    fn a_board_with_nothing_listening_closes_the_hosts_connection() {
        let mut lan = lan();
        let mut board = TestBoard::new(0, "lp-aaaa");
        board.join(&mut lan);
        let at = lan
            .forward(board.id(), "127.0.0.1:0".parse().unwrap(), 81)
            .unwrap();
        run(&mut lan, &mut [&mut board], 0, 20 * MS);
        let mut host = TcpStream::connect(at).unwrap();
        host.set_nonblocking(true).unwrap();
        let mut now = 20 * MS;
        let mut closed = false;
        for _ in 0..2_000 {
            run(&mut lan, &mut [&mut board], now, now + QUANTUM);
            now += QUANTUM;
            let mut buf = [0u8; 8];
            match host.read(&mut buf) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(_) => panic!("nothing was sent"),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(_) => {
                    closed = true;
                    break;
                }
            }
        }
        assert!(closed, "the board's reset reached the host");
        assert_eq!(lan.gateway().forwards()[0].open(), 0);
    }

    /// W9 of the emulated Wi-Fi walk: a board restarts onto a new lease while
    /// a host connection to its old address keeps talking. Past the
    /// gateway's neighbour lifetime (60 s) that connection asks for the old
    /// address once a second, and smoltcp's ARP rate limit is one for the
    /// whole stack — so, left alone, it starved every later connection's ARP
    /// for the new address, and nothing reached the board again.
    #[test]
    fn a_board_that_moved_address_is_reached_again_and_its_old_connections_close() {
        let mut lan = lan();
        let mut board = TestBoard::new(0, "lp-aaaa");
        board.join(&mut lan);
        let at = lan
            .forward(board.id(), "127.0.0.1:0".parse().unwrap(), 80)
            .unwrap();
        run(&mut lan, &mut [&mut board], 0, 20 * MS);
        let old_ip = board.ip.expect("an address");

        let mut chatty = TcpStream::connect(at).unwrap();
        chatty.set_nonblocking(true).unwrap();
        chatty.write_all(b"hello").unwrap();
        let mut now = 20 * MS;
        let echoed = drive_until(&mut lan, &mut board, &mut chatty, &mut now, |got| {
            got.len() >= 5
        });
        assert_eq!(echoed, b"hello");

        // The board restarts (a fresh stack, the same MAC) onto a new lease.
        lan.renumber_next_lease(board.id());
        lan.reset_station(board.id());
        let mut board = TestBoard::new(0, "lp-aaaa");
        assert!(lan.connect(board.id(), now, b"home", b"test-password-1"));
        let until = now + 100 * MS;
        while now < until && board.ip.is_none() {
            run(&mut lan, &mut [&mut board], now, now + QUANTUM);
            now += QUANTUM;
        }
        let new_ip = board.ip.expect("a new address");
        assert_ne!(new_ip, old_ip);

        // 70 s of LAN time with the old connection still talking, in 1 ms
        // steps: past the gateway's neighbour lifetime for the old address.
        let mut chatty_closed = false;
        for step in 0..70_000u64 {
            if step % 100 == 0 && !chatty_closed {
                match chatty.write(b".") {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => chatty_closed = true,
                }
            }
            run(&mut lan, &mut [&mut board], now, now + MS);
            now += MS;
            let mut buf = [0u8; 64];
            match chatty.read(&mut buf) {
                Ok(0) => chatty_closed = true,
                Ok(_) => panic!("the board at its new address never saw this connection"),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => chatty_closed = true,
            }
        }
        let mut fresh = TcpStream::connect(at).unwrap();
        fresh.set_nonblocking(true).unwrap();
        fresh.write_all(b"again").unwrap();
        let echoed = drive_until(&mut lan, &mut board, &mut fresh, &mut now, |got| {
            got.len() >= 5
        });
        assert_eq!(
            echoed, b"again",
            "the forward reaches the board's new address"
        );
        assert!(
            chatty_closed,
            "a connection to an address the board no longer holds is closed"
        );
    }

    /// A board that drops off the network keeps its address, so nothing
    /// closes its connections but their own timeout: until then each one
    /// asks ARP for it once a second, and the gateway's one rate limit would
    /// hold every other board's forward off its ARP too.
    #[test]
    fn a_vanished_boards_connection_times_out_and_frees_another_boards_forward() {
        let mut lan = lan();
        let (mut a, mut b) = (TestBoard::new(0, "lp-aaaa"), TestBoard::new(1, "lp-bbbb"));
        a.join(&mut lan);
        b.join(&mut lan);
        let at_a = lan
            .forward(a.id(), "127.0.0.1:0".parse().unwrap(), 80)
            .unwrap();
        let at_b = lan
            .forward(b.id(), "127.0.0.1:0".parse().unwrap(), 80)
            .unwrap();
        run(&mut lan, &mut [&mut a, &mut b], 0, 20 * MS);
        let mut now = 20 * MS;

        let mut chatty = TcpStream::connect(at_a).unwrap();
        chatty.set_nonblocking(true).unwrap();
        chatty.write_all(b"hello").unwrap();
        let echoed = drive_until(&mut lan, &mut a, &mut chatty, &mut now, |got| {
            got.len() >= 5
        });
        assert_eq!(echoed, b"hello");

        // a leaves the network (still holding its lease) while its
        // connection has data waiting; 130 s of LAN time pass.
        lan.disconnect(a.id());
        let mut chatty_closed = false;
        for step in 0..130_000u64 {
            if step % 100 == 0 && !chatty_closed {
                match chatty.write(b".") {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => chatty_closed = true,
                }
            }
            run(&mut lan, &mut [&mut a, &mut b], now, now + MS);
            now += MS;
            let mut buf = [0u8; 64];
            match chatty.read(&mut buf) {
                Ok(0) => chatty_closed = true,
                Ok(_) => panic!("a is off the network"),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => chatty_closed = true,
            }
        }

        let mut fresh = TcpStream::connect(at_b).unwrap();
        fresh.set_nonblocking(true).unwrap();
        fresh.write_all(b"b too").unwrap();
        let echoed = drive_until(&mut lan, &mut b, &mut fresh, &mut now, |got| got.len() >= 5);
        assert_eq!(echoed, b"b too", "b's forward is not held off by a's");
        assert!(chatty_closed, "a's connection timed out, host side too");
    }

    /// Run quanta until what the host has read satisfies `done`, or give up
    /// after a bounded number (the host edge needs the OS to move bytes).
    fn drive_until(
        lan: &mut VirtualLan,
        board: &mut TestBoard,
        host: &mut TcpStream,
        now: &mut Cycles,
        done: impl Fn(&[u8]) -> bool,
    ) -> Vec<u8> {
        let mut got = Vec::new();
        for _ in 0..4_000 {
            run(lan, &mut [&mut *board], *now, *now + QUANTUM);
            *now += QUANTUM;
            let mut buf = [0u8; 256];
            if let Ok(n) = host.read(&mut buf) {
                got.extend_from_slice(&buf[..n]);
            }
            if done(&got) {
                break;
            }
            std::thread::yield_now();
        }
        got
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
