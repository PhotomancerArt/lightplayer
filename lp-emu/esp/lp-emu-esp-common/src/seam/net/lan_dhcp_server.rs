//! The gateway's DHCP server: one lease per MAC, addresses handed out in a
//! fixed order, and nothing that depends on a clock.
//!
//! Addresses are deterministic: a board attached to the LAN is given its
//! lease when it is attached ([`DhcpServer::reserve`]), in attach order, so a
//! run's boards have the same addresses on every run and a host can know a
//! board's address before it boots. Anyone else on the segment (a probe, a
//! client nobody attached) gets the next free one on its first DISCOVER.
//!
//! To test "the board's address changed across a reset",
//! [`DhcpServer::renumber_next`] makes the next DISCOVER from a MAC get a
//! fresh address; a REQUEST for the old one is then refused (NAK), which is
//! what a real server does when a lease has moved.
//!
//! Leases never expire: the stated lease time is long ([`super::LanConfig`]),
//! and a renewal is answered like any REQUEST.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use smoltcp::wire::{DhcpMessageType, DhcpPacket, DhcpRepr, EthernetAddress};

use super::lan_frame::{BROADCAST_MAC, UdpDatagram};

/// The DHCP server port.
pub const DHCP_SERVER_PORT: u16 = 67;
/// The DHCP client port.
pub const DHCP_CLIENT_PORT: u16 = 68;

/// One MAC's address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease {
    pub mac: [u8; 6],
    pub ip: Ipv4Addr,
    /// The client has REQUESTed it and been ACKed.
    pub bound: bool,
}

/// The gateway's DHCP server on one /24.
#[derive(Clone, Debug)]
pub struct DhcpServer {
    server_ip: Ipv4Addr,
    network: [u8; 3],
    lease_secs: u32,
    next_host: u16,
    leases: Vec<Lease>,
    renumber: BTreeSet<[u8; 6]>,
    /// The DNS server it names (option 6): the gateway, once its LAN has an
    /// uplink; none before, so a LAN without one answers as it always has.
    dns_server: Option<Ipv4Addr>,
    offers: u64,
    acks: u64,
    naks: u64,
    exhausted: u64,
}

impl DhcpServer {
    /// A server at `server_ip` (also the router it names) on that address's
    /// /24, handing out hosts from `first_host` up.
    pub fn new(server_ip: Ipv4Addr, first_host: u8, lease_secs: u32) -> Self {
        let o = server_ip.octets();
        Self {
            server_ip,
            network: [o[0], o[1], o[2]],
            lease_secs,
            next_host: u16::from(first_host),
            leases: Vec::new(),
            renumber: BTreeSet::new(),
            dns_server: None,
            offers: 0,
            acks: 0,
            naks: 0,
            exhausted: 0,
        }
    }

    pub fn server_ip(&self) -> Ipv4Addr {
        self.server_ip
    }

    /// Name `server` as the DNS server in every offer and ack from now on.
    pub fn set_dns_server(&mut self, server: Option<Ipv4Addr>) {
        self.dns_server = server;
    }

    /// `mac`'s lease, made now if it has none. `None` when the /24 is full.
    pub fn reserve(&mut self, mac: [u8; 6]) -> Option<Ipv4Addr> {
        if let Some(lease) = self.leases.iter().find(|l| l.mac == mac) {
            return Some(lease.ip);
        }
        let ip = self.allocate()?;
        self.leases.push(Lease {
            mac,
            ip,
            bound: false,
        });
        Some(ip)
    }

    /// The next DISCOVER from `mac` gets a fresh address, and its old one is
    /// gone (a REQUEST for it is refused).
    pub fn renumber_next(&mut self, mac: [u8; 6]) {
        self.renumber.insert(mac);
    }

    pub fn lease(&self, mac: [u8; 6]) -> Option<&Lease> {
        self.leases.iter().find(|l| l.mac == mac)
    }

