//! M7's gates: the chip boots itself.
//!
//! One 4 MiB merged image goes into the flash chip, the hart starts at the
//! mask ROM's reset vector, and **nothing else is placed**. The real ROM
//! detects the part, reads the second-stage bootloader out of flash and
//! jumps to it; the real bootloader reads the partition table, hashes and
//! loads the app's segments, programs the cache MMU and jumps to `_start`.
//!
//! # What is compared against what
//!
//! DD45 rules that a reference image is not reproducible across builds and
//! hosts, so a boot log has two kinds of line in it and they get two
//! treatments:
//!
//! - **Image-independent lines** — the ROM banner, the SPI configuration,
//!   the bootloader's own version and banner, the partition table, `Loaded
//!   app from partition`, `Disabling RNG early entropy source` — are
//!   compared **literally** against the committed silicon transcript
//!   (`lp-emu/transcripts/esp32c6/boot-idle-flash/silicon-*.txt`), after
//!   masking the millisecond stamps and the monitor's own decorations.
//! - **Image-derived lines** — the ROM's `load:` lines and the bootloader's
//!   `esp_image: segment N` lines — are compared against the merged image
//!   **the machine was handed**, parsed independently by [`image`]. Gating
//!   those on the transcript would gate on the linker: the ELF this tree
//!   builds at the transcript's commit is not the one the desk flashed (its
//!   `.rodata` links 0x20 higher, so `espflash` splits the app into six
//!   segments where silicon's had five). The silicon values are printed
//!   beside ours when the comparison fails, so the difference is always
//!   visible even though it is not gated.
//!
//! The three bootloader `load:` lines and `entry` **are** compared
//! literally as well, and that is not an inconsistency: espflash 3.3.0's
//! bundled second-stage bootloader is a fixed binary, not a build of this
//! tree, so those four lines are the same on any host.
//!
//! `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::image::MergedImage;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink,
};
use lp_emu_esp32c6::test_support::{ReferenceImage, merged_image, reference_image, skip_notice};

/// Long enough for the ROM, the bootloader and the app's whole `[INIT]`
/// chain: the bootloader's own segment loads are the slow part, and on
/// silicon they take 585 ms of the 609 ms to `Loaded app`.
const GATE_US: u64 = 400_000;

/// The reference image both halves of the cross-check use: the shipped
/// feature set at the commit the silicon `boot-idle-flash` transcript was
/// captured from.
const IMAGE: ReferenceImage = ReferenceImage::SHIPPED_USB_SILICON;

/// Strip everything that is not the device's own bytes.
///
/// Three things are not: `\r`, the ANSI colour runs the IDF bootloader
/// wraps its lines in, and espflash's monitor decorations — it turns any
/// address it recognises into a two-line `0x… - symbol` / `    at ??:??`
/// annotation, which the board never sent.
fn device_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.replace('\r', "\n").split('\n') {
        let line = strip_ansi(raw);
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("    at ") {
            continue;
        }
        // `0x4080054c - swint_handler_trampoline`
        if let Some(rest) = trimmed.strip_prefix("0x")
            && rest.len() > 9
            && rest[..8].chars().all(|c| c.is_ascii_hexdigit())
            && rest[8..].starts_with(" - ")
        {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI: `[`, then parameter and intermediate bytes, then a final
        // byte in `@`..`~`. The `[` is itself in that range, so it has to
        // be consumed before the scan starts — which is the bug that left
        // a `0m` on the end of every bootloader line the first time.
        if chars.peek() == Some(&'[') {
            chars.next();
        }
        for c in chars.by_ref() {
            if ('\u{40}'..='\u{7e}').contains(&c) {
                break;
            }
        }
    }
    out
}

/// `I (608) boot: …` → `I (…) boot: …`. The stamp is grade-1 time on our
/// side and a real millisecond clock on silicon's; the plan gates neither.
fn mask_stamp(line: &str) -> String {
    let is_log = ["I (", "E (", "W (", "D (", "V ("]
        .iter()
        .any(|p| line.starts_with(p));
    if !is_log {
        return line.to_string();
    }
    match line.find(')') {
        Some(i) => format!("{}(\u{2026}){}", &line[..2], &line[i + 1..]),
        None => line.to_string(),
    }
}

