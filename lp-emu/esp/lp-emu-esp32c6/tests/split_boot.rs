//! The split image boots: the ESP32-C6 product image `lp-fw-split` lays out
//! — a RAM-only loader, boot records, a core and an engine inside `factory`
//! — from the reset vector, and the fast path that stands in for the
//! bootloader agrees with it.
//!
//! - **The ROM-up boot, through both front doors.** The whole merged image
//!   in the chip, the hart at the mask ROM's reset vector, nothing placed.
//!   Through **our door** (`merged.bin`) the ROM loads LightPlayer's own
//!   second stage from `0x0`, which does the loader's job itself; through
//!   **the IDF door** (`merged-idf.bin`, the same `app.bin`) the ROM loads
//!   the IDF bootloader the build def names, which loads the factory loader
//!   as its app. Either way the loader's logic reads the boot records, maps the core
//!   and copies its RAM segments; the core starts the radios and links and
//!   opens the engine. Asserted on the board's own words — each door's line,
//!   the loader's ROM print, the core's first line, the engine's server-loop
//!   line — and on the cache MMU page, which is the one the layout is aligned
//!   to (a second stage that picked another would fail here).
//! - **Direct load over the flashed image.** The same chip, the hart at the
//!   loader's entry in the state the bootloader leaves
//!   (`Esp32C6Builder::flash_holds_image`, 32 KiB pages): the loader does the
//!   rest as guest code. Compared with the ROM-up boot at the core's first
//!   console line, the way M7 compared a direct load with ROM-up at the
//!   app's (`rom_up_boot.rs`): the cache MMU's table and page size, every
//!   byte of the second link's segments, and then what the firmware says,
//!   line for line, through the engine's server-loop line.
//!
//! What the two paths are documented to disagree about (`loader.rs`, "What
//! this does NOT reproduce") is excluded by name, not by widening: the ROM's
//! console exists only on the ROM-up path, so the loader's line is asserted
//! there alone; the reset cause differs, so `[RECOVERY]` lines are left out
//! of the console comparison; and the binary lp-link frames that share the
//! port are not console lines (see `core_lines`).
//!
//! The engine's hello and the core's boot-state line ride lp-link, which
//! only a product crate may host (the MIT fence): those are
//! `lp-cli/tests/emu_split_boot.rs`.
//!
//! `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink, UsbHost,
};
use lp_emu_esp32c6::test_support::{FwImage, SplitImage, skip_notice, split_image};

/// Long enough for the ROM, the bootloader, the loader and the core's whole
/// bring-up to the engine's server loop (about 0.6 s of it is the
/// bootloader's segment loads on the ROM-up path).
const GATE_US: u64 = 3_000_000;

/// The engine's own line: printed by `lp_engine_entry`, after the door.
const ENGINE_LINE: &str = "[INIT] fw-esp32 initialized, starting server loop... proto=";

/// The core's first line, and where the two paths are compared.
const CORE_FIRST: &str = "[INIT] Initializing board";

/// The cache MMU page the layout is aligned to, and the one espflash
/// 3.3.0's bundled bootloader (and its quiet rebuild) picks on a 4 MB C6.
/// A bootloader that picked another would fail here.
const PAGE: u32 = 0x8000;

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn the_split_image_boots_from_the_reset_vector_to_the_engine() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(s) => s,
        Err(reason) => {
            skip_notice(
                "the_split_image_boots_from_the_reset_vector_to_the_engine",
                &reason,
            );
            return;
        }
    };
    for door in [Door::Ours, Door::Idf] {
        boots_to_the_engine(&split, door);
    }
}

/// The two second stages a C6 may carry at flash `0x0`.
#[derive(Clone, Copy, Debug)]
enum Door {
    /// LightPlayer's own (`merged.bin`).
    Ours,
    /// ESP-IDF's, starting the factory loader (`merged-idf.bin`).
    Idf,
}