    /// `mac`'s address once the client holds it (ACKed).
    pub fn bound_ip(&self, mac: [u8; 6]) -> Option<Ipv4Addr> {
        self.lease(mac).filter(|l| l.bound).map(|l| l.ip)
    }

    pub fn leases(&self) -> &[Lease] {
        &self.leases
    }

    /// OFFERs, ACKs and NAKs sent, and DISCOVERs nothing was left for.
    pub fn counts(&self) -> (u64, u64, u64, u64) {
        (self.offers, self.acks, self.naks, self.exhausted)
    }

    /// Answer one client datagram (to port 67) with the reply frame, sent
    /// from `server_mac` to the broadcast address. `None`: not DHCP, not a
    /// request, or for another server.
    pub fn answer(&mut self, request: &UdpDatagram<'_>, server_mac: [u8; 6]) -> Option<Vec<u8>> {
        if request.dst_port != DHCP_SERVER_PORT {
            return None;
        }
        let packet = DhcpPacket::new_checked(request.payload).ok()?;
        let repr = DhcpRepr::parse(&packet).ok()?;
        let mac = repr.client_hardware_address.0;
        let (kind, ip) = match repr.message_type {
            DhcpMessageType::Discover => {
                if self.renumber.remove(&mac) {
                    self.leases.retain(|l| l.mac != mac);
                }
                let Some(ip) = self.reserve(mac) else {
                    self.exhausted += 1;
                    return None;
                };
                self.offers += 1;
                (DhcpMessageType::Offer, ip)
            }
            DhcpMessageType::Request => {
                if repr
                    .server_identifier
                    .is_some_and(|id| id != self.server_ip)
                {
                    return None;
                }
                let asked = repr
                    .requested_ip
                    .or_else(|| (!repr.client_ip.is_unspecified()).then_some(repr.client_ip));
                let held = self.reserve(mac);
                match (asked, held) {
                    (Some(a), Some(h)) if a == h => {
                        if let Some(lease) = self.leases.iter_mut().find(|l| l.mac == mac) {
                            lease.bound = true;
                        }
                        self.acks += 1;
                        (DhcpMessageType::Ack, h)
                    }
                    _ => {
                        self.naks += 1;
                        (DhcpMessageType::Nak, Ipv4Addr::UNSPECIFIED)
                    }
                }
            }
            DhcpMessageType::Release => {
                if let Some(lease) = self.leases.iter_mut().find(|l| l.mac == mac) {
                    lease.bound = false;
                }
                return None;
            }
            _ => return None,
        };
        let nak = kind == DhcpMessageType::Nak;
        let reply = DhcpRepr {
            message_type: kind,
            transaction_id: repr.transaction_id,
            secs: 0,
            client_hardware_address: EthernetAddress(mac),
            client_ip: Ipv4Addr::UNSPECIFIED,
            your_ip: ip,
            server_ip: Ipv4Addr::UNSPECIFIED,
            router: (!nak).then_some(self.server_ip),
            subnet_mask: (!nak).then_some(Ipv4Addr::new(255, 255, 255, 0)),
            relay_agent_ip: Ipv4Addr::UNSPECIFIED,
            broadcast: repr.broadcast,
            requested_ip: None,
            client_identifier: None,
            server_identifier: Some(self.server_ip),
            parameter_request_list: None,
            dns_servers: self
                .dns_server
                .filter(|_| !nak)
                .map(|server| [server].into_iter().collect()),
            max_size: None,
            lease_duration: (!nak).then_some(self.lease_secs),
            renew_duration: None,
            rebind_duration: None,
            additional_options: &[],
        };
        let mut payload = vec![0u8; reply.buffer_len()];
        reply
            .emit(&mut DhcpPacket::new_unchecked(&mut payload[..]))
            .ok()?;
        Some(
            UdpDatagram {
                src_mac: server_mac,
                dst_mac: BROADCAST_MAC,
                src_ip: self.server_ip,
                dst_ip: Ipv4Addr::BROADCAST,
                src_port: DHCP_SERVER_PORT,
                dst_port: DHCP_CLIENT_PORT,
                payload: &payload,
            }
            .emit(),
        )
    }