/// The window of a boot log this milestone gates: the ROM's first line
/// through the bootloader's last.
const FIRST: &str = "ESP-ROM:esp32c6-";
const LAST: &str = "boot: Disabling RNG early entropy source...";

fn boot_window(lines: &[String]) -> Vec<String> {
    let start = lines
        .iter()
        .position(|l| l.starts_with(FIRST))
        .unwrap_or_else(|| panic!("no `{FIRST}` line in:\n{}", lines.join("\n")));
    let end = lines
        .iter()
        .position(|l| l.contains(LAST))
        .unwrap_or_else(|| panic!("no `{LAST}` line in:\n{}", lines[start..].join("\n")));
    lines[start..=end].iter().map(|l| mask_stamp(l)).collect()
}

/// Is this line derived from the image rather than from the chip?
fn is_image_derived(line: &str) -> bool {
    line.contains("esp_image: segment ")
}

struct Run {
    machine: Esp32C6Machine,
    outcome: Outcome,
}

fn boot(strap: Strap, cause: ResetCause) -> Result<Run, String> {
    let merged = merged_image(&IMAGE)?;
    let elf = reference_image(&IMAGE)?;
    let len = std::fs::metadata(&merged)
        .map_err(|e| format!("{}: {e}", merged.display()))?
        .len() as u32;
    let mut machine = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        // The ELF is a symbol table and a cross-check reference here; the
        // bytes the machine runs come out of the chip.
        .app(AppSource::Path(elf))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .reset_cause(cause)
        .strap(strap)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        .strict(true)
        .build()
        .map_err(|e| e.to_string())?;
    let outcome = machine.run_until(&StopCondition::after_micros(GATE_US));
    Ok(Run { machine, outcome })
}

