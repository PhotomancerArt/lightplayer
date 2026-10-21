//! `lp-cli emu run --chip esp32s3 --host-link`: the S3 machine with this
//! process as the host on its USB link.
//!
//! Only the hosted run, and only the flags a hosted run needs. The S3's own
//! binary (`lp-emu-esp32s3`) stays the workshop for everything else; this
//! door exists because since wire proto 30 the S3's console IS an lp-link,
//! and nothing under `lp-emu/` may host one (the MIT fence, D11) — so the
//! heap ratchet's S3 arm and the S3 host-side checks need a product crate to
//! stand on the other end.
//!
//! Since S3 radio M7 P5 it also takes the update host's three flags, the
//! C6's: `--rom-up-flash` (the reset vector out of a writable flash file,
//! written back when the run ends — so an early end is a power cut and the
//! next run boots what the flash held), `--flash-cut` (a power cut at the
//! Nth `lpfs` program/erase, then stop or power-cycle) and `--ota-offer` /
//! `--ota-cut-after` (an update on lp-link channel 3, from the host this
//! process is). The board is built to reboot on reset (`S3Board::build`),
//! so a requested reboot — the S3's performed software reset — keeps the
//! host attached across it.

use anyhow::{Context, Result, bail};
use lp_emu_esp32s3::control::parse_usb_script;
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{AppSource, BootMode, Esp32S3Builder, FrameSink, TimeGrade, UsbHost};

use super::args::{Grade, RunArgs, UsbHostArg};
use super::handler::{check_file, whole_part_len};
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
        (
            args.flash.is_some(),
            "--flash (use --rom-up-flash: the S3's writable chip boots from the reset vector)",
        ),
        (args.reboot_on_reset, "--reboot-on-reset"),
        (args.pin_log.is_some(), "--pin-log"),
        (args.tx_log.is_some(), "--tx-log"),
        (!args.pin_script.is_empty(), "--pin-script"),
        (!args.wire.is_empty(), "--wire"),
        (args.efuse_mac.is_some(), "--efuse-mac"),
        (args.efuse_rev.is_some(), "--efuse-rev"),
        (args.flash_part.is_some(), "--flash-part"),
        (args.reset_cause.is_some(), "--reset-cause"),
        (args.strap.is_some(), "--strap"),
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
    match (
        args.elf.as_deref(),
        args.merged.as_deref(),
        args.rom_up_flash.as_deref(),
    ) {
        // `--rom-up-flash`: the reset vector out of a writable flash file,
        // written back at the end (`S3Board::finish` flushes it) — a power
        // cut, when the run ends early.
        (None, None, Some(chip)) => {
            check_file(chip, "--rom-up-flash")?;
            builder = builder
                .boot_mode(BootMode::RomUp)
                .flash(FlashBacking::File(chip.to_path_buf()))
                .flash_len(whole_part_len(chip)?);
        }
        (Some(elf), None, None) => {
            check_file(elf, "--elf")?;
            builder = builder
                .boot_mode(BootMode::Direct)
                .app(AppSource::Path(elf.to_path_buf()));
        }
        (None, Some(merged), None) => {
            check_file(merged, "--merged")?;
            let len = std::fs::metadata(merged)?.len();
            builder = builder
                .boot_mode(BootMode::RomUp)
                .flash(FlashBacking::Copy(merged.to_path_buf()))
                .flash_len(len as u32);
        }
        _ => bail!(
            "pass one of --elf <fw-esp32s3>, --merged <chip.bin> or --rom-up-flash <chip.bin>"
        ),
    }
    if let Some(text) = args.flash_cut.as_deref() {
        let spec = lp_emu_esp_common::engine::flash_cut_spec::FlashCutSpec::parse(text)
            .map_err(|e| anyhow::anyhow!("--flash-cut: {e}"))?;
        builder = builder.flash_cut(spec);
    }
    if let Some(path) = &args.dump_frames {
        builder = builder.dump_frames(FrameSink::File(path.clone()));
    }
    // The cable's schedule beside a hosted link: control words only, exactly
    // as the C6 door restricts it (`handler.rs`) — the host IS the link, so
    // a scripted byte would be injected under it.
    if let Some(path) = &args.usb_script {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading --usb-script {}", path.display()))?;
        let script =
            parse_usb_script(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        if script.bytes.steps_left() > 0 {
            bail!(
                "--usb-script {} sends bytes, and with --host-link this process IS the host on \
                 that link: only the cable's control words (detach, attach, open, close, wait) \
                 may ride beside it",
                path.display()
            );
        }
        builder = builder.usb_script(script.commands);
    }
    // `--usb-host`, when given, overrides the hosted run's default
    // (attached and draining from power-on — `S3Board::build`'s own
    // default, matching what `--host-link` does for the C6).
    let usb_host = args
        .usb_host
        .map(UsbHostArg::usb_host)
        .unwrap_or(UsbHost::Attached { draining: true });
    let board = S3Board::build(builder, usb_host)?;
    let boot = format!(
        "esp32s3 {} boot, grade {}",
        board.machine.boot_mode().as_str(),
        board.machine.configuration_label()
    );
    super::run_hosted::run_hosted(board, boot, args, micros)
}

