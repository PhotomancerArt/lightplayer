//! RESEARCH (research/ram-e03): nothing the shipped C6 runs from the reset
//! vector writes LP SRAM above the recovery region.
//!
//! The question behind it: can the free part of LP SRAM (everything above
//! `.rtc_fast.persistent`) be a heap region, or does something already use it
//! that no linker section names — the mask ROM, the ESP-IDF second-stage
//! bootloader, the split image's loader, the core's startup, or the radios'
//! ROM calls? The ELF answers for the firmware's own sections; this answers
//! for code the ELF does not contain.
//!
//! How: the shipped split image boots ROM-up (the real mask ROM and
//! espflash 3.3.0's IDF bootloader, as `split_boot.rs` does) with every word
//! of LP SRAM first set to a sentinel, under each reset cause the ROM tells
//! apart, and runs past the engine's server-loop line — Bluetooth up and
//! advertising, the network seam's Wi‑Fi up. Then every word is read back.
//!
//! What it can and cannot say: the emulator runs the ROM's and the
//! bootloader's own instructions, so a store they make lands here exactly as
//! the model decodes it; but a ROM routine the emulator answers itself (a
//! hook) or a block it does not model (the PMU's sleep retention, the LP
//! core, deep-sleep wake — no reset cause for it is modelled) is not covered.
//! Emulated evidence, `lp-emu:esp32c6:t1`; never a silicon claim.
//!
//! `#[ignore]`d for the usual reason (`test_support`): it needs the shipped
//! split image (`just test-emu-c6`, or `LP_EMU_C6_SPLIT_<SLUG>`).

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, StopCondition, Uart0Sink, UsbHost,
};
use lp_emu_esp32c6::memmap::{LP_SRAM_BASE, LP_SRAM_LEN};
use lp_emu_esp32c6::test_support::{FwImage, skip_notice, split_image};

/// Past the engine's line (≈1.5 s on the ROM-up path) by a few seconds of
/// steady state, with the radios up.
const RUN_US: u64 = 6_000_000;

/// The engine's own line: printed by `lp_engine_entry`, after the door.
const ENGINE_LINE: &str = "[INIT] fw-esp32 initialized, starting server loop... proto=";

/// The recovery region's size: `.rtc_fast.persistent` in the shipped image,
/// the only LP SRAM section it links (`rust-objdump -h p2.elf`).
const RECOVERY_LEN: u32 = 0x400;

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn no_boot_path_writes_lp_sram_above_the_recovery_region() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(s) => s,
        Err(reason) => {
            skip_notice(
                "no_boot_path_writes_lp_sram_above_the_recovery_region",
                &reason,
            );
            return;
        }
    };
    let chip_len = std::fs::metadata(split.merged()).unwrap().len() as u32;
    for cause in [
        ResetCause::PowerOn,
        ResetCause::UsbUartHpSys,
        ResetCause::LpSwHpSys,
        ResetCause::LpWdtSys,
    ] {
        let mut m = Esp32C6Builder::new()
            .boot_mode(BootMode::RomUp)
            .app(AppSource::Path(split.p2_elf()))
            .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(split.merged()))
            .flash_len(chip_len)
            .reset_cause(cause)
            .strap(Strap::App)
            .uart0(Uart0Sink::Memory)
            .usb_host(UsbHost::Attached { draining: true })
            .build()
            .expect("the ROM-up machine builds");
        let words = LP_SRAM_LEN / 4;
        for i in 0..words {
            assert!(m.poke_word(LP_SRAM_BASE + 4 * i, sentinel(i)));
        }
        m.run_until(&StopCondition::after_micros(RUN_US));
        assert!(
            m.usb_sj().text().contains(ENGINE_LINE),
            "{cause:?}: the image never reached the engine's server loop"
        );

        let mut changed_low = 0u32;
        let mut changed_high = Vec::new();
        for i in 0..words {
            let at = LP_SRAM_BASE + 4 * i;
            let word = m.peek_word(at).expect("LP SRAM is mapped");
            if word != sentinel(i) {
                if 4 * i < RECOVERY_LEN {
                    changed_low += 1;
                } else {
                    changed_high.push((at, word));
                }
            }
        }
        println!(
            "{cause:?}: {changed_low} of {} recovery-region words written, {} of {} words above it",
            RECOVERY_LEN / 4,
            changed_high.len(),
            words - RECOVERY_LEN / 4
        );
        assert!(
            changed_high.is_empty(),
            "{cause:?}: something wrote LP SRAM above the recovery region: first {:x?}",
            &changed_high[..changed_high.len().min(8)]
        );
    }
}

/// A word no boot path would write by accident: its own index under a tag.
fn sentinel(i: u32) -> u32 {
    0xA5E0_0000 | i
}