fn boots_to_the_engine(split: &SplitImage, door: Door) {
    let mut m = rom_up_door(split, door, true);
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US).exit_on(ENGINE_LINE));
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{door:?}: the split image never reached the engine's server loop: {outcome:?}\n{}",
        consoles(&m)
    );
    assert!(
        m.bus.first_strict_violation().is_none(),
        "{door:?}: {:?}",
        m.bus.first_strict_violation()
    );

    let rom_console = m.uart0().text();
    let entry = rom_console
        .find("entry 0x")
        .unwrap_or_else(|| panic!("{door:?}: the ROM started the second stage:\n{rom_console}"));
    let loader = rom_console
        .find("[LOADER]")
        .unwrap_or_else(|| panic!("{door:?}: the loader's logic ran:\n{rom_console}"));
    assert!(
        entry < loader,
        "{door:?}: the loader ran before the ROM's entry:\n{rom_console}"
    );
    // Which second stage ran: ours says so in one line; the IDF's (whichever
    // the build def names — a quiet build says nothing) never does.
    let ours = rom_console.contains("[LPBOOT] LightPlayer bootloader v1");
    match door {
        Door::Ours => assert!(ours, "our bootloader, not the IDF's, ran:\n{rom_console}"),
        Door::Idf => assert!(!ours, "the IDF door ran our bootloader:\n{rom_console}"),
    }
    assert!(
        rom_console.contains("[LOADER] core @0x18000 (proven)"),
        "the loader chose the flashed core:\n{rom_console}"
    );
    let usb = m.usb_sj().text();
    let core_at = usb.find(CORE_FIRST).expect("the core's first line");
    let engine_at = usb.find(ENGINE_LINE).expect("the engine's line");
    assert!(
        core_at < engine_at,
        "the core ran before the engine:\n{usb}"
    );
    assert_eq!(
        m.cache().lock().unwrap().page_len(),
        PAGE,
        "the bootloader chose the page the split layout is aligned to"
    );
    println!(
        "split ROM-up ({door:?} door): loader → core → engine in {} cycles; {} cache page(s) mapped",
        m.cycles(),
        m.cache().lock().unwrap().mapped_pages().len()
    );
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn a_direct_load_over_the_flashed_image_agrees_with_rom_up() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(s) => s,
        Err(reason) => {
            skip_notice(
                "a_direct_load_over_the_flashed_image_agrees_with_rom_up",
                &reason,
            );
            return;
        }
    };
    let mut rom_up = rom_up(&split, false);
    let mut direct = Esp32C6Builder::new()
        .app(AppSource::Path(split.loader_elf()))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(split.merged()))
        .flash_len(chip_len(&split))
        .flash_holds_image(true)
        .mmu_page_len(PAGE)
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the direct machine builds");
    assert_eq!(
        direct.flash_staging().mismatched_bytes,
        0,
        "the loader ELF's flash-resident sections are the flashed image's bytes"
    );

    let first = StopCondition::after_micros(GATE_US).exit_on("[INIT] Board initialized");
    for (name, m) in [("ROM-up", &mut rom_up), ("direct", &mut direct)] {
        let outcome = m.run_until(&first);
        assert!(
            matches!(outcome, Outcome::ExitMatched { .. }),
            "the {name} boot never reached the core's first lines: {outcome:?}\n{}",
            consoles(m)
        );
    }

    // 1. The cache MMU, entry for entry: the loader mapped the core the same
    //    way whichever way it was started, at the page the bootloader chose.
    let table = |m: &Esp32C6Machine| {
        let mmu = m.cache().lock().unwrap();
        (mmu.page_len(), mmu.mapped_pages())
    };
    let (rom_page, rom_pages) = table(&rom_up);
    let (direct_page, direct_pages) = table(&direct);
    assert_eq!(rom_page, PAGE, "the ROM-up bootloader's page");
    assert_eq!(direct_page, rom_page, "the direct load's page");
    assert_eq!(
        direct_pages, rom_pages,
        "the loader left a different MMU table depending on how it was started"
    );

    // 2. Every byte of the second link's segments the core can see, in RAM
    //    and through the window.
    let p2 = lp_emu_esp_common::ElfImage::parse(&std::fs::read(split.p2_elf()).unwrap())
        .expect("p2.elf parses");
    let mut compared = 0usize;
    for seg in p2.segments.iter().filter(|s| !s.data.is_empty()) {
        let (Some(a), Some(b)) = (
            read_span(&rom_up, seg.vaddr, seg.data.len() as u32),
            read_span(&direct, seg.vaddr, seg.data.len() as u32),
        ) else {
            panic!(
                "segment {:#010x}+{:#x} is not readable on both machines",
                seg.vaddr,
                seg.data.len()
            );
        };
        if let Some(at) = a.iter().zip(&b).position(|(x, y)| x != y) {
            panic!(
                "segment {:#010x} differs at +{at:#x}: ROM-up {:#04x}, direct {:#04x}",
                seg.vaddr, a[at], b[at]
            );
        }
        compared += a.len();
    }
    assert!(compared > 2_000_000, "only {compared} bytes compared");

    // 3. What the firmware says, from the core's first line through the
    //    engine's, line for line.
    let to_engine = StopCondition::after_micros(GATE_US).exit_on(ENGINE_LINE);
    for (name, m) in [("ROM-up", &mut rom_up), ("direct", &mut direct)] {
        let outcome = m.run_until(&to_engine);
        assert!(
            matches!(outcome, Outcome::ExitMatched { .. }),
            "the {name} boot never reached the engine: {outcome:?}"
        );
    }
    let ours = core_lines(&rom_up);
    let theirs = core_lines(&direct);
    assert!(
        ours.len() > 10,
        "the ROM-up core printed almost nothing: {ours:#?}"
    );
    assert_eq!(
        ours, theirs,
        "the core and engine say different things depending on how the loader was started"
    );
    println!(
        "split parity: {} MMU entries, {compared} bytes and {} console lines equal",
        rom_pages.len(),
        ours.len()
    );
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn a_core_segment_at_address_0_faults_the_loader_as_on_silicon() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(s) => s,
        Err(reason) => {
            skip_notice(
                "a_core_segment_at_address_0_faults_the_loader_as_on_silicon",
                &reason,
            );
            return;
        }
    };
    // The shipped chip, with the core's first segment (a RAM one) re-aimed
    // at address 0: what espflash's padding segment is, and what the loader
    // copied to address 0 on a board, where the store faulted
    // (docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md).
    // Not strict: the fault is the chip's, not the checker's.
    let mut chip = std::fs::read(split.merged()).unwrap();
    let first_segment = CORE_OFFSET + 24;
    let addr = u32::from_le_bytes(chip[first_segment..first_segment + 4].try_into().unwrap());
    assert!(
        (0x4080_0000..0x4088_0000).contains(&addr),
        "the core's first segment is a RAM one: {addr:#x}"
    );
    chip[first_segment..first_segment + 4].copy_from_slice(&0u32.to_le_bytes());
    let len = chip.len() as u32;
    let mut m = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Bytes(chip))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the ROM-up machine builds");
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US).exit_on(CORE_FIRST));
    assert!(
        !matches!(outcome, Outcome::ExitMatched { .. }),
        "the core started over an address-0 segment: {outcome:?}\n{}",
        consoles(&m)
    );
    assert!(!m.usb_sj().text().contains(CORE_FIRST), "{}", consoles(&m));
    assert!(
        !m.uart0().text().contains("[LOADER] core @0x18000 (proven)"),
        "the loader said it booted the core:\n{}",
        consoles(&m)
    );
    assert!(
        m.bus.unmapped_writes() >= 1,
        "the loader's store to 0 was seen"
    );
    // Silicon's words for it (CX1, 2026-10-14): `Guru Meditation Error:
    // Core 0 panic'ed (Store/AMO access fault)`, then the registers.
    let rom = m.uart0().text();
    let said: Vec<&str> = rom
        .lines()
        .filter(|l| l.contains("Guru Meditation") || l.starts_with("PC ") || l.contains("MCAUSE"))
        .collect();
    assert!(
        rom.contains("Store/AMO access fault"),
        "the ROM named no store fault:\n{rom}"
    );
    println!(
        "address-0 segment: the loader faulted ({outcome:?}); {} unmapped store(s):\n{}",
        m.bus.unmapped_writes(),
        said.join("\n")
    );
}

