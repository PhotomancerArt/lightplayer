//! The network seam's answers (P12 §1), call by call, against a virtual LAN.
//!
//! No firmware carries the seam yet (P10 is building it), so a synthetic
//! guest does ([`seam_guest::net_guest`]): its table names the nine `net_*`
//! calls, and a command loop makes whichever call a test asks for, through
//! the patched entry, the `ebreak` and the answer. Each test checks one
//! call's contract from `lp-base/lp-seam` against what the LAN holds, and
//! that an answer writes only the memory its call handed over.

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp_common::seam::net::lan_dns::TYPE_A;
use lp_emu_esp_common::seam::net::lan_frame::{BROADCAST_MAC, UdpDatagram};
use lp_emu_esp_common::seam::net::{LanDriver, LanPort, SharedLan, net_endpoint};
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine};
use lp_seam::net as abi;
use seam_guest::net_guest::*;
use seam_guest::*;

#[test]
fn one_atom_arms_nine_sites_and_the_mac_call_writes_only_the_efuse_mac() {
    let mut m = board(None, NetPage::default(), SeamRequest::default());
    let lines = m.take_seam_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("SEAM net=lan engaged (capability, abi "),
        "{}",
        lines[0]
    );
    assert!(
        lines[0].ends_with(", 9 sites, engaged-byte@0x42000400)"),
        "{}",
        lines[0]
    );
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1+net=lan");
    assert_eq!(
        m.peek_word(NET_ENGAGED).unwrap() & 0xff,
        1,
        "the engaged byte reads 1 through the window"
    );
    assert_eq!(m.seams().sites.iter().filter(|s| s.armed).count(), 10);

    // A buffer of 0xaa; the call writes six bytes of it and no more.
    poke_bytes(&mut m, BUF, &[0xaa; 12]);
    assert_eq!(call(&mut m, &lp_seam::net_mac::DECL, [BUF, 0, 0, 0]), 1);
    let got = peek_bytes(&mut m, BUF, 12);
    assert_eq!(&got[..6], &MAC);
    assert_eq!(&got[6..], &[0xaa; 6], "nothing past the MAC");
    assert_eq!(m.snapshot().seam_calls, 1, "counted in seam_calls");

    // No LAN given: one of its own, self-driven, with this board on it.
    let lan = m.lan().expect("an empty private LAN").clone();
    assert_eq!(lan.driver(), LanDriver::SelfDriven);
    assert!(lan.with(|l| l.station(net_endpoint(ParticipantId(0))).is_some()));
}

#[test]
fn a_right_password_joins_after_the_stated_latency_and_a_wrong_or_unknown_one_says_why() {
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    let mut m = board(
        Some(lan.clone()),
        NetPage::default(),
        SeamRequest::default(),
    );
    let take = |m: &mut Esp32C6Machine| call(m, &lp_seam::net_event_take::DECL, [0; 4]);
    let link = |m: &mut Esp32C6Machine| call(m, &lp_seam::net_link::DECL, [0; 4]);

    assert_eq!(
        connect(&mut m, b"home", HOME_PASSWORD),
        1,
        "the attempt started"
    );
    assert_eq!(take(&mut m), abi::EVENT_NONE, "not yet: a join takes 10 ms");
    assert_eq!(link(&mut m), 0);
    run_for(&mut m, 11 * MS);
    assert_eq!(take(&mut m), abi::EVENT_ASSOCIATED);
    assert_eq!(take(&mut m), abi::EVENT_NONE, "once");
    assert_eq!(link(&mut m), 1);
    assert_eq!(
        lan.with(|l| l
            .station(m.net_endpoint_id())
            .unwrap()
            .network()
            .map(str::to_owned)),
        Some("home".to_string())
    );

    assert_eq!(connect(&mut m, b"home", b"wrong"), 1);
    assert_eq!(link(&mut m), 0, "a new join leaves the old network at once");
    run_for(&mut m, 11 * MS);
    assert_eq!(take(&mut m), abi::EVENT_AUTH_FAILED);
    assert_eq!(link(&mut m), 0);

    assert_eq!(connect(&mut m, b"nowhere", b"x"), 1);
    run_for(&mut m, 11 * MS);
    assert_eq!(take(&mut m), abi::EVENT_NOT_FOUND);

    // A hidden network joins by name; an open one with any password.
    assert_eq!(connect(&mut m, b"attic", b"test-password-2"), 1);
    run_for(&mut m, 11 * MS);
    assert_eq!(take(&mut m), abi::EVENT_ASSOCIATED);
    assert_eq!(call(&mut m, &lp_seam::net_disconnect::DECL, [0; 4]), 1);
    assert_eq!(link(&mut m), 0, "down at once");
    assert_eq!(take(&mut m), abi::EVENT_NONE, "and no event");

    // A name longer than an SSID, or a password longer than WPA2's, is
    // refused before the LAN hears it.
    assert_eq!(connect(&mut m, &[b'x'; abi::MAX_SSID_LEN + 1], b""), 0);
    assert_eq!(
        connect(&mut m, b"home", &[b'p'; abi::MAX_PASSWORD_LEN + 1]),
        0
    );
}

