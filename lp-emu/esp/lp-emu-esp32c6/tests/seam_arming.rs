//! Emulator seams M0: a seam's patch survives a cache refill (K2's second
//! half: "re-arm after any fill that covers it").
//!
//! The image is the spike's shipped image (`LP_M0_L1_ELF`, built from
//! `spike/emulator-seams` with default features). Direct load: armed at
//! build. Then the window page holding the seam is invalidated, which is
//! what an MMU write or a flash write under a mapped page does; the next
//! slice boundary refills it from flash — erasing the patch — and must plant
//! it again. `#[ignore]`: it needs that image.

use std::path::PathBuf;

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, StopCondition};

#[test]
#[ignore = "needs LP_M0_L1_ELF (the spike's shipped image)"]
fn a_refill_that_erases_the_patch_re_arms_it() {
    let Some(elf) = std::env::var_os("LP_M0_L1_ELF").map(PathBuf::from) else {
        eprintln!("seam_arming: skipped — set LP_M0_L1_ELF");
        return;
    };
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .seams(SeamRequest::parse("led=fast").unwrap())
        .build()
        .expect("a machine");
    let arm = m.seams().arms[0].clone();
    assert!(arm.armed, "armed at build on a direct load");
    assert_eq!(m.seams().arms_planted, 1);
    assert_eq!(m.peek_word(arm.vaddr & !3).map(|_| ()), Some(()));

    m.run_until(&StopCondition::after_micros(1_000));
    // What an MMU entry write or a flash write under the page does: the page
    // is refilled from flash at the next boundary, and flash has no patch.
    m.cache().lock().unwrap().invalidate_page_at(arm.paddr);
    m.run_until(&StopCondition::after_micros(2_000));

    let s = m.seams();
    assert_eq!(
        s.rearms, 1,
        "the refill erased the patch and it was planted again"
    );
    assert_eq!(s.arms_planted, 2);
    assert!(s.arms[0].armed);
    println!(
        "seam re-armed after a refill: planted at cycles {:?}",
        s.plant_cycles
    );
}
