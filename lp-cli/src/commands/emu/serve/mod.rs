//! `lp-cli emu serve` — N emulated C6 boards behind a WebSocket door.
//!
//! `run` is one image, one socket, a deadline. `serve` is a **registry**: it
//! holds N named boards, outlives any one of them, and exposes each as the
//! two endpoints the TCP pair already is —
//!
//! ```text
//! GET  /boards                 → the registry, as JSON
//! WS   /board/<id>/bytes       → binary frames, both ways, bytes and nothing else
//! WS   /board/<id>/control     → the control line protocol, verbatim (+ `renumber`)
//! GET  /lans/<name>/browse     → a DNS-SD browse on a served LAN, as JSON
//! ```
//!
//! — plus one flash file and one eFuse MAC per board, so `s9-two-boards` is
//! two boards rather than one board twice, and `s1-blank-flash → flash →
//! s3-current-fw` is a sequence rather than three unrelated runs (PD8).
//!
//! ```text
//! lp-cli emu serve --board c6-a=target/emu-ref/…/fw-esp32c6 \
//!                  --board c6-b=target/emu-ref/…/fw-esp32c6 \
//!                  --listen 127.0.0.1:5599 --state-dir target/emu-serve
//! lp-cli upload projects/test/basic serial:ws://127.0.0.1:5599/board/c6-a/bytes
//! ```
//!
//! **How a board is held** (plan two M1, approach (a)): each board runs on
//! its own OS thread with its byte link and control channel bound on
//! ephemeral loopback TCP ports, and the door is a byte pump between a
//! WebSocket and those ports. Nothing under `lp-emu/` changes, and the
//! coupling rule, the attach/detach rule and the one-reply-per-command rule
//! survive *literally* rather than by re-implementation — which is the whole
//! reason the shim is glue. The cost is a loopback hop per byte, which is
//! what a shim costs.
//!
//! **Virtual LANs** (Wi-Fi plan P12): `--lan <name>=<fixture.toml>` declares
//! one, and every board whose spec says `lan=<name>` shares it, each with its
//! own lease and its own **forward** — a loopback port carried to the board's
//! LAN endpoint, listed in `GET /boards` as `forward`
//! (`lan:127.0.0.1:<port>`). The forward is a separate door: the USB door
//! still admits one client per board. See [`served_lan`].
//!
//! ```text
//! lp-cli emu serve --lan home=lan.toml \
//!                  --board c6-a=…/merged.bin,kind=rom-up,lan=home \
//!                  --board c6-b=…/merged.bin,kind=rom-up,lan=home
//! ```
//!
//! **Nothing here is deterministic and nothing here is a gate.** A socket's
//! command lands at whichever slice boundary the poll fell on
//! (`lp-emu/esp/README.md` §Determinism); the reply names the cycle so a
//! session is auditable, and that is all a cycle in this door is for.

mod air;
mod board;
mod door;
mod lan_browse;
mod parent_watch;
mod served_lan;
mod wire_tap;
mod wire_tear;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use lp_emu_esp32c6::loader::EfuseIdentity;

use super::args::{EmuChip, PaceArg, ServeArgs};
use board::{Board, BoardKind, BoardOptions, BoardSpec, default_mac, format_mac};
use door::Registry;
use served_lan::{LanSeat, ServedLan, is_path_segment, parse_lans};

