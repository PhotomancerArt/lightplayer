//! Seam off is today's machine (A-1): with no seam asked for, the flash is
//! never scanned, no site exists, nothing is patched, and the snapshot's
//! seam counters are zero — even on an image that carries a table.

mod seam_guest;

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::{Esp32C6Builder, StopCondition};
use seam_guest::reg::*;
use seam_guest::*;

#[test]
fn an_empty_request_scans_nothing_and_patches_nothing() {
    for request in [SeamRequest::default(), SeamRequest::none()] {
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
            Esp32C6Builder::bare().seams(request.clone()),
        );
        boot_core(&mut m, CORE_A);
        m.run_until(&StopCondition::after_micros(200));

        let s = m.seams();
        assert_eq!(s.scans, 0, "{request}: never scanned");
        assert!(s.scan.is_none());
        assert!(s.sites.is_empty());
        assert!(!s.waiting_for_app);
        assert!(m.take_seam_lines().is_empty(), "nothing announced");
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
}
