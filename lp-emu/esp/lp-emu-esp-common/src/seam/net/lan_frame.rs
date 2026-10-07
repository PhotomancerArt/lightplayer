//! Ethernet frames: the addresses the segment switches on, and a UDP
//! datagram in a frame, the one shape the gateway's DHCP server (and the
//! tests) read and write by hand rather than through a socket.

use std::net::Ipv4Addr;

use smoltcp::phy::ChecksumCapabilities;
use smoltcp::wire::{
    EthernetAddress, EthernetFrame, EthernetProtocol, EthernetRepr, IpAddress, IpProtocol,
    Ipv4Packet, Ipv4Repr, UdpPacket, UdpRepr,
};

/// The broadcast MAC.
pub const BROADCAST_MAC: [u8; 6] = [0xff; 6];

/// Destination, source and EtherType.
pub const ETHERNET_HEADER_LEN: usize = 14;

/// The largest frame the segment carries: a 1500-byte payload and its
/// header (no FCS, no VLAN tag).
pub const MAX_FRAME_LEN: usize = 1514;

const IPV4_HEADER_LEN: usize = 20;
const UDP_HEADER_LEN: usize = 8;

/// A frame's destination MAC, or `None` for a runt.
pub fn frame_dst(frame: &[u8]) -> Option<[u8; 6]> {
    frame.get(0..6).map(|b| b.try_into().expect("six bytes"))
}

/// A frame's source MAC, or `None` for a runt.
pub fn frame_src(frame: &[u8]) -> Option<[u8; 6]> {
    frame.get(6..12).map(|b| b.try_into().expect("six bytes"))
}

/// A group address (broadcast or multicast): the I/G bit, the low bit of the
/// first octet.
pub fn is_group_mac(mac: &[u8; 6]) -> bool {
    mac[0] & 1 == 1
}

/// `aa:bb:cc:dd:ee:ff`.
pub fn mac_to_string(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// A UDP datagram over IPv4 in an Ethernet frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UdpDatagram<'a> {
    pub src_mac: [u8; 6],
    pub dst_mac: [u8; 6],
    pub src_ip: Ipv4Addr,
    pub dst_ip: Ipv4Addr,
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: &'a [u8],
}

impl<'a> UdpDatagram<'a> {
    /// Read one out of a frame: `None` for anything that is not a
    /// well-formed, checksummed UDP-over-IPv4 frame.
    pub fn parse(frame: &'a [u8]) -> Option<Self> {
        let caps = ChecksumCapabilities::default();
        let eth = EthernetFrame::new_checked(frame).ok()?;
        let eth_repr = EthernetRepr::parse(&eth).ok()?;
        if eth_repr.ethertype != EthernetProtocol::Ipv4 {
            return None;
        }
        let ip_bytes = &frame[ETHERNET_HEADER_LEN..];
        let ip = Ipv4Packet::new_checked(ip_bytes).ok()?;
        let ip_repr = Ipv4Repr::parse(&ip, &caps).ok()?;
        if ip_repr.next_header != IpProtocol::Udp {
            return None;
        }
        let header_len = usize::from(ip.header_len());
        let udp_bytes = ip_bytes.get(header_len..header_len + ip_repr.payload_len)?;
        let udp = UdpPacket::new_checked(udp_bytes).ok()?;
        let udp_repr = UdpRepr::parse(
            &udp,
            &IpAddress::Ipv4(ip_repr.src_addr),
            &IpAddress::Ipv4(ip_repr.dst_addr),
            &caps,
        )
        .ok()?;
        let udp_len = usize::from(udp.len());
        Some(Self {
            src_mac: eth_repr.src_addr.0,
            dst_mac: eth_repr.dst_addr.0,
            src_ip: ip_repr.src_addr,
            dst_ip: ip_repr.dst_addr,
            src_port: udp_repr.src_port,
            dst_port: udp_repr.dst_port,
            payload: udp_bytes.get(UDP_HEADER_LEN..udp_len)?,
        })
    }

    /// The whole frame, checksums filled.
    pub fn emit(&self) -> Vec<u8> {
        let caps = ChecksumCapabilities::default();
        let udp_len = UDP_HEADER_LEN + self.payload.len();
        let mut frame = vec![0u8; ETHERNET_HEADER_LEN + IPV4_HEADER_LEN + udp_len];
        EthernetRepr {
            src_addr: EthernetAddress(self.src_mac),
            dst_addr: EthernetAddress(self.dst_mac),
            ethertype: EthernetProtocol::Ipv4,
        }
        .emit(&mut EthernetFrame::new_unchecked(&mut frame[..]));
        Ipv4Repr {
            src_addr: self.src_ip,
            dst_addr: self.dst_ip,
            next_header: IpProtocol::Udp,
            payload_len: udp_len,
            hop_limit: 64,
        }
        .emit(
            &mut Ipv4Packet::new_unchecked(&mut frame[ETHERNET_HEADER_LEN..]),
            &caps,
        );
        UdpRepr {
            src_port: self.src_port,
            dst_port: self.dst_port,
        }
        .emit(
            &mut UdpPacket::new_unchecked(&mut frame[ETHERNET_HEADER_LEN + IPV4_HEADER_LEN..]),
            &IpAddress::Ipv4(self.src_ip),
            &IpAddress::Ipv4(self.dst_ip),
            self.payload.len(),
            |p| p.copy_from_slice(self.payload),
            &caps,
        );
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datagram_survives_emit_then_parse() {
        let d = UdpDatagram {
            src_mac: [2, 0, 0, 0, 0, 1],
            dst_mac: BROADCAST_MAC,
            src_ip: Ipv4Addr::new(192, 168, 4, 1),
            dst_ip: Ipv4Addr::BROADCAST,
            src_port: 67,
            dst_port: 68,
            payload: b"hello",
        };
        let frame = d.emit();
        assert_eq!(UdpDatagram::parse(&frame), Some(d));
        assert_eq!(frame_dst(&frame), Some(BROADCAST_MAC));
        assert_eq!(frame_src(&frame), Some([2, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn a_damaged_checksum_or_a_runt_is_not_a_datagram() {
        let mut frame = UdpDatagram {
            src_mac: [2, 0, 0, 0, 0, 1],
            dst_mac: [2, 0, 0, 0, 0, 2],
            src_ip: Ipv4Addr::new(10, 0, 0, 1),
            dst_ip: Ipv4Addr::new(10, 0, 0, 2),
            src_port: 1,
            dst_port: 2,
            payload: b"x",
        }
        .emit();
        let last = frame.len() - 1;
        frame[last] ^= 0xff;
        assert_eq!(UdpDatagram::parse(&frame), None);
        assert_eq!(UdpDatagram::parse(&frame[..10]), None);
        assert_eq!(frame_dst(&frame[..3]), None);
    }

    #[test]
    fn group_addresses_are_broadcast_and_multicast() {
        assert!(is_group_mac(&BROADCAST_MAC));
        assert!(is_group_mac(&[0x01, 0x00, 0x5e, 0x00, 0x00, 0xfb]));
        assert!(!is_group_mac(&[0x02, 0, 0, 0, 0, 1]));
        assert_eq!(
            mac_to_string(&[0x02, 0, 0, 0, 0xab, 1]),
            "02:00:00:00:ab:01"
        );
    }
}
