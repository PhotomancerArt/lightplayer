//! Reading a board's mDNS answer: pure, bytes in, boards out.
//!
//! A board answers a `_lightplayer._tcp.local` PTR question with the PTR in
//! the answer section and its SRV, TXT and A records alongside (RFC 6763
//! §12); this joins them by name. Anything that is not about that service, or
//! does not parse, yields nothing — never a panic.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use serde::Serialize;
use simple_dns::rdata::RData;
use simple_dns::{CLASS, Name, Packet, PacketFlag, QCLASS, QTYPE, Question, TYPE};

/// The service every board advertises, without the instance.
pub const SERVICE_NAME: &str = "_lightplayer._tcp.local";

/// One LightPlayer board found on the LAN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanBoard {
    /// The host label, e.g. `lp-8e30` (the SRV target without `.local`).
    pub name: String,
    /// The DNS-SD instance name: the board's own name as Studio shows it.
    pub instance: String,
    pub ip: Ipv4Addr,
    pub port: u16,
    /// The base MAC as 12 hex digits (TXT `mac=`).
    pub mac: Option<String>,
    /// The wire protocol version (TXT `proto=`).
    pub proto: Option<u32>,
    /// The link's path (TXT `path=`), `/link`.
    pub path: Option<String>,
    /// What to hand to `lp-cli`: `lan:<ip>`, or `lan:<ip>:<port>` off port 80.
    pub spec: String,
}

/// The one PTR question a browse sends, as a DNS packet carrying `id`.
///
/// Sent from an ephemeral port, so it is an RFC 6762 §6.7 legacy unicast
/// query: boards answer straight to the sender and echo `id`.
pub fn build_query(id: u16) -> Vec<u8> {
    let mut packet = Packet::new_query(id);
    packet.questions.push(Question::new(
        Name::new_unchecked(SERVICE_NAME),
        QTYPE::TYPE(TYPE::PTR),
        QCLASS::CLASS(CLASS::IN),
        false,
    ));
    packet.build_bytes_vec().unwrap_or_default()
}

/// The LightPlayer boards one answer packet describes, in answer order.
///
/// A board needs a SRV (its port and host) and an A (its address) to be
/// listed; a packet that is not a response, is malformed, or is about other
/// services gives an empty list.
pub fn parse_answer(bytes: &[u8]) -> Vec<LanBoard> {
    let Ok(packet) = Packet::parse(bytes) else {
        return Vec::new();
    };
    if !packet.has_flags(PacketFlag::RESPONSE) {
        return Vec::new();
    }

    let suffix = format!(".{SERVICE_NAME}");
    let mut instances: Vec<String> = Vec::new();
    let mut srv: HashMap<String, (String, u16)> = HashMap::new();
    let mut txt: HashMap<String, HashMap<String, Option<String>>> = HashMap::new();
    let mut addresses: HashMap<String, Ipv4Addr> = HashMap::new();

    for record in packet.answers.iter().chain(&packet.additional_records) {
        let name = record.name.to_string();
        match &record.rdata {
            RData::PTR(ptr) if name.eq_ignore_ascii_case(SERVICE_NAME) => {
                let instance = ptr.0.to_string();
                if has_suffix(&instance, &suffix) && !instances.contains(&instance) {
                    instances.push(instance);
                }
            }
            RData::SRV(record_srv) if has_suffix(&name, &suffix) => {
                srv.insert(
                    name.to_ascii_lowercase(),
                    (record_srv.target.to_string(), record_srv.port),
                );
                if !instances.contains(&name) {
                    instances.push(name);
                }
            }
            RData::TXT(record_txt) if has_suffix(&name, &suffix) => {
                txt.insert(name.to_ascii_lowercase(), record_txt.attributes());
            }
            RData::A(a) => {
                addresses.insert(name.to_ascii_lowercase(), Ipv4Addr::from(a.address));
            }
            _ => {}
        }
    }

    instances
        .into_iter()
        .filter_map(|full| {
            let key = full.to_ascii_lowercase();
            let (target, port) = srv.get(&key)?;
            let ip = *addresses.get(&target.to_ascii_lowercase())?;
            let attributes = txt.get(&key);
            let attribute = |name: &str| -> Option<String> {
                attributes?
                    .get(name)?
                    .clone()
                    .filter(|value| !value.is_empty())
            };
            let instance = full[..full.len() - suffix.len()].to_string();
            let name = match target.strip_suffix(".local") {
                Some(label) => label.to_string(),
                None => target.clone(),
            };
            Some(LanBoard {
                name,
                instance,
                ip,
                port: *port,
                mac: attribute("mac").map(|mac| mac.to_ascii_lowercase()),
                proto: attribute("proto").and_then(|proto| proto.parse().ok()),
                path: attribute("path"),
                spec: spec_for(ip, *port),
            })
        })
        .collect()
}

