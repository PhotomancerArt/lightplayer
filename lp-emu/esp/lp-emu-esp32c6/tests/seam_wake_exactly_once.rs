//! The wake delivers every event exactly once, in order (FD8; the roadmap's
//! R-WAKE trigger #1 is what a failure here would be).
//!
//! A synthetic guest — no firmware: the firmware's wake handler ships with
//! the Bluetooth seam (U4) — binds a handler on `FROM_CPU_INTR3` that clears
//! the line and then swaps the pending word to zero, and a main loop that
//! drains `test_take` until it returns 0 and then sleeps in `wfi` with
//! interrupts masked. The host queues sequence numbers on the machine's
//! `test` endpoint under the M0 spike's adversarial schedules:
//!
//! - **steady**: one event a millisecond;
//! - **burst**: forty at once, every five milliseconds;
//! - **masked**: only at a boundary where the guest has `mstatus.MIE` clear;
//! - **locked**: inside a window where the matrix's threshold masks the wake
//!   line (a `Priority1` critical section), which then opens;
//! - **parked**: only while the hart is parked in `wfi`;
//! - **all** of them at once, and **idle**: none, and no raise at all.
//!
//! Every case must log exactly the sequence it was sent. A lost or doubled
//! event the pending-word protocol cannot explain is R-WAKE: stop and report,
//! never work around it here.

#![cfg(feature = "test-seams")]

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId, SeamRequest};
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use seam_guest::*;

const MS: u64 = 1_000;
const STEP_US: u64 = 50;
const RUN_US: u64 = 20 * MS;
const PLIC_THRESH: u32 = 0x2000_1090;

#[derive(Clone, Copy, Default)]
struct Schedule {
    steady: bool,
    burst: bool,
    masked: bool,
    locked: bool,
    parked: bool,
}

#[test]
fn steady() {
    exactly_once(
        "steady",
        Schedule {
            steady: true,
            ..Schedule::default()
        },
    );
}

#[test]
fn burst() {
    exactly_once(
        "burst",
        Schedule {
            burst: true,
            ..Schedule::default()
        },
    );
}

#[test]
fn masked() {
    exactly_once(
        "masked",
        Schedule {
            masked: true,
            ..Schedule::default()
        },
    );
}

#[test]
fn locked() {
    exactly_once(
        "locked",
        Schedule {
            locked: true,
            ..Schedule::default()
        },
    );
}

#[test]
fn parked() {
    exactly_once(
        "parked",
        Schedule {
            parked: true,
            ..Schedule::default()
        },
    );
}

#[test]
fn all() {
    exactly_once(
        "all",
        Schedule {
            steady: true,
            burst: true,
            masked: true,
            locked: true,
            parked: true,
        },
    );
}

#[test]
fn idle_raises_nothing() {
    let (mut m, sent) = run(Schedule::default());
    assert_eq!(sent, 0);
    assert_eq!(m.seams().pacer.raised(), 0, "no work, no raise");
    assert!(logged(&mut m).is_empty());
}

fn exactly_once(name: &str, schedule: Schedule) {
    let (mut m, sent) = run(schedule);
    let got = logged(&mut m);
    let want: Vec<u32> = (0..sent).collect();
    assert!(sent > 0, "{name}: the schedule sent nothing");
    assert_eq!(got, want, "{name}: every event exactly once, in order");
    let p = &m.seams().pacer;
    assert_eq!(p.raised(), p.consumed(), "{name}: every raise was consumed");
    for line in m.seam_wake_lines() {
        println!("{name}: {line}");
    }
}

/// Run the consumer for [`RUN_US`] under `schedule`; the machine and how
/// many events were sent.
fn run(schedule: Schedule) -> (Esp32C6Machine, u32) {
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
    let id = EndpointId {
        board: ParticipantId(0),
        seam: "test",
    };
    assert!(m.seam_endpoint(id).is_some(), "the endpoint exists");

    let mut seq = 0u32;
    let mut next_cond = 0u64;
    let mut unlock_at: Option<u64> = None;
    let mut t = 0u64;
    while t < RUN_US {
        t += STEP_US;
        m.run_until(&StopCondition::after_micros(t));
        let now = m.cycles();
        let mut send = 0u32;
        if schedule.steady && t % MS == 0 {
            send += 1;
        }
        if schedule.burst && t % (5 * MS) == 0 {
            send += 40;
        }
        let mie = m.harts[0].csr().mstatus & 8 != 0;
        let cond_due = t >= next_cond;
        if schedule.masked && cond_due && !mie {
            send += 1;
            next_cond = t + MS;
        } else if schedule.parked && cond_due && m.harts[0].is_wfi() {
            send += 1;
            next_cond = t + MS;
        }
        if schedule.locked && t % (4 * MS) == 0 && unlock_at.is_none() {
            assert!(
                m.poke_word(PLIC_THRESH, 2),
                "the wake line masked by threshold"
            );
            unlock_at = Some(t + 300);
            send += 1;
        }
        if unlock_at.is_some_and(|at| t >= at) {
            assert!(m.poke_word(PLIC_THRESH, 1));
            unlock_at = None;
        }
        let e = m.seam_endpoint_mut(id).unwrap();
        for _ in 0..send {
            e.push_inbound(EndpointEvent {
                at: now,
                bytes: seq.to_le_bytes().to_vec(),
            })
            .expect("under the bound");
            seq += 1;
        }
    }
    // Let the last events land.
    m.poke_word(PLIC_THRESH, 1);
    m.run_until(&StopCondition::after_micros(RUN_US + 2 * MS));
    (m, seq)
}
