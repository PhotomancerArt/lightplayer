//! `lp-cli emu serve` — N emulated C6 boards behind a WebSocket door.
//!
//! `run` is one image, one socket, a deadline. `serve` is a **registry**: it
//! holds N named boards, outlives any one of them, and exposes each as the
//! two endpoints the TCP pair already is —
//!
//! ```text
//! GET  /boards                 → the registry, as JSON
//! WS   /board/<id>/bytes       → binary frames, both ways, bytes and nothing else
//! WS   /board/<id>/control     → the control line protocol, verbatim
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
//! **Nothing here is deterministic and nothing here is a gate.** A socket's
//! command lands at whichever slice boundary the poll fell on
//! (`lp-emu/esp/README.md` §Determinism); the reply names the cycle so a
//! session is auditable, and that is all a cycle in this door is for.

mod air;
mod board;
mod door;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use lp_emu_esp32c6::loader::EfuseIdentity;

use lp_emu_esp32c6::machine::UsbHost;

use super::args::{EmuChip, ServeArgs, ServeHost};
use board::{Board, BoardOptions, BoardSpec, default_mac, format_mac};
use door::Registry;

pub fn serve(args: ServeArgs) -> Result<()> {
    let EmuChip::Esp32C6 = args.chip;

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

    let specs = parse_boards(
        &args.board,
        args.state_dir.as_deref(),
        args.console_dir.as_deref(),
    )?;

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
    for (seat, spec) in specs.into_iter().enumerate() {
        let options = BoardOptions {
            grade: args.time_grade.time_grade(),
            strict_bus: args.strict_bus,
            usb_host: match args.usb_host {
                ServeHost::Attached => UsbHost::Attached { draining: true },
                ServeHost::AttachedIdle => UsbHost::Attached { draining: false },
                ServeHost::Absent => UsbHost::Absent,
            },
            air: air.clone(),
            air_seat: seat,
        };
        let id = spec.id.clone();
        let flash = spec.flash.clone();
        let board = Board::start(spec, options)?;
        eprintln!(
            "emu serve: board `{id}` mac {} flash {}{} — bytes /board/{id}/bytes, control \
             /board/{id}/control",
            format_mac(&board.mac),
            board.flash_state,
            flash
                .as_ref()
                .map(|p| format!(" ({})", p.display()))
                .unwrap_or_default(),
        );
        boards.push(board);
    }

    let registry = Arc::new(Registry { boards });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building the tokio runtime for the WebSocket door")?;

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

/// `--board <id>=<image>[,mac=<aa:bb:…>][,kind=elf|merged]`.
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
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "--board `{text}`: `{id}` is not a board id — letters, digits, `-` and `_`, because \
             the id is a path segment"
        );
    }

    let mut parts = rest.split(',');
    let image = PathBuf::from(parts.next().unwrap_or_default().trim());
    if image.as_os_str().is_empty() {
        bail!("--board `{text}`: no image");
    }

    let mut mac = default_mac(index);
    let mut merged = false;
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
            Some(("kind", "elf")) => merged = false,
            Some(("kind", "merged")) => merged = true,
            _ => bail!(
                "--board `{text}`: `{option}` is not a board option — mac=<aa:bb:cc:dd:ee:ff> or \
                 kind=elf|merged"
            ),
        }
    }

    // A merged image is the whole chip, so a flash file beside it would be a
    // second one — the same refusal `run` makes, per board.
    let flash = match (merged, state_dir) {
        (true, _) => None,
        (false, Some(dir)) => Some(dir.join(format!("{id}.flash.bin"))),
        (false, None) => None,
    };
    if merged && state_dir.is_some() {
        eprintln!(
            "emu serve: board `{id}` is kind=merged, which carries the whole chip — it keeps its \
             own flash and ignores --state-dir"
        );
    }

    Ok(BoardSpec {
        id: id.to_string(),
        image,
        merged,
        mac,
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
        assert!(spec.merged);
        assert_eq!(spec.flash, None, "--merged is the whole chip already");
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
