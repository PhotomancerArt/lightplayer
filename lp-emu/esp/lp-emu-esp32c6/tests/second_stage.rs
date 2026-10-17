//! The second stage at flash `0x0`, as the mask ROM sees it — LightPlayer's
//! own (ADR `docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`)
//! held to what it promised, on the real ROM:
//!
//! - **The write audit.** From the ROM's `entry` to the core's first
//!   instruction, every MMIO write the boot makes is traced, and none may
//!   land where the second stage promised never to go: the LP peripheral
//!   clock gates (`LP_PERI`), either analog I2C master (`LP_I2C_ANA_MST`,
//!   `I2C_ANA_MST`), the modem clock domains, the LP clock tree
//!   (`LP_CLKRST`), the CPU clock (`PCR` `sysclk_conf`, `cpu_freq_conf`),
//!   eFuses, the USB pads (`IO_MUX` GPIO 12/13) and the USB-Serial-JTAG
//!   block (from the stage's own code; the ROM's `ets_printf` is the ROM's),
//!   and no SPI1 command (`SPI1.cmd`). Every write that does land is in the
//!   contract's list.
//! - **The analog-master wedge cannot happen.** A board whose previous
//!   firmware gated the LP analog I2C clock (`--lpperi-clk-en 41000000`) —
//!   which hangs the IDF bootloader for ever (`bootloader_hang.rs`) — boots
//!   to the core's first line through our door with no restore.
//! - **A broken second stage is recoverable over USB.** A stage that spins
//!   at its entry (on silicon it boot-loops on the flash-boot watchdog); the
//!   host's download dance still reaches the ROM's own download console,
//!   because nothing of ours runs before the ROM decides.
//!
//! The emulator knows nothing of the layout (the MIT fence): the image is a
//! split build's `merged.bin`, whose `0x0` is the build's own door.
//! `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp_common::Strap;
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::control::parse_usb_script;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Outcome, StopCondition, Uart0Sink, UsbHost,
};
use lp_emu_esp32c6::test_support::{FwImage, SplitImage, skip_notice, split_image};

