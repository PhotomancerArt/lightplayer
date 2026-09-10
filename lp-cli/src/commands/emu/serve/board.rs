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
use crate::commands::emu::handler::{Image, apply_image, describe};

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

/// What kind of image a board was given, and with it which entry it takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardKind {
    /// `kind=elf`: a firmware ELF loaded at its entry point, with a separate
    /// flash part beside it. Fast, and what every walk so far used.
    #[default]
    Elf,
    /// `kind=merged`: a whole merged flash image booted from the reset
    /// vector, **read-only** — the image a gate named.
    Merged,
    /// `kind=rom-up`: the reset vector out of the board's own WRITABLE flash
    /// file. The only shape that can be flashed and then boot what was
    /// written, which is plan two's acceptance criterion 5.
    RomUp,
}

impl BoardKind {
    /// How the startup line and `GET /boards` name this board's entry.
    /// `direct` and `rom-up` are the machine's own two words
    /// (`--boot-mode`), and `merged` is the read-only rom-up.
    pub fn boot_word(self) -> &'static str {
        match self {
            BoardKind::Elf => "direct",
            BoardKind::Merged => "rom-up (read-only)",
            BoardKind::RomUp => "rom-up",
        }
    }
}

/// What one `--board` asked for.
#[derive(Clone, Debug)]
pub struct BoardSpec {
    pub id: String,
    /// The image, and what it is for: the ELF to load, the merged chip to
    /// boot read-only, or the whole-chip image a `kind=rom-up` board's flash
    /// file is **seeded** from the first time. `None` is a `kind=rom-up`
    /// board with nothing on it — an erased chip, which is what
    /// `s1-blank-flash` is.
    pub image: Option<PathBuf>,
    pub kind: BoardKind,
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
    /// True for a `kind=merged` board, whose chip is read-only by design.
    merged: bool,
    /// `direct` / `rom-up` / `rom-up (read-only)` — which entry this board
    /// takes, as `GET /boards` reports it. A flashable board is a `rom-up`
    /// one, and a page that offers "flash this board" wants to know.
    pub boot: &'static str,
    /// Whether the chip holds a bootable image at the reset vector, kept up
    /// to date by the board thread on every flush.
    ///
    /// Plan two M5: the word used to be computed once in [`Board::start`]
    /// from the flash FILE's length and never again, so a board flashed
    /// through esptool-js still said `blank` until the server restarted —
    /// and a board whose file was merely 4 MiB of `0xff` said `loaded`. Both
    /// are gone: this is the same question the mask ROM asks.
    has_image: Arc<AtomicBool>,
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
        let merged = matches!(spec.kind, BoardKind::Merged);
        // What is on the chip before the guest has run a cycle: the durable
        // flash file if there is one, else whatever a `kind=rom-up` board is
        // being seeded from. The board thread keeps this true from here on.
        let at_start = spec
            .flash
            .as_deref()
            .filter(|p| p.is_file())
            .or(spec.image.as_deref().filter(|_| !merged))
            .is_some_and(file_starts_with_an_image);

        let shutdown = Arc::new(AtomicBool::new(false));
        let flush_now = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let reboots = Arc::new(AtomicU64::new(0));
        let has_image = Arc::new(AtomicBool::new(at_start));
        let (tx, rx) = std::sync::mpsc::channel::<Result<(SocketAddr, SocketAddr)>>();

        let id = spec.id.clone();
        let mac = spec.mac;
        let boot = spec.kind.boot_word();
        let thread = {
            let shutdown = Arc::clone(&shutdown);
            let flush_now = Arc::clone(&flush_now);
            let stopped = Arc::clone(&stopped);
            let reboots = Arc::clone(&reboots);
            let has_image = Arc::clone(&has_image);
            std::thread::Builder::new()
                .name(format!("emu-board-{id}"))
                .spawn(move || {
                    run_board(RunBoard {
                        spec,
                        options,
                        tx,
                        shutdown,
                        flush_now,
                        stopped,
                        reboots,
                        has_image,
                    });
                })
                .context("spawning the board thread")?
        };

        let (bytes_addr, control_addr) = rx
            .recv()
            .map_err(|_| anyhow!("board `{id}` died before it bound its sockets"))??;

