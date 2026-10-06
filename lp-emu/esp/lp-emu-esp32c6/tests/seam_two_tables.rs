//! Two tables in flash is normal (FD3): after an update a chip holds two
//! cores, each with its own seam table, linked at the same address. Only the
//! one the live cache MMU maps arms. Remap to the other and restart: the
//! other arms, and nothing of the first start survives.

mod seam_guest;

use lp_emu_esp_common::Strap;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::Esp32C6Builder;
use seam_guest::*;

#[test]
fn only_the_mapped_core_arms_and_a_restart_follows_the_mapping() {
    let page = core_page(Tables::ALL, &[spin()], &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page), (CORE_B, &page)]),
        Esp32C6Builder::new()
            .reboot_on_reset(true)
            .seams(SeamRequest::strict("led=fast").unwrap()),
    );
    assert_eq!(
        m.seams().scan.as_ref().unwrap().candidates().count(),
        2,
        "both tables are candidates; neither is refused as ambiguous"
    );

    boot_core(&mut m, CORE_A);
    let e = m.seams().engaged.as_ref().expect("engaged");
    assert_eq!(e.table.offset, CORE_A + (TABLE - VBASE), "the live table");
    assert_eq!(m.seams().sites[0].paddr, Some(CORE_A + (WAIT - VBASE)));
    assert!(m.seams().sites[0].armed);

    assert!(m.power_cycle(Strap::App));
    assert!(!m.seams().engaged(), "a restart forgets the last start");
    assert!(m.seams().waiting_for_app, "and waits for the app again");
    boot_core(&mut m, CORE_B);
    let e = m.seams().engaged.as_ref().expect("engaged again");
    assert_eq!(e.table.offset, CORE_B + (TABLE - VBASE), "the other core's");
    assert_eq!(m.seams().sites[0].paddr, Some(CORE_B + (WAIT - VBASE)));
    assert!(m.seams().sites[0].armed);
    assert_eq!(m.seams().scans, 2, "one scan per chip start");
}
