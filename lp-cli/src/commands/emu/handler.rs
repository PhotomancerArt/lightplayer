use std::path::Path;

use anyhow::{Context, Result, bail};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::control::parse_usb_script;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::{EfuseIdentity, ResetCause};
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, FrameSink, Outcome, PinLogSink,
    StopCondition, TimeGrade, TxLogSink, Uart0Sink, UsbHost, UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::pinscript::{PinScript, parse_pin_script, parse_wire};

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::net::{LanDriver, SharedLan};

use super::args::{EmuChip, EmuCli, EmuCommand, Grade, LinkKind, RunArgs, UsbHostArg};
use super::lan_fixture::{BOARD_LAN_PORT, LanFixture, forward_spec, forward_to_board};

pub fn handle_emu(cli: EmuCli) -> Result<()> {
    match cli.command {
        EmuCommand::Run(args) => run(args),
        EmuCommand::Serve(args) => super::serve::serve(args),
    }
}

impl Grade {
    /// The machine's own name for this grade. Shared so that `serve` and
    /// `run` cannot drift into two spellings of one thing.
    pub(super) fn time_grade(self) -> TimeGrade {
        match self {
            Grade::T1 => TimeGrade::T1,
            Grade::T2 => TimeGrade::T2,
            Grade::T3 => TimeGrade::T3,
        }
    }
}

