//! `lp-cli link rtt`: how promptly a rendering board answers — request round
//! trips, the link's own round trips, and transfer rates — on silicon (wall
//! clock) or on the emulated C6 in this process (EMULATED time).
//!
//! Written for the C6's link thread (plan
//! `lp2025/2026-10-01-1756-c6-link-io-thread`, after the spike
//! `lp2025/2026-10-01-1200-io-thread-spike`); it measures any board that
//! speaks lp-link on a serial port, or on the LAN (`lan:<host>[:port]`, its
//! secure link inside a WebSocket — the same report, so a LAN run compares
//! with a USB one in frames).
//!
//! The run, after the hello (and on `emu:` a deploy of the project, which
//! loads it):
//!
//! 1. **warm-up**, `--warmup-s`: the render settles;
//! 2. **idle**, `--idle-s`: the render alone, its frame rate read from the
//!    heartbeats (≥ 10 s for two of them);
//! 3. **transfers**, from board time `--transfers-at-s` when it is set: one
//!    `--read-bytes` file written and read back `--reads`
//!    times (board → host), then `--writes` `WriteChunk`s of `--write-bytes`
//!    the board refuses without touching flash — an offset into a file that
//!    does not exist — so the host → board figure is the link and the parse,
//!    not the flash;
//! 4. **requests**: `--count` cheap requests (`ListLoadedProjects`), one at a
//!    time, each after a random pause of 0..`--max-gap-ms` so they land at
//!    every phase of the frame, starting at board time `--requests-at-s`;
//!    per request, the time from its
//!    first frame leaving to its reply arriving, and every round trip
//!    lp-link's estimator samples (send → ACK);
//! 5. **tail**, `--tail-s`: two more heartbeats (fps, heap, stack lines).
//!
//! **Board time** is what the phases are pinned to, because a project's frame
//! cost varies with its pattern time and a transfer or a request waits on the
//! frame in flight: two builds are only comparable at the same point in the
//! pattern. On `emu:` it is emulated time since power-on; on a serial device
//! it is the board's own uptime, read off its heartbeats (`uptime_ms`), so a
//! board that took longer to boot or to be opened still measures at the same
//! moment of its pattern.
//!
//! The summary goes to stderr; `--json` writes every sample, `--console` the
//! board's whole console. On `emu:` the report also carries the WS281x
//! frames decoded off the emulated pads (errors and incomplete frames: the
//! RMT check) and the RMT refill race. Emulated times are the emulator's
//! clock model: compare builds in frames, never against silicon in ms.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lpa_client::HostSpecifier;
use lpa_client::transport_lan::{LanLink, LanOptions, LanSocket, LanTarget, tier_words};
use lpc_wire::server::FsRequest;
use lpc_wire::{ClientMessage, ClientRequest, PortRead, WireEncoding, WireLinkPort};
use serde_json::{Value, json};

use super::args::RttArgs;
use super::lab_port::{LabPort, TermiosMode};
use super::rtt_emu_boards::ChipReport;
use crate::commands::emu::args::EmuChip;
use crate::commands::emu::link_host::{
    C6Board, EmuLinkHost, EmuUsbBoard, S3Board, SLICE_US, V3Board, console_lines, fresh_nonce,
};

/// First request id: far from anything else on the link.
const ID_BASE: u64 = 7_000_000;
/// Longest a single request may take before the run fails, µs.
const ANSWER_BUDGET_US: u64 = 30_000_000;
/// The file the read-rate phase writes, reads back and removes.
const BLOB_PATH: &str = "/lp-cli-link-rtt-blob.bin";
/// A file that does not exist: a `WriteChunk` at offset 1 is refused.
const NO_FILE_PATH: &str = "/lp-cli-link-rtt-none.bin";

