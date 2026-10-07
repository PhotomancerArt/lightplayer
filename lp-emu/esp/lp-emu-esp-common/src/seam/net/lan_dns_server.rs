//! The gateway's DNS answer for a LAN with an uplink (Wi-Fi relay plan P9):
//! the names the uplink carries resolve to its address, and nothing else
//! resolves at all.
//!
//! A board's resolver asks the gateway (DHCP's option 6 names it, once the
//! LAN has an uplink) one question per query; this answers an `A` question
//! for a configured name with one record, and every other question with
//! `NXDOMAIN` — there is no internet behind this router, only the hosts a
//! run named. Written from RFC 1035; the question is read with
//! [`super::lan_dns::read_name`].

use std::net::Ipv4Addr;

use super::lan_dns::{TYPE_A, read_name};

/// The DNS port.
pub const DNS_PORT: u16 = 53;

/// What an answer says a name lives for, in seconds.
const TTL_SECS: u32 = 60;
const HEADER_LEN: usize = 12;
const CLASS_IN: u16 = 1;
/// No such name.
const RCODE_NXDOMAIN: u8 = 3;
/// A query this server will not read (several questions, not a query).
const RCODE_FORMERR: u8 = 1;

/// The reply to the DNS message `query`, looking names up with `lookup`
/// (case-blind, no trailing dot). `None` for bytes that are not a query at
/// all (too short, or a reply).
pub fn answer_query(query: &[u8], lookup: impl Fn(&str) -> Option<Ipv4Addr>) -> Option<Vec<u8>> {
    if query.len() < HEADER_LEN || query[2] & 0x80 != 0 {
        return None;
    }
    let questions = u16::from_be_bytes([query[4], query[5]]);
    let mut out = query[..HEADER_LEN].to_vec();
    // QR, the opcode and RD as asked, RA; the counts filled in below.
    out[2] = 0x80 | (query[2] & 0x79);
    out[3] = 0x80;
    out[6..12].fill(0);
    if questions != 1 || query[2] & 0x78 != 0 {
        out[3] |= RCODE_FORMERR;
        out[4..6].fill(0);
        return Some(out);
    }
    let (name, at) = read_name(query, HEADER_LEN)?;
    let fixed = query.get(at..at + 4)?;
    let qtype = u16::from_be_bytes([fixed[0], fixed[1]]);
    let qclass = u16::from_be_bytes([fixed[2], fixed[3]]);
    out.extend_from_slice(&query[HEADER_LEN..at + 4]);
    match lookup(&name) {
        Some(ip) if qtype == TYPE_A && qclass == CLASS_IN => {
            out[6..8].copy_from_slice(&1u16.to_be_bytes());
            // The question's name, by pointer.
            out.extend_from_slice(&[0xc0, HEADER_LEN as u8]);
            out.extend_from_slice(&TYPE_A.to_be_bytes());
            out.extend_from_slice(&CLASS_IN.to_be_bytes());
            out.extend_from_slice(&TTL_SECS.to_be_bytes());
            out.extend_from_slice(&4u16.to_be_bytes());
            out.extend_from_slice(&ip.octets());
        }
        // The name exists, with no record of this type: no answers.
        Some(_) => {}
        None => out[3] |= RCODE_NXDOMAIN,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::net::lan_dns::{DnsData, encode_query, parse_reply};

    const UPLINK: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);

    fn lookup(name: &str) -> Option<Ipv4Addr> {
        name.eq_ignore_ascii_case("lightplayer.app")
            .then_some(UPLINK)
    }

    #[test]
    fn a_configured_name_resolves_to_the_uplink_and_others_do_not_exist() {
        let mut query = encode_query("LightPlayer.app", TYPE_A);
        query[0..2].copy_from_slice(&0x1234u16.to_be_bytes());
        query[2] = 0x01; // RD
        let reply = answer_query(&query, lookup).expect("a reply");
        assert_eq!(&reply[0..2], &[0x12, 0x34], "the query's id");
        assert_eq!(reply[2] & 0x80, 0x80, "a reply");
        assert_eq!(reply[3] & 0x0f, 0, "no error");
        let answers = parse_reply(&reply).expect("well formed");
        assert_eq!(answers.len(), 1);
        assert!(answers[0].is_named("lightplayer.app"));
        assert_eq!(answers[0].data, DnsData::A(UPLINK));

        let nx = answer_query(&encode_query("example.com", TYPE_A), lookup).unwrap();
        assert_eq!(nx[3] & 0x0f, RCODE_NXDOMAIN);
        assert!(parse_reply(&nx).unwrap().is_empty());

        let aaaa = answer_query(&encode_query("lightplayer.app", 28), lookup).unwrap();
        assert_eq!(aaaa[3] & 0x0f, 0, "the name exists");
        assert!(
            parse_reply(&aaaa).unwrap().is_empty(),
            "with no AAAA record"
        );
    }

    #[test]
    fn a_reply_or_scraps_are_not_answered() {
        assert_eq!(answer_query(&[0; 5], lookup), None);
        let mut reply = encode_query("lightplayer.app", TYPE_A);
        reply[2] = 0x80;
        assert_eq!(answer_query(&reply, lookup), None);
    }
}