/// Where the flashed image puts the core (`lp_bootctl::REGION_START`).
const CORE_OFFSET: usize = 0x1_8000;

/// The ROM-up machine: the merged image in the chip, nothing placed, the
/// second link as the symbol table.
fn rom_up(split: &SplitImage, strict: bool) -> Esp32C6Machine {
    rom_up_door(split, Door::Ours, strict)
}

fn rom_up_door(split: &SplitImage, door: Door, strict: bool) -> Esp32C6Machine {
    let chip = match door {
        Door::Ours => split.merged(),
        Door::Idf => split.merged_idf(),
    };
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(chip))
        .flash_len(chip_len(split))
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .strict(strict)
        .build()
        .expect("the ROM-up machine builds")
}

fn chip_len(split: &SplitImage) -> u32 {
    std::fs::metadata(split.merged()).unwrap().len() as u32
}

/// The USB console's text lines from the core's first, minus the lines the
/// two paths are supposed to disagree about.
///
/// Past the io thread's start the port also carries binary lp-link frames
/// (the link's handshake, NUL-delimited), which this crate cannot decode —
/// the MIT fence — and which are not console lines: a line keeps only what
/// follows its last NUL, and only text that opens with `[` is a line.
fn core_lines(m: &Esp32C6Machine) -> Vec<String> {
    m.usb_sj()
        .text()
        .replace('\r', "\n")
        .split('\n')
        .map(|l| l.rsplit('\0').next().unwrap_or(l).trim_end())
        .filter(|l| l.starts_with('['))
        .skip_while(|l| !l.starts_with(CORE_FIRST))
        .filter(|l| !PATH_DEPENDENT.iter().any(|p| l.contains(p)))
        .map(str::to_string)
        .collect()
}

/// Both consoles, for a failure message.
fn consoles(m: &Esp32C6Machine) -> String {
    format!(
        "--- uart0 ---\n{}\n--- usb ---\n{}",
        m.uart0().text(),
        m.usb_sj().text()
    )
}

/// `len` bytes out of whichever RAM region holds `address`, or `None`.
fn read_span(m: &Esp32C6Machine, address: u32, len: u32) -> Option<Vec<u8>> {
    m.bus
        .regions()
        .iter()
        .find(|r| r.contains(address) && r.contains(address + len - 1))
        .map(|region| {
            let at = (address - region.base) as usize;
            m.bus.region_bytes(region)[at..at + len as usize].to_vec()
        })
}

/// The recovery ledger records how the chip was reset: a ROM-up boot after
/// a serial reset is a user reset, a direct load asserts a power-on.
const PATH_DEPENDENT: &[&str] = &["[RECOVERY]"];