#[test]
fn a_scan_hears_the_networks_in_range_strongest_first_in_whole_records() {
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    let mut m = board(Some(lan), NetPage::default(), SeamRequest::default());
    assert_eq!(call(&mut m, &lp_seam::net_scan_start::DECL, [0; 4]), 1);
    run_for(&mut m, 101 * MS);
    assert_eq!(
        call(&mut m, &lp_seam::net_event_take::DECL, [0; 4]),
        abi::EVENT_SCAN_DONE
    );

    poke_bytes(&mut m, BUF, &[0xee; 32]);
    assert_eq!(
        call(&mut m, &lp_seam::net_scan_take::DECL, [BUF, 64, 0, 0]),
        3
    );
    let mut want = Vec::new();
    for (name, dbm, secure) in [("home", -45i8, 1u8), ("home", -70, 1), ("cafe", -80, 0)] {
        want.push(name.len() as u8);
        want.extend_from_slice(name.as_bytes());
        want.push(dbm as u8);
        want.push(secure);
    }
    assert_eq!(want.len(), 3 * abi::scan_record_len(4));
    let got = peek_bytes(&mut m, BUF, want.len() + 4);
    assert_eq!(&got[..want.len()], &want[..], "hidden `attic` left out");
    assert_eq!(&got[want.len()..], &[0xee; 4], "nothing past the records");

    // Whole records only: 15 bytes hold two seven-byte records, 6 none.
    assert_eq!(
        call(&mut m, &lp_seam::net_scan_take::DECL, [BUF, 15, 0, 0]),
        2
    );
    assert_eq!(
        call(&mut m, &lp_seam::net_scan_take::DECL, [BUF, 6, 0, 0]),
        0
    );
}

#[test]
fn frames_leave_only_while_linked_and_arrive_one_whole_frame_per_take() {
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    lan.with(|l| l.log_frames(true));
    let probe = lan.with(|l| l.add_probe());
    let mut m = board(
        Some(lan.clone()),
        NetPage::default(),
        SeamRequest::default(),
    );
    let me = m.net_endpoint_id();
    let frame = UdpDatagram {
        src_mac: MAC,
        dst_mac: BROADCAST_MAC,
        src_ip: "0.0.0.0".parse().unwrap(),
        dst_ip: "255.255.255.255".parse().unwrap(),
        src_port: 9,
        dst_port: 9,
        payload: b"hello, segment",
    }
    .emit();
    poke_bytes(&mut m, BUF, &frame);
    let give = |m: &mut Esp32C6Machine, len: usize| {
        call(m, &lp_seam::net_give_frame::DECL, [BUF, len as u32, 0, 0])
    };
    assert_eq!(give(&mut m, frame.len()), 0, "not joined: refused");

    assert_eq!(connect(&mut m, b"home", HOME_PASSWORD), 1);
    run_for(&mut m, 11 * MS);
    assert_eq!(give(&mut m, 0), 0);
    assert_eq!(give(&mut m, abi::MAX_FRAME_LEN + 1), 0);
    assert_eq!(give(&mut m, frame.len()), 1);
    run_for(&mut m, MS);
    let reached: Vec<LanPort> = lan.with(|l| {
        l.frame_log()
            .iter()
            .filter(|r| r.bytes == frame && r.from == LanPort::Board(me))
            .map(|r| r.to)
            .collect()
    });
    assert_eq!(reached, [LanPort::Gateway, LanPort::Probe(probe)]);

    // The probe asks the segment a name: an mDNS query, flooded to the
    // board. One frame per take, and a buffer too small leaves it queued.
    lan.with(|l| l.probe_mut(probe).query("lp-test.local", TYPE_A));
    run_for(&mut m, MS);
    let take = |m: &mut Esp32C6Machine, cap: u32| {
        call(m, &lp_seam::net_take_frame::DECL, [BUF2, cap, 0, 0])
    };
    assert_eq!(take(&mut m, 20), 0, "does not fit: stays queued");
    poke_bytes(&mut m, BUF2, &[0x55; 256]);
    let n = take(&mut m, abi::MAX_FRAME_LEN as u32) as usize;
    assert!(n > 42, "a whole query frame: {n}");
    let got = peek_bytes(&mut m, BUF2, n + 4);
    assert_eq!(
        &got[..6],
        &[0x01, 0x00, 0x5e, 0x00, 0x00, 0xfb],
        "to the mDNS group"
    );
    assert_eq!(&got[n..], &[0x55; 4], "nothing past the frame");
    assert_eq!(
        take(&mut m, abi::MAX_FRAME_LEN as u32),
        0,
        "one frame, not two"
    );

    assert_eq!(call(&mut m, &lp_seam::net_disconnect::DECL, [0; 4]), 1);
    assert_eq!(give(&mut m, frame.len()), 0, "left: refused again");
}