    fn allocate(&mut self) -> Option<Ipv4Addr> {
        let server_host = u16::from(self.server_ip.octets()[3]);
        while self.next_host <= 254 {
            let host = self.next_host;
            self.next_host += 1;
            let ip = Ipv4Addr::new(
                self.network[0],
                self.network[1],
                self.network[2],
                host as u8,
            );
            if host != server_host && !self.leases.iter().any(|l| l.ip == ip) {
                return Some(ip);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GW_MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];
    const A: [u8; 6] = [2, 0, 0, 0, 0, 0xa];
    const B: [u8; 6] = [2, 0, 0, 0, 0, 0xb];

    #[test]
    fn a_discover_is_offered_the_reserved_address_and_a_request_for_it_is_acked() {
        let mut s = server();
        assert_eq!(s.reserve(A), Some(ip(100)));
        assert_eq!(s.reserve(B), Some(ip(101)));
        assert_eq!(s.reserve(A), Some(ip(100)), "one lease per MAC");

        let offer = reply(&mut s, B, DhcpMessageType::Discover, None);
        assert_eq!(offer.message_type, DhcpMessageType::Offer);
        assert_eq!(offer.your_ip, ip(101));
        assert_eq!(offer.router, Some(ip(1)));
        assert_eq!(offer.subnet_mask, Some(Ipv4Addr::new(255, 255, 255, 0)));
        assert_eq!(offer.server_identifier, Some(ip(1)));
        assert_eq!(offer.lease_duration, Some(86_400));
        assert_eq!(s.bound_ip(B), None, "offered, not yet held");

        let ack = reply(&mut s, B, DhcpMessageType::Request, Some(ip(101)));
        assert_eq!(ack.message_type, DhcpMessageType::Ack);
        assert_eq!(ack.your_ip, ip(101));
        assert_eq!(s.bound_ip(B), Some(ip(101)));
        assert_eq!(s.counts(), (1, 1, 0, 0));
    }

    #[test]
    fn a_request_for_an_address_not_leased_is_refused() {
        let mut s = server();
        s.reserve(A);
        let nak = reply(&mut s, A, DhcpMessageType::Request, Some(ip(150)));
        assert_eq!(nak.message_type, DhcpMessageType::Nak);
        assert_eq!(nak.your_ip, Ipv4Addr::UNSPECIFIED);
        assert_eq!(nak.router, None);
        assert_eq!(s.bound_ip(A), None);
    }

    #[test]
    fn the_renumber_option_hands_out_a_different_address_on_the_next_lease() {
        let mut s = server();
        s.reserve(A);
        reply(&mut s, A, DhcpMessageType::Discover, None);
        reply(&mut s, A, DhcpMessageType::Request, Some(ip(100)));
        assert_eq!(s.bound_ip(A), Some(ip(100)));

        // "The board reset": without the option it gets the same address.
        let again = reply(&mut s, A, DhcpMessageType::Discover, None);
        assert_eq!(again.your_ip, ip(100));

        s.renumber_next(A);
        let moved = reply(&mut s, A, DhcpMessageType::Discover, None);
        assert_eq!(moved.your_ip, ip(101));
        let old = reply(&mut s, A, DhcpMessageType::Request, Some(ip(100)));
        assert_eq!(
            old.message_type,
            DhcpMessageType::Nak,
            "the old lease is gone"
        );
        let new = reply(&mut s, A, DhcpMessageType::Request, Some(ip(101)));
        assert_eq!(new.message_type, DhcpMessageType::Ack);
        assert_eq!(s.leases().len(), 1);
    }

    #[test]
    fn a_request_naming_another_server_is_left_alone() {
        let mut s = server();
        let frame = client_frame(A, DhcpMessageType::Request, Some(ip(100)), Some(ip(2)));
        let d = UdpDatagram::parse(&frame).unwrap();
        assert_eq!(s.answer(&d, GW_MAC), None);
    }

    #[test]
    fn the_address_space_skips_the_server_and_runs_out_honestly() {
        let mut s = DhcpServer::new(ip(253), 252, 60);
        assert_eq!(s.reserve(A), Some(ip(252)));
        assert_eq!(s.reserve(B), Some(ip(254)), "253 is the server");
        assert_eq!(s.reserve([2, 0, 0, 0, 0, 0xc]), None);
        let frame = client_frame([2, 0, 0, 0, 0, 0xd], DhcpMessageType::Discover, None, None);
        assert_eq!(s.answer(&UdpDatagram::parse(&frame).unwrap(), GW_MAC), None);
        assert_eq!(s.counts().3, 1);
    }

    fn server() -> DhcpServer {
        DhcpServer::new(ip(1), 100, 86_400)
    }

    fn ip(host: u8) -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 4, host)
    }

