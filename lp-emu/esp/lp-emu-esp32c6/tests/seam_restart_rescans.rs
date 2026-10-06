//! Seams resolve on every chip start (FD4), so a board created blank and then
//! flashed engages. This is Studio's end-user board: built with a blank chip
//! and a soft request, it says once why nothing engaged; after an image is
//! written into its flash (what `emu_flash_write` does) and the chip is
//! restarted, the seam engages.

mod seam_guest;

use lp_emu_esp_common::Strap;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::Esp32C6Builder;
use seam_guest::*;

#[test]
fn a_blank_board_says_why_once_then_engages_after_a_flash_and_a_restart() {
    let mut m = machine(
        vec![0xff; CHIP],
        Esp32C6Builder::new()
            .reboot_on_reset(true)
            .seams(SeamRequest::prefer("led=fast").unwrap()),
    );
    let lines = m.take_seam_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("SEAM none engaged: no seam table"),
        "{lines:?}"
    );
    assert!(!m.seams().engaged());
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1", "no atom");

    // An Update firmware: the image written into the chip, then a reset.
    let page = core_page(Tables::ALL, &[spin()], &[]);
    assert!(m.flash().lock().unwrap().stage(CORE_A, &page));
    assert!(m.power_cycle(Strap::App));
    assert!(
        m.take_seam_lines().is_empty(),
        "nothing to say until the app runs"
    );
    boot_core(&mut m, CORE_A);

    assert!(m.seams().engaged());
    assert!(m.seams().sites[0].armed);
    let lines = m.take_seam_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("SEAM led=fast engaged (performance, abi "),
        "{lines:?}"
    );
    assert!(
        lines[0].contains("lp_seam_ws281x_wait_step@0x42001200"),
        "{lines:?}"
    );
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1+led=fast");
}

#[test]
fn a_strict_request_on_a_blank_chip_fails_the_build() {
    let err = Esp32C6Builder::new()
        .seams(SeamRequest::strict("led=fast").unwrap())
        .build()
        .err()
        .expect("a strict request with no table cannot build");
    let text = err.to_string();
    assert!(
        text.contains("led=fast cannot engage: no seam table"),
        "{text}"
    );
}