#[test]
fn with_no_lan_given_an_engaged_board_hears_nothing_and_finds_nothing() {
    let mut m = board(None, NetPage::default(), SeamRequest::default());
    assert_eq!(call(&mut m, &lp_seam::net_scan_start::DECL, [0; 4]), 1);
    run_for(&mut m, 101 * MS);
    assert_eq!(
        call(&mut m, &lp_seam::net_event_take::DECL, [0; 4]),
        abi::EVENT_SCAN_DONE
    );
    assert_eq!(
        call(&mut m, &lp_seam::net_scan_take::DECL, [BUF, 256, 0, 0]),
        0
    );
    assert_eq!(connect(&mut m, b"home", HOME_PASSWORD), 1);
    run_for(&mut m, 11 * MS);
    assert_eq!(
        call(&mut m, &lp_seam::net_event_take::DECL, [0; 4]),
        abi::EVENT_NOT_FOUND,
        "nothing in range: an honest not found"
    );
}

#[test]
fn led_fast_strict_or_soft_engages_beside_the_default_net_lan() {
    for request in [
        SeamRequest::strict("led=fast").unwrap(),
        SeamRequest::prefer("led=fast").unwrap(),
    ] {
        let mut m = board(
            None,
            NetPage {
                led: true,
                ..NetPage::default()
            },
            request.clone(),
        );
        let lines = m.take_seam_lines();
        assert_eq!(lines.len(), 2, "{request}: {lines:?}");
        assert!(
            lines[0].starts_with("SEAM led=fast engaged (performance"),
            "{lines:?}"
        );
        assert!(
            lines[1].starts_with("SEAM net=lan engaged (capability"),
            "{lines:?}"
        );
        assert_eq!(
            m.configuration_label(),
            "lp-emu:esp32c6:t1+led=fast+net=lan"
        );
        assert_eq!(call(&mut m, &lp_seam::net_mac::DECL, [BUF, 0, 0, 0]), 1);
    }
    // An image without the LED seam: strict stops the build, soft says why
    // and the network still engages.
    assert!(
        Esp32C6Builder::new()
            .seams(SeamRequest::strict("led=fast").unwrap())
            .flash(lp_emu_esp32c6::flash::FlashBacking::Bytes(chip(&[(
                CORE_A,
                &net_core_page(NetPage::default())
            )])))
            .build()
            .is_err(),
        "no table entry could ever satisfy a strict led=fast"
    );
    let mut m = board(
        None,
        NetPage::default(),
        SeamRequest::prefer("led=fast").unwrap(),
    );
    let lines = m.take_seam_lines();
    assert!(
        lines.iter().any(|l| l.starts_with("SEAM net=lan engaged")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("SEAM none engaged: led=fast: ") && l.contains("ws281x")),
        "{lines:?}"
    );
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1+net=lan");
}

#[test]
fn an_incomplete_table_engages_no_network_and_says_which_call_is_missing() {
    let page = NetPage {
        without: Some(lp_seam::net_scan_take::ID),
        ..NetPage::default()
    };
    let mut m = board(None, page, SeamRequest::default());
    let lines = m.take_seam_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("SEAM none engaged: net=lan: ")
            && lines[0].contains("no entry for lp_seam_net_scan_take"),
        "{}",
        lines[0]
    );
    assert_eq!(
        m.peek_word(NET_ENGAGED).unwrap() & 0xff,
        0,
        "the radio would run"
    );
    assert!(m.lan().is_none());
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1");
}

#[test]
fn the_same_script_on_a_self_driven_board_gives_the_same_frame_log_twice() {
    let script = || {
        let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
        lan.with(|l| l.log_frames(true));
        let probe = lan.with(|l| l.add_probe());
        let mut m = board(
            Some(lan.clone()),
            NetPage::default(),
            SeamRequest::default(),
        );
        assert_eq!(connect(&mut m, b"home", HOME_PASSWORD), 1);
        run_for(&mut m, 11 * MS);
        lan.with(|l| l.probe_mut(probe).query("lp-test.local", TYPE_A));
        run_for(&mut m, 2 * MS);
        let mut taken = Vec::new();
        loop {
            let n = call(&mut m, &lp_seam::net_take_frame::DECL, [BUF, 1514, 0, 0]);
            if n == 0 {
                break;
            }
            taken.push((m.cycles(), peek_bytes(&mut m, BUF, n as usize)));
        }
        (lan.with(|l| l.frame_log().to_vec()), taken, m.cycles())
    };
    let first = script();
    assert!(!first.0.is_empty() && !first.1.is_empty());
    assert_eq!(first, script());
}

/// One millisecond of guest time at 160 MHz.
const MS: u64 = 160_000;
/// The board's eFuse MAC, locally administered.
const MAC: [u8; 6] = [0x02, 0x4c, 0x50, 0x00, 0x00, 0x01];

/// A booted synthetic board carrying the network seam.
fn board(lan: Option<SharedLan>, page: NetPage, request: SeamRequest) -> Esp32C6Machine {
    let page = net_core_page(page);
    let mut builder = Esp32C6Builder::new()
        .efuse(EfuseIdentity {
            mac: MAC,
            ..EfuseIdentity::default()
        })
        .seams(request);
    if let Some(lan) = lan {
        builder = builder.lan(lan, ParticipantId(0));
    }
    let mut m = machine(chip(&[(CORE_A, &page)]), builder);
    boot_core(&mut m, CORE_A);
    m
}
