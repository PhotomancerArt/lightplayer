//! `lp-cli emu run --chip esp32s3`: the S3 machine with this process as the
//! host on its USB link (`--host-link`), or with that link on a loopback
//! socket whose client is the host (`--link <addr>`).
//!
//! Only those two runs, and only the flags they need. The S3's own binary
//! (`lp-emu-esp32s3`) stays the workshop for everything else; this door
//! exists because since wire proto 30 the S3's console IS an lp-link, and
//! nothing under `lp-emu/` may host one (the MIT fence, D11) — so the heap
//! ratchet's S3 arm and the S3 host-side checks need a product crate to
//! stand on the other end.
//!
//! The socket run (S3 radio M6) is the C6's `--link` for the S3: the board
//! runs on its own while a client — `lp-cli … serial:tcp://<addr>`, or a
//! test holding the link — connects, and with `--lan` and `--rom-up-flash`
//! it is the board the relay cell (`lp-cli/tests/emu_relay_link.rs`) drives
//! from outside, over two boots of one flash file.
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
//!
//! Since S3 radio M5 it takes the C6's seam flags as well: `--seams`,
//! `--seams-prefer` (with neither, the capability defaults — `net=lan` —
//! softly, as on the C6), `--seam-trace`, `--pace`, and `--lan <fixture>`,
//! which puts the board on a virtual LAN of its own and prints its forward
//! (`lan:127.0.0.1:<port>`). A board on a LAN gets the locally administered
//! MAC `emu serve` gives its first board unless `--efuse-mac` names one: the
//! S3 machine's own default eFuse MAC is all zeros.

use anyhow::{Context, Result, bail};
use lp_emu_esp32s3::control::parse_usb_script;
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{
    AppSource, BootMode, Esp32S3Builder, FrameSink, Machine, Outcome, StopCondition, TimeGrade,
    UsbHost, UsbSjDrain, UsbSjSink,
};

use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp_common::seam::net::{LanDriver, SharedLan};
use lp_emu_esp32s3::loader::EfuseIdentity;

use super::args::{Grade, RunArgs, UsbHostArg};
use super::handler::{
    RUN_BOARD, SEAM_TRACE_ONLY, SeamLines, announce_lan_for, check_file, whole_part_len,
};
use super::lan_fixture::LanFixture;
use super::link_host::S3Board;
use super::serve::default_mac;

