//! One emulated board, on its own OS thread, behind two loopback TCP doors.
//!
//! The machine is built exactly as `lp-cli emu run` builds one — same
//! [`apply_image`](super::super::handler::apply_image), same grade, same
//! strictness — with three differences that are the whole of `serve`:
//!
//! 1. its byte link and its control channel are bound on **ephemeral
//!    loopback ports** (`127.0.0.1:0`), read back through
//!    [`Esp32C6Machine::usb_sj_tcp`] and [`Esp32C6Machine::control_tcp`], so
//!    the WebSocket door in [`super::door`] is a pure byte pump and **nothing
//!    under `lp-emu/` changes**;
//! 2. `--reboot-on-reset` is on (plan two PD11): a `setSignals()` dance that
//!    kills the server is not a board;
//! 3. it has its own eFuse MAC and its own flash file, because a registry of
//!    N boards that all answer with one identity is one board N times.
//!
//! The run loop is sliced by **wall** time rather than by a cycle deadline:
//! a server has no deadline, and [`Esp32C6Machine::reboot`] restarts the
//! guest's clock, so an absolute `stop_cycle` would mean something different
//! after every reset. Nothing here asserts a cycle — a socket is not
//! deterministic and `lp-emu/esp/README.md` §Determinism says so.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::machine::{
    Esp32C6Builder, Outcome, StopCondition, TimeGrade, UsbHost, UsbSjDrain, UsbSjSink,
};

use super::air::AirTap;
use crate::commands::emu::handler::{apply_image, describe};

/// How long one `run_until` call is allowed to hold the thread before the
/// loop gets a turn: the shutdown flag, the flash flush, the air drain. Short
/// enough that a Ctrl-C is answered promptly, long enough that the check is
/// not the run's cost.
const SLICE: Duration = Duration::from_millis(40);

/// Flash is written back on this cadence as well as on shutdown and whenever
/// a byte client hangs up. PD8: `s1-blank-flash → flash → s3-current-fw` is a
/// *sequence*, and a server that is killed must not lose the project a walk
/// just uploaded.
const FLUSH_EVERY: Duration = Duration::from_secs(2);

/// What one `--board` asked for.
#[derive(Clone, Debug)]
pub struct BoardSpec {
    pub id: String,
    pub image: PathBuf,
    /// A whole merged flash image rather than an ELF: boot from the reset
    /// vector through the real mask ROM.
    pub merged: bool,
    pub mac: [u8; 6],
    /// The persistent flash file, `None` for a merged board (which carries
    /// the whole chip already) and for a serve with no `--state-dir`.
    pub flash: Option<PathBuf>,
    /// Where to rewrite this board's console transcript, `None` with no
    /// `--console-dir`.
    pub console: Option<PathBuf>,
}

/// Everything the door needs about a board, plus the handle that stops it.
pub struct Board {
    pub id: String,
    pub mac: [u8; 6],
    /// `blank` / `loaded` / `merged` — what the board's flash was at
    /// power-on, which is what `GET /boards` reports.
    pub flash_state: &'static str,
    pub bytes_addr: SocketAddr,
    pub control_addr: SocketAddr,
    /// One byte client at a time, like [`lp_emu_esp_common::TcpHost`]. A
    /// second WebSocket on `/bytes` is refused, never silently multiplexed.
    pub bytes_busy: Arc<AtomicBool>,
    pub control_busy: Arc<AtomicBool>,
    /// Set by the door when a byte client hangs up: the application closed
    /// the port, which is the natural moment to write the flash back.
    pub flush_now: Arc<AtomicBool>,
    /// Set once the machine's run loop has stopped for a reason of its own —
    /// a fault, a strict refusal, a reset it could not perform. The board is
    /// still listed, and says so.
    pub stopped: Arc<AtomicBool>,
    pub reboots: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    thread: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// The knobs a whole `serve` shares across its boards.
#[derive(Clone)]
pub struct BoardOptions {
    pub grade: TimeGrade,
    pub strict_bus: bool,
    /// The USB host's state at power-on.
    pub usb_host: UsbHost,
    pub air: Option<Arc<AirTap>>,
    pub air_seat: usize,
}

impl Board {
    /// Build the machine, bind its two loopback doors, and start it running.
    ///
    /// Returns once the ports are known, so the caller can publish them
    /// before the guest has booted.
    pub fn start(spec: BoardSpec, options: BoardOptions) -> Result<Board> {
        let flash_state = if spec.merged {
            "merged"
        } else if spec
            .flash
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .is_some_and(|m| m.len() > 0)
        {
            "loaded"
        } else {
            "blank"
        };

        let shutdown = Arc::new(AtomicBool::new(false));
        let flush_now = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let reboots = Arc::new(AtomicU64::new(0));
        let (tx, rx) = std::sync::mpsc::channel::<Result<(SocketAddr, SocketAddr)>>();

        let id = spec.id.clone();
        let mac = spec.mac;
        let thread = {
            let shutdown = Arc::clone(&shutdown);
            let flush_now = Arc::clone(&flush_now);
            let stopped = Arc::clone(&stopped);
            let reboots = Arc::clone(&reboots);
            std::thread::Builder::new()
                .name(format!("emu-board-{id}"))
                .spawn(move || {
                    run_board(spec, options, tx, shutdown, flush_now, stopped, reboots);
                })
                .context("spawning the board thread")?
        };

        let (bytes_addr, control_addr) = rx
            .recv()
            .map_err(|_| anyhow!("board `{id}` died before it bound its sockets"))??;

        Ok(Board {
            id,
            mac,
            flash_state,
            bytes_addr,
            control_addr,
            bytes_busy: Arc::new(AtomicBool::new(false)),
            control_busy: Arc::new(AtomicBool::new(false)),
            flush_now,
            stopped,
            reboots,
            shutdown,
            thread: std::sync::Mutex::new(Some(thread)),
        })
    }