pub fn serve(args: ServeArgs) -> Result<()> {
    // A bad fault spec is refused here, before any board starts.
    wire_tear::WireTear::from_env()?;
    if args.chip != EmuChip::Esp32C6 {
        anyhow::bail!(
            "`emu serve` holds C6 boards only; the S3 and the classic are `emu run --chip \
             esp32s3|esp32v3 --host-link`"
        );
    }

    if args.board.is_empty() {
        bail!(
            "nothing to serve: pass --board <id>=<image> at least once, for example \
             `--board c6-a=target/emu-ref/esp32c6+server+radio/fw-esp32c6`"
        );
    }
    if let Some(dir) = &args.state_dir {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("--state-dir: creating {}", dir.display()))?;
    }

    if let Some(dir) = &args.console_dir {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("--console-dir: creating {}", dir.display()))?;
    }

    let mut specs = parse_boards(
        &args.board,
        args.state_dir.as_deref(),
        args.console_dir.as_deref(),
    )?;
    let lans = parse_lans(&args.lan)?;
    check_lans(&specs, &lans)?;
    // `--pace` is every board's, unless its own `pace=` says otherwise.
    if let Some(pace) = args.pace {
        for spec in &mut specs {
            spec.pace.get_or_insert(pace.pace());
        }
    }

    let air = match &args.air {
        Some(addr) => {
            let tap = air::AirTap::bind(addr)?;
            eprintln!(
                "emu serve: --air on {} — AUDITABLE ONLY: frames the boards' radios hand over, \
                 one way, never delivered and never a transcript",
                tap.local_addr()
            );
            Some(tap)
        }
        None => None,
    };

    let mut boards = Vec::with_capacity(specs.len());
    for lan in &lans {
        eprintln!(
            "emu serve: LAN `{}` from {} ({}) — on the host's clock, not deterministic",
            lan.name,
            lan.fixture.path.display(),
            lan.fixture.describe(),
        );
    }
    for (seat, spec) in specs.into_iter().enumerate() {
        let lan = spec.lan.as_deref().map(|name| {
            let served = lans
                .iter()
                .find(|l| l.name == name)
                .expect("check_lans named every LAN a board names");
            LanSeat {
                name: name.to_string(),
                lan: served.lan.clone(),
                participant: lp_emu_esp_common::ParticipantId(seat),
            }
        });
        let options = BoardOptions {
            grade: args.time_grade.time_grade(),
            strict_bus: args.strict_bus,
            usb_host: args.usb_host.usb_host(),
            air: air.clone(),
            air_seat: seat,
            lpperi_clk_en: args.lpperi_clk_en,
            lan,
        };
        let id = spec.id.clone();
        let flash = spec.flash.clone();
        let boot = spec.kind.boot_word();
        let board = Board::start(spec, options)?;
        eprintln!(
            "emu serve: board `{id}` mac {} boot {boot} flash {}{} — bytes /board/{id}/bytes, \
             control /board/{id}/control{}",
            format_mac(&board.mac),
            board.flash_state(),
            flash
                .as_ref()
                .map(|p| format!(" ({})", p.display()))
                .unwrap_or_default(),
            board
                .lan
                .as_ref()
                .map(|l| format!(", LAN `{}` forward {}", l.name, l.forward_spec()))
                .unwrap_or_default(),
        );
        boards.push(board);
    }

    let registry = Arc::new(Registry { boards, lans });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building the tokio runtime for the WebSocket door")?;

    // Opt-in (`LP_EMU_PARENT_PID`): exit when the process that started us dies.
    let parent = parent_watch::parent_pid_from_env()?;

    let door_registry = Arc::clone(&registry);
    let listen = args.listen.clone();
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&listen)
            .await
            .with_context(|| format!("--listen: binding {listen}"))?;
        let local = listener.local_addr().context("--listen: reading it back")?;
        // The bound address, always, and on stderr: `--listen 127.0.0.1:0`
        // is the right thing for a test and for a second server on one box,
        // and a port nobody can read back is a port nobody can use.
        eprintln!("emu serve: listening on http://{local}");
        eprintln!("emu serve: GET http://{local}/boards lists the boards");

        let door = door::run(listener, door_registry);
        tokio::select! {
            () = door => {}
            pid = parent_watch::gone(parent) => {
                eprintln!("emu serve: parent {pid} gone — exiting");
            }
            signal = tokio::signal::ctrl_c() => {
                match signal {
                    Ok(()) => eprintln!("emu serve: interrupted; writing every board's flash back"),
                    Err(e) => eprintln!("emu serve: could not watch for Ctrl-C ({e}); stopping"),
                }
            }
        }
        anyhow::Ok(())
    })?;

    // `Board`'s `Drop` stops the thread and flushes, but doing it here means
    // the message below is true when it is printed.
    for board in &registry.boards {
        board.stop();
    }
    eprintln!("emu serve: stopped; every board's flash is written back");
    Ok(())
}

