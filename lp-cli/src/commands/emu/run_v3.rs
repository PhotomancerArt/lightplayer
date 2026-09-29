//! `lp-cli emu run --chip esp32v3 --host-link`: the classic machine with this
//! process as the host on its UART0 link.
//!
//! The S3's door ([`super::run_s3`]) for the classic: only the hosted run,
//! and only the flags a hosted run needs. The classic's own binary
//! (`lp-emu-esp32v3`) stays the workshop for everything else. This door
//! exists because since wire proto 32 the classic's UART0 IS an lp-link
//! (plan `classic-uart-on-lp-link`), and nothing under `lp-emu/` may host one
//! (the MIT fence), so the classic's heap ratchet, its walk and its
//! host-side checks need a product crate on the other end.
//!
//! The machine is built to reboot on reset ([`super::link_host::V3Board`]),
//! so `--request '"reboot"'` reboots it — a software system reset, with RTC
//! fast memory kept — and the host sees the new session.

use anyhow::{Result, bail};
use lp_emu_esp_common::link_faults::LinkFaults;
use lp_emu_esp32v3::control::parse_control_script;
use lp_emu_esp32v3::flash::FlashBacking;
use lp_emu_esp32v3::machine::{AppSource, BootMode, Esp32V3Builder, FrameSink, TimeGrade};

use super::args::{Grade, RunArgs};
use super::handler::check_file;
use super::link_host::V3Board;

pub(super) fn run_v3(args: &RunArgs, micros: u64) -> Result<()> {
    if !args.host_link {
        bail!(
            "`emu run --chip esp32v3` is the hosted run only: pass --host-link (the classic's own \
             binary, lp-emu-esp32v3, is the door for everything else)"
        );
    }
    let unsupported = [
        (args.link.is_some(), "--link"),
        (args.flash.is_some(), "--flash"),
        (args.pin_log.is_some(), "--pin-log"),
        (args.tx_log.is_some(), "--tx-log"),
        (!args.pin_script.is_empty(), "--pin-script"),
        (!args.wire.is_empty(), "--wire"),
        (args.efuse_mac.is_some(), "--efuse-mac"),
        (args.efuse_rev.is_some(), "--efuse-rev"),
        (args.reset_cause.is_some(), "--reset-cause"),
        (args.strap.is_some(), "--strap"),
        (args.usb_host.is_some(), "--usb-host"),
        (args.usb_script.is_some(), "--usb-script"),
        (args.uart0_baud.is_some(), "--uart0-baud"),
        (args.lpperi_clk_en.is_some(), "--lpperi-clk-en"),
    ];
    if let Some((_, flag)) = unsupported.iter().find(|(set, _)| *set) {
        bail!("{flag} is a C6 flag here; the classic's own binary (lp-emu-esp32v3) has its twin");
    }
    if args.time_grade != Grade::T1 {
        bail!("the classic machine has one time grade, t1");
    }
    let mut builder = Esp32V3Builder::new()
        .time_grade(TimeGrade::T1)
        .strict(args.strict_bus);
    match (args.elf.as_deref(), args.merged.as_deref()) {
        (Some(elf), None) => {
            check_file(elf, "--elf")?;
            builder = builder
                .boot_mode(BootMode::Direct)
                .app(AppSource::Path(elf.to_path_buf()));
        }
        (None, Some(merged)) => {
            check_file(merged, "--merged")?;
            let len = std::fs::metadata(merged)?.len();
            builder = builder
                .boot_mode(BootMode::RomUp)
                .flash(FlashBacking::Copy(merged.to_path_buf()))
                .flash_len(len as u32);
        }
        _ => bail!("pass --elf <fw-esp32v3> or --merged <chip.bin>"),
    }
    if let Some(path) = &args.dump_frames {
        builder = builder.dump_frames(FrameSink::File(path.clone()));
    }
    if let Some(path) = &args.control_script {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("--control-script {}: {e}", path.display()))?;
        let script = parse_control_script(&text)
            .map_err(|e| anyhow::anyhow!("--control-script {}: {e}", path.display()))?;
        builder = builder.control_script(script);
    }
    if let Some(spec) = &args.uart_faults {
        let faults = LinkFaults::parse(spec).map_err(|e| anyhow::anyhow!("--uart-faults: {e}"))?;
        builder = builder.uart0_faults(faults);
    }
    let board = V3Board::build(builder)?;
    let boot = format!(
        "esp32v3 {} boot, grade {}",
        board.machine.boot_mode().as_str(),
        board.machine.time_grade().configuration()
    );
    super::run_hosted::run_hosted(board, boot, args, micros)
}