    /// Ask the board to stop, and wait for it to write its flash back.
    /// Idempotent: the second call finds the thread already joined.
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let thread = self
            .thread
            .lock()
            .expect("board thread handle poisoned")
            .take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

impl Drop for Board {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments, reason = "one thread body, one call site")]
fn run_board(
    spec: BoardSpec,
    options: BoardOptions,
    tx: std::sync::mpsc::Sender<Result<(SocketAddr, SocketAddr)>>,
    shutdown: Arc<AtomicBool>,
    flush_now: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    reboots: Arc<AtomicU64>,
) {
    let mut machine = match build(&spec, &options) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };

    // The ports the machine actually bound. `TcpHost::listen` goes through
    // `std::net::TcpListener::bind` and remembers `local_addr()`, so
    // `127.0.0.1:0` is an ephemeral port nobody had to pick.
    let addrs = match (machine.usb_sj_tcp(), machine.control_tcp()) {
        (Some(bytes), Some(control)) => Ok((bytes.local_addr(), control.local_addr())),
        _ => Err(anyhow!(
            "board `{}` did not bind both of its loopback doors",
            spec.id
        )),
    };
    let ok = addrs.is_ok();
    let _ = tx.send(addrs);
    if !ok {
        return;
    }

    if let Some(air) = &options.air {
        machine.arm_air(lp_emu_esp_common::air::ParticipantId(options.air_seat));
        air.announce(&spec.id, options.air_seat);
    }

    let stop = StopCondition {
        // No cycle deadline: a server has none, and `reboot()` restarts the
        // guest's clock, so an absolute one would mean a different thing
        // after every reset. The wall net is what gives the loop its turn.
        stop_cycle: None,
        exit_on: None,
        wall_timeout: Some(SLICE),
        probes: Vec::new(),
    };
    let mut last_flush = Instant::now();
    while !shutdown.load(Ordering::SeqCst) {
        let outcome = machine.run_until(&stop);
        reboots.store(machine.reboots(), Ordering::SeqCst);
        if let Some(air) = &options.air {
            for frame in machine.take_air_frames() {
                air.publish(options.air_seat, frame.at, &frame.bytes);
            }
        }
        match outcome {
            // The only ordinary end of a slice.
            Outcome::WallTimeout { .. } => {}
            other => {
                // A fault, a strict refusal, or a reset the guest had
                // disabled. The board is over; the server is not.
                eprintln!(
                    "emu serve: board `{}` stopped — {}",
                    spec.id,
                    describe(&other)
                );
                stopped.store(true, Ordering::SeqCst);
                break;
            }
        }
        if flush_now.swap(false, Ordering::SeqCst) || last_flush.elapsed() >= FLUSH_EVERY {
            flush(&mut machine, &spec);
            last_flush = Instant::now();
        }
    }
    flush(&mut machine, &spec);
    machine.flush_frames();
}

/// Where the machine writes its flash part before it is moved into place.
///
/// `FlashImage::flush` is a `std::fs::write`: it truncates and then writes
/// four megabytes. A server killed mid-flush would leave a half-image, and a
/// half-image loses the project more thoroughly than no write at all —
/// the next boot mounts it, calls it corrupt and **reformats**. So the
/// machine writes here and the result is `rename`d into place, which is
/// atomic: the durable file is a whole image or the previous whole image,
/// never a prefix of one.
///
/// This is a hazard reasoned about rather than one observed: no run has been
/// caught mid-write. It costs one rename per flush and it removes the whole
/// class, which is worth it for the file a walk's project lives in.
fn working_path(flash: &std::path::Path) -> PathBuf {
    let mut path = flash.to_path_buf();
    let name = path
        .file_name()
        .map(|n| format!("{}.part", n.to_string_lossy()))
        .unwrap_or_else(|| "flash.bin.part".to_string());
    path.set_file_name(name);
    path
}