/// Run the measurement on `args.target`.
pub fn rtt(args: &RttArgs) -> Result<()> {
    let wall = Instant::now();
    let elf = args.target.strip_prefix("emu:");
    check_chip_on_target(&args.target, elf.is_some(), args.chip)?;
    let mut session = match elf {
        Some(elf) => open_emu(args, Path::new(elf))?,
        None if args.target.starts_with("lan:") => open_lan(args)?,
        None => open_serial(args)?,
    };
    let mut rng = XorShift(args.seed.max(1));

    // 1. Warm-up.
    session.idle(secs_us(args.warmup_s))?;
    eprintln!(
        "link rtt: warm-up done at {:.3} s",
        session.pipe.now_us() as f64 / 1e6
    );

    // 2. Idle: the render alone, for its frame rate.
    let idle_start = session.heartbeats.len();
    session.idle(secs_us(args.idle_s))?;
    let idle_fps = window_fps(&session.heartbeats[idle_start..]);
    eprintln!("link rtt: idle fps {idle_fps:?}");
    let rtt_mark = session.link_rtt.len();

    // 3. Transfers.
    session.wait_for_board_time("transfers", args.transfers_at_s)?;
    let transfers = transfers(&mut session, args)?;

    // 4. Requests.
    session.wait_for_board_time("requests", args.requests_at_s)?;
    let request_mark = session.link_rtt.len();
    let mut requests = Vec::new();
    for _ in 0..args.count {
        let gap = rng.next() % (args.max_gap_ms * 1000 + 1);
        session.idle(gap)?;
        let (sent, rtt, len) = session.request(ClientRequest::ListLoadedProjects)?;
        requests.push(json!({ "sent_us": sent, "rtt_us": rtt, "wire_len": len }));
    }
    let request_end = session.link_rtt.len();
    let request_rtts: Vec<f64> = requests
        .iter()
        .map(|r| r["rtt_us"].as_u64().unwrap_or(0) as f64 / 1000.0)
        .collect();
    let request_link_rtts = rtt_ms(&session.link_rtt[request_mark..request_end]);
    let link_rtts = rtt_ms(&session.link_rtt[rtt_mark..]);
    // The same round trips in frames of the idle render: the column a LAN
    // run and a USB run compare in (their frame rates differ by project).
    let request_rtt_frames = in_frames(&request_rtts, idle_fps);
    eprintln!("link rtt: request RTT ms {}", stats(&request_rtts));
    eprintln!(
        "link rtt: request RTT frames {}",
        stats(&request_rtt_frames)
    );
    eprintln!(
        "link rtt: link RTT (request phase) ms {}",
        stats(&request_link_rtts)
    );
    eprintln!(
        "link rtt: link RTT (transfers + requests) ms {}",
        stats(&link_rtts)
    );
    eprintln!(
        "link rtt: board→host {:.1} KiB/s ({} B in {:.1} ms); host→board {:.1} KiB/s ({} B in \
         {:.1} ms); {} B file write {:.1} ms",
        transfers.read_rate_kib_s,
        transfers.read_bytes,
        transfers.read_us as f64 / 1000.0,
        transfers.write_rate_kib_s,
        transfers.write_bytes,
        transfers.write_us as f64 / 1000.0,
        args.read_bytes,
        transfers.blob_write_us as f64 / 1000.0,
    );

    // 5. Tail.
    session.idle(secs_us(args.tail_s))?;

    let notable: Vec<&String> = session
        .lines
        .iter()
        .map(|(_, line)| line)
        .filter(|line| NOTABLE.iter().any(|needle| line.contains(needle)))
        .collect();
    for line in &notable {
        eprintln!("  {line}");
    }
    if session.resets > 0 {
        eprintln!(
            "link rtt: WARNING {} link reset(s) during the run",
            session.resets
        );
    }

    let emu = session.pipe.report();
    let report = json!({
        "label": args.label,
        "target": args.target,
        "time": if args.target.starts_with("emu:") { "emulated" } else { "wall" },
        "link": link_kind(&args.target),
        "idle_fps": idle_fps,
        "requests": requests,
        "request_rtt_ms": stats(&request_rtts),
        "request_rtt_frames": stats(&request_rtt_frames),
        "link_rtt_ms": stats(&link_rtts),
        "link_rtt_request_phase_ms": stats(&request_link_rtts),
        "link_rtt_samples": session.link_rtt[rtt_mark..]
            .iter()
            .map(|(at, us)| json!([at, us]))
            .collect::<Vec<_>>(),
        "reads": transfers.reads,
        "read_rate_kib_s": transfers.read_rate_kib_s,
        "read_total_us": transfers.read_us,
        "writes": transfers.writes,
        "write_rate_kib_s": transfers.write_rate_kib_s,
        "write_total_us": transfers.write_us,
        "write_chunk_request_bytes": transfers.chunk_request_bytes,
        "blob_write_us": transfers.blob_write_us,
        "heartbeats": session.heartbeats,
        "lines": notable,
        "link_resets": session.resets,
        "host_link_counters": format!("{:?}", session.link.counters()),
        "emu": emu,
        "wall_s": wall.elapsed().as_secs_f64(),
    });
    if let Some(path) = &args.json {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Console lines the summary repeats: the board's own stack, frame and
/// thread figures, and anything that went wrong.
const NOTABLE: &[&str] = &[
    "[stack]",
    "[iostack]",
    "[perf]",
    "io thread",
    "main stack",
    "panic",
    "Stack overflow",
];

/// What the transfer phase measured.
struct Transfers {
    blob_write_us: u64,
    reads: Vec<Value>,
    read_bytes: u64,
    read_us: u64,
    read_rate_kib_s: f64,
    writes: Vec<Value>,
    write_bytes: u64,
    write_us: u64,
    write_rate_kib_s: f64,
    chunk_request_bytes: u64,
}

/// Board → host (a file written, read back), then host → board (refused
/// chunks, no flash); the file is removed after.
fn transfers(session: &mut Session, args: &RttArgs) -> Result<Transfers> {
    let blob: Vec<u8> = (0..args.read_bytes).map(|i| (i * 7 + 3) as u8).collect();
    let (_, blob_write_us, _) = session.request(ClientRequest::Filesystem(FsRequest::Write {
        path: BLOB_PATH.into(),
        data: blob,
    }))?;
    if let Some(line) = session.reply_json(session.next_id)
        && line.contains("\"error\":\"")
    {
        bail!("the file write failed: {line}");
    }

    let mut reads = Vec::new();
    let read_start = session.pipe.now_us();
    for _ in 0..args.reads {
        let (_, rtt, len) = session.request(ClientRequest::Filesystem(FsRequest::Read {
            path: BLOB_PATH.into(),
        }))?;
        reads.push(json!({ "rtt_us": rtt, "wire_len": len }));
    }
    let read_us = session.pipe.now_us() - read_start;
    let read_bytes: u64 = reads
        .iter()
        .map(|r| r["wire_len"].as_u64().unwrap_or(0))
        .sum();

    let chunk: Vec<u8> = (0..args.write_bytes).map(|i| (i * 13 + 1) as u8).collect();
    let chunk_request = ClientRequest::Filesystem(FsRequest::WriteChunk {
        path: NO_FILE_PATH.into(),
        offset: 1,
        data: chunk,
    });
    // What one chunk request weighs on the wire: its JSON (base64 data).
    let chunk_request_bytes = serde_json::to_string(&ClientMessage {
        id: 1,
        msg: chunk_request.clone(),
    })?
    .len() as u64;
    let mut writes = Vec::new();
    let write_start = session.pipe.now_us();
    for _ in 0..args.writes {
        let (_, rtt, len) = session.request(chunk_request.clone())?;
        writes.push(json!({ "rtt_us": rtt, "reply_wire_len": len }));
    }
    let write_us = session.pipe.now_us() - write_start;
    let write_bytes = chunk_request_bytes * args.writes as u64;

    session.request(ClientRequest::Filesystem(FsRequest::DeleteFile {
        path: BLOB_PATH.into(),
    }))?;

    Ok(Transfers {
        blob_write_us,
        reads,
        read_bytes,
        read_us,
        read_rate_kib_s: kib_per_s(read_bytes, read_us),
        writes,
        write_bytes,
        write_us,
        write_rate_kib_s: kib_per_s(write_bytes, write_us),
        chunk_request_bytes,
    })
}

/// The pipe under the link: the emulated C6 or a serial port (bytes), or a
/// board on the LAN (one frame per WebSocket message).
trait Pipe {
    /// Move time on: one slice of the emulated board; nothing on a port
    /// (its read waits ~1 ms).
    fn advance(&mut self) -> Result<()>;
    fn now_us(&self) -> u64;
    fn take(&mut self) -> Result<Vec<u8>>;
    /// Hand what arrived to `link` at `now`: bytes by default, as `take`
    /// reads them; a datagram pipe overrides it.
    fn feed(&mut self, now: u64, link: &mut WireLinkPort) -> Result<()> {
        let bytes = self.take()?;
        if !bytes.is_empty() {
            link.on_bytes(now, &bytes);
        }
        Ok(())
    }
    /// Write one frame the link handed out.
    fn write(&mut self, bytes: &[u8]) -> Result<()>;
    /// End-of-run figures only an emulator has.
    fn report(&mut self) -> Value {
        Value::Null
    }
}

/// One host link over a pipe, and what it heard.
struct Session {
    pipe: Box<dyn Pipe>,
    link: WireLinkPort,
    console: Option<std::io::LineWriter<std::fs::File>>,
    lines: Vec<(u64, String)>,
    /// Each reply's arrival, by request id.
    replies: HashMap<u64, Seen>,
    heartbeats: Vec<Value>,
    /// Every link-level round trip sample: (when, µs).
    link_rtt: Vec<(u64, u64)>,
    rtt_count: u32,
    resets: u32,
    next_id: u64,
    /// Board time minus pipe time, µs, once a heartbeat has said it (a
    /// serial device; see the module docs). `None` on `emu:`, whose pipe
    /// time is board time.
    board_offset_us: Option<i64>,
    /// Whether board time comes from the heartbeats.
    board_time_from_heartbeats: bool,
}

/// One wire message read: when, and its size.
#[derive(Clone, Copy)]
struct Seen {
    at_us: u64,
    wire_len: usize,
}

impl Session {
    fn new(pipe: Box<dyn Pipe>, link: WireLinkPort, console: Option<&Path>) -> Result<Self> {
        Ok(Self {
            pipe,
            link,
            console: console_file(console)?,
            lines: Vec::new(),
            replies: HashMap::new(),
            heartbeats: Vec::new(),
            link_rtt: Vec::new(),
            rtt_count: 0,
            resets: 0,
            next_id: ID_BASE,
            board_offset_us: None,
            board_time_from_heartbeats: false,
        })
    }

    /// The board's clock, µs (see the module docs).
    fn board_now_us(&self) -> u64 {
        let now = self.pipe.now_us();
        match self.board_offset_us {
            Some(offset) => now.saturating_add_signed(offset),
            None => now,
        }
    }

    /// Idle until board time `at_s` (0 = do not wait); say so when the phase
    /// is already late.
    fn wait_for_board_time(&mut self, phase: &str, at_s: f64) -> Result<()> {
        if at_s <= 0.0 {
            return Ok(());
        }
        if self.board_time_from_heartbeats && self.board_offset_us.is_none() {
            bail!("no heartbeat yet, so no board time to start the {phase} at");
        }
        let at = secs_us(at_s);
        let now = self.board_now_us();
        if now < at {
            self.idle(at - now)?;
        } else {
            eprintln!(
                "link rtt: WARNING the {phase} start late, at board time {:.3} s",
                now as f64 / 1e6
            );
        }
        Ok(())
    }

    fn step(&mut self) -> Result<()> {
        self.pipe.advance()?;
        let now = self.pipe.now_us();
        self.pipe.feed(now, &mut self.link)?;
        self.flush(now)?;
        while let Some(read) = self.link.poll_read() {
            for line in console_lines(&read) {
                self.note_line(now, line)?;
            }
            match read {
                PortRead::Message(payload) => {
                    if let Ok(message) = &payload.message {
                        self.replies.insert(
                            message.id,
                            Seen {
                                at_us: now,
                                wire_len: payload.wire_len,
                            },
                        );
                    }
                    if payload.json.contains("\"heartbeat\"")
                        && let Ok(v) = serde_json::from_str::<Value>(&payload.json)
                        && let Some(beat) = v.pointer("/msg/heartbeat")
                    {
                        self.heartbeats.push(json!({
                            "at_us": now,
                            "uptime_ms": beat.get("uptime_ms"),
                            "frame_count": beat.get("frame_count"),
                            "fps": beat.get("fps"),
                            "memory": beat.get("memory"),
                        }));
                        if self.board_time_from_heartbeats
                            && let Some(uptime_ms) = beat.get("uptime_ms").and_then(Value::as_u64)
                        {
                            self.board_offset_us = Some(uptime_ms as i64 * 1000 - now as i64);
                        }
                    }
                }
                PortRead::Reset { .. } => self.resets += 1,
                _ => {}
            }
        }
        let (last, count) = self.link.link().rtt_last_sample();
        if count != self.rtt_count {
            self.rtt_count = count;
            self.link_rtt.push((now, last));
        }
        Ok(())
    }

    fn note_line(&mut self, now: u64, line: String) -> Result<()> {
        if let Some(console) = self.console.as_mut() {
            writeln!(console, "{line}")?;
        }
        self.lines.push((now, line));
        Ok(())
    }

    fn flush(&mut self, now: u64) -> Result<()> {
        while let Some(frame) = self.link.poll_transmit(now) {
            let frame = frame.to_vec();
            self.pipe.write(&frame)?;
        }
        Ok(())
    }

    fn idle(&mut self, us: u64) -> Result<()> {
        let until = self.pipe.now_us() + us;
        while self.pipe.now_us() < until {
            self.step()?;
        }
        Ok(())
    }

    fn wait_line(&mut self, needle: &str, budget_us: u64) -> Result<bool> {
        let until = self.pipe.now_us() + budget_us;
        let mut seen = 0;
        loop {
            if self.lines[seen..].iter().any(|(_, l)| l.contains(needle)) {
                return Ok(true);
            }
            seen = self.lines.len();
            if self.pipe.now_us() >= until {
                return Ok(false);
            }
            self.step()?;
        }
    }

    /// Send one request and wait for its reply: (sent at, round trip µs,
    /// reply wire bytes).
    fn request(&mut self, msg: ClientRequest) -> Result<(u64, u64, usize)> {
        self.next_id += 1;
        let id = self.next_id;
        let message = ClientMessage { id, msg };
        loop {
            match self.link.send_client(&message) {
                Ok(()) => break,
                Err(lp_link::SendError::Full) => self.step()?,
                Err(e) => bail!("the link refused request {id}: {e:?}"),
            }
        }
        let sent = self.pipe.now_us();
        self.flush(sent)?;
        let until = sent + ANSWER_BUDGET_US;
        loop {
            if let Some(seen) = self.replies.get(&id).copied() {
                return Ok((sent, seen.at_us.saturating_sub(sent), seen.wire_len));
            }
            if self.pipe.now_us() > until {
                bail!("no reply to request {id} within {ANSWER_BUDGET_US} µs");
            }
            self.step()?;
        }
    }

    /// The `M!` console line of the reply to `id`.
    fn reply_json(&self, id: u64) -> Option<String> {
        self.lines
            .iter()
            .rev()
            .find(|(_, l)| l.starts_with("M!") && l.contains(&format!("\"id\":{id},")))
            .map(|(_, l)| l.clone())
    }
}

/// A serial port: wall-clock time.
struct SerialPipe {
    port: LabPort,
    t0: Instant,
    buf: Vec<u8>,
}

impl Pipe for SerialPipe {
    fn advance(&mut self) -> Result<()> {
        Ok(())
    }

    fn now_us(&self) -> u64 {
        self.t0.elapsed().as_micros() as u64
    }

    fn take(&mut self) -> Result<Vec<u8>> {
        let n = self.port.read(&mut self.buf).context("reading the port")?;
        Ok(self.buf[..n].to_vec())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.port.write_all(bytes).context("writing the port")
    }
}

/// A board on the LAN: wall-clock time (the link's own clock, from the
/// moment its WebSocket connected), one lp-link frame per message.
struct LanPipe {
    socket: LanSocket,
    t0: Instant,
}

impl Pipe for LanPipe {
    fn advance(&mut self) -> Result<()> {
        Ok(())
    }

    fn now_us(&self) -> u64 {
        self.t0.elapsed().as_micros() as u64
    }

    fn take(&mut self) -> Result<Vec<u8>> {
        bail!("a LAN link carries frames, not bytes")
    }

    fn feed(&mut self, _now: u64, link: &mut WireLinkPort) -> Result<()> {
        let mut next = self.socket.recv(Duration::from_millis(1))?;
        while let Some(frame) = next {
            link.on_datagram(self.now_us(), &frame);
            next = self.socket.recv(Duration::ZERO)?;
        }
        Ok(())
    }

    fn write(&mut self, frame: &[u8]) -> Result<()> {
        Ok(self.socket.send(frame)?)
    }
}

/// An emulated board in this process: emulated time. Generic over which
/// chip (`B: EmuUsbBoard`); the end-of-run report is chip-specific
/// ([`ChipReport`], `rtt_emu_boards.rs` — the three machines' RMT models
/// don't share one `RefillStats` type).
struct EmuPipe<B: EmuUsbBoard> {
    board: B,
}

impl<B: EmuUsbBoard + ChipReport> Pipe for EmuPipe<B> {
    fn advance(&mut self) -> Result<()> {
        self.board
            .run_for_us(SLICE_US)
            .map_err(|why| anyhow::anyhow!("the emulated board stopped: {why}"))
    }

    fn now_us(&self) -> u64 {
        self.board.micros()
    }

    fn take(&mut self) -> Result<Vec<u8>> {
        Ok(self.board.take_usb_output())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.board.push_usb_input(bytes);
        Ok(())
    }

    fn report(&mut self) -> Value {
        self.board.chip_report()
    }
}

/// `--chip` only means anything on an `emu:` target: a serial device is real
/// hardware, and it picks its own chip.
fn check_chip_on_target(target: &str, is_emu: bool, chip: Option<EmuChip>) -> Result<()> {
    if !is_emu && chip.is_some() {
        bail!(
            "--chip only applies to an emu: target; {target} is a serial device (real hardware \
             picks its own chip)"
        );
    }
    Ok(())
}

/// A chip's name, as `--chip` spells it, for error messages.
fn chip_label(chip: EmuChip) -> &'static str {
    match chip {
        EmuChip::Esp32C6 => "esp32c6",
        EmuChip::Esp32S3 => "esp32s3",
        EmuChip::Esp32V3 => "esp32v3",
    }
}

/// `--grade`, resolved per chip: the C6 has two grades and defaults to t2;
/// the S3 and classic have only t1 (their emulators' one grade) and are
/// refused anything else.
fn resolve_grade(chip: EmuChip, requested: Option<&str>) -> Result<&'static str> {
    match chip {
        EmuChip::Esp32C6 => match requested.unwrap_or("t2") {
            "t1" => Ok("t1"),
            "t2" => Ok("t2"),
            g => bail!("--grade must be t1 or t2, not {g}"),
        },
        EmuChip::Esp32S3 | EmuChip::Esp32V3 => match requested {
            None | Some("t1") => Ok("t1"),
            Some(g) => bail!(
                "--grade {g}: the {} emulator has one time grade, t1",
                chip_label(chip)
            ),
        },
    }
}