/// Which entry a machine takes, and out of what.
///
/// Three combinations, and the third is the one a *flashing* run needs: the
/// bytes are not there yet, so the chip has to be writable AND booted from
/// the reset vector.
#[derive(Clone, Copy, Debug)]
pub(super) enum Image<'a> {
    /// `--elf` / `kind=elf`: loaded straight into memory at its entry point.
    /// The flash part beside it is a separate, initially empty thing.
    Elf(&'a Path),
    /// `--merged` / `kind=merged`: the whole chip, booted ROM-up and
    /// **read-only** — it is the image a gate named.
    Merged(&'a Path),
    /// `kind=rom-up`: ROM-up out of the WRITABLE flash file, which is the
    /// only shape a board can be flashed in and then boot what was written.
    /// The chip is whatever the flash file holds — nothing is loaded.
    RomUp,
}

/// The image, and with it the boot path — the half of `run` that `serve`
/// needs per board.
///
/// `--merged` is the whole chip: the hart starts at the reset vector and the
/// real ROM finds the bootloader, so a flash file beside it would be a second
/// chip and is refused rather than silently ignored. [`Image::RomUp`] is the
/// same boot path over a chip that *does* keep its writes.
pub(super) fn apply_image(
    mut builder: Esp32C6Builder,
    image: Image<'_>,
    flash: Option<&Path>,
) -> Result<Esp32C6Builder> {
    match image {
        Image::Elf(elf) => {
            check_file(elf, "--elf")?;
            builder = builder.app(AppSource::Path(elf.to_path_buf()));
            builder = match flash {
                Some(path) => builder.flash(FlashBacking::File(path.to_path_buf())),
                None => builder.flash(FlashBacking::Blank),
            };
        }
        Image::Merged(merged) => {
            check_file(merged, "--merged")?;
            if flash.is_some() {
                bail!(
                    "--merged is the whole chip's bytes; --flash would be a second one. \
                     Drop one: --merged for a boot from the reset vector through the real \
                     ROM, --elf --flash for a direct load with a flash part that persists."
                );
            }
            let len = whole_part_len(merged)?;
            builder = builder
                .boot_mode(BootMode::RomUp)
                .flash(FlashBacking::Copy(merged.to_path_buf()))
                .flash_len(len);
        }
        // Nothing is loaded and nothing is copied: the hart starts at the
        // reset vector and the real mask ROM reads whatever is in the part.
        // An erased part has no valid image there, which is how a board with
        // nothing on it reaches the ROM's download console by itself.
        Image::RomUp => {
            builder = builder.boot_mode(BootMode::RomUp).flash(match flash {
                Some(path) => FlashBacking::File(path.to_path_buf()),
                None => FlashBacking::Blank,
            });
        }
    }
    Ok(builder)
}

/// A merged image's length, which must be a whole flash part.
fn whole_part_len(merged: &Path) -> Result<u32> {
    let len = std::fs::metadata(merged)
        .with_context(|| format!("reading {}", merged.display()))?
        .len();
    u32::try_from(len)
        .ok()
        .filter(|n| n.is_power_of_two())
        .with_context(|| {
            format!(
                "{} is {len} bytes, which is not a whole flash part. A merged image is the \
                 WHOLE chip padded to its size — `espflash save-image --merge`, or \
                 scripts/emu/build-merged-image.sh.",
                merged.display()
            )
        })
}

fn run(args: RunArgs) -> Result<()> {
    if args.chip != EmuChip::Esp32V3 {
        let classic_only = [
            (args.control_script.is_some(), "--control-script"),
            (args.uart_faults.is_some(), "--uart-faults"),
        ];
        if let Some((_, flag)) = classic_only.iter().find(|(set, _)| *set) {
            bail!("{flag} is the classic's (--chip esp32v3): its host link is UART0");
        }
    }
    if let Some(path) = &args.seams_info {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        println!("{}", lp_emu_esp32c6::seams::seams_info(&bytes));
        return Ok(());
    }
    let seams = seam_request(args.seams.as_deref(), args.seams_prefer.as_deref())?;
    if args.chip != EmuChip::Esp32C6 && (args.lan.is_some() || args.seam_trace.is_some()) {
        bail!("--lan and --seam-trace are the C6's: Xtensa seams are the roadmap's M7");
    }
    if args.chip != EmuChip::Esp32C6 && args.pace.is_some() {
        bail!("--pace is the C6's: a pace is held at its network seam's LAN pump");
    }
    if args.chip != EmuChip::Esp32C6 && (args.seams.is_some() || args.seams_prefer.is_some()) {
        bail!("--seams / --seams-prefer are the C6's (Xtensa seams are the roadmap's M7)");
    }
    if args.chip != EmuChip::Esp32C6
        && (args.ota.ota_offer.is_some() || args.rom_up_flash.is_some())
    {
        bail!("--ota-offer and --rom-up-flash are the C6's: only its image is split");
    }
    if args.chip == EmuChip::Esp32S3 {
        return super::run_s3::run_s3(&args, parse_duration_us(&args.timeout)?);
    }
    if args.chip == EmuChip::Esp32V3 {
        return super::run_v3::run_v3(&args, parse_duration_us(&args.timeout)?);
    }

    let micros = parse_duration_us(&args.timeout)?;
    let grade = args.time_grade.time_grade();

    // `--lan`: one board on a LAN of its own, driven on its own guest clock
    // (deterministic: one board, one clock).
    let lan = match args.lan.as_deref() {
        Some(path) => {
            let fixture = LanFixture::read(path)?;
            Some((fixture.lan(LanDriver::SelfDriven), fixture))
        }
        None => None,
    };

    let mut builder = Esp32C6Builder::new()
        .time_grade(grade)
        .strict(args.strict_bus)
        .reboot_on_reset(args.reboot_on_reset)
        .usb_host(usb_host_at_power_on(&args))
        // `--monitor` takes the socket out of the port's open/close story:
        // the host is declared attached and draining from power-on and stays
        // that way, so a client that uploads and leaves does not take the
        // console with it.
        .usb_sj_drain(if args.monitor {
            UsbSjDrain::Manual
        } else {
            UsbSjDrain::Auto
        })
        .seams(seams);
    // `--pace`: left out, the board is held to wall time only while a host
    // is connected through its `--lan` forward.
    if let Some(pace) = args.pace {
        builder = builder.pace(pace.pace());
    }

    if args.ota.ota_offer.is_some() && !args.host_link {
        bail!("--ota-offer drives the update over the link this process hosts: add --host-link");
    }
    if let Some(chip) = args.rom_up_flash.as_deref() {
        // `--rom-up-flash`: the reset vector out of a writable flash file,
        // written back at the end (a power cut, when the run ends early).
        check_file(chip, "--rom-up-flash")?;
        if args.flash.is_some() {
            bail!("--rom-up-flash is the chip's flash already; drop --flash");
        }
        builder = apply_image(builder, Image::RomUp, Some(chip))?.flash_len(whole_part_len(chip)?);
    } else {
        let image = match (args.elf.as_deref(), args.merged.as_deref()) {
            (Some(elf), None) => Image::Elf(elf),
            (None, Some(merged)) => Image::Merged(merged),
            (Some(_), Some(_)) => unreachable!("clap's `image` group allows only one"),
            (None, None) => bail!(
                "nothing to run: pass --elf <fw-esp32c6> for a direct load, --merged \
                 <chip.bin> to boot from the reset vector through the real ROM, or \
                 --rom-up-flash <chip.bin> for the same boot over a chip that keeps its writes"
            ),
        };
        builder = apply_image(builder, image, args.flash.as_deref())?;
    }
    if let Some((shared, _)) = &lan {
        builder = builder.lan(shared.clone(), RUN_BOARD);
    }
    // The seams' own trace lines are notes, which pass any block filter: a
    // filter naming no block keeps the bus's MMIO lines out, and the sink
    // keeps only the `SEAM` notes of what is left (watchpoints and the like
    // are notes too).
    if let Some(path) = &args.seam_trace {
        let file = std::fs::File::create(path)
            .with_context(|| format!("--seam-trace: creating {}", path.display()))?;
        builder = builder.trace(
            Box::new(SeamLines::new(file)),
            vec![SEAM_TRACE_ONLY.to_string()],
        );
    }
    if let Some(text) = &args.mmu_page {
        let len = match text.to_ascii_lowercase().as_str() {
            "64k" => 0x1_0000,
            "32k" => 0x8000,
            "16k" => 0x4000,
            "8k" => 0x2000,
            _ => bail!("--mmu-page `{text}`: one of 64k, 32k, 16k, 8k"),
        };
        builder = builder.mmu_page_len(len);
    }
    // `--over`: the chip holds a whole flashed image and the `--elf` is
    // direct-loaded over it, writing nothing (clap ties it to `--elf`).
    if let Some(over) = args.over.as_deref() {
        check_file(over, "--over")?;
        builder = builder
            .flash(FlashBacking::Copy(over.to_path_buf()))
            .flash_len(whole_part_len(over)?)
            .flash_holds_image(true);
    }

    // The eFuse identity. `run` serves one board, so the default — the desk
    // board's MAC — is the right one; `serve` gives every board its own,
    // because a registry of N boards that all answer with one MAC is one
    // board N times.
    if args.efuse_mac.is_some() || args.efuse_rev.is_some() {
        let mut efuse = EfuseIdentity::default();
        if let Some(text) = &args.efuse_mac {
            efuse.mac = EfuseIdentity::parse_mac(text)
                .map_err(|e| anyhow::anyhow!("--efuse-mac `{text}`: {e}"))?;
        }
        if let Some(text) = &args.efuse_rev {
            (efuse.wafer_major, efuse.wafer_minor) = EfuseIdentity::parse_rev(text)
                .map_err(|e| anyhow::anyhow!("--efuse-rev `{text}`: {e}"))?;
        }
        builder = builder.efuse(efuse);
    }
    // The reset the chip is starting from, which a ROM-up boot's banner prints
    // verbatim (`rst:` / `boot:`), so a recording states them.
    if let Some(text) = &args.reset_cause {
        builder = builder.reset_cause(ResetCause::parse(text).with_context(|| {
            format!("--reset-cause `{text}`: expected poweron, usb-uart or tg0-wdt")
        })?);
    }
    if let Some(text) = &args.strap {
        builder = builder.strap(match text.as_str() {
            "download" => Strap::Download,
            _ => Strap::App,
        });
    }
    // The cable's schedule beside a hosted link: control words only, because
    // the host is the link and a scripted byte would be injected under it.
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

    // The link. Both kinds listen; the difference is which of the chip's two
    // serials the socket is, and a shipped image only speaks on the USB one.
    // With no `--link` neither is a socket: the console is still collected in
    // memory, and `--console` still writes it.
    if let Some(addr) = &args.link {
        builder = match args.link_kind {
            LinkKind::Usb => builder.usb_sj(UsbSjSink::Tcp(addr.clone())),
            LinkKind::Uart0 => builder.uart0(Uart0Sink::Tcp(addr.clone())),
        };
    }
    // The rate the auto-baud counters report, when a run drives the ROM's
    // UART0 console. Mirrored from the bin because the console gate is only
    // reachable from a run that can state it.
    if let Some(baud) = args.uart0_baud {
        builder = builder.uart0_baud(baud);
    }
    // The LP domain's power-on gate word: the one register that says whether
    // this is a clean board or one a previous firmware wedged. Applied before
    // the power-on snapshot, so a `power-cycle` hands the same board back.
    if let Some(clk_en) = args.lpperi_clk_en {
        builder = builder.lp_peri_clk_en(clk_en);
    }
    if let Some(path) = &args.dump_frames {
        builder = builder.dump_frames(FrameSink::File(path.clone()));
    }
    if let Some(path) = &args.pin_log {
        builder = builder.pin_log(PinLogSink::File(path.clone()));
    }
    if let Some(path) = &args.tx_log {
        builder = builder.tx_log(TxLogSink::File(path.clone()));
    }
    // `--pin-log` is mirrored here, so its input twin is mirrored too: a run
    // driven from `lp-cli emu run` can script the pads and read the edges
    // back the same way the bin does. The script *grammar* stays one
    // implementation (`lp_emu_esp32c6::pinscript`); this is the flag, not a
    // second dialect.
    for text in &args.wire {
        let (a, b) = parse_wire(text).map_err(|e| anyhow::anyhow!("--wire: {e}"))?;
        builder = builder.wire(a, b);
    }
    if !args.pin_script.is_empty() {
        let mut script = PinScript::new();
        for path in &args.pin_script {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            script.extend(
                parse_pin_script(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?,
            );
        }
        eprintln!(
            "emu: pin script: {} level(s) in {} step(s)",
            script.remaining(),
            script.steps_left()
        );
        builder = builder.pin_script(script);
    }

    if args.host_link {
        if args.link.is_some() && args.link_kind == LinkKind::Usb {
            bail!(
                "--host-link makes this process the host on the USB link; a --link socket \
                 cannot be that port too (a UART0 --link is fine)"
            );
        }
        builder = builder.usb_sj_queue_source();
        let mut machine = builder
            .build()
            .map_err(|e| anyhow::anyhow!("building the machine: {e}"))?;
        install_alloc_trace(&mut machine, &args)?;
        print_seam_lines(&mut machine);
        announce_lan(&machine, lan.as_ref())?;
        let boot = format!(
            "esp32c6 {} boot, grade {}",
            machine.boot_mode().as_str(),
            machine.configuration_label()
        );
        let board = super::link_host::C6Board::new(machine)?;
        return super::run_hosted::run_hosted(board, boot, &args, micros);
    }

    let mut machine = builder
        .build()
        .map_err(|e| anyhow::anyhow!("building the machine: {e}"))?;
    install_alloc_trace(&mut machine, &args)?;
    print_seam_lines(&mut machine);
    announce_lan(&machine, lan.as_ref())?;

    let link = match args.link_kind {
        LinkKind::Usb => "usb-serial-jtag",
        LinkKind::Uart0 => "uart0",
    };
    match &args.link {
        Some(addr) => eprintln!(
            "emu: esp32c6 {} boot, grade {}, {link} on {addr} — connect with `lp-cli upload \
             <project> serial:tcp://{addr}`",
            machine.boot_mode().as_str(),
            machine.configuration_label(),
        ),
        None => eprintln!(
            "emu: esp32c6 {} boot, grade {}, no socket (console in memory; pass --link \
             <addr> to serve the {link} link)",
            machine.boot_mode().as_str(),
            machine.configuration_label(),
        ),
    }
    if let Some(over) = &args.over {
        let staging = machine.flash_staging();
        eprintln!(
            "emu: direct load over {} (nothing written): {} page(s) mapped, {} byte(s) of the \
             ELF's flash segments not in the image",
            over.display(),
            staging.pages.len(),
            staging.mismatched_bytes
        );
    }
    eprintln!(
        "emu: running for {micros} us of EMULATED time (wall-clock net: {} s)",
        args.wall_timeout_secs
    );

    let stop = StopCondition {
        stop_cycle: Some(micros * memmap::CYCLES_PER_US),
        exit_on: args.exit_on.clone(),
        wall_timeout: Some(std::time::Duration::from_secs(args.wall_timeout_secs)),
        probes: Vec::new(),
    };
    let outcome = machine.run_until(&stop);
    print_seam_lines(&mut machine);
    // A frame still open on a pad is reported as incomplete rather than
    // silently dropped.
    machine.flush_frames();
    // `--flash` promises a project uploaded in one run is still there in the
    // next, and that promise is kept HERE: the flash part is written back
    // when the run ends, however it ended — the standalone
    // `lp-emu-esp32c6` does the same, and `emu serve` does it per board.
    match machine.flush_flash() {
        Ok(true) => eprintln!("emu: flash image written back"),
        Ok(false) => {}
        Err(e) => eprintln!("emu: could not write the flash image back: {e}"),
    }

    let console = console_bytes(&machine, args.link_kind);
    if let Some(path) = &args.console {
        std::fs::write(path, &console)
            .with_context(|| format!("writing the console transcript to {}", path.display()))?;
        eprintln!(
            "emu: console → {} ({} bytes)",
            path.display(),
            console.len()
        );
    }

    eprintln!(
        "emu: {} — {} us emulated, {} instructions, {} bytes on the console ({})",
        describe(&outcome),
        machine.micros(),
        machine.instructions(),
        console.len(),
        machine.configuration_label(),
    );
    // Unmapped accesses, always, even on a clean run. An address no
    // peripheral claims reads as zero and the guest believes it — a run that
    // ends well with a hundred of them has told you less than it appears to,
    // and `--strict-bus` is the flag that turns each one into a fault with a
    // pc. Reported rather than gated because a walk is not a bring-up.
    let (reads, writes) = (machine.bus.unmapped_reads(), machine.bus.unmapped_writes());
    if reads + writes > 0 {
        eprintln!(
            "emu: {reads} unmapped read(s), {writes} unmapped write(s) at {} distinct site(s) — \
             each read zero and was believed. Re-run with --strict-bus to fault on them.",
            machine.bus.unmapped_sites(),
        );
    } else {
        eprintln!("emu: no unmapped accesses");
    }

    // The machine's own exit code, in this door's vocabulary. `Deadline` and
    // `ExitMatched` are the two ways a run ends well; everything else is a
    // finding, and saying which one it was is the whole value of the line.
    match outcome {
        Outcome::Deadline { .. } | Outcome::ExitMatched { .. } => Ok(()),
        other => bail!("the run ended at {}", describe(&other)),
    }
}

/// The USB host at power-on: `--usb-host` when given, otherwise whatever
/// makes the run's reader the one silicon would have.
///
/// When the USB port IS the `--link` socket, the socket's client is the
/// application, so the port starts closed and the client's connect opens it
/// (`attached-idle`). Draining from power-on with nobody connected would
/// succeed every guest write and replay them all to a late client, which a
/// board with no application on the port never does
/// (`docs/defects/2026-09-23-emulated-usb-port-drains-with-no-client-attached.md`).
/// `--monitor` declares a reader present from power-on, and with no socket
/// on the port (no `--link`, or a UART0 one) the emulator itself is that
/// reader: both are `attached`.
fn usb_host_at_power_on(args: &RunArgs) -> UsbHost {
    if let Some(host) = args.usb_host {
        return host.usb_host();
    }
    // The in-process host holds the port from power-on.
    if args.host_link {
        return UsbHostArg::Attached.usb_host();
    }
    let port_is_the_socket = args.link.is_some() && args.link_kind == LinkKind::Usb;
    if port_is_the_socket && !args.monitor {
        UsbHostArg::AttachedIdle.usb_host()
    } else {
        UsbHostArg::Attached.usb_host()
    }
}

/// A trace block filter no peripheral is called: `--seam-trace` keeps the
/// seams' note lines and none of the bus's.
const SEAM_TRACE_ONLY: &str = "(seam notes only)";

/// A trace sink that writes only the lines naming a seam (` SEAM `), each
/// as soon as it is whole.
struct SeamLines<W: std::io::Write> {
    out: W,
    line: Vec<u8>,
}

impl<W: std::io::Write> SeamLines<W> {
    fn new(out: W) -> Self {
        Self {
            out,
            line: Vec::new(),
        }
    }
}

impl<W: std::io::Write> std::io::Write for SeamLines<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &b in bytes {
            self.line.push(b);
            if b == b'\n' {
                if self.line.windows(6).any(|w| w == b" SEAM ") {
                    self.out.write_all(&self.line)?;
                }
                self.line.clear();
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// `emu run`'s one board on its `--lan`: participant 0, endpoint `0/net`.
const RUN_BOARD: ParticipantId = ParticipantId(0);

/// `--lan`: forward a loopback port to the board's LAN endpoint and say
/// where, as one line naming `lan:127.0.0.1:<port>` (what `lp-cli … lan:`
/// connects to). Nothing without `--lan`: the seam's own lines say the rest.
fn announce_lan(machine: &Esp32C6Machine, lan: Option<&(SharedLan, LanFixture)>) -> Result<()> {
    let Some((shared, fixture)) = lan else {
        return Ok(());
    };
    let at = forward_to_board(shared, machine.net_endpoint_id())
        .with_context(|| format!("--lan {}", fixture.path.display()))?;
    eprintln!(
        "emu: board on LAN {} ({}) · forward {} → board :{BOARD_LAN_PORT}",
        fixture.path.display(),
        fixture.describe(),
        forward_spec(at),
    );
    Ok(())
}

/// What the host saw, on whichever serial the link is.
fn console_bytes(machine: &Esp32C6Machine, kind: LinkKind) -> Vec<u8> {
    match kind {
        LinkKind::Usb => machine.usb_sj().bytes().to_vec(),
        LinkKind::Uart0 => machine.uart0().bytes().to_vec(),
    }
}

/// `--seams` (strict) and `--seams-prefer` (soft), folded into one request.
/// Neither given is the capability defaults (`net=lan`), softly; only
/// `none` asks for nothing, and an empty request scans nothing.
/// Shared with `serve`'s `seams=` / `seams_prefer=` board options.
pub(super) fn seam_request(
    strict: Option<&str>,
    prefer: Option<&str>,
) -> Result<lp_emu_esp_common::seam::SeamRequest> {
    use lp_emu_esp_common::seam::{SeamRequest, Strength};
    let mut request = SeamRequest::default();
    if let Some(text) = strict {
        request = request
            .with(text, Strength::Strict)
            .map_err(|e| anyhow::anyhow!("--seams: {e}"))?;
    }
    if let Some(text) = prefer {
        request = request
            .with(text, Strength::Soft)
            .map_err(|e| anyhow::anyhow!("--seams-prefer: {e}"))?;
    }
    Ok(request)
}

/// A chip start's `SEAM …` lines, as the machine produced them.
pub(super) fn print_seam_lines(machine: &mut Esp32C6Machine) {
    for line in machine.take_seam_lines() {
        eprintln!("emu: {line}");
    }
}

pub(super) fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Deadline { .. } => "reached its deadline".to_string(),
        Outcome::ExitMatched { .. } => "stopped on --exit-on".to_string(),
        Outcome::Fault { pc, fault, .. } => format!("faulted at {pc:#010x}: {fault:?}"),
        Outcome::StrictBus { violation } => format!("strict-bus refused an access: {violation:?}"),
        Outcome::Reset { source, strap, .. } => {
            format!("the chip asked to reset ({source}, into {strap:?})")
        }
        Outcome::WallTimeout { .. } => {
            "hit the wall-clock net — raise --wall-timeout, or lower --timeout".to_string()
        }
        Outcome::Breakpoint { pc, .. } => format!("stopped at a breakpoint, pc {pc:#010x}"),
        Outcome::DeepSleep { wake, .. } => {
            format!("guest entered deep sleep ({wake})")
        }
        Outcome::Seam { why, .. } => format!("a --seams seam cannot engage: {why}"),
    }
}

pub(super) fn check_file(path: &Path, flag: &str) -> Result<()> {
    if !path.is_file() {
        bail!("{flag} {} is not a file", path.display());
    }
    Ok(())
}

/// `5s`, `1500ms`, `900us` → microseconds. The unit is required: a bare
/// number would read as seconds to one caller and microseconds to the next,
/// and this clock is emulated, which is confusing enough already.
pub(super) fn parse_duration_us(text: &str) -> Result<u64> {
    let (digits, scale) = if let Some(d) = text.strip_suffix("ms") {
        (d, 1_000u64)
    } else if let Some(d) = text.strip_suffix("us") {
        (d, 1)
    } else if let Some(d) = text.strip_suffix('s') {
        (d, 1_000_000)
    } else {
        bail!("--timeout `{text}` has no unit — write 5s, 1500ms or 900us (emulated time)");
    };
    let n: u64 = digits
        .parse()
        .with_context(|| format!("--timeout `{text}`"))?;
    Ok(n * scale)
}

/// `--alloc-trace`: resolve the two hooks in the image's ELF (or
/// `--alloc-trace-elf`) and claim them. An ELF without them is refused by
/// name — a trace that silently recorded nothing would read as "no heap".
fn install_alloc_trace(
    machine: &mut lp_emu_esp32c6::machine::Esp32C6Machine,
    args: &RunArgs,
) -> Result<()> {
    let Some(out) = &args.alloc_trace else {
        return Ok(());
    };
    let Some(elf_path) = args.alloc_trace_elf.as_ref().or(args.elf.as_ref()) else {
        bail!("--alloc-trace needs the ELF that names its hooks: --alloc-trace-elf <p2.elf>");
    };
    let bytes =
        std::fs::read(elf_path).with_context(|| format!("reading {}", elf_path.display()))?;
    let elf = lp_emu_esp_common::elf::ElfImage::parse(&bytes)
        .map_err(|e| anyhow::anyhow!("{}: {e:?}", elf_path.display()))?;
    let find = |name: &str| -> Result<u32> {
        elf.symbol(name).map(|s| s.address).ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no `{name}`: build the image with fw-esp32c6's `alloc_trace_emu` feature",
                elf_path.display()
            )
        })
    };
    let (alloc_at, dealloc_at) = (find("_esp_alloc_alloc")?, find("_esp_alloc_dealloc")?);
    let file = std::fs::File::create(out)
        .with_context(|| format!("--alloc-trace: creating {}", out.display()))?;
    let header = vec![
        format!("elf {}", elf_path.display()),
        format!("hooks alloc {alloc_at:#010x} dealloc {dealloc_at:#010x}"),
        format!("configuration {}", machine.configuration_label()),
        "cycles_per_us 160".to_string(),
    ];
    let trace = lp_emu_esp32c6::alloc_trace::AllocTrace::new(
        Box::new(std::io::BufWriter::with_capacity(1 << 20, file)),
        &header,
    );
    lp_emu_esp32c6::alloc_trace::install(machine, alloc_at, dealloc_at, trace);
    // Optional: an image whose logger marks its records (`alloc-trace-marks`).
    let marks = elf.symbol("_lp_alloc_trace_mark").map(|s| s.address);
    if let Some(mark_at) = marks {
        lp_emu_esp32c6::alloc_trace::install_marks(machine, mark_at);
    }
    eprintln!(
        "emu: alloc trace: log-record marks {}",
        match marks {
            Some(at) => format!("at {at:#010x} (exact points)"),
            None => "absent (points are console arrival)".to_string(),
        }
    );
    eprintln!(
        "emu: alloc trace -> {} (hooks {alloc_at:#010x} / {dealloc_at:#010x} from {})",
        out.display(),
        elf_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn a_usb_link_socket_starts_with_the_port_closed() {
        assert_eq!(
            host(&["--link", "127.0.0.1:5591"]),
            UsbHost::Attached { draining: false },
            "a client of the socket is the application: nobody reads before it connects"
        );
    }

    #[test]
    fn a_reader_from_power_on_is_attached_and_draining() {
        let draining = UsbHost::Attached { draining: true };
        // No socket on the port: the emulator is the reader (`--console`).
        assert_eq!(host(&[]), draining);
        assert_eq!(
            host(&["--link", "127.0.0.1:5591", "--link-kind", "uart0"]),
            draining
        );
        // `--monitor` says a reader holds the port the whole run.
        assert_eq!(host(&["--link", "127.0.0.1:5591", "--monitor"]), draining);
        // And `attached` is the explicit opt-in to the replay.
        assert_eq!(
            host(&["--link", "127.0.0.1:5591", "--usb-host", "attached"]),
            draining
        );
    }

    #[test]
    fn usb_host_is_honoured_as_given() {
        assert_eq!(host(&["--usb-host", "absent"]), UsbHost::Absent);
        assert_eq!(
            host(&["--usb-host", "attached-idle"]),
            UsbHost::Attached { draining: false }
        );
    }

    #[test]
    fn monitor_refuses_a_usb_host() {
        let parsed = Cli::try_parse_from([
            "run",
            "--elf",
            "fw",
            "--monitor",
            "--usb-host",
            "attached-idle",
        ]);
        assert!(
            parsed.is_err(),
            "--monitor already says the host is attached"
        );
    }

    #[test]
    fn the_seam_trace_keeps_only_the_seams_lines() {
        use std::io::Write;
        let mut sink = SeamLines::new(Vec::new());
        sink.write_all(b"cyc=1 pc=0x1 SEAM net=lan link\ncyc=2 pc=0x2 WATCHPOINT slot=0\n")
            .unwrap();
        sink.write_all(b"cyc=3 SEAM net=lan ev").unwrap();
        sink.write_all(b"ent associated\n").unwrap();
        assert_eq!(
            String::from_utf8(sink.out).unwrap(),
            "cyc=1 pc=0x1 SEAM net=lan link\ncyc=3 SEAM net=lan event associated\n"
        );
    }

    #[test]
    fn a_pace_is_the_c6s_alone() {
        for chip in ["esp32s3", "esp32v3"] {
            let args = Cli::try_parse_from(["run", "--elf", "fw", "--chip", chip, "--pace", "max"])
                .expect("parses")
                .run;
            let err = run(args).unwrap_err();
            assert!(format!("{err:#}").contains("--pace is the C6's"), "{err:#}");
        }
    }

    #[test]
    fn a_duration_needs_a_unit() {
        assert_eq!(parse_duration_us("5s").unwrap(), 5_000_000);
        assert_eq!(parse_duration_us("1500ms").unwrap(), 1_500_000);
        assert_eq!(parse_duration_us("900us").unwrap(), 900);
        assert!(parse_duration_us("100").is_err());
        assert!(parse_duration_us("s").is_err());
    }

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        run: RunArgs,
    }

    fn host(extra: &[&str]) -> UsbHost {
        let mut argv = vec!["run", "--elf", "fw"];
        argv.extend_from_slice(extra);
        usb_host_at_power_on(&Cli::try_parse_from(argv).expect("parses").run)
    }
}
