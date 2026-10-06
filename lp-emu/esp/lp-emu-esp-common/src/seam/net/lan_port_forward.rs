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

use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};

use smoltcp::iface::SocketHandle;
use smoltcp::socket::tcp;
use smoltcp::wire::IpAddress;

use super::lan_stack::LanStack;

/// Each forwarded connection's buffer, each way, in the gateway's stack.
const SOCKET_BUFFER: usize = 16 * 1024;

/// The first of the gateway's own ports for forwarded connections.
const FIRST_LOCAL_PORT: u16 = 49_152;

/// What one forward has done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForwardCounters {
    /// Host connections accepted and forwarded.
    pub accepted: u64,
    /// Host connections closed at once: the board had no address.
    pub refused: u64,
    pub bytes_to_board: u64,
    pub bytes_to_host: u64,
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

struct ForwardConn {
    host: TcpStream,
    socket: SocketHandle,
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
    pub fn bind(host: SocketAddr, board_mac: [u8; 6], board_port: u16) -> io::Result<Self> {
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
            conn.pump(stack, &mut self.counters);
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
                let _ = host.shutdown(Shutdown::Both);
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
                let _ = host.shutdown(Shutdown::Both);
                continue;
            }
            let handle = stack.sockets.add(socket);
            self.counters.accepted += 1;
            self.conns.push(ForwardConn {
                host,
                socket: handle,
                to_board: Vec::new(),
                to_host: Vec::new(),
                host_eof: false,
                fin_sent: false,
                host_shut: false,
                dead: false,
            });
        }
    }
}

impl ForwardConn {
    fn pump(&mut self, stack: &mut LanStack, counters: &mut ForwardCounters) {
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
            let _ = self.host.shutdown(Shutdown::Write);
            self.host_shut = true;
        }
        let finished = matches!(socket.state(), tcp::State::Closed | tcp::State::TimeWait);
        if finished && self.to_host.is_empty() {
            let _ = self.host.shutdown(Shutdown::Both);
            self.dead = true;
        }
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