/// Every LAN a board names is declared, and a LAN no board names says so.
fn check_lans(specs: &[BoardSpec], lans: &[ServedLan]) -> Result<()> {
    for spec in specs {
        if let Some(name) = &spec.lan
            && !lans.iter().any(|l| &l.name == name)
        {
            bail!(
                "--board `{}`: lan={name}, but no `--lan {name}=<fixture.toml>` declares it",
                spec.id
            );
        }
    }
    for lan in lans {
        if !specs
            .iter()
            .any(|s| s.lan.as_deref() == Some(lan.name.as_str()))
        {
            eprintln!(
                "emu serve: LAN `{}` has no boards (a board joins it with `,lan={}` in its \
                 --board)",
                lan.name, lan.name
            );
        }
    }
    Ok(())
}

/// `--board <id>=<image>[,mac=<aa:bb:…>][,kind=elf|merged|rom-up][,lan=<name>][,pace=realtime|max]`.
fn parse_boards(
    specs: &[String],
    state_dir: Option<&Path>,
    console_dir: Option<&Path>,
) -> Result<Vec<BoardSpec>> {
    let mut out: Vec<BoardSpec> = Vec::with_capacity(specs.len());
    for (index, text) in specs.iter().enumerate() {
        let spec = parse_board(text, index, state_dir, console_dir)?;
        if out.iter().any(|b| b.id == spec.id) {
            bail!(
                "--board `{}`: two boards cannot share the id `{}` — the id is the endpoint path",
                text,
                spec.id
            );
        }
        if out.iter().any(|b| b.mac == spec.mac) {
            bail!(
                "--board `{}`: two boards cannot share the MAC {} — a registry of N boards that \
                 answer with one identity is one board N times. Spell `mac=` on one of them.",
                text,
                format_mac(&spec.mac)
            );
        }
        out.push(spec);
    }
    Ok(out)
}

