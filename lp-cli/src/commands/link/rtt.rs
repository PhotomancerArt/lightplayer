//! `lp-cli link rtt`: how promptly a rendering board answers — request round
//! trips, the link's own round trips, and transfer rates — on silicon (wall
//! clock) or on the emulated C6 in this process (EMULATED time).
//!
//! Written for the C6's link thread (plan
//! `lp2025/2026-10-01-1756-c6-link-io-thread`, after the spike
//! `lp2025/2026-10-01-1200-io-thread-spike`); it measures any board that
//! speaks lp-link on a serial port.
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
use std::time::Instant;

use anyhow::{Context, Result, bail};
use lpc_wire::server::FsRequest;
use lpc_wire::{ClientMessage, ClientRequest, PortRead, WireLinkPort};
use serde_json::{Value, json};

use super::args::RttArgs;
use super::lab_port::{LabPort, TermiosMode};
use crate::commands::emu::link_host::{
    C6Board, EmuLinkHost, EmuUsbBoard, SLICE_US, console_lines, fresh_nonce,
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
    let mut session = match args.target.strip_prefix("emu:") {
        Some(elf) => open_emu(args, Path::new(elf))?,
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
    eprintln!("link rtt: request RTT ms {}", stats(&request_rtts));
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
        "idle_fps": idle_fps,
        "requests": requests,
        "request_rtt_ms": stats(&request_rtts),
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

/// The byte pipe under the link: the emulated C6, or a serial port.
trait Pipe {
    /// Move time on: one slice of the emulated board; nothing on a port
    /// (its read waits ~1 ms).
    fn advance(&mut self) -> Result<()>;
    fn now_us(&self) -> u64;
    fn take(&mut self) -> Result<Vec<u8>>;
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
        let bytes = self.pipe.take()?;
        if !bytes.is_empty() {
            self.link.on_bytes(now, &bytes);
        }
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

/// The emulated C6 in this process: emulated time.
struct EmuPipe {
    board: C6Board,
}

impl Pipe for EmuPipe {
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
        let m = &mut self.board.machine;
        let mut pads = Vec::new();
        for (pad, _) in m.routed_pads() {
            let frames = m.frames(pad.0);
            if frames.is_empty() {
                continue;
            }
            let starts: Vec<f64> = frames
                .iter()
                .map(|f| f.start as f64 / lp_emu_esp32c6::memmap::CYCLES_PER_US as f64)
                .collect();
            let gaps: Vec<f64> = starts.windows(2).map(|w| (w[1] - w[0]) / 1000.0).collect();
            pads.push(json!({
                "pad": pad.0,
                "frames": frames.len(),
                "errors": frames.iter().map(|f| f.error_count).sum::<u64>(),
                "incomplete": frames.iter().filter(|f| !f.is_complete()).count(),
                "leds": frames.last().map(|f| f.leds()),
                "interval_ms": stats(&gaps),
                "frame_starts_us": starts.iter().map(|s| s.round() as u64).collect::<Vec<_>>(),
            }));
        }
        let mut refills = Vec::new();
        // The C6 has two RMT TX channels.
        for ch in 0..2 {
            let r = m.rmt_refill_stats(ch);
            if r.refills == 0 && r.unanswered == 0 {
                continue;
            }
            refills.push(json!({
                "ch": ch,
                "refills": r.refills,
                "unanswered": r.unanswered,
                "entry_max_words": r.entry_max,
                "fill_max_words": r.fill_max,
                "half_words": r.half_words,
                "entry_hist": r.entry_hist.to_vec(),
                "fill_hist": r.fill_hist.to_vec(),
            }));
        }
        json!({ "ws281x": pads, "rmt_refill": refills, "instructions": m.instructions() })
    }
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

fn open_emu(args: &RttArgs, elf: &Path) -> Result<Session> {
    use lp_emu_esp32c6::flash::FlashBacking;
    use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
    let grade = match args.grade.as_str() {
        "t1" => TimeGrade::T1,
        "t2" => TimeGrade::T2,
        g => bail!("--grade must be t1 or t2, not {g}"),
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
    let mut host = EmuLinkHost::new(C6Board::new(machine)?, fresh_nonce(), true);
    if host.wait_for_line("\"hello\":{", 5_000_000)?.is_none() {
        bail!("no hello:\n{}", host.console().join("\n"));
    }
    let dir: PathBuf = args.project.clone().unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../catalog/projects/playful-choker")
    });
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
fn stats(xs: &[f64]) -> Value {
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
}
