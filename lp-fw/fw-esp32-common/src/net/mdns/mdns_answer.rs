//! Building the mDNS / DNS-SD answer: A, NSEC, PTR, SRV and TXT records for
//! this board, written straight into a caller-provided buffer.
//!
//! Compression is deliberately not implemented: every name is written out
//! in full. Our answers are a handful of records, each a few dozen bytes —
//! well short of a single Ethernet frame — so correctness-first and a
//! small, easy-to-read writer win over shaving the last bytes off the
//! wire.

use alloc::string::String;
use core::fmt::Write as _;

use super::mdns_query::MdnsQuery;

const FLAGS_AUTHORITATIVE_RESPONSE: u16 = 0x8400;
const CLASS_IN: u16 = 1;
const CACHE_FLUSH: u16 = 0x8000;

const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_AAAA: u16 = 28;
const TYPE_SRV: u16 = 33;
const TYPE_NSEC: u16 = 47;
const TYPE_ANY: u16 = 255;

const TTL_HOST: u32 = 120;
const TTL_SERVICE: u32 = 4500;

const SERVICE_LABELS: [&[u8]; 3] = [b"_lightplayer", b"_tcp", b"local"];

/// The longest a DNS label (and so a DNS-SD instance name's own label) may
/// be on the wire.
const MAX_LABEL_LEN: usize = 63;

/// What this board advertises itself as on the LAN — enough to answer
/// every record this module builds.
#[derive(Debug, Clone)]
pub struct MdnsIdentity {
    /// The host label, e.g. `"lp-8e30"` (see
    /// [`super::mdns_name::mdns_label`]). Never more than 63 bytes.
    pub label: String,
    /// The DNS-SD instance name — normally the board's own name as Studio
    /// shows it. [`effective_instance`] truncates it to 63 bytes on a
    /// UTF-8 boundary and falls back to [`Self::label`] if that leaves it
    /// empty.
    pub instance: String,
    /// The base MAC, for the TXT record's `mac=` string.
    pub mac: [u8; 6],
    /// The wire protocol version (`lpc_wire::WIRE_PROTO_VERSION`), taken
    /// as a plain number so this codec does not depend on `lpc-wire`.
    pub proto: u32,
    /// The link's TCP port (80).
    pub port: u16,
    /// The station's IPv4 address, as four octets.
    pub ipv4: [u8; 4],
}

/// Build a normal (or legacy-unicast) mDNS answer for `query` into `out`.
/// Returns the number of bytes written, or `None` if it would not fit —
/// in which case the content of `out` is left unspecified and must not be
/// sent.
///
/// `legacy_unicast_id` is `Some(query.id)` when the query arrived from a
/// unicast source port (RFC 6762 §6.7): the id is echoed and the matching
/// question is repeated in the question section. `None` means a normal
/// multicast response: id 0, no question section.
#[must_use]
pub fn build_answer(
    out: &mut [u8],
    identity: &MdnsIdentity,
    query: &MdnsQuery,
    legacy_unicast_id: Option<u16>,
) -> Option<usize> {
    build(out, identity, query, legacy_unicast_id, false)
}

/// Build a goodbye packet: every record this board advertises, with
/// TTL 0 (RFC 6762 §10.1), for a clean departure from the network. Always
/// a normal multicast packet (id 0, no question section).
#[must_use]
pub fn build_goodbye(out: &mut [u8], identity: &MdnsIdentity) -> Option<usize> {
    let everything = MdnsQuery {
        host_a: true,
        host_aaaa: true,
        service_ptr: true,
        instance_srv: true,
        instance_txt: true,
        ..MdnsQuery::default()
    };
    build(out, identity, &everything, None, true)
}

/// The instance name, truncated to 63 bytes on a UTF-8 character
/// boundary; an empty instance (before or after truncation) falls back to
/// the host label, which is always short enough on its own.
#[must_use]
pub fn effective_instance<'a>(identity: &'a MdnsIdentity) -> &'a str {
    let truncated = truncate_utf8(&identity.instance, MAX_LABEL_LEN);
    if truncated.is_empty() {
        &identity.label
    } else {
        truncated
    }
}

fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn build(
    out: &mut [u8],
    identity: &MdnsIdentity,
    query: &MdnsQuery,
    legacy_unicast_id: Option<u16>,
    goodbye: bool,
) -> Option<usize> {
    let host_ttl = if goodbye { 0 } else { TTL_HOST };
    let service_ttl = if goodbye { 0 } else { TTL_SERVICE };

    let want_a_answer = query.host_a;
    let want_nsec_answer = query.host_aaaa;
    let want_ptr_answer = query.service_ptr;
    let want_srv_answer = query.instance_srv;
    let want_txt_answer = query.instance_txt;

    // RFC 6763 §12's recommended additionals, but never duplicating a
    // record that is already going into the answer section.
    let want_srv_additional = want_ptr_answer && !want_srv_answer;
    let want_txt_additional = want_ptr_answer && !want_txt_answer;
    let want_a_additional =
        (want_ptr_answer || want_srv_answer || want_txt_answer) && !want_a_answer;

    let host_labels: [&[u8]; 2] = [identity.label.as_bytes(), b"local"];
    let instance = effective_instance(identity);
    let instance_labels: [&[u8]; 4] = [instance.as_bytes(), b"_lightplayer", b"_tcp", b"local"];

    let mut w = Writer::new(out);
    w.put_u16(legacy_unicast_id.unwrap_or(0))?;
    w.put_u16(FLAGS_AUTHORITATIVE_RESPONSE)?;
    let qdcount_pos = w.reserve_u16()?;
    let ancount_pos = w.reserve_u16()?;
    w.put_u16(0)?; // nscount: always 0, nothing of ours is delegated
    let arcount_pos = w.reserve_u16()?;

    let mut qdcount: u16 = 0;
    if legacy_unicast_id.is_some()
        && write_echoed_question(&mut w, query, &host_labels, &instance_labels)?
    {
        qdcount = 1;
    }

    let mut ancount: u16 = 0;
    if want_a_answer {
        write_a(&mut w, &host_labels, identity, host_ttl)?;
        ancount += 1;
    }
    if want_nsec_answer {
        write_host_nsec(&mut w, &host_labels, host_ttl)?;
        ancount += 1;
    }
    if want_ptr_answer {
        write_ptr(&mut w, &instance_labels, service_ttl)?;
        ancount += 1;
    }
    if want_srv_answer {
        write_srv(
            &mut w,
            &instance_labels,
            &host_labels,
            identity,
            service_ttl,
        )?;
        ancount += 1;
    }
    if want_txt_answer {
        write_txt(&mut w, &instance_labels, identity, service_ttl)?;
        ancount += 1;
    }

    let mut arcount: u16 = 0;
    if want_srv_additional {
        write_srv(
            &mut w,
            &instance_labels,
            &host_labels,
            identity,
            service_ttl,
        )?;
        arcount += 1;
    }
    if want_txt_additional {
        write_txt(&mut w, &instance_labels, identity, service_ttl)?;
        arcount += 1;
    }
    if want_a_additional {
        write_a(&mut w, &host_labels, identity, host_ttl)?;
        arcount += 1;
    }

    w.set_u16_at(qdcount_pos, qdcount);
    w.set_u16_at(ancount_pos, ancount);
    w.set_u16_at(arcount_pos, arcount);

    Some(w.len())
}

/// For a legacy-unicast reply, repeat the one question this answer is for
/// (RFC 6762 §6.7). `query`'s flags map back onto exactly one of our three
/// names, since that is all a single question could have asked for.
/// Returns whether a question was written (`false` for an empty `query`).
fn write_echoed_question(
    w: &mut Writer<'_>,
    query: &MdnsQuery,
    host_labels: &[&[u8]; 2],
    instance_labels: &[&[u8]; 4],
) -> Option<bool> {
    if query.host_a || query.host_aaaa {
        let qtype = match (query.host_a, query.host_aaaa) {
            (true, true) => TYPE_ANY,
            (false, true) => TYPE_AAAA,
            _ => TYPE_A,
        };
        w.put_name(host_labels)?;
        w.put_u16(qtype)?;
        w.put_u16(CLASS_IN)?;
    } else if query.service_ptr {
        w.put_name(&SERVICE_LABELS)?;
        w.put_u16(TYPE_PTR)?;
        w.put_u16(CLASS_IN)?;
    } else if query.instance_srv || query.instance_txt {
        let qtype = match (query.instance_srv, query.instance_txt) {
            (true, true) => TYPE_ANY,
            (false, true) => TYPE_TXT,
            _ => TYPE_SRV,
        };
        w.put_name(instance_labels)?;
        w.put_u16(qtype)?;
        w.put_u16(CLASS_IN)?;
    } else {
        return Some(false);
    }
    Some(true)
}

fn write_a(
    w: &mut Writer<'_>,
    host_labels: &[&[u8]; 2],
    identity: &MdnsIdentity,
    ttl: u32,
) -> Option<()> {
    let rdlen_pos = w.put_rr_header(host_labels, TYPE_A, true, ttl)?;
    w.put_bytes(&identity.ipv4)?;
    w.finish_rdata(rdlen_pos);
    Some(())
}