/// The project `emu:` deploys when `--project` is not given. The PLAYFUL
/// choker targets the C6's own XIAO board (D10); the S3's default is
/// `shader-oracle` — the same project `scripts/emu/m4-walk.sh --chip
/// esp32s3` uploads (`ws281x:local:D10`, which both the XIAO C6 and XIAO S3
/// Plus profiles alias, so it renders unmodified on either chip); the
/// classic's is `five-wire` — the DOM-Z-102's four fused pads plus a spare,
/// what `emu_v3_link_gates.rs`'s log-drop load deploys and what P4 reuses
/// for the same check.
fn default_project_path(chip: EmuChip) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    match chip {
        EmuChip::Esp32C6 => root.join("../catalog/projects/playful-choker"),
        EmuChip::Esp32S3 => root.join("../projects/test/shader-oracle"),
        EmuChip::Esp32V3 => root.join("../projects/test/five-wire"),
    }
}

fn project_dir(args: &RttArgs, chip: EmuChip) -> PathBuf {
    args.project
        .clone()
        .unwrap_or_else(|| default_project_path(chip))
}

fn open_serial(args: &RttArgs) -> Result<Session> {
    let port = LabPort::open(&args.target, TermiosMode::Raw)?;
    let config = lpa_client::transport_serial::link_config_for_port(&args.target);
    let pipe = SerialPipe {
        port,
        t0: Instant::now(),
        buf: vec![0; 4096],
    };
    let mut session = Session::new(
        Box::new(pipe),
        WireLinkPort::new(config, fresh_nonce(), true),
        args.console.as_deref(),
    )?;
    session.board_time_from_heartbeats = true;
    if !session.wait_line("\"hello\":{", 10_000_000)? {
        bail!("no hello from {} within 10 s", args.target);
    }
    // The packed-reply opt-in's answer, as a product host waits for it.
    session.idle(500_000)?;
    Ok(session)
}