    /// Send one client message and read the reply back.
    fn reply(
        s: &mut DhcpServer,
        mac: [u8; 6],
        kind: DhcpMessageType,
        requested: Option<Ipv4Addr>,
    ) -> OwnedReply {
        let frame = client_frame(mac, kind, requested, None);
        let out = s
            .answer(&UdpDatagram::parse(&frame).unwrap(), GW_MAC)
            .expect("a reply");
        let d = UdpDatagram::parse(&out).expect("the reply is a datagram");
        assert_eq!(d.dst_mac, BROADCAST_MAC);
        assert_eq!((d.src_port, d.dst_port), (67, 68));
        let packet = DhcpPacket::new_checked(d.payload).unwrap();
        let r = DhcpRepr::parse(&packet).unwrap();
        assert_eq!(r.client_hardware_address.0, mac);
        assert_eq!(r.transaction_id, 0x1234);
        OwnedReply {
            message_type: r.message_type,
            your_ip: r.your_ip,
            router: r.router,
            subnet_mask: r.subnet_mask,
            server_identifier: r.server_identifier,
            lease_duration: r.lease_duration,
        }
    }

    struct OwnedReply {
        message_type: DhcpMessageType,
        your_ip: Ipv4Addr,
        router: Option<Ipv4Addr>,
        subnet_mask: Option<Ipv4Addr>,
        server_identifier: Option<Ipv4Addr>,
        lease_duration: Option<u32>,
    }

    fn client_frame(
        mac: [u8; 6],
        kind: DhcpMessageType,
        requested: Option<Ipv4Addr>,
        server: Option<Ipv4Addr>,
    ) -> Vec<u8> {
        let repr = DhcpRepr {
            message_type: kind,
            transaction_id: 0x1234,
            secs: 0,
            client_hardware_address: EthernetAddress(mac),
            client_ip: Ipv4Addr::UNSPECIFIED,
            your_ip: Ipv4Addr::UNSPECIFIED,
            server_ip: Ipv4Addr::UNSPECIFIED,
            router: None,
            subnet_mask: None,
            relay_agent_ip: Ipv4Addr::UNSPECIFIED,
            broadcast: false,
            requested_ip: requested,
            client_identifier: Some(EthernetAddress(mac)),
            server_identifier: server,
            parameter_request_list: None,
            dns_servers: None,
            max_size: None,
            lease_duration: None,
            renew_duration: None,
            rebind_duration: None,
            additional_options: &[],
        };
        let mut payload = vec![0u8; repr.buffer_len()];
        repr.emit(&mut DhcpPacket::new_unchecked(&mut payload[..]))
            .unwrap();
        UdpDatagram {
            src_mac: mac,
            dst_mac: BROADCAST_MAC,
            src_ip: Ipv4Addr::UNSPECIFIED,
            dst_ip: Ipv4Addr::BROADCAST,
            src_port: DHCP_CLIENT_PORT,
            dst_port: DHCP_SERVER_PORT,
            payload: &payload,
        }
        .emit()
    }
}