/// A negative response to AAAA: an NSEC naming ourselves as the next
/// record, with only the A bit set in window 0 — RFC 6762 §6.1.
fn write_host_nsec(w: &mut Writer<'_>, host_labels: &[&[u8]; 2], ttl: u32) -> Option<()> {
    let rdlen_pos = w.put_rr_header(host_labels, TYPE_NSEC, true, ttl)?;
    w.put_name(host_labels)?; // next name: ourselves
    w.put_u8(0)?; // window block 0
    w.put_u8(1)?; // bitmap length
    w.put_u8(0x40)?; // bit 1 (TYPE A) set, counting from the high bit
    w.finish_rdata(rdlen_pos);
    Some(())
}

fn write_ptr(w: &mut Writer<'_>, instance_labels: &[&[u8]; 4], ttl: u32) -> Option<()> {
    // PTR is a shared record (several boards may answer it), so it never
    // carries the cache-flush bit.
    let rdlen_pos = w.put_rr_header(&SERVICE_LABELS, TYPE_PTR, false, ttl)?;
    w.put_name(instance_labels)?;
    w.finish_rdata(rdlen_pos);
    Some(())
}

fn write_srv(
    w: &mut Writer<'_>,
    instance_labels: &[&[u8]; 4],
    host_labels: &[&[u8]; 2],
    identity: &MdnsIdentity,
    ttl: u32,
) -> Option<()> {
    let rdlen_pos = w.put_rr_header(instance_labels, TYPE_SRV, true, ttl)?;
    w.put_u16(0)?; // priority
    w.put_u16(0)?; // weight
    w.put_u16(identity.port)?;
    w.put_name(host_labels)?; // target
    w.finish_rdata(rdlen_pos);
    Some(())
}

fn write_txt(
    w: &mut Writer<'_>,
    instance_labels: &[&[u8]; 4],
    identity: &MdnsIdentity,
    ttl: u32,
) -> Option<()> {
    let rdlen_pos = w.put_rr_header(instance_labels, TYPE_TXT, true, ttl)?;

    let mut mac = String::with_capacity(16);
    let [a, b, c, d, e, f] = identity.mac;
    let _ = write!(mac, "mac={a:02x}{b:02x}{c:02x}{d:02x}{e:02x}{f:02x}");
    w.put_character_string(mac.as_bytes())?;

    let mut proto = String::with_capacity(16);
    let _ = write!(proto, "proto={}", identity.proto);
    w.put_character_string(proto.as_bytes())?;

    w.put_character_string(b"path=/link")?;

    w.finish_rdata(rdlen_pos);
    Some(())
}