/// Open a `lan:` target: its secure link up (a locked board's password from
/// `--password-stdin` or `LP_PASSWORD`) and its hello read, board time from
/// its heartbeats, as on a serial device.
fn open_lan(args: &RttArgs) -> Result<Session> {
    let spec = HostSpecifier::parse(&args.target)?;
    let target = LanTarget::from_specifier(&spec)
        .with_context(|| format!("{} is not a lan: address", args.target))?;
    let options = LanOptions {
        password: args.board_password.resolve(&spec)?,
        want_packed: lpa_client::requested_wire_encoding() == WireEncoding::Packed,
        held_keys: Vec::new(),
    };
    let opened = LanLink::open(&target.endpoint(), &options)?;
    eprintln!(
        "link rtt: {target} up at the {} tier",
        tier_words(opened.granted)
    );
    let (socket, port, t0) = opened.link.into_parts();
    let mut session = Session::new(
        Box::new(LanPipe { socket, t0 }),
        port,
        args.console.as_deref(),
    )?;
    session.board_time_from_heartbeats = true;
    let now = session.pipe.now_us();
    for read in &opened.early {
        for line in console_lines(read) {
            session.note_line(now, line)?;
        }
    }
    // The packed-reply opt-in's answer, as on a serial device.
    session.idle(500_000)?;
    Ok(session)
}

