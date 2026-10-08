//! A board stand-in for the LAN's tests: a smoltcp stack on a seam
//! endpoint, as the firmware's network seam adapter will be, with no
//! emulated hart in the middle.
//!
//! It takes its address over DHCP, echoes whatever a TCP peer sends to its
//! port 80, and answers an mDNS query for `<name>.local` with its address.
//! It takes one frame per take (`take_one`), as the guest's
//! `net_take_frame` does.

use std::net::Ipv4Addr;

use lp_emu_core::sched::Cycles;
use smoltcp::iface::SocketHandle;
use smoltcp::socket::{dhcpv4, tcp, udp};
use smoltcp::wire::{IpAddress, IpEndpoint};

use crate::air::ParticipantId;
use crate::seam::{EndpointEvent, EndpointId, PacerConfig, SeamEndpoint};

use super::lan_dns::{self, MDNS_GROUP, MDNS_PORT, TYPE_A};
use super::lan_frame::MAX_FRAME_LEN;
use super::lan_stack::LanStack;
use super::virtual_lan::{VirtualLan, net_pacer_config};

pub const QUANTUM: Cycles = 16_000;

/// The port the stand-in asks DNS questions from.
const DNS_CLIENT_PORT: u16 = 5300;

pub struct TestBoard {
    pub endpoint: SeamEndpoint,
    pub mac: [u8; 6],
    pub name: String,
    pub ip: Option<Ipv4Addr>,
    /// The DNS server DHCP named, if it named one.
    pub dns_server: Option<Ipv4Addr>,
    stack: LanStack,
    dhcp: SocketHandle,
    echo: SocketHandle,
    mdns: SocketHandle,
    /// A UDP socket for DNS questions (`ask_dns`), and the replies heard.
    dns: SocketHandle,
    pub dns_replies: Vec<Vec<u8>>,
    /// A TCP connection the board dialled out (`dial`).
    client: Option<SocketHandle>,
}

impl TestBoard {
    pub fn new(index: usize, name: &str) -> Self {
        Self::with_config(index, name, net_pacer_config())
    }

