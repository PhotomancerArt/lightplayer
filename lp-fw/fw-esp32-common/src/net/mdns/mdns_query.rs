//! Parsing an incoming mDNS packet: which of our names it asks about.
//!
//! This only reads the header and the question section — a query's answer,
//! authority and additional sections (known-answer suppression) are never
//! needed to decide what we answer, so they are not parsed at all.

/// A bounded guard against a compression-pointer loop (RFC 1035 §4.1.4):
/// no legitimate mDNS name needs anywhere near this many redirections.
const MAX_POINTER_HOPS: u8 = 16;

const DNS_HEADER_LEN: usize = 12;
const FLAG_QR: u16 = 0x8000;
const QCLASS_QU: u16 = 0x8000;

const QTYPE_A: u16 = 1;
const QTYPE_PTR: u16 = 12;
const QTYPE_TXT: u16 = 16;
const QTYPE_AAAA: u16 = 28;
const QTYPE_SRV: u16 = 33;
const QTYPE_ANY: u16 = 255;

/// Which of our own names (and record types) an incoming query asked
/// about. Built by [`parse_query`]; a packet that is not a query (the `QR`
/// bit set) or that asks about none of our names parses to `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MdnsQuery {
    /// The query's id. Zero for a normal multicast query; a legacy
    /// (unicast-source) resolver's own id otherwise, to echo back
    /// (RFC 6762 §6.7) — the caller decides "legacy" from the source port,
    /// not from this value.
    pub id: u16,
    /// A matched question asked for a unicast reply (the QU bit,
    /// RFC 6762 §5.4).
    pub unicast_response: bool,
    /// The host name (`<label>.local`) was asked for A.
    pub host_a: bool,
    /// The host name was asked for AAAA (we have none — answered with
    /// NSEC).
    pub host_aaaa: bool,
    /// `_lightplayer._tcp.local` was asked for PTR.
    pub service_ptr: bool,
    /// The instance name (`<instance>._lightplayer._tcp.local`) was asked
    /// for SRV.
    pub instance_srv: bool,
    /// The instance name was asked for TXT.
    pub instance_txt: bool,
}

impl MdnsQuery {
    /// Nothing in this query concerns us.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !(self.host_a
            || self.host_aaaa
            || self.service_ptr
            || self.instance_srv
            || self.instance_txt)
    }
}