/// Which link a target measures, for the report.
fn link_kind(target: &str) -> &'static str {
    if target.starts_with("emu:") {
        "emulated-usb"
    } else if target.starts_with("lan:") {
        "lan"
    } else {
        "serial"
    }
}

/// Round trips `ms` in frames of a render at `fps` (none without a rate).
fn in_frames(ms: &[f64], fps: Option<f64>) -> Vec<f64> {
    match fps {
        Some(fps) if fps > 0.0 => ms.iter().map(|ms| ms * fps / 1000.0).collect(),
        _ => Vec::new(),
    }
}

/// Open the `emu:` target: build the chip's machine, bring its host link up,
/// deploy the chip's project (`--project`, or its own default), and hand
/// back a [`Session`] over it. `--chip` picks the machine (default: C6, so
/// every pre-P1 `emu:` invocation is unchanged).
fn open_emu(args: &RttArgs, elf: &Path) -> Result<Session> {
    match args.chip.unwrap_or_default() {
        EmuChip::Esp32C6 => open_emu_c6(args, elf),
        EmuChip::Esp32S3 => open_emu_s3(args, elf),
        EmuChip::Esp32V3 => open_emu_v3(args, elf),
    }
}

fn open_emu_c6(args: &RttArgs, elf: &Path) -> Result<Session> {
    use lp_emu_esp32c6::flash::FlashBacking;
    use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
    let grade = match resolve_grade(EmuChip::Esp32C6, args.grade.as_deref())? {
        "t1" => TimeGrade::T1,
        "t2" => TimeGrade::T2,
        g => unreachable!("resolve_grade only returns t1/t2 for the C6: {g}"),
    };
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        .strict(false)
        .time_grade(grade)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .map_err(|e| anyhow::anyhow!("the image builds no machine: {e:?}"))?;
    let host = EmuLinkHost::new(C6Board::new(machine)?, fresh_nonce(), true);
    finish_open(args, host, project_dir(args, EmuChip::Esp32C6))
}