/// Write back everything a killed server must not lose: the flash part, and
/// the console transcript.
fn flush(machine: &mut lp_emu_esp32c6::machine::Esp32C6Machine, spec: &BoardSpec) {
    match machine.flush_flash() {
        Ok(true) => {
            if let Some(durable) = &spec.flash
                && let Err(e) = std::fs::rename(working_path(durable), durable)
            {
                eprintln!(
                    "emu serve: board `{}`: moving the flash into place failed: {e}",
                    spec.id
                );
            }
        }
        Ok(false) => {}
        Err(e) => eprintln!(
            "emu serve: board `{}`: writing the flash back failed: {e}",
            spec.id
        ),
    }
    if let Some(path) = &spec.console {
        write_console(path, machine.usb_sj().bytes(), &spec.id);
        // What the guest handed the IN endpoint that no host took — the
        // boot log a byte client that connects later never sees, because
        // the firmware pauses its writes when nothing is draining. A board
        // does that too; the difference is that here it can still be read.
        let untaken = machine.usb_sj_tried().bytes();
        if !untaken.is_empty() {
            write_console(&untaken_path(path), untaken, &spec.id);
        }
    }
}

/// `<id>.console.log` → `<id>.console-untaken.log`.
fn untaken_path(console: &std::path::Path) -> PathBuf {
    let mut path = console.to_path_buf();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().replace(".console.", ".console-untaken."))
        .unwrap_or_else(|| "console-untaken.log".to_string());
    path.set_file_name(name);
    path
}

fn write_console(path: &std::path::Path, bytes: Vec<u8>, id: &str) {
    if let Err(e) = std::fs::write(path, bytes) {
        eprintln!(
            "emu serve: board `{id}`: writing the console to {} failed: {e}",
            path.display()
        );
    }
}

fn build(
    spec: &BoardSpec,
    options: &BoardOptions,
) -> Result<lp_emu_esp32c6::machine::Esp32C6Machine> {
    let mut builder = Esp32C6Builder::new()
        .time_grade(options.grade)
        .strict(options.strict_bus)
        // `attached` by default, as `emu run` is: the board's boot console
        // is then on the wire and the first byte client is replayed it.
        // `attached-idle` is what makes the coupling rule literal — the
        // client's connect IS the `open` — at the cost of the boot log,
        // which the firmware does not write while nothing is draining.
        // Either way `attach` and `detach` stay the cable's, never a
        // socket's.
        .usb_host(options.usb_host)
        .usb_sj_drain(UsbSjDrain::Auto)
        // PD11, and the one place the default flips: without it a
        // `MachineRequest::Reset` ends the run, and a board that disappears
        // when esptool-js does its DTR/RTS dance is not a board.
        .reboot_on_reset(true)
        .efuse(EfuseIdentity {
            mac: spec.mac,
            ..EfuseIdentity::default()
        })
        // Ephemeral loopback ports: the door reads them back and never has
        // to pick one, so N boards never collide and never race another
        // process for a number.
        .usb_sj(UsbSjSink::Tcp("127.0.0.1:0".to_string()))
        .control("127.0.0.1:0");

    let (elf, merged) = if spec.merged {
        (None, Some(spec.image.as_path()))
    } else {
        (Some(spec.image.as_path()), None)
    };
    // The machine reads and writes the working file; `flush` renames it onto
    // the durable one. Seed it from whatever the last server left, so this
    // board boots what it wrote.
    let working = match &spec.flash {
        Some(durable) => {
            let working = working_path(durable);
            let _ = std::fs::remove_file(&working);
            if durable.is_file() {
                std::fs::copy(durable, &working).with_context(|| {
                    format!("board `{}`: seeding {}", spec.id, working.display())
                })?;
            }
            Some(working)
        }
        None => None,
    };
    builder = apply_image(builder, elf, merged, working.as_deref())
        .with_context(|| format!("board `{}`", spec.id))?;

    builder
        .build()
        .map_err(|e| anyhow!("board `{}`: building the machine: {e}", spec.id))
}

/// `a0:f2:62:87:b4:8c`.
pub fn format_mac(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The desk board's MAC with `index` added to the last octet.
///
/// The default identity for board `n` of an unconfigured serve, so that two
/// boards are two boards even when nobody spelled a MAC. `mac=` on a
/// `--board` overrides it.
pub fn default_mac(index: usize) -> [u8; 6] {
    let mut mac = EfuseIdentity::default().mac;
    mac[5] = mac[5].wrapping_add(u8::try_from(index % 256).expect("modulo 256"));
    mac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_board_gets_its_own_identity() {
        let a = default_mac(0);
        let b = default_mac(1);
        assert_eq!(a, EfuseIdentity::default().mac, "board 0 is the desk board");
        assert_ne!(a, b, "two boards are two identities");
        assert_eq!(a[..5], b[..5], "only the last octet moves");
    }

    #[test]
    fn a_mac_reads_back_the_way_it_is_written() {
        let mac = EfuseIdentity::parse_mac("a0:f2:62:87:b4:8c").expect("six octets");
        assert_eq!(format_mac(&mac), "a0:f2:62:87:b4:8c");
    }
}
