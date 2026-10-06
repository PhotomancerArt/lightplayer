//! A switch-shape seam's engaged byte (FD7): a byte in flash `.rodata` that
//! reads 0 with the seam off, 1 with it on, and is still 1 after a forced
//! refill of its page — the patch lives in the cache window, and a fill from
//! flash erases it, so arming plants it again in the same boundary.

#![cfg(feature = "test-seams")]

mod seam_guest;

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::{Esp32C6Builder, StopCondition};
use seam_guest::reg::*;
use seam_guest::*;

#[test]
fn the_engaged_byte_reads_zero_off_and_one_on_and_survives_a_refill() {
    let (off, _) = run(SeamRequest::none(), false);
    assert_eq!(off, 0, "seam off: the image's byte");
    let (on, m) = run(SeamRequest::strict("test=take").unwrap(), false);
    assert_eq!(on, 1, "engaged: the window's byte is 1");
    assert_eq!(m.seams().arms_planted, 2, "the take entry and its byte");
    let (refilled, m) = run(SeamRequest::strict("test=take").unwrap(), true);
    assert_eq!(
        refilled, 1,
        "still 1 after the page was refilled from flash"
    );
    assert_eq!(m.seams().rearms, 2, "both sites planted again");
    // The flash chip itself was never written: the bootloader's hash covers it.
    let byte = m
        .flash()
        .lock()
        .unwrap()
        .peek(CORE_A + (ENGAGED - VBASE), 1)
        .unwrap()[0];
    assert_eq!(byte, 0);
}

/// The guest reads the byte in a loop; `refill` invalidates its page halfway.
fn run(request: SeamRequest, refill: bool) -> (u32, lp_emu_esp32c6::Esp32C6Machine) {
    let mut main = Vec::new();
    main.extend(li(T0, ENGAGED));
    let top = MAIN + 4 * main.len() as u32;
    main.push(lbu(A0, T0, 0));
    main.push(jal(0, top.wrapping_sub(top + 4) as i32));
    let page = core_page(Tables::ALL, &main, &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().seams(request),
    );
    boot_core(&mut m, CORE_A);
    m.run_until(&StopCondition::after_micros(100));
    if refill {
        m.cache()
            .lock()
            .unwrap()
            .invalidate_page_at(CORE_A + (ENGAGED - VBASE));
        m.run_until(&StopCondition::after_micros(200));
    }
    (m.harts[0].regs()[A0 as usize] as u32, m)
}