/// A small cursor over the caller's output buffer. Every `put_*` fails
/// (returns `None`) rather than panicking when `out` is too small, so a
/// too-small buffer surfaces as this module's documented `None`.
struct Writer<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }

    fn len(&self) -> usize {
        self.pos
    }

    fn put_u8(&mut self, v: u8) -> Option<()> {
        let slot = self.out.get_mut(self.pos)?;
        *slot = v;
        self.pos += 1;
        Some(())
    }

    fn put_u16(&mut self, v: u16) -> Option<()> {
        for b in v.to_be_bytes() {
            self.put_u8(b)?;
        }
        Some(())
    }

    fn put_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        let end = self.pos.checked_add(bytes.len())?;
        self.out.get_mut(self.pos..end)?.copy_from_slice(bytes);
        self.pos = end;
        Some(())
    }

    /// Reserve two bytes (a placeholder, patched later with
    /// [`Self::set_u16_at`]) and return their offset.
    fn reserve_u16(&mut self) -> Option<usize> {
        let pos = self.pos;
        self.put_u16(0)?;
        Some(pos)
    }

    fn set_u16_at(&mut self, offset: usize, v: u16) {
        let [hi, lo] = v.to_be_bytes();
        self.out[offset] = hi;
        self.out[offset + 1] = lo;
    }

    fn put_label(&mut self, label: &[u8]) -> Option<()> {
        if label.len() > MAX_LABEL_LEN {
            return None;
        }
        self.put_u8(label.len() as u8)?;
        self.put_bytes(label)
    }

    fn put_name(&mut self, labels: &[&[u8]]) -> Option<()> {
        for label in labels {
            self.put_label(label)?;
        }
        self.put_u8(0) // the root label
    }

    /// A TXT record's one entry: a length byte then up to 255 bytes.
    fn put_character_string(&mut self, s: &[u8]) -> Option<()> {
        if s.len() > 255 {
            return None;
        }
        self.put_u8(s.len() as u8)?;
        self.put_bytes(s)
    }

    /// NAME, TYPE, CLASS (with the cache-flush bit if `cache_flush`), TTL,
    /// and a zero RDLENGTH placeholder. Returns that placeholder's offset
    /// for [`Self::finish_rdata`].
    fn put_rr_header(
        &mut self,
        labels: &[&[u8]],
        type_code: u16,
        cache_flush: bool,
        ttl: u32,
    ) -> Option<usize> {
        self.put_name(labels)?;
        self.put_u16(type_code)?;
        let class = if cache_flush {
            CLASS_IN | CACHE_FLUSH
        } else {
            CLASS_IN
        };
        self.put_u16(class)?;
        for b in ttl.to_be_bytes() {
            self.put_u8(b)?;
        }
        self.reserve_u16()
    }

    /// Patch the RDLENGTH placeholder at `rdlen_pos` with the number of
    /// rdata bytes written since.
    fn finish_rdata(&mut self, rdlen_pos: usize) {
        let rdlen = (self.pos - (rdlen_pos + 2)) as u16;
        self.set_u16_at(rdlen_pos, rdlen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simple_dns::{CLASS, Packet, TYPE, rdata::RData};

    fn identity() -> MdnsIdentity {
        MdnsIdentity {
            label: "lp-8e30".into(),
            instance: "MyLamp".into(),
            mac: [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30],
            proto: 33,
            port: 80,
            ipv4: [192, 168, 1, 42],
        }
    }

    fn parse(bytes: &[u8]) -> Packet<'_> {
        Packet::parse(bytes).expect("built answer should parse")
    }

    #[test]
    fn a_host_question_answers_with_the_address() {
        let id = identity();
        let query = MdnsQuery {
            host_a: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, None).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.id(), 0);
        assert!(pkt.has_flags(
            simple_dns::PacketFlag::RESPONSE | simple_dns::PacketFlag::AUTHORITATIVE_ANSWER
        ));
        assert!(pkt.questions.is_empty());
        assert_eq!(pkt.answers.len(), 1);
        let rr = &pkt.answers[0];
        assert_eq!(rr.name, "lp-8e30.local".try_into().unwrap());
        assert!(rr.cache_flush);
        assert_eq!(rr.ttl, 120);
        match &rr.rdata {
            RData::A(a) => assert_eq!(a.address, u32::from_be_bytes([192, 168, 1, 42])),
            other => panic!("expected A, got {other:?}"),
        }
    }

    #[test]
    fn an_aaaa_host_question_answers_with_nsec() {
        let id = identity();
        let query = MdnsQuery {
            host_aaaa: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, None).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.answers.len(), 1);
        match &pkt.answers[0].rdata {
            RData::NSEC(nsec) => {
                assert_eq!(nsec.next_name, "lp-8e30.local".try_into().unwrap());
                assert_eq!(nsec.type_bit_maps.len(), 1);
                assert_eq!(nsec.type_bit_maps[0].window_block, 0);
                // Bit for TYPE A (1): the second-highest bit of byte 0.
                assert_eq!(&*nsec.type_bit_maps[0].bitmap, [0x40]);
            }
            other => panic!("expected NSEC, got {other:?}"),
        }
    }

    #[test]
    fn a_and_aaaa_both_asked_both_answered() {
        let id = identity();
        let query = MdnsQuery {
            host_a: true,
            host_aaaa: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, None).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.answers.len(), 2);
        assert!(pkt.answers.iter().any(|rr| matches!(rr.rdata, RData::A(_))));
        assert!(
            pkt.answers
                .iter()
                .any(|rr| matches!(rr.rdata, RData::NSEC(_)))
        );
    }

    #[test]
    fn a_ptr_question_answers_with_ptr_srv_txt_and_a() {
        let id = identity();
        let query = MdnsQuery {
            service_ptr: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, None).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.answers.len(), 1);
        let ptr_target = match &pkt.answers[0].rdata {
            RData::PTR(ptr) => ptr.0.clone(),
            other => panic!("expected PTR, got {other:?}"),
        };
        assert_eq!(
            ptr_target,
            "MyLamp._lightplayer._tcp.local".try_into().unwrap()
        );
        assert!(!pkt.answers[0].cache_flush);

        assert_eq!(pkt.additional_records.len(), 3);
        assert!(
            pkt.additional_records
                .iter()
                .any(|rr| matches!(rr.rdata, RData::SRV(_)))
        );
        assert!(
            pkt.additional_records
                .iter()
                .any(|rr| matches!(rr.rdata, RData::TXT(_)))
        );
        assert!(
            pkt.additional_records
                .iter()
                .any(|rr| matches!(rr.rdata, RData::A(_)))
        );
    }

    #[test]
    fn srv_and_txt_instance_questions_answer_with_a_as_additional() {
        let id = identity();
        let query = MdnsQuery {
            instance_srv: true,
            instance_txt: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, None).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.answers.len(), 2);
        assert!(
            pkt.answers
                .iter()
                .any(|rr| matches!(rr.rdata, RData::SRV(_)))
        );
        assert!(
            pkt.answers
                .iter()
                .any(|rr| matches!(rr.rdata, RData::TXT(_)))
        );

        let srv = pkt
            .answers
            .iter()
            .find_map(|rr| match &rr.rdata {
                RData::SRV(srv) => Some(srv),
                _ => None,
            })
            .unwrap();
        assert_eq!(srv.port, 80);
        assert_eq!(srv.priority, 0);
        assert_eq!(srv.weight, 0);
        assert_eq!(srv.target, "lp-8e30.local".try_into().unwrap());

        let txt = pkt
            .answers
            .iter()
            .find_map(|rr| match &rr.rdata {
                RData::TXT(txt) => Some(txt),
                _ => None,
            })
            .unwrap();
        let attrs = txt.attributes();
        assert_eq!(attrs.get("mac").unwrap().as_deref(), Some("10bda3b08e30"));
        assert_eq!(attrs.get("proto").unwrap().as_deref(), Some("33"));
        assert_eq!(attrs.get("path").unwrap().as_deref(), Some("/link"));

        assert_eq!(pkt.additional_records.len(), 1);
        assert!(matches!(pkt.additional_records[0].rdata, RData::A(_)));
    }

    #[test]
    fn a_question_for_another_name_answers_nothing() {
        let id = identity();
        let query = MdnsQuery::default();
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, None).unwrap();
        let pkt = parse(&buf[..len]);

        assert!(pkt.answers.is_empty());
        assert!(pkt.additional_records.is_empty());
    }

    #[test]
    fn legacy_unicast_echoes_id_and_question() {
        let id = identity();
        let query = MdnsQuery {
            id: 4242,
            host_a: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 512];
        let len = build_answer(&mut buf, &id, &query, Some(4242)).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.id(), 4242);
        assert_eq!(pkt.questions.len(), 1);
        assert_eq!(pkt.questions[0].qname, "lp-8e30.local".try_into().unwrap());
        assert_eq!(pkt.questions[0].qtype, TYPE::A.into());
        assert_eq!(pkt.questions[0].qclass, CLASS::IN.into());
    }

    #[test]
    fn goodbye_has_ttl_zero_on_every_record() {
        let id = identity();
        let mut buf = [0u8; 512];
        let len = build_goodbye(&mut buf, &id).unwrap();
        let pkt = parse(&buf[..len]);

        assert_eq!(pkt.id(), 0);
        assert!(pkt.questions.is_empty());
        // A, NSEC, PTR, SRV, TXT: one each, with no duplicates from the
        // additional-record logic.
        assert_eq!(pkt.answers.len(), 5);
        assert!(pkt.additional_records.is_empty());
        for rr in &pkt.answers {
            assert_eq!(rr.ttl, 0, "{:?} should have ttl 0", rr.rdata);
        }
    }

    #[test]
    fn the_instance_name_is_cut_at_63_bytes_on_a_char_boundary() {
        // "é" is 2 bytes in UTF-8; 62 ASCII bytes + "é" lands the cut
        // right on its first byte unless the boundary search steps back.
        let mut id = identity();
        id.instance = alloc::format!("{}é", "a".repeat(62));
        assert_eq!(id.instance.len(), 64);

        let effective = effective_instance(&id);
        assert!(effective.len() <= 63);
        assert!(id.instance.starts_with(effective));
    }

    #[test]
    fn an_empty_instance_falls_back_to_the_host_label() {
        let mut id = identity();
        id.instance = String::new();
        assert_eq!(effective_instance(&id), "lp-8e30");
    }

    #[test]
    fn a_buffer_too_small_is_none() {
        let id = identity();
        let query = MdnsQuery {
            host_a: true,
            ..MdnsQuery::default()
        };
        let mut buf = [0u8; 4];
        assert!(build_answer(&mut buf, &id, &query, None).is_none());
    }
}