fn open_emu_s3(args: &RttArgs, elf: &Path) -> Result<Session> {
    use lp_emu_esp32s3::machine::{AppSource, BootMode, Esp32S3Builder, TimeGrade, UsbHost};
    resolve_grade(EmuChip::Esp32S3, args.grade.as_deref())?;
    let builder = Esp32S3Builder::new()
        .time_grade(TimeGrade::T1)
        .strict(false)
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf.to_path_buf()));
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .map_err(|e| anyhow::anyhow!("the image builds no machine: {e}"))?;
    let host = EmuLinkHost::new(board, fresh_nonce(), true);
    finish_open(args, host, project_dir(args, EmuChip::Esp32S3))
}

fn open_emu_v3(args: &RttArgs, elf: &Path) -> Result<Session> {
    use lp_emu_esp32v3::machine::{AppSource, BootMode, Esp32V3Builder, TimeGrade};
    resolve_grade(EmuChip::Esp32V3, args.grade.as_deref())?;
    let builder = Esp32V3Builder::new()
        .time_grade(TimeGrade::T1)
        .strict(false)
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf.to_path_buf()));
    let board =
        V3Board::build(builder).map_err(|e| anyhow::anyhow!("the image builds no machine: {e}"))?;
    let host = EmuLinkHost::new(board, fresh_nonce(), true);
    finish_open(args, host, project_dir(args, EmuChip::Esp32V3))
}

