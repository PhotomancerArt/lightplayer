//! The network seam's wake (P12 §1): raised, through the pacer, when the
//! board's endpoint holds a frame **or** its station holds an event — and a
//! guest asleep in `wfi` is not slept through either, because the LAN's next
//! due cycle bounds the machine's idle skip.
//!
//! A synthetic guest ([`seam_guest::net_guest`]) whose table names a pending
//! word, with the wake handler bound the way the firmware binds it: clear
//! `FROM_CPU_INTR3`, swap the word to zero, OR what it saw into `WOKEN`.

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp_common::seam::net::lan_dns::TYPE_A;
use lp_emu_esp_common::seam::net::{LanDriver, SharedLan};
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine};
use lp_seam::net as abi;
use seam_guest::net_guest::*;
use seam_guest::*;

#[test]
fn an_event_and_then_a_frame_each_raise_the_wake_until_the_guest_takes_them() {
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    let probe = lan.with(|l| l.add_probe());
    let mut m = board(&lan, false);
    let bit = 1u32; // the network endpoint is the only one: index 0

    assert_eq!(connect(&mut m, b"home", HOME_PASSWORD), 1);
    run_for(&mut m, 5 * MS);
    assert_eq!(woken(&mut m), 0, "nothing yet: the join lands at 10 ms");
    run_for(&mut m, 6 * MS);
    assert_eq!(woken(&mut m), bit, "the station's event raised it");
    assert_eq!(m.peek_word(PENDING), Some(0), "and the handler swapped it");
    // The guest drains its events. While one waits the wake is raised again
    // each spacing; once none is left, nothing more is.
    assert_eq!(
        call(&mut m, &lp_seam::net_event_take::DECL, [0; 4]),
        abi::EVENT_ASSOCIATED
    );
    assert_eq!(call(&mut m, &lp_seam::net_event_take::DECL, [0; 4]), 0);
    settle(&mut m);
    assert!(m.poke_word(WOKEN, 0));
    run_for(&mut m, 3 * MS);
    assert_eq!(woken(&mut m), 0, "no work, no raise");

    // A frame for the board: the probe asks the segment a name.
    lan.with(|l| l.probe_mut(probe).query("lp-test.local", TYPE_A));
    run_for(&mut m, MS);
    assert_eq!(woken(&mut m), bit, "the frame raised it");
    let mut frames = 0;
    while call(&mut m, &lp_seam::net_take_frame::DECL, [BUF, 1514, 0, 0]) != 0 {
        frames += 1;
    }
    assert_eq!(frames, 1);
    settle(&mut m);
    assert!(m.poke_word(WOKEN, 0));
    run_for(&mut m, 3 * MS);
    assert_eq!(woken(&mut m), 0, "taken: no more raises");

    let lines = m.seam_wake_lines();
    assert_eq!(lines.len(), 1);
    let s = &m.seams().wake_stats[0];
    assert!(s.raised >= 2, "{}", lines[0]);
    assert_eq!(s.raised, s.consumed, "every raise consumed: {}", lines[0]);
    assert_eq!(m.seams().endpoints[0].refused(), 0);
}

#[test]
fn a_guest_asleep_in_wfi_is_woken_when_its_join_lands_not_after() {
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    let mut m = board(&lan, true);
    let me = m.net_endpoint_id();
    // The host starts the join for the sleeping guest (the call's own path is
    // covered by `seam_net_answers`): only the LAN's due cycle can end the
    // guest's sleep, since nothing else is scheduled.
    assert!(lan.connect(me, m.cycles(), b"home", HOME_PASSWORD));
    let join = m.cycles() + 10 * MS;
    run_for(&mut m, 10 * MS + MS / 2);
    assert_eq!(woken(&mut m), 1, "woken by the join, within the run");
    assert!(lan.link_up(me));
    assert!(m.cycles() >= join, "{} < {join}", m.cycles());
    assert!(m.harts[0].is_wfi(), "and back asleep");
}

#[test]
fn a_runner_lan_raises_the_wake_from_the_runners_boundary() {
    use lp_emu_esp32c6::lockstep::Lockstep;
    use lp_emu_esp32c6::machine::StopCondition;
    let lan = SharedLan::new(fixture_lan(), LanDriver::Runner);
    let a = board_at(&lan, true, 0, MAC);
    let b = board_at(&lan, true, 1, MAC_B);
    let mut pair = Lockstep::new(vec![a, b])
        .unwrap()
        .with_medium(Box::new(lan.clone()));
    let me = pair.machine(ParticipantId(1)).unwrap().net_endpoint_id();
    assert!(lan.connect(me, 0, b"home", HOME_PASSWORD));
    pair.run_until(12 * MS, &StopCondition::default());
    let m = pair.machine_mut(ParticipantId(1)).unwrap();
    assert_eq!(m.peek_word(WOKEN), Some(1), "board 1 heard its join");
    let other = pair.machine_mut(ParticipantId(0)).unwrap();
    assert_eq!(other.peek_word(WOKEN), Some(0), "board 0 asked for nothing");
}

/// One millisecond of guest time at 160 MHz.
const MS: u64 = 160_000;
const MAC: [u8; 6] = [0x02, 0x4c, 0x50, 0x00, 0x00, 0x01];
const MAC_B: [u8; 6] = [0x02, 0x4c, 0x50, 0x00, 0x00, 0x02];

/// Let a raise the pacer had already decided on land before the test looks.
fn settle(m: &mut Esp32C6Machine) {
    run_for(m, MS);
}

fn woken(m: &mut Esp32C6Machine) -> u32 {
    m.peek_word(WOKEN).unwrap()
}

fn board(lan: &SharedLan, sleeper: bool) -> Esp32C6Machine {
    board_at(lan, sleeper, 0, MAC)
}

fn board_at(lan: &SharedLan, sleeper: bool, index: usize, mac: [u8; 6]) -> Esp32C6Machine {
    let page = net_core_page(NetPage {
        pending: PENDING,
        sleeper,
        ..NetPage::default()
    });
    let builder = Esp32C6Builder::new()
        .efuse(EfuseIdentity {
            mac,
            ..EfuseIdentity::default()
        })
        .seams(SeamRequest::default())
        .lan(lan.clone(), ParticipantId(index));
    let mut m = machine(chip(&[(CORE_A, &page)]), builder);
    bind_wake(&mut m);
    boot_core(&mut m, CORE_A);
    m
}
