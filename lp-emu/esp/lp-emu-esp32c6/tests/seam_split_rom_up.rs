//! `--seams led=fast` on the shipped **split** image, booted ROM-up from the
//! reset vector (A-5, FD2, FD4): the seams wait for the app and arm only once
//! the hart runs from the flash window — the core — after the IDF bootloader
//! and the RAM-only loader are done, through the live MMU the loader
//! programmed, against the table the split tool rooted in the core.
//!
//! The K2 lesson, kept here because this is where it would regress: the IDF
//! bootloader **reads the app through the cache window** to checksum and
//! hash it, and the M0 spike's first ROM-up arm — planted as soon as the
//! window mapped the app's bytes — failed the boot with `esp_image: Checksum
//! failed`. Arming waits for the hart to execute from the window instead, so
//! the boot reaching the engine is itself the checksum passing.
//!
//! `#[ignore]`d: needs the split image (`LP_EMU_BUILD_FW=1`; `just
//! test-emu-c6`).

use lp_emu_esp_common::Strap;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Outcome, StopCondition, Uart0Sink, UsbHost,
};
use lp_emu_esp32c6::test_support::{FwImage, skip_notice, split_image};

const LOADER_LINE: &str = "[LOADER] core @";
const ENGINE_LINE: &str = "[INIT] fw-esp32 initialized, starting server loop... proto=";

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn led_fast_arms_on_the_split_image_only_after_the_core_starts() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(s) => s,
        Err(reason) => {
            skip_notice("led_fast_arms_on_the_split_image", &reason);
            return;
        }
    };
    let len = std::fs::metadata(split.merged()).unwrap().len() as u32;
    let mut m = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(split.merged()))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .seams(SeamRequest::strict("led=fast").unwrap())
        .build()
        .expect("a strict request builds: the merged image carries a table");
    assert!(m.seams().waiting_for_app, "ROM-up waits for the app");
    assert!(m.take_seam_lines().is_empty());

    let out = m.run_until(&StopCondition::after_micros(3_000_000).exit_on(ENGINE_LINE));
    assert!(
        matches!(out, Outcome::ExitMatched { .. }),
        "the boot reached the engine with the seam armed (the bootloader's checksum \
         passed): {out:?}\n{}",
        m.uart0().text()
    );
    let rom_console = m.uart0().text();
    let core_at = rom_console
        .split(LOADER_LINE)
        .nth(1)
        .and_then(|rest| rest.strip_prefix("0x"))
        .and_then(|rest| {
            let hex: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
            u32::from_str_radix(&hex, 16).ok()
        })
        .unwrap_or_else(|| panic!("the loader named the core it chose:\n{rom_console}"));
    let lines = m.take_seam_lines();
    let label = m.configuration_label();
    let s = m.seams();
    let started = s.app_started_at.expect("the core started");
    let armed = s.first_arm_at.expect("the wait seam armed");
    assert!(
        armed >= started,
        "armed only once the app ran from the window"
    );
    let e = s.engaged.as_ref().expect("engaged");
    assert!(
        e.table.self_addr < 0x4240_0000,
        "the live table is linked in the core, below the engine region"
    );
    assert!(
        e.table.offset > core_at,
        "and it sits in the core the loader chose (core @{core_at:#x}, table @{:#x})",
        e.table.offset
    );
    assert!(s.sites[0].armed);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("SEAM led=fast engaged (performance")),
        "{lines:?}"
    );
    println!(
        "split ROM-up: core @{core_at:#x}, app at {started}, armed at {armed}; \
         table at flash {:#x} (self {:#010x}); {}",
        e.table.offset, e.table.self_addr, label
    );
}
