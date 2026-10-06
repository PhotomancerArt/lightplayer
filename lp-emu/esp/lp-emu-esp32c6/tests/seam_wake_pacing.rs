//! Pacing holds a busy guest's main loop under a flood (G0 rule (b)).
//!
//! The M0 spike's flood — inject again right after every take that returned
//! something — starved the render loop: a drain-until-empty consumer never
//! left its drain. Here the host does exactly that, 24 events after every
//! step in which the guest took anything, against a consumer that never
//! sleeps (a render loop, counting its passes in `s4`). With the pacer — one
//! raise outstanding, a minimum spacing, a bounded queue that refuses — the
//! main loop keeps at least half the passes it makes with no flood at all,
//! and the refusals are counted and reported, never grown into.

#![cfg(feature = "test-seams")]

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId, PacerConfig, SeamRequest};
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use seam_guest::reg::*;
use seam_guest::*;

const STEP_US: u64 = 10;
const RUN_US: u64 = 10_000;

#[test]
fn a_flood_after_every_take_does_not_starve_the_main_loop() {
    let (calm, _, calm_raised) = run(false);
    let (flooded, refused, raised) = run(true);
    println!(
        "main-loop passes over {RUN_US} us (emulated): {calm} calm, {flooded} flooded \
         ({raised} raises vs {calm_raised}; {refused} events refused at the bound)"
    );
    assert!(
        flooded * 2 >= calm,
        "the flood took more than half the main loop: {flooded} of {calm}"
    );
    assert!(raised > 0, "the flood was delivered");
    let most = RUN_US * 160 / 1_600; // one raise per 1,600 cycles at most
    assert!(
        raised <= most,
        "{raised} raises: the spacing held ({most} at most)"
    );
    assert!(refused > 0, "the bound refused, and counted it");
}

/// `(main-loop passes, events refused, raises)` over [`RUN_US`].
fn run(flood: bool) -> (u64, u64, u64) {
    let (main, isr) = wake_consumer(Idle::Busy);
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
        Esp32C6Builder::new()
            .seams(SeamRequest::strict("test=take").unwrap())
            .seam_pacing(PacerConfig {
                min_spacing: 1_600,
                queue_bound: 16,
                take_cap: 64,
            }),
    );
    bind_wake(&mut m);
    boot_core(&mut m, CORE_A);
    let id = EndpointId {
        board: ParticipantId(0),
        seam: "test",
    };
    let mut taken = 0u64;
    let mut seq = 0u32;
    let mut t = 0;
    push(&mut m, id, &mut seq, 1);
    while t < RUN_US {
        t += STEP_US;
        m.run_until(&StopCondition::after_micros(t));
        let now_taken = m.seam_endpoint(id).unwrap().taken_events();
        if flood && now_taken > taken {
            push(&mut m, id, &mut seq, 24);
        }
        taken = now_taken;
    }
    let passes = m.harts[0].regs()[S4 as usize] as u32 as u64;
    let refused = m.seam_endpoint(id).unwrap().refused();
    (passes, refused, m.seams().pacer.raised())
}

fn push(m: &mut Esp32C6Machine, id: EndpointId, seq: &mut u32, n: u32) {
    let now = m.cycles();
    let e = m.seam_endpoint_mut(id).unwrap();
    for _ in 0..n {
        let _ = e.push_inbound(EndpointEvent {
            at: now,
            bytes: seq.to_le_bytes().to_vec(),
        });
        *seq += 1;
    }
}