/// Add `found` to `boards`, keeping one entry per board: by MAC when both
/// have one, by instance name otherwise. The first sighting wins.
pub fn merge_boards(boards: &mut Vec<LanBoard>, found: Vec<LanBoard>) {
    for board in found {
        if !boards.iter().any(|known| same_board(known, &board)) {
            boards.push(board);
        }
    }
}

fn same_board(a: &LanBoard, b: &LanBoard) -> bool {
    match (&a.mac, &b.mac) {
        (Some(a_mac), Some(b_mac)) => a_mac == b_mac,
        _ => a.instance.eq_ignore_ascii_case(&b.instance),
    }
}

fn spec_for(ip: Ipv4Addr, port: u16) -> String {
    if port == 80 {
        format!("lan:{ip}")
    } else {
        format!("lan:{ip}:{port}")
    }
}

fn has_suffix(name: &str, suffix: &str) -> bool {
    name.len() > suffix.len() && name.to_ascii_lowercase().ends_with(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fw_esp32_common::net::mdns::{MdnsIdentity, MdnsQuery, build_answer, parse_query};
    use simple_dns::ResourceRecord;
    use simple_dns::rdata::{A, SRV, TXT};

    #[test]
    fn a_boards_own_legacy_unicast_answer_reads_back_as_that_board() {
        let identity = porch_sign();
        // Our question, as the board's own parser reads it.
        let query = parse_query(&build_query(77), &identity.label, &identity.instance)
            .expect("the board understands the question");
        assert!(query.service_ptr);
        assert_eq!(query.id, 77);

        let answer = board_answer(&identity, &query, 77);

        assert_eq!(
            parse_answer(&answer),
            vec![LanBoard {
                name: "lp-8e30".into(),
                instance: "Porch sign".into(),
                ip: Ipv4Addr::new(192, 168, 4, 100),
                port: 80,
                mac: Some("10bda3b08e30".into()),
                proto: Some(33),
                path: Some("/link".into()),
                spec: "lan:192.168.4.100".into(),
            }]
        );
    }

    #[test]
    fn the_query_is_one_service_ptr_question() {
        let bytes = build_query(5);
        let packet = Packet::parse(&bytes).unwrap();
        assert_eq!(packet.id(), 5);
        assert!(!packet.has_flags(PacketFlag::RESPONSE));
        assert_eq!(packet.questions.len(), 1);
        assert_eq!(packet.questions[0].qname.to_string(), SERVICE_NAME);
        assert_eq!(packet.questions[0].qtype, QTYPE::TYPE(TYPE::PTR));
    }

    #[test]
    fn a_packet_about_other_services_yields_nothing() {
        let mut packet = Packet::new_reply(0);
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked("_http._tcp.local"),
            CLASS::IN,
            120,
            RData::PTR(Name::new_unchecked("Printer._http._tcp.local").into()),
        ));
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked("printer.local"),
            CLASS::IN,
            120,
            RData::A(A {
                address: 0xC0A8_0107,
            }),
        ));
        let bytes = packet.build_bytes_vec().unwrap();

        assert!(parse_answer(&bytes).is_empty());
    }

    #[test]
    fn a_query_packet_and_garbage_yield_nothing() {
        assert!(parse_answer(&build_query(1)).is_empty());
        assert!(parse_answer(&[]).is_empty());
        assert!(parse_answer(&[0xff; 7]).is_empty());
        assert!(
            parse_answer(&[0x00, 0x01, 0x84, 0x00, 0xff, 0xff, 0xff, 0xff, 1, 2, 3]).is_empty()
        );
        let mut truncated = board_answer(&porch_sign(), &service_query(), 9);
        truncated.truncate(truncated.len() / 2);
        assert!(parse_answer(&truncated).is_empty());
    }

    #[test]
    fn a_board_without_an_address_is_not_listed() {
        let mut packet = Packet::new_reply(0);
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked(SERVICE_NAME),
            CLASS::IN,
            120,
            RData::PTR(Name::new_unchecked("Lonely._lightplayer._tcp.local").into()),
        ));
        packet.additional_records.push(ResourceRecord::new(
            Name::new_unchecked("Lonely._lightplayer._tcp.local"),
            CLASS::IN,
            120,
            RData::SRV(SRV {
                priority: 0,
                weight: 0,
                port: 80,
                target: Name::new_unchecked("lp-0001.local"),
            }),
        ));
        let bytes = packet.build_bytes_vec().unwrap();

        assert!(parse_answer(&bytes).is_empty());
    }

    #[test]
    fn the_same_mac_answering_twice_is_one_board() {
        let mut boards = Vec::new();
        let first = parse_answer(&board_answer(&porch_sign(), &service_query(), 1));
        let mut renamed = porch_sign();
        renamed.instance = "Porch sign (again)".into();
        let second = parse_answer(&board_answer(&renamed, &service_query(), 2));
        assert_eq!(second.len(), 1);

        merge_boards(&mut boards, first);
        merge_boards(&mut boards, second);
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].instance, "Porch sign");

        let mut other = porch_sign();
        other.mac = [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x31];
        other.label = "lp-8e31".into();
        merge_boards(
            &mut boards,
            parse_answer(&board_answer(&other, &service_query(), 3)),
        );
        assert_eq!(boards.len(), 2);
    }

    #[test]
    fn boards_without_a_mac_dedupe_by_instance_name() {
        let board = |ip: [u8; 4]| LanBoard {
            name: "lp-x".into(),
            instance: "Same".into(),
            ip: Ipv4Addr::from(ip),
            port: 80,
            mac: None,
            proto: None,
            path: None,
            spec: spec_for(Ipv4Addr::from(ip), 80),
        };
        let mut boards = vec![board([10, 0, 0, 1])];
        merge_boards(&mut boards, vec![board([10, 0, 0, 2])]);
        assert_eq!(boards.len(), 1);
    }

    #[test]
    fn a_port_other_than_80_goes_into_the_spec() {
        let mut identity = porch_sign();
        identity.port = 8080;
        let boards = parse_answer(&board_answer(&identity, &service_query(), 4));

        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].port, 8080);
        assert_eq!(boards[0].spec, "lan:192.168.4.100:8080");
    }

    #[test]
    fn txt_values_that_are_missing_or_odd_leave_those_fields_empty() {
        let mut packet = Packet::new_reply(0);
        let instance = "Odd._lightplayer._tcp.local";
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked(SERVICE_NAME),
            CLASS::IN,
            120,
            RData::PTR(Name::new_unchecked(instance).into()),
        ));
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked(instance),
            CLASS::IN,
            120,
            RData::SRV(SRV {
                priority: 0,
                weight: 0,
                port: 80,
                target: Name::new_unchecked("LP-ODD.local"),
            }),
        ));
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked(instance),
            CLASS::IN,
            120,
            RData::TXT(TXT::new().with_string("proto=soon").unwrap()),
        ));
        packet.answers.push(ResourceRecord::new(
            Name::new_unchecked("lp-odd.local"),
            CLASS::IN,
            120,
            RData::A(A {
                address: 0x0A00_0009,
            }),
        ));
        let bytes = packet.build_bytes_vec().unwrap();

        let boards = parse_answer(&bytes);
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].name, "LP-ODD");
        assert_eq!(boards[0].ip, Ipv4Addr::new(10, 0, 0, 9));
        assert_eq!(boards[0].mac, None);
        assert_eq!(boards[0].proto, None);
        assert_eq!(boards[0].path, None);
    }

    fn porch_sign() -> MdnsIdentity {
        MdnsIdentity {
            label: "lp-8e30".into(),
            instance: "Porch sign".into(),
            mac: [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30],
            proto: 33,
            port: 80,
            ipv4: [192, 168, 4, 100],
        }
    }

    fn service_query() -> MdnsQuery {
        MdnsQuery {
            service_ptr: true,
            ..MdnsQuery::default()
        }
    }

    /// The board's own legacy-unicast answer (the firmware's `build_answer`).
    fn board_answer(identity: &MdnsIdentity, query: &MdnsQuery, id: u16) -> Vec<u8> {
        let mut buffer = [0u8; 512];
        let len = build_answer(&mut buffer, identity, query, Some(id)).expect("fits");
        buffer[..len].to_vec()
    }
}