/// G7-1: the ROM banner, the SPI configuration, the bootloader's log and the
/// partition table, line for line against silicon.
#[test]
#[ignore = "needs a fw-esp32c6 build; `just test-emu-c6`"]
fn the_boot_log_is_silicons_line_for_line() {
    let run = match boot(Strap::App, ResetCause::UsbUartHpSys) {
        Ok(run) => run,
        Err(reason) => {
            skip_notice("the_boot_log_is_silicons_line_for_line", &reason);
            return;
        }
    };
    assert!(
        !matches!(
            run.outcome,
            Outcome::StrictBus { .. } | Outcome::Fault { .. }
        ),
        "{:?}",
        run.outcome
    );

    // The USB link, which is the console silicon's own capture came over —
    // so this is like for like, not two links that ought to agree.
    //
    // It was UART0 for a day. The mask ROM's console **drops** a character
    // rather than waiting when the IN endpoint is not free, so the modelled
    // drain latency was losing runs of the densest output; M5 P3's IN-FIFO
    // auto-commit (PR #595) closed it, and the two consoles now carry
    // identical bytes across this whole window. The UART0 copy is asserted
    // to be identical below, so a regression in either shows up here.
    let ours = boot_window(&device_lines(&String::from_utf8_lossy(
        &run.machine.usb_sj().bytes(),
    )));
    let on_uart0 = boot_window(&device_lines(&String::from_utf8_lossy(
        &run.machine.uart0().bytes(),
    )));
    assert_eq!(
        ours, on_uart0,
        "the ROM writes both consoles with the same bytes; these two disagree"
    );

    let (path, silicon_text) =
        lp_emu_esp32c6::test_support::transcript("boot-idle-flash", "silicon-esp32c6-")
            .expect("the committed silicon boot-idle-flash transcript");
    let mut silicon = boot_window(&device_lines(&silicon_text));
    // A fresh chip has no memory of a previous run. `Saved PC:` is printed
    // only when `ASSIST_DEBUG.core_0_lastpc_before_exception` is non-zero,
    // and on the board it held the PC the espflash reset interrupted
    // (`0x4080054c`, `swint_handler_trampoline`). Documented deviation.
    let saved_pc = silicon.iter().position(|l| l.starts_with("Saved PC:"));
    if let Some(i) = saved_pc {
        silicon.remove(i);
    }
    assert!(
        saved_pc.is_some(),
        "the silicon transcript at {} has no `Saved PC:` line; the deviation this test \
         subtracts is not there any more",
        path.display()
    );

    // The image-derived lines, against the image the machine was handed.
    let merged = std::fs::read(merged_image(&IMAGE).unwrap()).unwrap();
    let parsed = MergedImage::parse(&merged).expect("the merged image parses");
    let (partition, app) = parsed.app.as_ref().expect("an app partition with an image");
    assert_eq!(partition.offset, 0x0001_0000, "the factory partition");
    let expected_segments: Vec<String> = app
        .segments
        .iter()
        .enumerate()
        .map(|(n, s)| {
            format!(
                "esp_image: segment {n}: paddr={:08x} vaddr={:08x} size={:05x}h ({:6}) {}",
                s.paddr,
                s.vaddr,
                s.len,
                s.len,
                if s.is_mapped() { "map" } else { "load" }
            )
        })
        .collect();
    let our_segments: Vec<String> = ours
        .iter()
        .filter(|l| is_image_derived(l))
        .map(|l| l[l.find("esp_image:").unwrap()..].to_string())
        .collect();
    let silicon_segments: Vec<String> = silicon
        .iter()
        .filter(|l| is_image_derived(l))
        .map(|l| l[l.find("esp_image:").unwrap()..].to_string())
        .collect();
    assert_eq!(
        our_segments,
        expected_segments,
        "the bootloader's segment table is not the image it was given.\n\
         silicon printed (a different build of the same commit — DD45):\n  {}",
        silicon_segments.join("\n  ")
    );

    // Everything else, literally.
    let ours_fixed: Vec<&String> = ours.iter().filter(|l| !is_image_derived(l)).collect();
    let silicon_fixed: Vec<&String> = silicon.iter().filter(|l| !is_image_derived(l)).collect();
    assert_eq!(
        ours_fixed,
        silicon_fixed,
        "\nours:\n  {}\nsilicon ({}):\n  {}",
        ours_fixed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  "),
        path.display(),
        silicon_fixed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// G7-2: the app the bootloader loaded is the app, and the state it starts
/// in is the state direct load asserts — on the memory-class facts, which
/// are the ones DD45 leaves exact.
#[test]
#[ignore = "needs a fw-esp32c6 build; `just test-emu-c6`"]
fn rom_up_and_direct_load_agree_on_what_the_app_sees() {
    let elf = match reference_image(&IMAGE) {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("rom_up_and_direct_load_agree_on_what_the_app_sees", &reason);
            return;
        }
    };
    let merged = merged_image(&IMAGE).expect("the merged image");
    let len = std::fs::metadata(&merged).unwrap().len() as u32;

    // The ROM-up boot, run to the app's own first line.
    let mut rom_up = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(elf.clone()))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged.clone()))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        .build()
        .expect("the ROM-up machine builds");
    let outcome =
        rom_up.run_until(&StopCondition::after_micros(GATE_US).exit_on("[INIT] Board initialized"));
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the ROM-up boot never reached the app's first line: {outcome:?}"
    );

    // Direct load, the same ELF, the same chip behind it.
    let mut direct = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        .build()
        .expect("the direct machine builds");
    let outcome =
        direct.run_until(&StopCondition::after_micros(GATE_US).exit_on("[INIT] Board initialized"));
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{outcome:?}"
    );

    // 1. The app's own segments, byte for byte in RAM and through the
    //    window. This is the whole claim: the bootloader put the same bytes
    //    in the same places the loader does, having found them itself.
    let app = direct.app().expect("the app ELF").clone();
    let mut compared = 0usize;
    for seg in &app.segments {
        if seg.memsz == 0 || seg.data.is_empty() {
            continue;
        }
        let a = read_span(&rom_up, seg.vaddr, seg.data.len() as u32);
        let b = read_span(&direct, seg.vaddr, seg.data.len() as u32);
        assert_eq!(
            a.len(),
            b.len(),
            "segment {:#010x} is not readable on both machines",
            seg.vaddr
        );
        let first = a.iter().zip(b.iter()).position(|(x, y)| x != y);
        assert!(
            first.is_none(),
            "segment {:#010x} differs at +{:#x}: ROM-up {:#04x}, direct {:#04x}",
            seg.vaddr,
            first.unwrap(),
            a[first.unwrap()],
            b[first.unwrap()]
        );
        compared += a.len();
    }
    assert!(compared > 2_000_000, "only {compared} bytes compared");

    // 2. The flash offsets the loader synthesises are the ones the real
    //    image has (DD40's first cross-check item). Direct load computes
    //    `factory + (vaddr - 0x42000000)`; the bootloader reads whatever
    //    `espflash` wrote.
    let bytes = std::fs::read(merged_image(&IMAGE).unwrap()).unwrap();
    let parsed = MergedImage::parse(&bytes).unwrap();
    let (_, image) = parsed.app.as_ref().unwrap();
    for page in &direct.flash_staging().pages {
        let offset = page.vaddr - lp_emu_esp32c6::memmap::FLASH_CACHE_BASE;
        assert_eq!(
            page.paddr,
            lp_emu_esp32c6::flash::FACTORY_OFFSET + offset,
            "the loader's synthetic offset for {:#010x}",
            page.vaddr
        );
    }
    // …and every mapped segment of the real image obeys the same 64 KiB
    // congruence, which is why the two can agree at all.
    for seg in image.segments.iter().filter(|s| s.is_mapped()) {
        assert_eq!(
            seg.paddr % 0x1_0000,
            seg.vaddr % 0x1_0000,
            "segment at {:#010x} is not page-congruent",
            seg.vaddr
        );
    }

    // 3. The ROM's flash chip-size word (DD40's second item): direct load
    //    writes it in place of the bootloader's
    //    `esp_rom_spiflash_config_param`, and the ROM-up boot has the
    //    bootloader do it for real. Both must describe the same part.
    let ptr = lp_emu_esp32c6::memmap::ROM_SPIFLASH_LEGACY_DATA;
    let chip_a = u32::from_le_bytes(read_span(&rom_up, ptr, 4).try_into().unwrap());
    let chip_b = u32::from_le_bytes(read_span(&direct, ptr, 4).try_into().unwrap());
    assert_eq!(chip_a, chip_b, "the legacy chip struct is at one address");
    let size_a = u32::from_le_bytes(read_span(&rom_up, chip_a + 4, 4).try_into().unwrap());
    let size_b = u32::from_le_bytes(read_span(&direct, chip_b + 4, 4).try_into().unwrap());
    assert_eq!(size_a, len, "the ROM-up boot found a {len}-byte part");
    assert_eq!(size_b, len, "direct load seeded a {len}-byte part");

    // 4. The reset cause reaches the firmware's recovery ledger, and the two
    //    paths differ there **on purpose**: a ROM-up boot after a serial
    //    reset is a user reset, a direct load asserts a power-on. Checked
    //    after the byte comparison above, not before: the ledger is one of
    //    the few things the two runs are *supposed* to disagree about, and
    //    it is written into RAM a few lines later.
    let ledger = StopCondition::after_micros(GATE_US).exit_on("[RECOVERY] RWDT armed");
    assert!(
        matches!(rom_up.run_until(&ledger), Outcome::ExitMatched { .. }),
        "the ROM-up boot never armed the RWDT"
    );
    assert!(
        matches!(direct.run_until(&ledger), Outcome::ExitMatched { .. }),
        "the direct boot never armed the RWDT"
    );
    let rom_up_log = String::from_utf8_lossy(&rom_up.usb_sj().bytes()).into_owned();
    let direct_log = String::from_utf8_lossy(&direct.usb_sj().bytes()).into_owned();
    assert!(
        rom_up_log.contains("[RECOVERY] boot: cause=user-reset"),
        "{rom_up_log}"
    );
    assert!(
        direct_log.contains("[RECOVERY] boot: cause=power-on"),
        "{direct_log}"
    );

    // 5. **What the app SAYS**, line for line, once both have run past the
    //    radio bring-up.
    //
    //    The mask ROM's `wait_rfpll_cal_end` (`0x40005984`) polls one analog
    //    register through the PHY function table — block `0x62`, register 7,
    //    bit 1 — and prints `error: pll_cal exceeds 2ms!!!` when it gives up.
    //    Silicon never prints it. This machine printed it three times for as
    //    long as `I2C_ANA_MST` was an accept block with a single shared
    //    `data` byte, because the ROM's own `regi2c` traffic had left
    //    something else in it
    //    (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`).
    //
    //    The assertion is the whole console rather than a grep for that one
    //    string: a model that answers `regi2c` reads differently changes what
    //    the clock and radio paths of every image see, and the way to notice
    //    is a line appearing or disappearing anywhere. M7's own boot-log
    //    window stops at `Disabling RNG early entropy source`, which is why
    //    it never saw these three.
    let past_the_radio = StopCondition::after_micros(APP_US).exit_on("[INIT] fw-esp32 initialized");
    assert!(
        matches!(
            rom_up.run_until(&past_the_radio),
            Outcome::ExitMatched { .. }
        ),
        "the ROM-up boot never finished initialising"
    );
    assert!(
        matches!(
            direct.run_until(&past_the_radio),
            Outcome::ExitMatched { .. }
        ),
        "the direct boot never finished initialising"
    );
    let app_lines = |m: &Esp32C6Machine| -> Vec<String> {
        let text = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
        device_lines(&text)
            .into_iter()
            .skip_while(|l| !l.starts_with(APP_FIRST))
            .filter(|l| !PATH_DEPENDENT.iter().any(|p| l.contains(p)))
            .collect()
    };
    let ours = app_lines(&rom_up);
    let theirs = app_lines(&direct);
    assert!(
        ours.len() > 15,
        "the ROM-up app printed almost nothing: {ours:#?}"
    );
    assert_eq!(
        ours, theirs,
        "the app says different things depending on how it was loaded"
    );
    assert!(
        !ours.iter().any(|l| l.contains("pll_cal")),
        "the ROM's PLL calibration timed out: {ours:#?}"
    );
}

