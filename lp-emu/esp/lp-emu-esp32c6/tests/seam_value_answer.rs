//! A value seam answers through the registers (FD6's emulator half): a guest
//! calls `test_echo(1, 2, 4)` through its seam function, and with
//! `test=echo` engaged the answer in `a0` is the emulator's, not silicon's
//! `7`; with no seam asked for it is `7`.
//!
//! The real-code-generation half — the same call through the generated shim
//! on the `release-esp32` LTO build — is `seam_abi_harness.rs`.

#![cfg(feature = "test-seams")]

mod seam_guest;

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::{Esp32C6Builder, StopCondition};
use lp_emu_esp32c6::seams::TEST_ECHO_MARK;
use seam_guest::reg::*;
use seam_guest::*;

#[test]
fn an_engaged_value_seam_answers_in_a0() {
    let m = run(SeamRequest::strict("test=echo").unwrap());
    assert_eq!(
        m.0,
        TEST_ECHO_MARK | 7,
        "the emulator's answer, not silicon's"
    );
    assert_eq!(m.1, 1, "one call answered");
}

#[test]
fn seam_off_the_silicon_body_runs() {
    let m = run(SeamRequest::none());
    assert_eq!(m.0, 7, "a ^ b ^ c, from the real body");
    assert_eq!(m.1, 0);
}

/// `(a0 after the call, seam calls answered)`.
fn run(request: SeamRequest) -> (u32, u64) {
    let mut main = Vec::new();
    main.extend(li(A0, 1));
    main.extend(li(A1, 2));
    main.extend(li(A2, 4));
    let at = MAIN + 4 * main.len() as u32;
    main.push(jal(RA, ECHO.wrapping_sub(at) as i32));
    main.push(spin());
    let page = core_page(Tables::ALL, &main, &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().seams(request),
    );
    boot_core(&mut m, CORE_A);
    m.run_until(&StopCondition::after_micros(200));
    (m.harts[0].regs()[A0 as usize] as u32, m.seams().calls)
}