/// Bring a fresh [`EmuLinkHost`] up (wait for its hello), deploy `dir`, and
/// hand the board and its link port off to a [`Session`] — the chip-generic
/// back half of every `open_emu_*`.
fn finish_open<B: EmuUsbBoard + ChipReport + 'static>(
    args: &RttArgs,
    mut host: EmuLinkHost<B>,
    dir: PathBuf,
) -> Result<Session> {
    if host.wait_for_line("\"hello\":{", 5_000_000)?.is_none() {
        bail!("no hello:\n{}", host.console().join("\n"));
    }
    let (uid, _) = crate::commands::dev::validation::validate_local_project(&dir)
        .with_context(|| format!("{} validates", dir.display()))?;
    let files =
        crate::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir.clone()))?;
    {
        let mut client = lpa_client::LpClient::new(&mut host);
        block_on(client.deploy_project_files(&uid, files))
            .map_err(|e| anyhow::anyhow!("the deploy failed: {e}"))?;
    }
    eprintln!(
        "link rtt: {} deployed at {:.3} s emulated",
        dir.display(),
        host.board_seconds()
    );
    let now = host.board.micros();
    let earlier: Vec<String> = host.console().to_vec();
    let EmuLinkHost { board, port, .. } = host;
    let mut session = Session::new(Box::new(EmuPipe { board }), port, args.console.as_deref())?;
    for line in earlier {
        session.note_line(now, line)?;
    }
    Ok(session)
}

fn console_file(path: Option<&Path>) -> Result<Option<std::io::LineWriter<std::fs::File>>> {
    path.map(|p| {
        std::fs::File::create(p)
            .map(std::io::LineWriter::new)
            .with_context(|| format!("creating {}", p.display()))
    })
    .transpose()
}

fn secs_us(seconds: f64) -> u64 {
    (seconds * 1e6) as u64
}

fn rtt_ms(samples: &[(u64, u64)]) -> Vec<f64> {
    samples.iter().map(|(_, us)| *us as f64 / 1000.0).collect()
}

fn kib_per_s(bytes: u64, us: u64) -> f64 {
    bytes as f64 / (us.max(1) as f64 / 1e6) / 1024.0
}

/// Frames per second across a run of heartbeats (first to last).
fn window_fps(beats: &[Value]) -> Option<f64> {
    let first = beats.first()?;
    let last = beats.last()?;
    let frames = last["frame_count"].as_f64()? - first["frame_count"].as_f64()?;
    let ms = last["uptime_ms"].as_f64()? - first["uptime_ms"].as_f64()?;
    (ms > 0.0).then(|| round3(frames * 1000.0 / ms))
}

