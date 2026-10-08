//! Just enough DNS for a probe to ask a LAN by name: one question out, every
//! record of a reply back.
//!
//! A probe asks over multicast DNS (`224.0.0.251:5353`) and reads whatever
//! the boards answer: A records for `<name>.local`, the PTR, SRV and TXT of
//! a DNS-SD service. It reads names with compression and ignores the
//! cache-flush bit mDNS sets on the class. It writes only queries (and, for
//! the tests' board stand-in, a reply). Written from RFC 1035 and RFC 6762;
//! nothing here is the firmware's codec, which this crate cannot see.

use std::net::Ipv4Addr;

/// The record types a probe asks for and reads.
pub const TYPE_A: u16 = 1;
pub const TYPE_PTR: u16 = 12;
pub const TYPE_TXT: u16 = 16;
pub const TYPE_AAAA: u16 = 28;
pub const TYPE_SRV: u16 = 33;
pub const TYPE_NSEC: u16 = 47;
pub const TYPE_ANY: u16 = 255;

/// The mDNS group and port.
pub const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_PORT: u16 = 5353;

const CLASS_IN: u16 = 1;
const HEADER_LEN: usize = 12;
/// The most compression pointers one name may follow.
const MAX_POINTERS: usize = 32;

/// One record from a reply (answers, authority and additional alike).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsAnswer {
    /// The owner name, dotted, without the trailing dot.
    pub name: String,
    pub rtype: u16,
    pub ttl: u32,
    pub data: DnsData,
}

/// A record's data, read for the types a probe cares about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DnsData {
    A(Ipv4Addr),
    Ptr(String),
    Srv {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
    Txt(Vec<Vec<u8>>),
    /// Anything else, raw.
    Other(Vec<u8>),
}

impl DnsAnswer {
    /// Its name is `name`, ignoring case (DNS names are case-blind).
    pub fn is_named(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name.trim_end_matches('.'))
    }
}

/// A query for `name` of type `rtype`, ID 0 (mDNS), one question.
pub fn encode_query(name: &str, rtype: u16) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_LEN];
    out[4..6].copy_from_slice(&1u16.to_be_bytes());
    encode_name(&mut out, name);
    out.extend_from_slice(&rtype.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out
}

/// Every record of a reply, or `None` when `bytes` is not a reply or is cut
/// short.
pub fn parse_reply(bytes: &[u8]) -> Option<Vec<DnsAnswer>> {
    if bytes.len() < HEADER_LEN || bytes[2] & 0x80 == 0 {
        return None;
    }
    let count = |at: usize| usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]));
    let (questions, records) = (count(4), count(6) + count(8) + count(10));
    let mut at = HEADER_LEN;
    for _ in 0..questions {
        let (_, next) = read_name(bytes, at)?;
        at = next.checked_add(4)?;
    }
    let mut answers = Vec::with_capacity(records);
    for _ in 0..records {
        let (name, next) = read_name(bytes, at)?;
        let fixed = bytes.get(next..next + 10)?;
        let rtype = u16::from_be_bytes([fixed[0], fixed[1]]);
        let ttl = u32::from_be_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]);
        let len = usize::from(u16::from_be_bytes([fixed[8], fixed[9]]));
        let start = next + 10;
        let rdata = bytes.get(start..start + len)?;
        let data = match rtype {
            TYPE_A if len == 4 => DnsData::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])),
            TYPE_PTR => DnsData::Ptr(read_name(bytes, start)?.0),
            TYPE_SRV if len >= 7 => DnsData::Srv {
                priority: u16::from_be_bytes([rdata[0], rdata[1]]),
                weight: u16::from_be_bytes([rdata[2], rdata[3]]),
                port: u16::from_be_bytes([rdata[4], rdata[5]]),
                target: read_name(bytes, start + 6)?.0,
            },
            TYPE_TXT => DnsData::Txt(read_strings(rdata)?),
            _ => DnsData::Other(rdata.to_vec()),
        };
        answers.push(DnsAnswer {
            name,
            rtype,
            ttl,
            data,
        });
        at = start + len;
    }
    Some(answers)
}

