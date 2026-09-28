//! `lp-cli emu run --chip esp32s3 --host-link`: the S3 machine with this
//! process as the host on its USB link.
//!
//! Only the hosted run, and only the flags a hosted run needs. The S3's own
//! binary (`lp-emu-esp32s3`) stays the workshop for everything else; this
//! door exists because since wire proto 30 the S3's console IS an lp-link,
//! and nothing under `lp-emu/` may host one (the MIT fence, D11) — so the
//! heap ratchet's S3 arm and the S3 host-side checks need a product crate to
//! stand on the other end.

use anyhow::{Result, bail};
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{AppSource, BootMode, Esp32S3Builder, FrameSink, TimeGrade};

use super::args::{Grade, RunArgs};
use super::handler::check_file;
use super::link_host::S3Board;

pub(super) fn run_s3(args: &RunArgs, micros: u64) -> Result<()> {
    if !args.host_link {
        bail!(
            "`emu run --chip esp32s3` is the hosted run only: pass --host-link (the S3's own \
             binary, lp-emu-esp32s3, is the door for everything else)"
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
        (args.uart0_baud.is_some(), "--uart0-baud"),
        (args.lpperi_clk_en.is_some(), "--lpperi-clk-en"),
    ];
    if let Some((_, flag)) = unsupported.iter().find(|(set, _)| *set) {
        bail!("{flag} is a C6 flag here; the S3's own binary (lp-emu-esp32s3) has its twin");
    }
    if args.time_grade != Grade::T1 {
        bail!("the S3 machine has one time grade, t1");
    }
    let mut builder = Esp32S3Builder::new()
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
        _ => bail!("pass --elf <fw-esp32s3> or --merged <chip.bin>"),
    }
    if let Some(path) = &args.dump_frames {
        builder = builder.dump_frames(FrameSink::File(path.clone()));
    }
    let board = S3Board::build(builder)?;
    let boot = format!(
        "esp32s3 {} boot, grade {}",
        board.machine.boot_mode().as_str(),
        board.machine.time_grade().configuration()
    );
    super::run_hosted::run_hosted(board, boot, args, micros)
}
