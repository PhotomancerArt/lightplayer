//! The router on the segment: it answers ARP for its address, hands out
//! addresses over DHCP, and carries the host's port forwards to the boards.
//!
//! DHCP is answered by hand ([`super::lan_dhcp_server`]), straight from the
//! frame, because a client with no address yet speaks from `0.0.0.0` to the
//! broadcast address and a socket is the wrong shape for that. Everything
//! else addressed to the gateway goes into its smoltcp stack, which answers
//! ARP for the gateway's address (and asks the boards theirs) and runs the
//! TCP connections each [`LanPortForward`] opens.
//!
//! **An uplink, only where a run names one** (Wi-Fi relay plan P9,
//! [`LanUplink`]): the gateway then answers DNS for the named hosts with its
//! uplink address, names itself the DNS server over DHCP, and carries each
//! connection a board opens there to the host address the run gave. Nothing
//! else routes beyond the /24: no other name resolves, no other address
//! answers.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use lp_emu_core::sched::Cycles;

use super::lan_dhcp_server::{DHCP_SERVER_PORT, DhcpServer};
use super::lan_dns_server::{DNS_PORT, answer_query};
use super::lan_frame::UdpDatagram;
use super::lan_port_forward::LanPortForward;
use super::lan_stack::LanStack;
use super::lan_uplink::{LanUplink, UPLINK_IP};

/// The router.
pub struct LanGateway {
    pub mac: [u8; 6],
    pub ip: Ipv4Addr,
    pub dhcp: DhcpServer,
    stack: LanStack,
    forwards: Vec<LanPortForward>,
    uplinks: Vec<LanUplink>,
}

impl LanGateway {
    pub fn new(
        mac: [u8; 6],
        ip: Ipv4Addr,
        first_lease_host: u8,
        lease_secs: u32,
        cycles_per_us: u64,
    ) -> Self {
        let mut stack = LanStack::new(mac, cycles_per_us, 0);
        stack.set_address(ip, 24, None);
        Self {
            mac,
            ip,
            dhcp: DhcpServer::new(ip, first_lease_host, lease_secs),
            stack,
            forwards: Vec::new(),
            uplinks: Vec::new(),
        }
    }