/// A reply carrying `records` as answers: `(name, type, rdata)`, with
/// rdata names written uncompressed. Only the tests' board stand-in answers.
#[cfg(test)]
pub fn encode_reply(records: &[(&str, u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_LEN];
    out[2] = 0x84; // QR, AA
    out[6..8].copy_from_slice(&(records.len() as u16).to_be_bytes());
    for (name, rtype, rdata) in records {
        encode_name(&mut out, name);
        out.extend_from_slice(&rtype.to_be_bytes());
        out.extend_from_slice(&(0x8000 | CLASS_IN).to_be_bytes()); // cache flush
        out.extend_from_slice(&120u32.to_be_bytes());
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
    }
    out
}

/// `name` as labels, uncompressed.
pub fn encode_name(out: &mut Vec<u8>, name: &str) {
    for label in name
        .trim_end_matches('.')
        .split('.')
        .filter(|l| !l.is_empty())
    {
        let label = &label.as_bytes()[..label.len().min(63)];
        out.push(label.len() as u8);
        out.extend_from_slice(label);
    }
    out.push(0);
}

/// The name at `at`, and where the bytes after it (in place) start.
fn read_name(bytes: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    let mut pointers = 0;
    loop {
        let len = *bytes.get(at)?;
        match len & 0xc0 {
            0x00 if len == 0 => {
                return Some((labels.join("."), end.unwrap_or(at + 1)));
            }
            0x00 => {
                let label = bytes.get(at + 1..at + 1 + usize::from(len))?;
                labels.push(String::from_utf8_lossy(label).into_owned());
                at += 1 + usize::from(len);
            }
            0xc0 => {
                pointers += 1;
                if pointers > MAX_POINTERS {
                    return None;
                }
                let low = *bytes.get(at + 1)?;
                end.get_or_insert(at + 2);
                at = usize::from(u16::from_be_bytes([len & 0x3f, low]));
            }
            _ => return None,
        }
    }
}

fn read_strings(mut rdata: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    while let Some((&len, rest)) = rdata.split_first() {
        let s = rest.get(..usize::from(len))?;
        out.push(s.to_vec());
        rdata = &rest[usize::from(len)..];
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_one_question_with_id_zero() {
        let q = encode_query("lp-a1b2.local", TYPE_A);
        assert_eq!(&q[..6], &[0, 0, 0, 0, 0, 1]);
        assert_eq!(&q[12..], b"\x07lp-a1b2\x05local\x00\x00\x01\x00\x01");
        assert_eq!(parse_reply(&q), None, "a query is not a reply");
    }

    #[test]
    fn a_reply_reads_back_with_the_cache_flush_bit_and_every_record_type() {
        let mut srv = vec![0, 0, 0, 0, 0, 80];
        encode_name(&mut srv, "lp-a1b2.local");
        let mut ptr = Vec::new();
        encode_name(&mut ptr, "lp-a1b2._lightplayer._tcp.local");
        let reply = encode_reply(&[
            ("lp-a1b2.local", TYPE_A, vec![192, 168, 4, 100]),
            ("_lightplayer._tcp.local", TYPE_PTR, ptr),
            ("lp-a1b2._lightplayer._tcp.local", TYPE_SRV, srv),
            (
                "lp-a1b2._lightplayer._tcp.local",
                TYPE_TXT,
                b"\x07proto=9\x03mac".to_vec(),
            ),
        ]);
        let answers = parse_reply(&reply).unwrap();
        assert_eq!(answers.len(), 4);
        assert!(answers[0].is_named("LP-A1B2.local."));
        assert_eq!(answers[0].data, DnsData::A(Ipv4Addr::new(192, 168, 4, 100)));
        assert_eq!(answers[0].ttl, 120);
        assert_eq!(
            answers[1].data,
            DnsData::Ptr("lp-a1b2._lightplayer._tcp.local".into())
        );
        assert_eq!(
            answers[2].data,
            DnsData::Srv {
                priority: 0,
                weight: 0,
                port: 80,
                target: "lp-a1b2.local".into()
            }
        );
        assert_eq!(
            answers[3].data,
            DnsData::Txt(vec![b"proto=9".to_vec(), b"mac".to_vec()])
        );
    }

    #[test]
    fn compressed_names_are_followed_and_a_pointer_loop_is_refused() {
        // A reply with two A records: the first spells "a.local" out at
        // offset 12, the second names it by a pointer to 12.
        let mut r = vec![0, 0, 0x84, 0, 0, 0, 0, 2, 0, 0, 0, 0];
        r.extend_from_slice(b"\x01a\x05local\x00");
        r.extend_from_slice(&[0, 1, 0, 1, 0, 0, 0, 9, 0, 4, 10, 0, 0, 1]);
        r.extend_from_slice(&[0xc0, 12]);
        r.extend_from_slice(&[0, 1, 0, 1, 0, 0, 0, 9, 0, 4, 10, 0, 0, 2]);
        let answers = parse_reply(&r).unwrap();
        assert_eq!(answers[0].name, "a.local");
        assert_eq!(answers[1].name, "a.local");
        assert_eq!(answers[1].data, DnsData::A(Ipv4Addr::new(10, 0, 0, 2)));

        let mut looped = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        looped.extend_from_slice(&[0xc0, 12]);
        looped.extend_from_slice(&[0, 1, 0, 1, 0, 0, 0, 9, 0, 4, 10, 0, 0, 1]);
        assert_eq!(parse_reply(&looped), None);
    }
}
