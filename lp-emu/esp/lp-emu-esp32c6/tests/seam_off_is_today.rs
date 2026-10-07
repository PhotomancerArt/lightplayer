//! Seam off is today's machine (A-1): with no seam asked for (`--seams none`,
//! or every default pinned `real`), the flash is never scanned, no site
//! exists, nothing is patched, and the snapshot's seam counters are zero —
//! even on an image that carries a table.
//!
//! And the default, which now asks for `net=lan` softly (FD5), adds nothing
//! but one honest line on an image that cannot engage it: an image from
//! before the network seam (a table without its entries), or one with no
//! table at all, runs with `SEAM none engaged: …`, no endpoint, no LAN, no
//! patch and its label unchanged.

mod seam_guest;

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use seam_guest::reg::*;
use seam_guest::*;

#[test]
fn an_empty_request_scans_nothing_and_patches_nothing() {
    for request in [
        SeamRequest::none(),
        SeamRequest::strict("net=real").unwrap(),
    ] {
        let mut m = run_echo_and_wait(request.clone());
        let s = m.seams();
        assert_eq!(s.scans, 0, "{request}: never scanned");
        assert!(s.scan.is_none());
        assert!(s.sites.is_empty());
        assert!(!s.waiting_for_app);
        assert!(m.take_seam_lines().is_empty(), "nothing announced");
        assert_untouched(&mut m);
    }
}

#[test]
fn the_default_on_an_image_from_before_the_network_seam_says_so_and_runs_as_today() {
    // `Tables::ALL` is the foundation's test image: a table, but no `net_*`
    // entry in it.
    let mut m = run_echo_and_wait(SeamRequest::default());
    let s = m.seams();
    assert_eq!(s.scans, 1, "the default scans once per chip start");
    assert!(s.sites.is_empty());
    assert!(s.endpoints.is_empty());
    assert!(m.lan().is_none(), "no network");
    let lines = m.take_seam_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("SEAM none engaged: net=lan: ")
            && lines[0].contains("no entry for lp_seam_net_mac"),
        "{}",
        lines[0]
    );
    assert_untouched(&mut m);
}

#[test]
fn the_default_on_an_image_with_no_table_says_so_and_runs_as_today() {
    let mut main = Vec::new();
    main.extend(li(A0, 7));
    main.push(spin());
    let mut page = vec![0u8; PAGE];
    put(&mut page, MAIN, &main);
    let mut m = machine(chip(&[(CORE_A, &page)]), Esp32C6Builder::bare());
    let lines = m.take_seam_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("SEAM none engaged: no seam table in the image"),
        "{}",
        lines[0]
    );
    boot_core(&mut m, CORE_A);
    m.run_until(&StopCondition::after_micros(200));
    assert!(m.seams().sites.is_empty());
    assert!(m.lan().is_none());
    assert_eq!(m.harts[0].regs()[A0 as usize], 7);
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1");
    assert_eq!(m.snapshot().seam_calls, 0);
}

/// The foundation's image (`Tables::ALL`), whose main calls `test_echo` and
/// `ws281x_wait_step`, run for 200 µs under `request`.
fn run_echo_and_wait(request: SeamRequest) -> Esp32C6Machine {
    let mut main = Vec::new();
    main.extend(li(A0, 1));
    main.extend(li(A1, 2));
    main.extend(li(A2, 4));
    let at = MAIN + 4 * main.len() as u32;
    main.push(jal(RA, ECHO.wrapping_sub(at) as i32));
    let at = MAIN + 4 * main.len() as u32;
    main.push(jal(RA, WAIT.wrapping_sub(at) as i32));
    main.push(spin());
    let page = core_page(Tables::ALL, &main, &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().seams(request),
    );
    boot_core(&mut m, CORE_A);
    m.run_until(&StopCondition::after_micros(200));
    m
}

/// Nothing a seam does happened: no call, no patch, the silicon bodies ran,
/// the label is the grade's own.
fn assert_untouched(m: &mut Esp32C6Machine) {
    let snap = m.snapshot();
    assert_eq!((snap.seam_calls, snap.seam_arms), (0, 0));
    assert_eq!(m.harts[0].regs()[A0 as usize], 7, "the silicon body ran");
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1");
    // The wait seam's entry is still its own first instruction.
    let first = u32::from_le_bytes(
        m.flash()
            .lock()
            .unwrap()
            .peek(CORE_A + (WAIT - VBASE), 4)
            .unwrap()[..4]
            .try_into()
            .unwrap(),
    );
    assert_eq!(first, seam_guest::addi(0, 0, 1));
}
