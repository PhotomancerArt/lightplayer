//! Two boards wired together by a seam medium, in lockstep (FD9).
//!
//! Two synthetic machines, each with `test=take` engaged and a wake consumer
//! running, join a `LoopbackMedium` routing board A's `test` endpoint to
//! board B's and back. An event board A's guest "gave" (queued on A's
//! endpoint's outbound side, as a future give seam would) reaches board B's
//! guest buffer only after the medium's latency, and the reverse. The
//! lockstep runner is the deterministic driver: run twice, the two runs log
//! the same bytes at the same cycles.

#![cfg(feature = "test-seams")]

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId, LoopbackMedium, SeamRequest};
use lp_emu_esp32c6::lockstep::Lockstep;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use lp_emu_esp32c6::memmap;
use seam_guest::*;

const LATENCY: u64 = 200 * memmap::CYCLES_PER_US;

#[test]
fn an_event_crosses_the_medium_after_its_latency_both_ways_and_replays_identically() {
    let first = run();
    let second = run();
    assert_eq!(first, second, "lockstep is deterministic");
    let (a_log, b_log, b_arrived, a_arrived) = first;
    assert_eq!(b_log, [0xa0a0_0001], "A's event reached B's guest");
    assert_eq!(a_log, [0xb0b0_0001], "and B's reached A's");
    assert!(b_arrived >= LATENCY, "not before the latency: {b_arrived}");
    assert!(a_arrived >= LATENCY, "not before the latency: {a_arrived}");
}

/// `(A's log, B's log, cycle B first had it, cycle A first had it)`.
fn run() -> (Vec<u32>, Vec<u32>, u64, u64) {
    let a = board();
    let b = board();
    let ea = EndpointId {
        board: ParticipantId(0),
        seam: "test",
    };
    let eb = EndpointId {
        board: ParticipantId(1),
        seam: "test",
    };
    let medium = LoopbackMedium::new(LATENCY).route(ea, eb).route(eb, ea);
    let mut pair = Lockstep::with_latency(vec![a, b], LATENCY)
        .expect("a pair")
        .with_medium(Box::new(medium));
    give(pair.machine_mut(ParticipantId(0)).unwrap(), ea, 0xa0a0_0001);
    give(pair.machine_mut(ParticipantId(1)).unwrap(), eb, 0xb0b0_0001);

    let mut arrived = [0u64; 2];
    let quantum = pair.quantum();
    let mut horizon = 0;
    while horizon < 2 * LATENCY && arrived.contains(&0) {
        horizon += quantum;
        pair.run_until(horizon, &StopCondition::default());
        for (i, at) in arrived.iter_mut().enumerate() {
            let m = pair.machine_mut(ParticipantId(i)).unwrap();
            if *at == 0 && !logged(m).is_empty() {
                *at = m.cycles();
            }
        }
    }
    let mut machines = pair.into_machines();
    let a_log = logged(&mut machines[0]);
    let b_log = logged(&mut machines[1]);
    (a_log, b_log, arrived[1], arrived[0])
}

fn board() -> Esp32C6Machine {
    let (main, isr) = wake_consumer(Idle::Wfi);
    let page = core_page(
        Tables {
            pending: PENDING,
            ..Tables::ALL
        },
        &main,
        &isr,
    );
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::new().seams(SeamRequest::strict("test=take").unwrap()),
    );
    bind_wake(&mut m);
    boot_core(&mut m, CORE_A);
    m
}

/// What a guest's give seam would hand its endpoint.
fn give(m: &mut Esp32C6Machine, id: EndpointId, word: u32) {
    let now = m.cycles();
    m.seam_endpoint_mut(id)
        .expect("the board's endpoint, named by its seat")
        .push_outbound(EndpointEvent {
            at: now,
            bytes: word.to_le_bytes().to_vec(),
        });
}