/// The app's first line, and the line the console comparison runs to.
/// `fw-esp32 initialized` is the end of the `[INIT]` chain and, more to the
/// point, it is past the radio bring-up — which is where the ROM's
/// PLL-calibration wait runs.
const APP_FIRST: &str = "[INIT] Initializing board";
/// Two emulated seconds: `fw-esp32 initialized` lands well inside them, and
/// this test already has both machines built, so the whole console
/// comparison costs one extra stretch of guest time rather than a second
/// pair of boots.
const APP_US: u64 = 2_000_000;

/// G7-4: what the app reports after booting itself is what it reports after
/// being placed — and what silicon reports.
///
/// DD50 named an 8 B block silicon's heap has and this machine's does not,
/// constant across boot counts, commits and links, and told M7 not to
/// "find" it by accident: *if the ROM-up path changes the idle heap by
/// exactly 8 B, that is the finding of the plan.* It does not. The ROM-up
/// boot's first heartbeat is **byte-identical** to direct load's, so the
/// second-stage bootloader leaves nothing behind that the allocator sees,
/// and the gap to silicon is the same 8 B it was — neither closed nor
/// widened, and therefore still DD50's to answer with a power-on capture.
#[test]
#[ignore = "needs a fw-esp32c6 build; `just test-emu-c6`"]
fn the_heap_ledger_is_the_same_whichever_way_the_app_arrived() {
    let elf = match reference_image(&IMAGE) {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "the_heap_ledger_is_the_same_whichever_way_the_app_arrived",
                &reason,
            );
            return;
        }
    };
    let merged = merged_image(&IMAGE).expect("the merged image");
    let len = std::fs::metadata(&merged).unwrap().len() as u32;
    // The first heartbeat is at 5 s.
    let stop = StopCondition::after_micros(6_000_000).exit_on(STACK_LINE);

    let mut rom_up = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged.clone()))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        .build()
        .expect("the ROM-up machine builds");
    assert!(
        matches!(rom_up.run_until(&stop), Outcome::ExitMatched { .. }),
        "the ROM-up boot never heartbeated"
    );

    let mut direct = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        .build()
        .expect("the direct machine builds");
    assert!(
        matches!(direct.run_until(&stop), Outcome::ExitMatched { .. }),
        "the direct boot never heartbeated"
    );

    let ours = memory_object(&String::from_utf8_lossy(&rom_up.usb_sj().bytes()));
    let theirs = memory_object(&String::from_utf8_lossy(&direct.usb_sj().bytes()));
    assert_eq!(
        ours, theirs,
        "the ROM-up path moved the idle heap; DD50 says report the allocation, never pass"
    );
    assert_eq!(
        ours, EMULATOR_MEMORY,
        "the idle heap moved from what M6 P5 pinned"
    );

    // The stack high-water is exact here, not a band: this figure is
    // silicon's own, and both boot paths land on it.
    for (name, m) in [("rom-up", &rom_up), ("direct", &direct)] {
        let log = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
        let line = log
            .lines()
            .find(|l| l.contains(STACK_LINE))
            .unwrap_or_else(|| panic!("{name} has no stack heartbeat"));
        assert!(line.contains(SILICON_STACK), "{name}: {line}");
    }

    // And silicon's own figures, from the committed transcript, with the
    // residual named rather than masked.
    let (path, silicon_text) =
        lp_emu_esp32c6::test_support::transcript("boot-idle-flash", "silicon-esp32c6-")
            .expect("the committed silicon transcript");
    let silicon = memory_object(&silicon_text);
    assert_eq!(
        silicon,
        SILICON_MEMORY,
        "the transcript at {} is not the one this gate was written against",
        path.display()
    );
    assert_eq!(
        field(&ours, "freeBytes") - field(&silicon, "freeBytes"),
        8,
        "DD50's 8 B, in the direction it has always had"
    );
    assert_eq!(field(&silicon, "usedBytes") - field(&ours, "usedBytes"), 8);
    assert_eq!(field(&ours, "totalBytes"), field(&silicon, "totalBytes"));
}

