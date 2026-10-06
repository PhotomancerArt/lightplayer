//! Two machines in one process, built independently, share no seam state:
//! no static anywhere in the seam path (FD9). Each has its own endpoint, its
//! own queue and its own counters, and a board's name makes its endpoint id
//! distinct from another's.

#![cfg(feature = "test-seams")]

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId, SeamRequest};
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine};
use seam_guest::*;

#[test]
fn two_machines_hold_two_of_everything() {
    let mut a = board();
    let mut b = board();
    b.set_seam_board(ParticipantId(7));
    let ea = a.seam_endpoints()[0].id;
    let eb = b.seam_endpoints()[0].id;
    assert_ne!(ea, eb, "endpoint ids differ by board");
    assert_eq!(ea.to_string(), "0/test");
    assert_eq!(eb.to_string(), "7/test");

    a.seam_endpoint_mut(ea)
        .unwrap()
        .push_inbound(EndpointEvent {
            at: 0,
            bytes: vec![1, 2, 3, 4],
        })
        .unwrap();
    assert_eq!(a.seam_endpoint(ea).unwrap().inbound_len(), 1);
    assert_eq!(
        b.seam_endpoint(eb).unwrap().inbound_len(),
        0,
        "b's queue is b's"
    );
    assert!(
        b.seam_endpoint(EndpointId {
            board: ParticipantId(0),
            seam: "test"
        })
        .is_none(),
        "b holds no endpoint of a's"
    );
    assert_eq!(a.seams().arms_planted, b.seams().arms_planted);
    assert_eq!(a.seams().starts, 1);
    assert_eq!(b.seams().starts, 1, "each machine counted its own start");
}

fn board() -> Esp32C6Machine {
    let page = core_page(Tables::ALL, &[spin()], &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().seams(SeamRequest::strict("test=take").unwrap()),
    );
    boot_core(&mut m, CORE_A);
    m
}