pub(super) fn run_s3(args: &RunArgs, seams: SeamRequest, micros: u64) -> Result<()> {
    // Two runs: the hosted one (this process the host on the USB link), and
    // the socket one (`--link`: the USB-Serial-JTAG byte stream on a loopback
    // socket, the client the host — the C6's `--link`, which a relay or LAN
    // cell reaches as `serial:tcp://…` while the board runs on its own).
    match (args.host_link, args.link.is_some()) {
        (true, true) => bail!(
            "--host-link makes this process the host on the USB link; a --link socket cannot \
             be that port too"
        ),
        (false, false) => bail!(
            "`emu run --chip esp32s3` needs a host on its USB link: --host-link (this process) \
             or --link <addr> (a socket a client connects to); the S3's own binary, \
             lp-emu-esp32s3, is the door for everything else"
        ),
        _ => {}
    }
    let unsupported = [
        (
            args.flash.is_some(),
            "--flash (use --rom-up-flash: the S3's writable chip boots from the reset vector)",
        ),
        (
            args.host_link && args.reboot_on_reset,
            "--reboot-on-reset (a hosted S3 run always reboots on reset)",
        ),
        (args.pin_log.is_some(), "--pin-log"),
        (
            args.link_kind != super::args::LinkKind::Usb,
            "--link-kind uart0 (the S3's shipped image speaks on its USB link)",
        ),
        (args.tx_log.is_some(), "--tx-log"),
        (!args.pin_script.is_empty(), "--pin-script"),
        (!args.wire.is_empty(), "--wire"),
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
        .strict(args.strict_bus)
        .block_cache(!args.no_block_cache)
        .flash_busy(args.flash_busy);
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
    if let Some(size) = args.psram.as_deref() {
        let len =
            lp_emu_esp32s3::psram::parse_size(size).map_err(|e| anyhow::anyhow!("--psram: {e}"))?;
        builder = builder.psram_len(len);
    }

    // The seams (S3 radio M5): the request, the board's LAN and its pace.
    builder = builder.seams(seams);
    if let Some(pace) = args.pace {
        builder = builder.pace(pace.pace());
    }
    // `--lan`: one board on a LAN of its own, driven on its own guest clock.
    let lan = match args.lan.as_deref() {
        Some(path) => {
            let fixture = LanFixture::read(path)?;
            Some((fixture.lan(LanDriver::SelfDriven), fixture))
        }
        None => None,
    };
    if let Some((shared, _)) = &lan {
        builder = builder.lan(shared.clone(), RUN_BOARD);
    }
    if let Some(path) = &args.seam_trace {
        let file = std::fs::File::create(path)
            .with_context(|| format!("--seam-trace: creating {}", path.display()))?;
        builder = builder.trace(
            Box::new(SeamLines::new(file)),
            vec![SEAM_TRACE_ONLY.to_string()],
        );
    }
    // The eFuse identity: what `--efuse-*` names; on a LAN with none named,
    // `emu serve`'s first board's MAC rather than the machine's all-zero one.
    let mut efuse = EfuseIdentity::default();
    if lan.is_some() {
        efuse.mac = default_mac(0);
    }
    if let Some(text) = &args.efuse_mac {
        efuse.mac = EfuseIdentity::parse_mac(text)
            .map_err(|e| anyhow::anyhow!("--efuse-mac `{text}`: {e}"))?;
    }
    if let Some(text) = &args.efuse_rev {
        (efuse.wafer_major, efuse.wafer_minor) = EfuseIdentity::parse_rev(text)
            .map_err(|e| anyhow::anyhow!("--efuse-rev `{text}`: {e}"))?;
    }
    if efuse != EfuseIdentity::default() {
        builder = builder.efuse(efuse);
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
    if let Some(addr) = &args.link {
        return run_socket(builder, addr, lan.as_ref(), args, micros);
    }
    // `--usb-host`, when given, overrides the hosted run's default
    // (attached and draining from power-on — `S3Board::build`'s own
    // default, matching what `--host-link` does for the C6).
    let usb_host = args
        .usb_host
        .map(UsbHostArg::usb_host)
        .unwrap_or(UsbHost::Attached { draining: true });
    let mut board = S3Board::build(builder, usb_host)?;
    board.print_seam_lines();
    announce_lan_for(board.machine.net_endpoint_id(), lan.as_ref())?;
    let boot = format!(
        "esp32s3 {} boot, grade {}",
        board.machine.boot_mode().as_str(),
        board.machine.configuration_label()
    );
    super::run_hosted::run_hosted(board, boot, args, micros)
}

/// The socket run (`--link <addr>`): the USB-Serial-JTAG byte stream on a
/// loopback socket whose client is the application — its connect opens the
/// port and its close closes it (`UsbSjDrain::Auto`), so the port starts
/// closed (`attached-idle`) unless `--usb-host` says otherwise, as on the C6
/// (`handler::usb_host_at_power_on`). It ends at `--exit-on`, the deadline or
/// the wall-clock net, and writes a `--rom-up-flash` file back however it
/// ended, so the next run boots what this one left.
fn run_socket(
    builder: Esp32S3Builder,
    addr: &str,
    lan: Option<&(SharedLan, LanFixture)>,
    args: &RunArgs,
    micros: u64,
) -> Result<()> {
    let usb_host = args
        .usb_host
        .map(UsbHostArg::usb_host)
        .unwrap_or(UsbHost::Attached { draining: false });
    let mut machine = builder
        .usb_host(usb_host)
        .usb_sj(UsbSjSink::Tcp(addr.to_string()))
        .usb_sj_drain(UsbSjDrain::Auto)
        .reboot_on_reset(args.reboot_on_reset)
        .build()
        .map_err(|e| anyhow::anyhow!("building the S3 machine: {e}"))?;
    let print_seams = |machine: &mut Machine| {
        for line in machine.take_seam_lines() {
            eprintln!("emu: {line}");
        }
    };
    print_seams(&mut machine);
    announce_lan_for(machine.net_endpoint_id(), lan)?;
    eprintln!(
        "emu: esp32s3 {} boot, grade {}, usb-serial-jtag on {addr} — connect with `lp-cli \
         upload <project> serial:tcp://{addr}`",
        machine.boot_mode().as_str(),
        machine.configuration_label()
    );
    eprintln!(
        "emu: running for {micros} us of EMULATED time (wall-clock net: {} s)",
        args.wall_timeout_secs
    );
    let stop = StopCondition {
        stop_cycle: Some(micros * lp_emu_esp32s3::memmap::CYCLES_PER_US),
        wall_timeout: Some(std::time::Duration::from_secs(args.wall_timeout_secs)),
        probes: Vec::new(),
        exit_on: args.exit_on.clone(),
    };
    let outcome = machine.run_until(&stop);
    print_seams(&mut machine);
    machine.flush_frames();
    match machine.flush_flash() {
        Ok(true) => eprintln!("emu: flash image written back"),
        Ok(false) => {}
        Err(e) => eprintln!("emu: could not write the flash image back: {e}"),
    }
    let console = machine.console().bytes();
    if let Some(path) = &args.console {
        std::fs::write(path, &console)
            .with_context(|| format!("writing the console transcript to {}", path.display()))?;
    }
    eprintln!(
        "emu: {outcome:?} — {} us emulated, {} instructions, {} reboot(s) ({})",
        machine.micros(),
        machine.instructions(),
        machine.reboots(),
        machine.configuration_label()
    );
    match outcome {
        Outcome::Deadline { .. } | Outcome::ExitMatched { .. } => Ok(()),
        Outcome::FlashBusyFetch {
            cycle,
            core,
            pc,
            addr,
            fetch,
            busy_until,
            ..
        } => bail!(
            "{}",
            machine.flash_busy_message(cycle, core, pc, addr, fetch, busy_until)
        ),
        other => bail!("the run ended at {other:?}"),
    }
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