    /// Carry `name` beyond the LAN: a board that resolves it gets the
    /// uplink address, and its connections to `<that>:port` reach `to` on
    /// the host. Returns the uplink address. Several names share it; each
    /// needs its own port.
    pub fn uplink(&mut self, name: &str, port: u16, to: SocketAddr) -> io::Result<Ipv4Addr> {
        if self.uplinks.iter().any(|u| u.port == port) {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("the LAN's uplink already carries port {port}"),
            ));
        }
        if self.uplinks.is_empty() {
            self.stack
                .add_address(UPLINK_IP, 32)
                .map_err(|()| io::Error::other("the gateway's stack holds no more addresses"))?;
            self.dhcp.set_dns_server(Some(self.ip));
        }
        self.uplinks.push(LanUplink::new(name, port, to));
        Ok(UPLINK_IP)
    }

    pub fn uplinks(&self) -> &[LanUplink] {
        &self.uplinks
    }

    /// Forward `host` (port 0: the OS picks) to `board_port` on the board
    /// with `board_mac`. Returns where a host connects.
    pub fn forward(
        &mut self,
        host: SocketAddr,
        board_mac: [u8; 6],
        board_port: u16,
    ) -> io::Result<SocketAddr> {
        let f = LanPortForward::bind(host, board_mac, board_port)?;
        let at = f.local_addr();
        self.forwards.push(f);
        Ok(at)
    }

    pub fn forwards(&self) -> &[LanPortForward] {
        &self.forwards
    }

    /// One frame the segment delivered here, at its arrival cycle: what the
    /// gateway sends in answer.
    pub(super) fn receive(&mut self, at: Cycles, frame: Vec<u8>) -> Vec<Vec<u8>> {
        if let Some(d) = UdpDatagram::parse(&frame) {
            if d.dst_port == DHCP_SERVER_PORT {
                return self.dhcp.answer(&d, self.mac).into_iter().collect();
            }
            if d.dst_port == DNS_PORT && d.dst_ip == self.ip && !self.uplinks.is_empty() {
                return self.answer_dns(&d).into_iter().collect();
            }
        }
        self.stack.push_frame(frame);
        self.stack.poll(at)
    }

    /// Run at `now`: the host edge of every forward and uplink, then the
    /// stack's timers.
    pub(super) fn run(&mut self, now: Cycles) -> Vec<Vec<u8>> {
        self.pump_host_edges();
        let mut out = self.stack.poll(now);
        // What the stack just took from the boards goes to the host now,
        // not at the next boundary.
        self.pump_host_edges();
        out.extend(self.stack.poll(now));
        out
    }

    fn pump_host_edges(&mut self) {
        for f in &mut self.forwards {
            let ip = self.dhcp.bound_ip(f.board_mac);
            f.pump(&mut self.stack, ip);
        }
        let dhcp = &self.dhcp;
        let bound = |ip: Ipv4Addr| dhcp.leases().iter().any(|l| l.bound && l.ip == ip);
        for u in &mut self.uplinks {
            u.pump(&mut self.stack, &bound);
        }
    }

    /// A board's DNS question, answered from the uplinks' names.
    fn answer_dns(&self, query: &UdpDatagram<'_>) -> Option<Vec<u8>> {
        let reply = answer_query(query.payload, |name| {
            self.uplinks
                .iter()
                .any(|u| u.is_named(name))
                .then_some(UPLINK_IP)
        })?;
        Some(
            UdpDatagram {
                src_mac: self.mac,
                dst_mac: query.src_mac,
                src_ip: self.ip,
                dst_ip: query.src_ip,
                src_port: DNS_PORT,
                dst_port: query.src_port,
                payload: &reply,
            }
            .emit(),
        )
    }

    pub(super) fn poll_at(&mut self, now: Cycles) -> Option<Cycles> {
        self.stack.poll_at(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smoltcp::wire::{
        ArpOperation, ArpPacket, ArpRepr, EthernetAddress, EthernetFrame, EthernetProtocol,
        EthernetRepr,
    };

    use super::super::lan_frame::BROADCAST_MAC;

    const GW_MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];
    const BOARD: [u8; 6] = [2, 0, 0, 0, 0, 0x20];

    #[test]
    fn the_gateway_answers_arp_for_its_address_and_not_for_others() {
        let mut gw = gateway();
        let out = gw.receive(1_600, arp_request(Ipv4Addr::new(192, 168, 4, 1)));
        assert_eq!(out.len(), 1, "one reply");
        let eth = EthernetFrame::new_checked(&out[0][..]).unwrap();
        assert_eq!(eth.dst_addr().0, BOARD);
        assert_eq!(eth.ethertype(), EthernetProtocol::Arp);
        let arp = ArpRepr::parse(&ArpPacket::new_checked(eth.payload()).unwrap()).unwrap();
        match arp {
            ArpRepr::EthernetIpv4 {
                operation,
                source_hardware_addr,
                source_protocol_addr,
                ..
            } => {
                assert_eq!(operation, ArpOperation::Reply);
                assert_eq!(source_hardware_addr.0, GW_MAC);
                assert_eq!(source_protocol_addr, Ipv4Addr::new(192, 168, 4, 1));
            }
            _ => panic!("an Ethernet/IPv4 reply"),
        }
        assert!(
            gw.receive(3_200, arp_request(Ipv4Addr::new(192, 168, 4, 77)))
                .is_empty(),
            "not ours: no answer"
        );
    }

    #[test]
    fn a_forward_binds_a_host_port_and_refuses_a_board_with_no_address() {
        let mut gw = gateway();
        let at = gw
            .forward("127.0.0.1:0".parse().unwrap(), BOARD, 80)
            .unwrap();
        assert_ne!(at.port(), 0);
        let host = std::net::TcpStream::connect(at).unwrap();
        // The OS completes the host connection; the next run sees it.
        for i in 0..1_000 {
            gw.run(i * 1_600);
            if gw.forwards()[0].counters().refused == 1 {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(gw.forwards()[0].counters().refused, 1);
        drop(host);
    }

    fn gateway() -> LanGateway {
        LanGateway::new(GW_MAC, Ipv4Addr::new(192, 168, 4, 1), 100, 86_400, 160)
    }

    fn arp_request(target: Ipv4Addr) -> Vec<u8> {
        let arp = ArpRepr::EthernetIpv4 {
            operation: ArpOperation::Request,
            source_hardware_addr: EthernetAddress(BOARD),
            source_protocol_addr: Ipv4Addr::new(192, 168, 4, 100),
            target_hardware_addr: EthernetAddress([0; 6]),
            target_protocol_addr: target,
        };
        let mut frame = vec![0u8; 14 + arp.buffer_len()];
        EthernetRepr {
            src_addr: EthernetAddress(BOARD),
            dst_addr: EthernetAddress(BROADCAST_MAC),
            ethertype: EthernetProtocol::Arp,
        }
        .emit(&mut EthernetFrame::new_unchecked(&mut frame[..]));
        arp.emit(&mut ArpPacket::new_unchecked(&mut frame[14..]));
        frame
    }
}
