//! Two boards on one virtual LAN, driven by the lockstep runner (P12 §3, the
//! machine side): a [`LanDriver::Runner`] LAN goes in
//! `Lockstep::with_medium`, each board joins through its own seam calls,
//! one board's frame reaches the other's take, and the same script gives the
//! same frame log twice — the deterministic multi-board driver CI uses.
//!
//! Synthetic guests ([`seam_guest::net_guest`]); the same pair on the shipped
//! firmware is `seam_net_two_boards.rs`.

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp_common::seam::net::lan_frame::{BROADCAST_MAC, UdpDatagram};
use lp_emu_esp_common::seam::net::{FrameRecord, LanDriver, LanPort, SharedLan, net_endpoint};
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::lockstep::Lockstep;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use lp_seam::SeamDecl;
use lp_seam::net as abi;
use seam_guest::net_guest::*;
use seam_guest::*;

#[test]
fn two_boards_join_one_lan_and_a_frame_from_one_is_taken_by_the_other() {
    let run = script();
    assert_eq!(run.joined, [abi::EVENT_ASSOCIATED, abi::EVENT_ASSOCIATED]);
    assert_eq!(run.taken, run.sent, "board 1 took board 0's frame whole");
    assert!(
        run.log
            .iter()
            .any(|r| r.from == LanPort::Board(net_endpoint(ParticipantId(0)))
                && r.to == LanPort::Board(net_endpoint(ParticipantId(1)))),
        "carried board to board"
    );
}

#[test]
fn the_same_pair_script_gives_the_same_frame_log_twice() {
    let first = script();
    assert!(!first.log.is_empty());
    let second = script();
    assert_eq!(first.log, second.log);
    assert_eq!(first.cycles, second.cycles);
}

/// What one run of the script saw.
struct Run {
    joined: [u32; 2],
    sent: Vec<u8>,
    taken: Vec<u8>,
    log: Vec<FrameRecord>,
    cycles: [u64; 2],
}

/// Both boards join `home`; board 0 broadcasts one frame; board 1 takes it.
fn script() -> Run {
    let lan = SharedLan::new(fixture_lan(), LanDriver::Runner);
    lan.with(|l| l.log_frames(true));
    let boards = vec![board(&lan, 0), board(&lan, 1)];
    let mut pair = Lockstep::new(boards)
        .unwrap()
        .with_medium(Box::new(lan.clone()));

    for i in 0..2 {
        let m = machine_of(&mut pair, i);
        poke_bytes(m, BUF2, b"home");
        poke_bytes(m, BUF3, HOME_PASSWORD);
    }
    let started = both(
        &mut pair,
        &lp_seam::net_connect::DECL,
        [BUF2, 4, BUF3, HOME_PASSWORD.len() as u32],
    );
    assert_eq!(started, [1, 1]);
    advance(&mut pair, 11 * MS);
    let joined = both(&mut pair, &lp_seam::net_event_take::DECL, [0; 4]);

    let sent = UdpDatagram {
        src_mac: MACS[0],
        dst_mac: BROADCAST_MAC,
        src_ip: "0.0.0.0".parse().unwrap(),
        dst_ip: "255.255.255.255".parse().unwrap(),
        src_port: 9,
        dst_port: 9,
        payload: b"from board 0",
    }
    .emit();
    poke_bytes(machine_of(&mut pair, 0), BUF, &sent);
    let done = post(
        machine_of(&mut pair, 0),
        &lp_seam::net_give_frame::DECL,
        [BUF, sent.len() as u32, 0, 0],
    );
    advance(&mut pair, MS);
    assert_eq!(answered(machine_of(&mut pair, 0), done), Some(1));
    advance(&mut pair, MS);
    let done = post(
        machine_of(&mut pair, 1),
        &lp_seam::net_take_frame::DECL,
        [BUF, abi::MAX_FRAME_LEN as u32, 0, 0],
    );
    advance(&mut pair, MS);
    let n = answered(machine_of(&mut pair, 1), done).expect("the take was made");
    let taken = peek_bytes(machine_of(&mut pair, 1), BUF, n as usize);

    let report = pair.report();
    Run {
        joined,
        sent,
        taken,
        log: lan.with(|l| l.frame_log().to_vec()),
        cycles: [report.machines[0].cycles, report.machines[1].cycles],
    }
}

/// One millisecond of guest time at 160 MHz.
const MS: u64 = 160_000;
const MACS: [[u8; 6]; 2] = [
    [0x02, 0x4c, 0x50, 0x00, 0x00, 0x01],
    [0x02, 0x4c, 0x50, 0x00, 0x00, 0x02],
];

fn board(lan: &SharedLan, index: usize) -> Esp32C6Machine {
    let page = net_core_page(NetPage::default());
    let builder = Esp32C6Builder::new()
        .efuse(EfuseIdentity {
            mac: MACS[index],
            ..EfuseIdentity::default()
        })
        .seams(SeamRequest::default())
        .lan(lan.clone(), ParticipantId(index));
    let mut m = machine(chip(&[(CORE_A, &page)]), builder);
    boot_core(&mut m, CORE_A);
    m
}

fn machine_of(pair: &mut Lockstep, i: usize) -> &mut Esp32C6Machine {
    pair.machine_mut(ParticipantId(i)).unwrap()
}

/// Run the pair `cycles` more.
fn advance(pair: &mut Lockstep, cycles: u64) {
    let horizon = pair.cycles() + cycles;
    pair.run_until(horizon, &StopCondition::default());
}

/// Ask both boards to make the same call, and wait for both answers.
fn both(pair: &mut Lockstep, decl: &SeamDecl, args: [u32; 4]) -> [u32; 2] {
    let done = [0, 1].map(|i| post(machine_of(pair, i), decl, args));
    for _ in 0..1_000 {
        advance(pair, 2_000);
        let got = [0, 1].map(|i| answered(machine_of(pair, i), done[i]));
        if let [Some(a), Some(b)] = got {
            return [a, b];
        }
    }
    panic!("the pair never made the call to {}", decl.name);
}