/// The second stage's RAM (`bootloader.x`).
const STAGE_RAM: std::ops::Range<u32> = 0x4085_0000..0x4086_0000;
/// The cached flash window the core runs from: the core's first fetch ends
/// the second stage's phase.
const CORE_WINDOW: std::ops::Range<u32> = 0x4200_0000..0x4300_0000;
/// Our stage's own line.
const STAGE_LINE: &str = "[LPBOOT] LightPlayer bootloader v1";
/// The core's first line.
const CORE_FIRST: &str = "[INIT] Initializing board";

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn our_second_stage_writes_only_what_its_contract_lists() {
    let Some(split) = split("our_second_stage_writes_only_what_its_contract_lists") else {
        return;
    };
    let buf = SharedBuffer::new();
    let mut m = rom_up(&split, 0)
        .trace(Box::new(buf.clone()), Vec::new())
        .build()
        .expect("the ROM-up machine builds");
    let outcome = m.run_until(&StopCondition::after_micros(1_000_000).exit_on(CORE_FIRST));
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the boot never reached the core: {outcome:?}\n{}",
        m.uart0().text()
    );
    // The ROM's console reaches the host over USB at once; UART0 at 115,200
    // baud is still sending the ROM's banner when the core starts.
    let usb = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
    assert!(usb.contains(STAGE_LINE), "{usb}");

    let trace = buf.contents();
    let mut in_stage = false;
    let mut writes = 0usize;
    let mut refused = Vec::new();
    for line in trace.lines() {
        let Some((pc, rest)) = parse(line) else {
            continue;
        };
        if !in_stage {
            in_stage = STAGE_RAM.contains(&pc);
            if !in_stage {
                continue;
            }
        }
        if CORE_WINDOW.contains(&pc) {
            break;
        }
        let Some(write) = rest.strip_prefix('W') else {
            continue;
        };
        let reg = write.split_whitespace().nth(1).unwrap_or("");
        writes += 1;
        let ours = STAGE_RAM.contains(&pc);
        if forbidden(reg, ours) || (ours && !allowed(reg)) {
            refused.push(line.to_string());
        }
    }
    assert!(writes > 0, "no writes traced in the second stage's phase");
    assert!(
        refused.is_empty(),
        "the second stage wrote where its contract says it never does:\n{}",
        refused.join("\n")
    );
    println!("second stage: {writes} MMIO writes, every one in the contract");
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn a_board_with_the_lp_analog_clock_gated_boots_through_our_door() {
    let Some(split) = split("a_board_with_the_lp_analog_clock_gated_boots_through_our_door") else {
        return;
    };
    // The factory firmware's gate word (bootloader_hang.rs): bit 29 clear.
    let mut m = rom_up(&split, 0x4100_0000)
        .build()
        .expect("the ROM-up machine builds");
    let outcome = m.run_until(&StopCondition::after_micros(1_000_000).exit_on(CORE_FIRST));
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the gated board did not reach the core through our door: {outcome:?}\n{}",
        m.uart0().text()
    );
    assert!(
        !m.uart0().text().contains("TG0_WDT"),
        "{}",
        m.uart0().text()
    );
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6`"]
fn a_broken_second_stage_is_recovered_by_the_download_dance() {
    let Some(split) = split("a_broken_second_stage_is_recovered_by_the_download_dance") else {
        return;
    };
    // A stage that spins at its entry (`j .`), checksummed so the ROM loads
    // it; its appended hash is left zero, which the ROM reports and, with
    // secure boot off, does not refuse.
    let mut chip = std::fs::read(split.merged()).unwrap();
    let spin = spinning_stage();
    chip[..0x8000].fill(0xff);
    chip[..spin.len()].copy_from_slice(&spin);
    // The host's download dance at 200 ms, while the stage spins — its
    // effect (`download-mode`: the USB-Serial-JTAG block's reset with the
    // download strap, which the dance's DTR/RTS edges produce;
    // `usb_control.rs` proves the edges). Before the flash-boot watchdog's
    // first reset: the machine's clock restarts with each reboot, and so
    // would a script's.
    let dance = parse_usb_script(
        "\
200   download-mode
",
    )
    .expect("the dance parses");
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
        .usb_script(dance.commands)
        .reboot_on_reset(true)
        .build()
        .expect("the machine builds");
    let outcome =
        m.run_until(&StopCondition::after_micros(1_000_000).exit_on("waiting for download"));
    let uart = m.uart0().text();
    let usb = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the download dance did not reach the ROM's console: {outcome:?}\n{uart}\n--- usb ---\n{usb}"
    );
    assert!(
        usb.contains("entry 0x40850000") && !usb.contains("[LOADER]"),
        "the ROM did not start the broken stage first:\n{usb}"
    );
}

/// Writes nothing of ours may make, whoever's code makes them in the phase.
fn forbidden(reg: &str, ours: bool) -> bool {
    const BLOCKS: [&str; 8] = [
        "LP_PERI+",
        "LP_I2C_ANA_MST+",
        "I2C_ANA_MST+",
        "MODEM_LPCON+",
        "MODEM_SYSCON+",
        "LP_CLKRST+",
        "EFUSE+",
        "SPI1+0x000",
    ];
    BLOCKS.iter().any(|b| reg.starts_with(b))
        || matches!(reg, "IO_MUX+0x034" | "IO_MUX+0x038")
        || matches!(reg, "PCR+0x110" | "PCR+0x118")
        || (ours && reg.starts_with("USB_DEVICE+"))
}

/// The registers the contract lists (`fw-esp32c6-loader/src/prologue/`, and
/// the loader's MMU maps).
fn allowed(reg: &str) -> bool {
    const PREFIXES: [&str; 6] = [
        "TIMG0+0x048",
        "TIMG0+0x064",
        "LP_WDT+",
        "LP_ANA+",
        "SPI0+",
        "EXTMEM+",
    ];
    const EXACT: [&str; 20] = [
        "LP_AON+0x010",
        "HP_APM+0x0c4",
        "LP_APM0+0x0c4",
        "LP_APM+0x0c4",
        "PCR+0x01c",
        "PCR+0x100",
        "ASSIST_DEBUG+0x044",
        "LP_TIMER+0x040",
        "LP_TIMER+0x044",
        "PMU+0x164",
        "SPI1+0x004",
        "SPI1+0x014",
        "SPI1+0x018",
        "SPI1+0x01c",
        "SPI1+0x020",
        "SPI1+0x024",
        "SPI1+0x028",
        "SPI1+0x058",
        "IO_MUX+0x064",
        "IO_MUX+0x068",
    ];
    PREFIXES.iter().any(|p| reg.starts_with(p))
        || EXACT.contains(&reg)
        || matches!(
            reg,
            "IO_MUX+0x06c" | "IO_MUX+0x074" | "IO_MUX+0x078" | "IO_MUX+0x07c"
        )
}

/// `cyc=N pc=0x… <rest>`: the pc and the rest.
fn parse(line: &str) -> Option<(u32, &str)> {
    let rest = line.strip_prefix("cyc=")?;
    let (_, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix("pc=0x")?;
    let (pc, rest) = rest.split_once(' ')?;
    Some((u32::from_str_radix(pc, 16).ok()?, rest))
}

/// An ESP image of one RAM segment holding `j .` at the stage's entry.
fn spinning_stage() -> Vec<u8> {
    let entry = STAGE_RAM.start;
    let code = 0x0000_006fu32.to_le_bytes();
    let mut img = vec![0xE9, 1, 2, 0x20];
    img.extend_from_slice(&entry.to_le_bytes());
    img.extend_from_slice(&[0xEE, 0, 0, 0, 0x0D, 0, 0, 0, 0, 0x63, 0, 0, 0, 0, 0, 1]);
    img.extend_from_slice(&entry.to_le_bytes());
    img.extend_from_slice(&(code.len() as u32).to_le_bytes());
    img.extend_from_slice(&code);
    let sum = code.iter().fold(0xEFu8, |a, b| a ^ b);
    while img.len() % 16 != 15 {
        img.push(0);
    }
    img.push(sum);
    img.extend_from_slice(&[0; 32]);
    img
}

fn split(test: &str) -> Option<SplitImage> {
    match split_image(&FwImage::SHIPPED) {
        Ok(s) => Some(s),
        Err(reason) => {
            skip_notice(test, &reason);
            None
        }
    }
}

/// The ROM-up machine over the build's `merged.bin` (our door), with the
/// LP peripheral clock gates at `clk_en` (0: the chip's own power-on word).
fn rom_up(split: &SplitImage, clk_en: u32) -> Esp32C6Builder {
    let chip = std::fs::read(split.merged()).unwrap();
    let len = chip.len() as u32;
    let b = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(lp_emu_esp32c6::flash::FlashBacking::Bytes(chip))
        .flash_len(len)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .strict(true);
    if clk_en == 0 {
        b
    } else {
        b.lp_peri_clk_en(clk_en)
    }
}