/// Parse an mDNS query packet, keeping only what concerns this board:
/// `host_label.local` (A/AAAA/ANY), `_lightplayer._tcp.local` (PTR/ANY),
/// and `<instance_label>._lightplayer._tcp.local` (SRV/TXT/ANY).
///
/// `host_label` is the bare label (e.g. `"lp-8e30"`, no `.local`).
/// `instance_label` is the DNS-SD instance name's first label; an empty
/// instance is never matched (nothing to compare against — the caller's
/// effective instance already falls back to the host label, see
/// `mdns_answer`).
///
/// Returns `None` for a response packet (`QR` set), a malformed packet, or
/// a query that asks about none of our names.
#[must_use]
pub fn parse_query(data: &[u8], host_label: &str, instance_label: &str) -> Option<MdnsQuery> {
    if data.len() < DNS_HEADER_LEN {
        return None;
    }
    let flags = u16::from_be_bytes([data[2], data[3]]);
    if flags & FLAG_QR != 0 {
        return None; // a response, not a query
    }
    let id = u16::from_be_bytes([data[0], data[1]]);
    let qdcount = u16::from_be_bytes([data[4], data[5]]);

    let host_labels: [&[u8]; 2] = [host_label.as_bytes(), b"local"];
    let service_labels: [&[u8]; 3] = [b"_lightplayer", b"_tcp", b"local"];
    let instance_labels: [&[u8]; 4] = [
        instance_label.as_bytes(),
        b"_lightplayer",
        b"_tcp",
        b"local",
    ];

    let mut result = MdnsQuery {
        id,
        ..MdnsQuery::default()
    };
    let mut pos = DNS_HEADER_LEN;

    for _ in 0..qdcount {
        let name_start = pos;
        skip_name(data, &mut pos)?;
        if pos + 4 > data.len() {
            return None;
        }
        let qtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let qclass_raw = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        pos += 4;
        let qu = qclass_raw & QCLASS_QU != 0;

        if name_matches(data, name_start, &host_labels)? {
            match qtype {
                QTYPE_A => result.host_a = true,
                QTYPE_AAAA => result.host_aaaa = true,
                QTYPE_ANY => {
                    result.host_a = true;
                    result.host_aaaa = true;
                }
                _ => continue,
            }
            result.unicast_response |= qu;
        } else if name_matches(data, name_start, &service_labels)? {
            match qtype {
                QTYPE_PTR | QTYPE_ANY => result.service_ptr = true,
                _ => continue,
            }
            result.unicast_response |= qu;
        } else if !instance_label.is_empty() && name_matches(data, name_start, &instance_labels)? {
            match qtype {
                QTYPE_SRV => result.instance_srv = true,
                QTYPE_TXT => result.instance_txt = true,
                QTYPE_ANY => {
                    result.instance_srv = true;
                    result.instance_txt = true;
                }
                _ => continue,
            }
            result.unicast_response |= qu;
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Advance `pos` past one DNS name in `data`, without resolving
/// compression pointers (a pointer is always the last thing in a name on
/// the wire, so skipping it is just two bytes — RFC 1035 §4.1.4).
fn skip_name(data: &[u8], pos: &mut usize) -> Option<()> {
    loop {
        let len_byte = *data.get(*pos)?;
        if len_byte & 0xC0 == 0xC0 {
            if *pos + 2 > data.len() {
                return None;
            }
            *pos += 2;
            return Some(());
        } else if len_byte & 0xC0 != 0 {
            return None; // reserved label-length encoding
        } else if len_byte == 0 {
            *pos += 1;
            return Some(());
        } else {
            let next = *pos + 1 + len_byte as usize;
            if next > data.len() {
                return None;
            }
            *pos = next;
        }
    }
}

/// Whether the name starting at `start` in `data` equals `candidate`
/// (case-insensitive, ASCII only), resolving compression pointers.
/// Bounded by [`MAX_POINTER_HOPS`] and a pointer must always target an
/// earlier offset, so no cycle can loop forever. Returns `None` on a
/// malformed name.
fn name_matches(data: &[u8], start: usize, candidate: &[&[u8]]) -> Option<bool> {
    let mut pos = start;
    let mut hops = 0u8;
    let mut idx = 0usize;

    loop {
        let len_byte = *data.get(pos)?;
        if len_byte & 0xC0 == 0xC0 {
            if hops >= MAX_POINTER_HOPS {
                return None;
            }
            hops += 1;
            let lo = *data.get(pos + 1)?;
            let target = (usize::from(len_byte & 0x3F) << 8) | usize::from(lo);
            if target >= pos {
                return None; // forward or self pointer: refuse the loop risk
            }
            pos = target;
        } else if len_byte & 0xC0 != 0 {
            return None;
        } else if len_byte == 0 {
            return Some(idx == candidate.len());
        } else {
            let label_len = len_byte as usize;
            let label_start = pos + 1;
            let label_end = label_start + label_len;
            if label_end > data.len() {
                return None;
            }
            let label = &data[label_start..label_end];
            if idx >= candidate.len() || !ascii_label_eq(label, candidate[idx]) {
                return Some(false);
            }
            idx += 1;
            pos = label_end;
        }
    }
}

/// ASCII case-insensitive label comparison (RFC 1035 §3.1): non-ASCII
/// bytes, which a DNS-SD instance label may contain, compare byte for
/// byte.
fn ascii_label_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use simple_dns::{CLASS, Name, Packet, QTYPE, Question, TYPE, rdata::RData};

    const HOST: &str = "lp-8e30";
    const INSTANCE: &str = "MyLamp";

    fn question(name: &str, qtype: QTYPE, unicast: bool) -> Question<'static> {
        Question::new(
            Name::new(name).unwrap().into_owned(),
            qtype,
            CLASS::IN.into(),
            unicast,
        )
    }

    #[test]
    fn a_question_for_the_host_name_is_recognized() {
        let mut pkt = Packet::new_query(42);
        pkt.questions
            .push(question("lp-8e30.local", TYPE::A.into(), false));
        let bytes = pkt.build_bytes_vec().unwrap();

        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.host_a);
        assert!(!q.host_aaaa);
        assert!(!q.unicast_response);
    }

    #[test]
    fn aaaa_question_for_the_host_name_is_recognized() {
        let mut pkt = Packet::new_query(0);
        pkt.questions
            .push(question("lp-8e30.local", TYPE::AAAA.into(), false));
        let bytes = pkt.build_bytes_vec().unwrap();

        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.host_aaaa);
        assert!(!q.host_a);
    }

    #[test]
    fn a_and_aaaa_both_asked_are_both_recognized() {
        let mut pkt = Packet::new_query(0);
        pkt.questions
            .push(question("lp-8e30.local", TYPE::A.into(), false));
        pkt.questions
            .push(question("lp-8e30.local", TYPE::AAAA.into(), false));
        let bytes = pkt.build_bytes_vec().unwrap();

        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.host_a);
        assert!(q.host_aaaa);
    }

    #[test]
    fn ptr_question_for_the_service_name_is_recognized() {
        let mut pkt = Packet::new_query(0);
        pkt.questions
            .push(question("_lightplayer._tcp.local", TYPE::PTR.into(), false));
        let bytes = pkt.build_bytes_vec().unwrap();

        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.service_ptr);
    }

    #[test]
    fn srv_and_txt_questions_for_the_instance_are_recognized() {
        let mut pkt = Packet::new_query(0);
        pkt.questions.push(question(
            "MyLamp._lightplayer._tcp.local",
            TYPE::SRV.into(),
            false,
        ));
        pkt.questions.push(question(
            "MyLamp._lightplayer._tcp.local",
            TYPE::TXT.into(),
            false,
        ));
        let bytes = pkt.build_bytes_vec().unwrap();

        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.instance_srv);
        assert!(q.instance_txt);
    }

    #[test]
    fn a_question_for_another_name_is_silence() {
        let mut pkt = Packet::new_query(0);
        pkt.questions
            .push(question("something-else.local", TYPE::A.into(), false));
        let bytes = pkt.build_bytes_vec().unwrap();

        assert!(parse_query(&bytes, HOST, INSTANCE).is_none());
    }

    #[test]
    fn a_compressed_name_still_matches() {
        let mut pkt = Packet::new_query(0);
        pkt.questions
            .push(question("lp-8e30.local", TYPE::A.into(), false));
        pkt.questions
            .push(question("_lightplayer._tcp.local", TYPE::PTR.into(), false));
        let bytes = pkt.build_bytes_vec_compressed().unwrap();

        // The second question's "local" is a pointer into the first, so
        // this packet only parses correctly if we are willing to resolve
        // one.
        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.host_a);
        assert!(q.service_ptr);
    }

    #[test]
    fn a_response_packet_is_ignored() {
        let mut pkt = Packet::new_reply(7);
        pkt.answers.push(simple_dns::ResourceRecord::new(
            Name::new("lp-8e30.local").unwrap(),
            CLASS::IN,
            120,
            RData::A(simple_dns::rdata::A { address: 0 }),
        ));
        let bytes = pkt.build_bytes_vec().unwrap();

        assert!(parse_query(&bytes, HOST, INSTANCE).is_none());
    }

    #[test]
    fn the_qu_bit_is_detected() {
        let mut pkt = Packet::new_query(0);
        pkt.questions
            .push(question("lp-8e30.local", TYPE::A.into(), true));
        let bytes = pkt.build_bytes_vec().unwrap();

        let q = parse_query(&bytes, HOST, INSTANCE).expect("should match");
        assert!(q.unicast_response);
    }

    #[test]
    fn too_short_a_packet_is_not_a_crash() {
        assert!(parse_query(&[0u8; 4], HOST, INSTANCE).is_none());
    }
}