#[cfg(test)]
mod tests {
    use lp_emu_esp32s3::flash::{FACTORY_LEN, FACTORY_OFFSET, FlashBacking};
    use lp_emu_esp32s3::machine::{AppSource, Esp32S3Builder, UsbHost};
    use lp_emu_esp32s3::test_support;
    use lpc_wire::{ClientMessage, ClientRequest};

    use crate::commands::emu::link_host::{EmuLinkHost, S3Board};
    use crate::commands::firmware::ota_fixture::Fixture;
    use crate::commands::ota_host::{OtaArgs, OtaHost};

    /// Emulated microseconds a wait may take.
    const BUDGET_US: u64 = 2_000_000;

    /// **The update host's plumbing on the S3** (M7 P5): an `--ota-offer`
    /// rides the S3's hosted link, and the board — today's single image,
    /// which installs no channel-3 hook (`fw_esp32_common::usb_link::
    /// set_update_hook` is the C6's split core's) — answers it with
    /// **nothing**: no manifest, no request, no refusal, not one byte of its
    /// `factory` touched. Studio's "needs USB" for such a board is decided
    /// from the hello's `ota_layout: 0` (`lpa-update`), never on this
    /// channel. The offer then survives the board's performed software
    /// reset (#1238): a `Reboot` brings the board back as
    /// `cause=software-reset` on a new link session, with the host and its
    /// offer still attached. An update that runs end to end needs an
    /// update-capable S3 core (M7 P8) and is P10's.
    #[test]
    #[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
    fn an_offer_to_the_single_image_goes_unanswered_and_survives_a_reboot() {
        let (elf, merged) = match (
            test_support::fw_esp32s3_image(),
            test_support::merged_chip_image(),
        ) {
            (Ok(elf), Ok(merged)) => (elf, merged),
            (Err(reason), _) | (_, Err(reason)) => {
                test_support::skip_notice(
                    "an_offer_to_the_single_image_goes_unanswered_and_survives_a_reboot",
                    &reason,
                );
                return;
            }
        };
        let fx = Fixture::new("abc1234");
        fx.write().unwrap().expect("OTA files written");
        let builder = Esp32S3Builder::new()
            .app(AppSource::Path(elf))
            .flash(FlashBacking::Copy(merged))
            .strict(true);
        let board = S3Board::build(builder, UsbHost::Attached { draining: true })
            .expect("the shipped image direct-loads");
        let mut host = EmuLinkHost::new(board, 0x5E55_0305, true);
        host.ota = OtaHost::from_args(&OtaArgs {
            ota_offer: Some(fx.ota_dir()),
            ..OtaArgs::default()
        })
        .expect("the fixture's OTA directory is an offer");
        let factory = |host: &EmuLinkHost<S3Board>| {
            host.board
                .machine
                .flash()
                .lock()
                .expect("flash")
                .peek(FACTORY_OFFSET, FACTORY_LEN)
                .expect("factory is on the chip")
                .to_vec()
        };
        let before = factory(&host);

        let hello = host
            .wait_for_line("\"hello\":{", BUDGET_US)
            .expect("the run");
        assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));
        // Long enough for a board with an update hook to have answered the
        // host's first `?` many times over.
        let settle = host.board.machine.micros() + 500_000;
        host.run_until(settle, None).expect("the run");

        host.send(&ClientMessage {
            id: 9,
            msg: ClientRequest::Reboot,
        })
        .expect("the link takes the request");
        let second = host
            .wait_for_line("[RECOVERY] boot: cause=software-reset", BUDGET_US)
            .expect("the reboot");
        let text = host.console().join("\n");
        assert!(second.is_some(), "no second boot:\n{text}");
        let hello = host
            .wait_for_line("\"hello\":{", BUDGET_US)
            .expect("the run");
        assert!(hello.is_some(), "no hello after the reboot:\n{text}");
        let settle = host.board.machine.micros() + 500_000;
        host.run_until(settle, None).expect("the run");

        let text = host.console().join("\n");
        let ota = host.ota.as_ref().expect("the offer is still attached");
        assert!(text.contains("offering "), "the host offered:\n{text}");
        assert!(
            ota.manifests.is_empty(),
            "the board said nothing: {}",
            ota.summary()
        );
        assert_eq!(ota.requests, 0, "{}", ota.summary());
        assert!(ota.refusals.is_empty(), "{}", ota.summary());
        assert!(!ota.done(), "{}", ota.summary());
        assert_eq!(host.board.machine.reboots(), 1, "one software reboot");
        assert!(host.board.machine.first_strict_violation().is_none());
        // The reboot's own `PeerRestarted` is the one reset the host counts.
        assert_eq!(host.link_errors, 1, "{text}");
        assert!(factory(&host) == before, "factory untouched");
        println!(
            "S3 OFFER (lp-emu:esp32s3:t1): ota — {}; host link — {} reset(s)",
            ota.summary(),
            host.link_errors
        );
    }
}