        Ok(Board {
            id,
            mac,
            merged,
            boot,
            has_image,
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

    /// `blank` / `loaded` / `merged`, as of right now.
    ///
    /// The word is about the CHIP, never about what the board is running: a
    /// `kind=elf` board runs an image that was never in its flash and
    /// truthfully reports `blank` for as long as it lives (M3 measured that
    /// and read it as a defect; it is the two things being independent). And
    /// `loaded` is not a claim that the board BOOTS — only that there is an
    /// image where the mask ROM looks for one. The board's own hello is the
    /// evidence for booting, and nothing else is.
    pub fn flash_state(&self) -> &'static str {
        if self.merged {
            "merged"
        } else if self.has_image.load(Ordering::SeqCst) {
            "loaded"
        } else {
            "blank"
        }
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

/// One board thread's whole world. A struct rather than eight arguments,
/// because every one of them is an `Arc` the door also holds.
struct RunBoard {
    spec: BoardSpec,
    options: BoardOptions,
    tx: std::sync::mpsc::Sender<Result<(SocketAddr, SocketAddr)>>,
    shutdown: Arc<AtomicBool>,
    flush_now: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    reboots: Arc<AtomicU64>,
    has_image: Arc<AtomicBool>,
}

fn run_board(this: RunBoard) {
    let RunBoard {
        spec,
        options,
        tx,
        shutdown,
        flush_now,
        stopped,
        reboots,
        has_image,
    } = this;
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
            flush(&mut machine, &spec, &has_image);
            last_flush = Instant::now();
        }
    }
    flush(&mut machine, &spec, &has_image);
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
fn flush(
    machine: &mut lp_emu_esp32c6::machine::Esp32C6Machine,
    spec: &BoardSpec,
    has_image: &AtomicBool,
) {
    // Asked of the CHIP rather than of the file, and on every flush rather
    // than once: this is what turns `blank → flash → loaded` into a sequence
    // a page can watch. `peek` does not count as a flash read.
    if let Ok(flash) = machine.flash().lock() {
        has_image.store(
            flash
                .peek(0, 1)
                .is_some_and(|head| head[0] == ESP_IMAGE_MAGIC),
            Ordering::SeqCst,
        );
    }
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

/// `esp_image_header_t.magic` — the byte the mask ROM looks for at the reset
/// vector, and the one it complains about as `invalid header: 0xffffffff`
/// when a chip is erased. It is the whole difference between `blank` and
/// `loaded`, so it is asked rather than guessed from a file's length.
const ESP_IMAGE_MAGIC: u8 = 0xe9;

/// Does `path`'s first byte say there is a bootable image there?
///
/// Cheap on purpose: one byte, before the machine exists. A 4 MiB file of
/// `0xff` is a chip with nothing on it, and the old "the file has bytes"
/// test called that `loaded`.
fn file_starts_with_an_image(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 1];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head[0] == ESP_IMAGE_MAGIC
}

/// `<id>.console.log` → `<id>.console-untaken.log`.
fn untaken_path(console: &std::path::Path) -> PathBuf {
    let mut path = console.to_path_buf();
    let name = path
        .file_name()
        .map(|n| {
            n.to_string_lossy()
                .replace(".console.", ".console-untaken.")
        })
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

    let image = match spec.kind {
        BoardKind::Elf => Image::Elf(
            spec.image
                .as_deref()
                .ok_or_else(|| anyhow!("board `{}`: kind=elf needs an image", spec.id))?,
        ),
        BoardKind::Merged => Image::Merged(
            spec.image
                .as_deref()
                .ok_or_else(|| anyhow!("board `{}`: kind=merged needs an image", spec.id))?,
        ),
        BoardKind::RomUp => Image::RomUp,
    };
    // The machine reads and writes the working file; `flush` renames it onto
    // the durable one. Seed it from whatever the last server left, so this
    // board boots what it wrote — and, on the FIRST run of a `kind=rom-up`
    // board, from the image the `--board` named, which is that board's
    // starting contents rather than something loaded into memory.
    let working = match &spec.flash {
        Some(durable) => {
            let working = working_path(durable);
            let _ = std::fs::remove_file(&working);
            let source = if durable.is_file() {
                Some(durable.as_path())
            } else if matches!(spec.kind, BoardKind::RomUp) {
                spec.image.as_deref()
            } else {
                None
            };
            if let Some(source) = source {
                std::fs::copy(source, &working).with_context(|| {
                    format!(
                        "board `{}`: seeding {} from {}",
                        spec.id,
                        working.display(),
                        source.display()
                    )
                })?;
            }
            Some(working)
        }
        None => None,
    };
    builder = apply_image(builder, image, working.as_deref())
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