/// The sentinel the firmware's per-heartbeat stack line starts with.
const STACK_LINE: &str = "[stack] heartbeat: high-water ";
/// Silicon's own first-heartbeat stack figure for this image, exactly.
const SILICON_STACK: &str = "high-water 11908 B of 71512 B";
/// The first heartbeat's heap on this machine, both boot paths.
const EMULATOR_MEMORY: &str =
    r#"{"freeBytes":265104,"usedBytes":60432,"totalBytes":325536,"largestFreeBlock":198876}"#;
/// Silicon's, from the committed `boot-idle-flash` transcript.
const SILICON_MEMORY: &str =
    r#"{"freeBytes":265096,"usedBytes":60440,"totalBytes":325536,"largestFreeBlock":198886}"#;

/// The first `"memory":{…}` object in a log.
fn memory_object(log: &str) -> String {
    let at = log
        .find(r#""memory":{"#)
        .unwrap_or_else(|| panic!("no heartbeat memory object in:\n{log}"));
    let start = at + r#""memory":"#.len();
    let end = log[start..]
        .find('}')
        .unwrap_or_else(|| panic!("unterminated memory object"));
    log[start..start + end + 1].to_string()
}

/// One `"name":<number>` out of a memory object.
fn field(object: &str, name: &str) -> i64 {
    let key = format!("\"{name}\":");
    let at = object
        .find(&key)
        .unwrap_or_else(|| panic!("{object} has no {name}"));
    let rest = &object[at + key.len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().expect("a number")
}

/// G7-3: the download strap reaches the mask ROM's own console.
#[test]
#[ignore = "needs a fw-esp32c6 build; `just test-emu-c6`"]
fn the_download_strap_reaches_the_roms_console() {
    let run = match boot(Strap::Download, ResetCause::UsbUartHpSys) {
        Ok(run) => run,
        Err(reason) => {
            skip_notice("the_download_strap_reaches_the_roms_console", &reason);
            return;
        }
    };
    assert!(
        !matches!(
            run.outcome,
            Outcome::StrictBus { .. } | Outcome::Fault { .. }
        ),
        "{:?}",
        run.outcome
    );
    let lines = device_lines(&String::from_utf8_lossy(&run.machine.uart0().bytes()));
    assert_eq!(
        lines,
        vec![
            "ESP-ROM:esp32c6-20220919",
            "Build:Sep 19 2022",
            // `0x16` is the app strap with the one bit the ROM's decode
            // tests cleared; the string is the ROM's own.
            "rst:0x15 (USB_UART_HPSYS),boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))",
            "waiting for download",
        ],
        "the download console's whole output"
    );
    // Nothing was loaded: the ROM never opened the image at 0x0.
    assert!(run.machine.app_segments().is_empty());
}

/// G7-5: the host's reset dance reboots a running application into the ROM's
/// download console (DD50's second item).
///
/// M6 could only *report* a reset request — there was no boot chain to
/// reboot into, so `MachineRequest::Reset` ended the run with exit 2 and the
/// strap named. There is one now. The exit-2 behaviour stays the default,
/// because three merged M6 scenarios read it as their evidence; a run that
/// wants the real thing asks with `reboot_on_reset`.
#[test]
#[ignore = "needs a fw-esp32c6 build; `just test-emu-c6`"]
fn a_reset_request_reboots_the_running_app_into_the_download_console() {
    let merged = match merged_image(&IMAGE) {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "a_reset_request_reboots_the_running_app_into_the_download_console",
                &reason,
            );
            return;
        }
    };
    let len = std::fs::metadata(&merged).unwrap().len() as u32;
    let mut m = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(lp_emu_esp32c6::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .reboot_on_reset(true)
        .uart0(Uart0Sink::Memory)
        .usb_host(lp_emu_esp32c6::machine::UsbHost::Attached { draining: true })
        // The dance, well past `[INIT] fw-esp32 initialized`, so the reset
        // lands on a running application rather than on the bootloader.
        .usb_script(vec![(
            1_200 * 1_000 * lp_emu_esp32c6::memmap::CYCLES_PER_US,
            lp_emu_esp32c6::control::ControlCommand::DownloadMode,
        )])
        .build()
        .expect("the machine builds");

    let outcome = m.run_until(&StopCondition::after_micros(2_000_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the run should have carried on past the reset, not ended: {outcome:?}"
    );
    assert_eq!(m.reboots(), 1, "one reset request, one reboot");
    assert_eq!(m.strap(), Strap::Download, "the request named the strap");

    let log = device_lines(&String::from_utf8_lossy(&m.uart0().bytes()));
    let banners: Vec<&String> = log.iter().filter(|l| l.starts_with(FIRST)).collect();
    assert_eq!(
        banners.len(),
        2,
        "two boots in one log:\n{}",
        log.join("\n")
    );
    // The first boot ran the application; the second is the ROM's console.
    let second = log
        .iter()
        .rposition(|l| l.starts_with(FIRST))
        .expect("the second banner");
    assert_eq!(
        &log[second..],
        &[
            "ESP-ROM:esp32c6-20220919".to_string(),
            "Build:Sep 19 2022".to_string(),
            "rst:0x15 (USB_UART_HPSYS),boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))".to_string(),
            "waiting for download".to_string(),
        ]
    );
    assert!(
        log[..second]
            .iter()
            .any(|l| l.contains("Loaded app from partition")),
        "the first boot loaded the app"
    );
}

/// Read `len` bytes out of whichever RAM region holds `address`.
fn read_span(m: &Esp32C6Machine, address: u32, len: u32) -> Vec<u8> {
    for region in m.bus.regions() {
        if region.contains(address) && region.contains(address + len - 1) {
            let at = (address - region.base) as usize;
            return region.data[at..at + len as usize].to_vec();
        }
    }
    panic!("{address:#010x}+{len} is not in one RAM region");
}

/// Lines the two paths are *supposed* to disagree about: the recovery
/// ledger records how the chip was reset, and a ROM-up boot after a serial
/// reset is a user reset where a direct load asserts a power-on. Everything
/// else is the same application doing the same thing.
const PATH_DEPENDENT: &[&str] = &["[RECOVERY]"];