fn parse_board(
    text: &str,
    index: usize,
    state_dir: Option<&Path>,
    console_dir: Option<&Path>,
) -> Result<BoardSpec> {
    let (id, rest) = text.split_once('=').with_context(|| {
        format!("--board `{text}`: expected <id>=<image>, for example c6-a=target/…/fw-esp32c6")
    })?;
    let id = id.trim();
    if !is_path_segment(id) {
        bail!(
            "--board `{text}`: `{id}` is not a board id — letters, digits, `-` and `_`, because \
             the id is a path segment"
        );
    }

    let mut parts = rest.split(',');
    let image_text = parts.next().unwrap_or_default().trim().to_string();

    let mut mac = default_mac(index);
    let mut kind = BoardKind::Elf;
    let mut seams_strict: Option<String> = None;
    let mut seams_prefer: Option<String> = None;
    let mut lan: Option<String> = None;
    let mut pace: Option<lp_emu_esp_common::seam::net::Pace> = None;
    let mut flash_cut: Option<lp_emu_esp32c6::flash_cut_spec::FlashCutSpec> = None;
    for option in parts {
        let option = option.trim();
        if option.is_empty() {
            continue;
        }
        match option.split_once('=') {
            Some(("mac", value)) => {
                mac = EfuseIdentity::parse_mac(value)
                    .map_err(|e| anyhow::anyhow!("--board `{text}`: mac=: {e}"))?;
            }
            Some(("kind", "elf")) => kind = BoardKind::Elf,
            Some(("kind", "merged")) => kind = BoardKind::Merged,
            Some(("kind", "rom-up")) => kind = BoardKind::RomUp,
            Some(("seams", value)) => seams_strict = Some(value.to_string()),
            Some(("seams_prefer", value)) => seams_prefer = Some(value.to_string()),
            Some(("lan", value)) if is_path_segment(value) => lan = Some(value.to_string()),
            Some(("pace", value)) => {
                pace = Some(
                    PaceArg::parse(value)
                        .map_err(|e| anyhow::anyhow!("--board `{text}`: pace=: {e}"))?
                        .pace(),
                );
            }
            // `run --flash-cut`'s spec, its own options joined with `;`
            // (`flash_cut=40:calibrated:7;then=power-cycle`): a `,` here
            // already ends a board option.
            Some(("flash_cut", value)) => {
                flash_cut = Some(
                    lp_emu_esp32c6::flash_cut_spec::FlashCutSpec::parse(value)
                        .map_err(|e| anyhow::anyhow!("--board `{text}`: flash_cut=: {e}"))?,
                );
            }
            _ => bail!(
                "--board `{text}`: `{option}` is not a board option — mac=<aa:bb:cc:dd:ee:ff>, \
                 kind=elf|merged|rom-up, seams=<atoms|none>, seams_prefer=<atoms>, \
                 lan=<name>, pace=realtime|max or \
                 flash_cut=<n>:<model>:<seed>[;range=<off>+<len>][;then=stop|power-cycle]"
            ),
        }
    }

    // `blank` is the only reserved image word, and it means what `GET
    // /boards` already calls a chip with nothing on it. A file of that name
    // is still reachable as `./blank`.
    let image = match (kind, image_text.as_str()) {
        (BoardKind::RomUp, "blank" | "") => None,
        (_, "") => bail!(
            "--board `{text}`: no image. `kind=rom-up` may take `blank` for a chip with nothing \
             on it; every other kind needs a file."
        ),
        (_, path) => Some(PathBuf::from(path)),
    };

    // A merged image is the whole chip, so a flash file beside it would be a
    // second one — the same refusal `run` makes, per board. A `rom-up` board
    // is the opposite case: its flash file IS the chip, so it wants one more
    // than anybody.
    let flash = match (kind, state_dir) {
        (BoardKind::Merged, _) | (_, None) => None,
        (_, Some(dir)) => Some(dir.join(format!("{id}.flash.bin"))),
    };
    if matches!(kind, BoardKind::Merged) && state_dir.is_some() {
        eprintln!(
            "emu serve: board `{id}` is kind=merged, which carries the whole chip — it keeps its \
             own flash and ignores --state-dir"
        );
    }
    // A rom-up board with no `--state-dir` still boots from the reset vector
    // and is still writable; it just forgets. Worth one line, because a
    // flashing walk that forgets is a walk whose gate 6 cannot pass.
    if matches!(kind, BoardKind::RomUp) && state_dir.is_none() {
        eprintln!(
            "emu serve: board `{id}` is kind=rom-up with no --state-dir — it boots ROM-up from a \
             writable chip that lives and dies with this process, so anything flashed into it is \
             gone when the server stops"
        );
    }

    let seams = super::handler::seam_request(seams_strict.as_deref(), seams_prefer.as_deref())
        .with_context(|| format!("--board `{text}`"))?;

    Ok(BoardSpec {
        id: id.to_string(),
        image,
        kind,
        mac,
        seams,
        lan,
        pace,
        flash_cut,
        flash,
        console: console_dir.map(|dir| dir.join(format!("{id}.console.log"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_is_an_id_an_image_and_a_flash_file() {
        let dir = PathBuf::from("/tmp/state");
        let specs = parse_boards(
            &["c6-a=fw-esp32c6".to_string(), "c6-b=fw-esp32c6".to_string()],
            Some(&dir),
            None,
        )
        .expect("two boards");
        assert_eq!(specs[0].id, "c6-a");
        assert_eq!(specs[0].flash, Some(dir.join("c6-a.flash.bin")));
        assert_eq!(specs[1].flash, Some(dir.join("c6-b.flash.bin")));
        assert_ne!(specs[0].mac, specs[1].mac, "two boards, two identities");
    }

    /// A plain board asks for the capability defaults (`net=lan`) softly
    /// and nothing else: no performance seam unless spelled, and `none`
    /// turns even the defaults off. The atom keeps its `=`.
    #[test]
    fn a_board_asks_only_for_the_capability_defaults_unless_spelled() {
        use lp_emu_esp_common::seam::Strength;
        let atoms = |spec: &BoardSpec| -> Vec<(String, Strength)> {
            spec.seams
                .wanted()
                .iter()
                .map(|(i, s)| (i.atom(), *s))
                .collect()
        };
        let plain = parse_board("c6-a=fw", 0, None, None).expect("parses");
        assert_eq!(
            atoms(&plain),
            vec![("net=lan".to_string(), Strength::Soft)],
            "the capability defaults, softly; no performance seam (PD4)"
        );
        let soft = parse_board("c6-a=fw,seams_prefer=led=fast", 0, None, None).expect("parses");
        assert_eq!(
            atoms(&soft),
            vec![
                ("led=fast".to_string(), Strength::Soft),
                ("net=lan".to_string(), Strength::Soft)
            ]
        );
        let strict =
            parse_board("c6-a=fw,kind=rom-up,seams=led=fast", 0, None, None).expect("parses");
        assert_eq!(
            atoms(&strict)[0],
            ("led=fast".to_string(), Strength::Strict)
        );
        assert!(
            parse_board("c6-a=fw,seams=none", 0, None, None)
                .unwrap()
                .seams
                .is_empty()
        );
        let err = parse_board("c6-a=fw,seams=led=slow", 0, None, None).unwrap_err();
        assert!(format!("{err:#}").contains("led=fast"), "{err:#}");
    }

    #[test]
    fn a_board_names_its_lan_and_the_lan_must_be_declared() {
        let on = parse_board("c6-a=fw,kind=rom-up,lan=home", 0, None, None).expect("parses");
        assert_eq!(on.lan.as_deref(), Some("home"));
        assert_eq!(parse_board("c6-a=fw", 0, None, None).unwrap().lan, None);
        assert!(parse_board("c6-a=fw,lan=", 0, None, None).is_err());
        assert!(parse_board("c6-a=fw,lan=ho/me", 0, None, None).is_err());
        let err = check_lans(&[on], &[]).unwrap_err();
        assert!(format!("{err:#}").contains("--lan home="), "{err:#}");
    }

    #[test]
    fn a_board_may_set_its_pace_and_leaves_it_unset_by_default() {
        use lp_emu_esp_common::seam::net::Pace;
        assert_eq!(parse_board("c6-a=fw", 0, None, None).unwrap().pace, None);
        let real = parse_board("c6-a=fw,lan=home,pace=realtime", 0, None, None).expect("parses");
        assert_eq!(real.pace, Some(Pace::Realtime));
        assert_eq!(real.lan.as_deref(), Some("home"), "beside lan=");
        let max = parse_board("c6-a=fw,pace=max,seams=none", 0, None, None).expect("parses");
        assert_eq!(max.pace, Some(Pace::Max));
        let err = parse_board("c6-a=fw,pace=fast", 0, None, None).unwrap_err();
        assert!(format!("{err:#}").contains("realtime or max"), "{err:#}");
    }

    #[test]
    fn a_board_may_arm_a_flash_cut_with_its_options_joined_by_semicolons() {
        use lp_emu_esp32c6::flash_cut_spec::{AfterCut, FlashCutSpec};
        assert_eq!(
            parse_board("c6-a=fw", 0, None, None).unwrap().flash_cut,
            None
        );
        let cut = parse_board(
            "c6-a=fw,flash_cut=40:calibrated:7;then=power-cycle,seams=none",
            0,
            None,
            None,
        )
        .expect("parses");
        let mut want = FlashCutSpec::new(
            40,
            lp_emu_esp_common::engine::flash_cut::TearModel::Calibrated,
            7,
        );
        want.then = AfterCut::PowerCycle;
        assert_eq!(cut.flash_cut, Some(want));
        assert!(cut.seams.is_empty(), "the next board option still parses");
        let err = parse_board("c6-a=fw,flash_cut=1:gentle:1", 0, None, None).unwrap_err();
        assert!(format!("{err:#}").contains("no tear model"), "{err:#}");
    }

    #[test]
    fn a_spelled_mac_wins_and_a_repeat_is_refused() {
        let one = parse_board("c6-a=fw,mac=aa:bb:cc:dd:ee:ff", 0, None, None).expect("parses");
        assert_eq!(format_mac(&one.mac), "aa:bb:cc:dd:ee:ff");
        let clash = parse_boards(
            &[
                "c6-a=fw,mac=aa:bb:cc:dd:ee:ff".to_string(),
                "c6-b=fw,mac=aa:bb:cc:dd:ee:ff".to_string(),
            ],
            None,
            None,
        );
        assert!(clash.is_err(), "one identity twice is one board twice");
    }

    #[test]
    fn a_merged_board_keeps_its_own_flash() {
        let dir = PathBuf::from("/tmp/state");
        let spec = parse_board("c6-a=chip.bin,kind=merged", 0, Some(&dir), None).expect("parses");
        assert_eq!(spec.kind, BoardKind::Merged);
        assert_eq!(spec.flash, None, "--merged is the whole chip already");
    }

    /// The third combination, and plan two's criterion 5: a board that boots
    /// from the reset vector out of a flash file it can be flashed into.
    #[test]
    fn a_rom_up_board_boots_from_a_writable_flash_file() {
        let dir = PathBuf::from("/tmp/state");
        let spec = parse_board("c6-a=chip.bin,kind=rom-up", 0, Some(&dir), None).expect("parses");
        assert_eq!(spec.kind, BoardKind::RomUp);
        assert_eq!(
            spec.flash,
            Some(dir.join("c6-a.flash.bin")),
            "a rom-up board's flash file IS the chip"
        );
        assert_eq!(
            spec.image,
            Some(PathBuf::from("chip.bin")),
            "the image seeds the chip the first time"
        );
        assert_eq!(spec.kind.boot_word(), "rom-up");
    }

    /// `blank` is the only reserved image word, and it is the word `GET
    /// /boards` already uses for a chip with nothing on it.
    #[test]
    fn a_blank_rom_up_board_has_no_image_at_all() {
        let dir = PathBuf::from("/tmp/state");
        let spec = parse_board("c6-a=blank,kind=rom-up", 0, Some(&dir), None).expect("parses");
        assert_eq!(spec.kind, BoardKind::RomUp);
        assert_eq!(spec.image, None, "nothing seeds it: the chip is erased");
        assert_eq!(spec.flash, Some(dir.join("c6-a.flash.bin")));
        // `blank` only means "no image" for a rom-up board; every other kind
        // gets a file called `blank`, because that is what it was told.
        let elf = parse_board("c6-b=blank", 1, Some(&dir), None).expect("parses");
        assert_eq!(elf.image, Some(PathBuf::from("blank")));
    }

    #[test]
    fn an_id_is_a_path_segment() {
        assert!(parse_board("c6/a=fw", 0, None, None).is_err());
        assert!(parse_board("=fw", 0, None, None).is_err());
        assert!(parse_board("c6-a", 0, None, None).is_err());
        assert!(parse_board("c6-a=fw,nonsense=1", 0, None, None).is_err());
    }

    #[test]
    fn two_boards_may_not_share_an_id() {
        let clash = parse_boards(
            &["c6-a=one".to_string(), "c6-a=two".to_string()],
            None,
            None,
        );
        assert!(clash.is_err(), "the id is the endpoint path");
    }
}