/// n, min, p10, p50, p90, p99, max, mean.
pub(super) fn stats(xs: &[f64]) -> Value {
    if xs.is_empty() {
        return json!({ "n": 0 });
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    json!({
        "n": v.len(),
        "min": round3(v[0]),
        "p10": round3(q(0.10)),
        "p50": round3(q(0.50)),
        "p90": round3(q(0.90)),
        "p99": round3(q(0.99)),
        "max": round3(v[v.len() - 1]),
        "mean": round3(mean),
    })
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// The request pauses' generator: seeded, so two runs pause alike.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Drive a future whose every await completes synchronously (the emulated
/// host steps the board inside `receive`).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_reads_the_quantiles_off_the_sorted_samples() {
        let xs: Vec<f64> = (1..=11).map(f64::from).rev().collect();
        let s = stats(&xs);
        assert_eq!(s["n"], 11);
        assert_eq!(s["min"], 1.0);
        assert_eq!(s["p50"], 6.0);
        assert_eq!(s["p90"], 10.0);
        assert_eq!(s["max"], 11.0);
        assert_eq!(s["mean"], 6.0);
        assert_eq!(stats(&[]), json!({ "n": 0 }));
    }

    #[test]
    fn round_trips_in_frames_are_ms_over_the_frame_time() {
        assert_eq!(in_frames(&[50.0, 100.0], Some(20.0)), vec![1.0, 2.0]);
        assert!(in_frames(&[50.0], None).is_empty());
        assert_eq!(link_kind("lan:10.0.0.7"), "lan");
        assert_eq!(link_kind("/dev/cu.usbmodem1101"), "serial");
    }

    #[test]
    fn window_fps_is_frames_over_uptime_between_the_first_and_last_beat() {
        let beats = [
            json!({ "frame_count": 100, "uptime_ms": 5_000 }),
            json!({ "frame_count": 250, "uptime_ms": 10_000 }),
            json!({ "frame_count": 400, "uptime_ms": 15_000 }),
        ];
        assert_eq!(window_fps(&beats), Some(30.0));
        assert_eq!(window_fps(&beats[..1]), None, "one beat is no window");
    }

    #[test]
    fn the_pause_generator_is_seeded() {
        let (mut a, mut b) = (XorShift(7), XorShift(7));
        let first: Vec<u64> = (0..4).map(|_| a.next()).collect();
        assert_eq!(first, (0..4).map(|_| b.next()).collect::<Vec<_>>());
        assert_ne!(first[0], first[1]);
    }

    #[test]
    fn chip_is_refused_on_a_serial_target_but_fine_on_emu() {
        assert!(
            check_chip_on_target("/dev/cu.usbmodem1101", false, Some(EmuChip::Esp32S3)).is_err()
        );
        assert!(check_chip_on_target("/dev/cu.usbmodem1101", false, None).is_ok());
        assert!(check_chip_on_target("emu:fw-esp32c6", true, Some(EmuChip::Esp32S3)).is_ok());
        assert!(check_chip_on_target("emu:fw-esp32c6", true, None).is_ok());
    }

    #[test]
    fn grade_defaults_to_t2_on_the_c6_and_t1_elsewhere_and_refuses_the_other_grade() {
        assert_eq!(resolve_grade(EmuChip::Esp32C6, None).unwrap(), "t2");
        assert_eq!(resolve_grade(EmuChip::Esp32C6, Some("t1")).unwrap(), "t1");
        assert_eq!(resolve_grade(EmuChip::Esp32C6, Some("t2")).unwrap(), "t2");
        assert!(resolve_grade(EmuChip::Esp32C6, Some("t3")).is_err());

        for chip in [EmuChip::Esp32S3, EmuChip::Esp32V3] {
            assert_eq!(resolve_grade(chip, None).unwrap(), "t1");
            assert_eq!(resolve_grade(chip, Some("t1")).unwrap(), "t1");
            assert!(
                resolve_grade(chip, Some("t2")).is_err(),
                "{chip:?} has only t1"
            );
        }
    }

    #[test]
    fn each_chip_has_its_own_default_project() {
        assert!(
            default_project_path(EmuChip::Esp32C6).ends_with("catalog/projects/playful-choker")
        );
        assert!(default_project_path(EmuChip::Esp32S3).ends_with("projects/test/shader-oracle"));
        assert!(default_project_path(EmuChip::Esp32V3).ends_with("projects/test/five-wire"));
    }

    #[test]
    fn project_dir_prefers_an_explicit_project_over_the_chip_default() {
        let args = RttArgs {
            target: "emu:fw".to_string(),
            board_password: Default::default(),
            chip: Some(EmuChip::Esp32S3),
            json: None,
            console: None,
            count: 1,
            max_gap_ms: 1,
            seed: 1,
            reads: 1,
            read_bytes: 1,
            writes: 1,
            write_bytes: 1,
            warmup_s: 0.0,
            idle_s: 0.0,
            transfers_at_s: 0.0,
            requests_at_s: 0.0,
            tail_s: 0.0,
            grade: None,
            project: Some(PathBuf::from("/explicit/project")),
            label: String::new(),
        };
        assert_eq!(
            project_dir(&args, EmuChip::Esp32S3),
            PathBuf::from("/explicit/project")
        );
        let mut no_override = args;
        no_override.project = None;
        assert_eq!(
            project_dir(&no_override, EmuChip::Esp32S3),
            default_project_path(EmuChip::Esp32S3)
        );
    }
}