    pub fn with_config(index: usize, name: &str, config: PacerConfig) -> Self {
        let mac = [0x02, 0x00, 0x00, 0x00, 0xb0, index as u8];
        let id = EndpointId {
            board: ParticipantId(index),
            seam: "net",
        };
        let mut stack = LanStack::new(mac, 160, 0);
        let dhcp = stack.sockets.add(dhcpv4::Socket::new());
        let echo = stack.sockets.add(tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0; 4096]),
            tcp::SocketBuffer::new(vec![0; 4096]),
        ));
        stack
            .sockets
            .get_mut::<tcp::Socket>(echo)
            .listen(80)
            .unwrap();
        let mut mdns_socket = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; 4096]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; 4096]),
        );
        mdns_socket.bind(MDNS_PORT).unwrap();
        let mdns = stack.sockets.add(mdns_socket);
        let mut dns_socket = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0; 2048]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0; 2048]),
        );
        dns_socket.bind(DNS_CLIENT_PORT).unwrap();
        let dns = stack.sockets.add(dns_socket);
        Self {
            endpoint: SeamEndpoint::new(id, 1, config),
            mac,
            name: name.to_string(),
            ip: None,
            dns_server: None,
            stack,
            dhcp,
            echo,
            mdns,
            dns,
            dns_replies: Vec::new(),
            client: None,
        }
    }

    /// Ask the DNS server DHCP named for `name`'s A record.
    pub fn ask_dns(&mut self, name: &str) {
        let server = self.dns_server.expect("DHCP named a DNS server");
        let query = lan_dns::encode_query(name, TYPE_A);
        let to = IpEndpoint::new(IpAddress::Ipv4(server), 53);
        self.stack
            .sockets
            .get_mut::<udp::Socket>(self.dns)
            .send_slice(&query, to)
            .unwrap();
    }

    /// Open a TCP connection to `ip:port` (through the default route when
    /// it is off the /24).
    pub fn dial(&mut self, ip: Ipv4Addr, port: u16) {
        let mut socket = tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0; 4096]),
            tcp::SocketBuffer::new(vec![0; 4096]),
        );
        socket
            .connect(
                self.stack.iface.context(),
                (IpAddress::Ipv4(ip), port),
                50_000,
            )
            .unwrap();
        self.client = Some(self.stack.sockets.add(socket));
    }

    /// Send on the dialled connection what it will take now.
    pub fn client_send(&mut self, bytes: &[u8]) -> usize {
        let handle = self.client.expect("dialled");
        let socket = self.stack.sockets.get_mut::<tcp::Socket>(handle);
        if socket.can_send() {
            socket.send_slice(bytes).unwrap_or(0)
        } else {
            0
        }
    }

    /// What the dialled connection has received.
    pub fn client_recv(&mut self) -> Vec<u8> {
        let handle = self.client.expect("dialled");
        let socket = self.stack.sockets.get_mut::<tcp::Socket>(handle);
        let mut buf = [0u8; 4096];
        match socket.recv_slice(&mut buf) {
            Ok(n) => buf[..n].to_vec(),
            Err(_) => Vec::new(),
        }
    }

    pub fn id(&self) -> EndpointId {
        self.endpoint.id
    }

    /// Attach to `lan` and join its network `home` (password
    /// `test-password-1`) at cycle 0.
    pub fn join(&self, lan: &mut VirtualLan) {
        lan.attach(self.id(), self.mac);
        assert!(lan.connect(self.id(), 0, b"home", b"test-password-1"));
    }

    /// One guest step at `now`: take every frame, run the stack, answer, and
    /// give what it sent. Its stack starts (DHCP and all) once the link is
    /// up, as the firmware's does (DHCP starts on link-up).
    pub fn step(&mut self, now: Cycles, linked: bool) {
        if !linked {
            return;
        }
        while let Some(frame) = self.endpoint.take_one(MAX_FRAME_LEN) {
            self.stack.push_frame(frame);
        }
        let mut out = self.stack.poll(now);

        let configured = match self
            .stack
            .sockets
            .get_mut::<dhcpv4::Socket>(self.dhcp)
            .poll()
        {
            Some(dhcpv4::Event::Configured(c)) => {
                self.dns_server = c.dns_servers.first().copied();
                Some((c.address, c.router))
            }
            _ => None,
        };
        if let Some((address, router)) = configured {
            let ip = address.address();
            self.stack.set_address(ip, address.prefix_len(), router);
            self.stack.iface.join_multicast_group(MDNS_GROUP).unwrap();
            self.ip = Some(ip);
        }

        let echo = self.stack.sockets.get_mut::<tcp::Socket>(self.echo);
        if echo.can_recv() {
            let mut buf = [0u8; 4096];
            let n = echo.recv_slice(&mut buf).unwrap();
            echo.send_slice(&buf[..n]).unwrap();
        }
        if !echo.is_open() || echo.state() == tcp::State::CloseWait {
            echo.close();
            if echo.state() == tcp::State::Closed {
                echo.listen(80).unwrap();
            }
        }

        let own = format!("{}.local", self.name);
        let socket = self.stack.sockets.get_mut::<udp::Socket>(self.mdns);
        let mut buf = [0u8; 1500];
        while let Ok((n, _)) = socket.recv_slice(&mut buf) {
            let q = &buf[..n];
            let mut asked = Vec::new();
            lan_dns::encode_name(&mut asked, &own);
            let names_us = q.len() >= 12 + asked.len() + 4
                && q[2] & 0x80 == 0
                && q[12..12 + asked.len()].eq_ignore_ascii_case(&asked);
            if let (true, Some(ip)) = (names_us, self.ip) {
                let reply = lan_dns::encode_reply(&[(&own, TYPE_A, ip.octets().to_vec())]);
                let to = IpEndpoint::new(IpAddress::Ipv4(MDNS_GROUP), MDNS_PORT);
                socket.send_slice(&reply, to).unwrap();
            }
        }

        let socket = self.stack.sockets.get_mut::<udp::Socket>(self.dns);
        while let Ok((n, _)) = socket.recv_slice(&mut buf) {
            self.dns_replies.push(buf[..n].to_vec());
        }

        out.extend(self.stack.poll(now));
        for bytes in out {
            self.endpoint
                .push_outbound(EndpointEvent { at: now, bytes });
        }
    }
}

/// Step every board and deliver, one quantum at a time, from `from` up to
/// (not including) `to`.
pub fn run(lan: &mut VirtualLan, boards: &mut [&mut TestBoard], from: Cycles, to: Cycles) {
    use crate::seam::SeamMedium;
    let mut now = from;
    while now < to {
        for b in boards.iter_mut() {
            let linked = lan.link_up(b.id());
            b.step(now, linked);
        }
        let mut eps: Vec<&mut SeamEndpoint> = boards.iter_mut().map(|b| &mut b.endpoint).collect();
        lan.deliver(now, &mut eps);
        now += QUANTUM;
    }
}
