//! A refill that erases a seam's patch re-arms it at the next boundary.
//!
//! The patch lives in the cache window, and the window is refilled from flash
//! whenever the MMU or the flash under a mapped page moves — which silently
//! erases it. The boundary after the refill plants it again: `rearms == 1`.

mod seam_guest;

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::{Esp32C6Builder, StopCondition};
use seam_guest::*;

#[test]
fn an_invalidated_page_is_refilled_and_its_site_planted_again() {
    let page = core_page(Tables::ALL, &[spin()], &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().seams(SeamRequest::strict("led=fast").unwrap()),
    );
    boot_core(&mut m, CORE_A);
    assert_eq!(m.seams().arms_planted, 1, "armed when the app started");
    assert!(m.seams().sites[0].armed);
    assert_eq!(m.seams().sites[0].paddr, Some(CORE_A + (WAIT - VBASE)));

    m.run_until(&StopCondition::after_micros(100));
    m.cache()
        .lock()
        .unwrap()
        .invalidate_page_at(CORE_A + (WAIT - VBASE));
    m.run_until(&StopCondition::after_micros(200));

    let s = m.seams();
    assert_eq!(
        s.rearms, 1,
        "the refill erased the patch and it was planted again"
    );
    assert_eq!(s.arms_planted, 2);
    assert!(s.sites[0].armed);
    assert_eq!(m.snapshot().seam_arms, 2, "the snapshot carries the count");
}

#[test]
fn a_page_that_holds_another_function_is_never_patched() {
    // The entry names WAIT, but the bytes there carry no hint: a mapping that
    // put another function at a seam's address. Nothing is planted.
    let mut page = core_page(Tables::ALL, &[spin()], &[]);
    put(
        &mut page,
        WAIT,
        &[seam_guest::addi(10, 0, 5), jalr(0, 1, 0)],
    );
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().seams(SeamRequest::strict("led=fast").unwrap()),
    );
    boot_core(&mut m, CORE_A);
    assert!(m.seams().engaged(), "the table engaged");
    assert_eq!(m.seams().arms_planted, 0, "but no hint, so no patch");
    assert!(!m.seams().sites[0].armed);
}
