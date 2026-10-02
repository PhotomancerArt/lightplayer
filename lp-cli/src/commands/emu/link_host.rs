//! The product's host end of an emulated board's USB link, in this process.
//!
//! Since `WIRE_PROTO_VERSION` 30 the shipped C6 and S3 images speak lp-link
//! on USB-Serial-JTAG (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`),
//! and since 32 the classic does on UART0 (plan `classic-uart-on-lp-link`):
//! nothing but boot text and panics reaches the port as plain text, the
//! board's `log` lines ride the link's log channel, and its hello and
//! heartbeats go out only once a host has brought the link up. So a tool
//! that used to read an emulated board's console straight off the port — the
//! heap ratchet, the figures, the walk — now needs a host on the link.
//!
//! [`EmuLinkHost`] is that host: the machine, stepped in short slices of
//! EMULATED time, and one [`WireLinkPort`] serviced between the slices, the
//! shape of `lp-cli link lab`'s `EmuPipe` and `tests/emu_usb_link.rs`. What
//! the board says comes out as console lines — raw text and log records
//! rendered as the board used to print them (`[INFO] module: text`), and each
//! wire message as the `M!{json}` line it used to be — so a caller that
//! greps a console keeps grepping. It is also an `lpa-client` `ClientIo`,
//! so the product's own client can drive a conversation over it.
//!
//! It lives in lp-cli rather than under `lp-emu/` because lp-link and
//! lpc-wire are product crates and the emulators are MIT (the fence, D11).

use std::collections::VecDeque;
use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use lp_emu_esp_common::QueueHandle;
use lpc_wire::lp_link::LinkConfig;
use lpc_wire::{
    ClientMessage, LinkCounters, PortRead, TransportError, WireLinkPort, WireServerMessage,
};

/// Emulated microseconds per slice: the host services its link between
/// slices, so this bounds its reaction time (the comms lab's own figure).
pub const SLICE_US: u64 = 250;

/// One emulated board whose host link the host holds in process: the
/// USB-Serial-JTAG port on the C6 and S3, UART0 on the classic (the `usb`
/// in the method names is the first two's; the classic's bytes are its
/// UART's).
pub trait EmuUsbBoard {
    /// The host end's link configuration: the preset for this board's
    /// transport, as a product host picks it for the port it opened.
    fn link_config(&self) -> LinkConfig {
        LinkConfig::usb()
    }
    /// What the host link is, for the run's report.
    fn link_name(&self) -> &'static str {
        "usb-serial-jtag"
    }
    /// Run the board `us` more microseconds of emulated time. `Err` is the
    /// machine stopping for any reason other than its deadline.
    fn run_for_us(&mut self, us: u64) -> Result<(), String>;
    /// Emulated microseconds since power-on.
    fn micros(&self) -> u64;
    /// Bytes the board wrote to the host since the last call.
    fn take_usb_output(&mut self) -> Vec<u8>;
    /// Bytes for the board's OUT endpoint.
    fn push_usb_input(&mut self, bytes: &[u8]);
    /// End of a run: flush what the machine buffers (decoded frames, the
    /// flash write-back) and describe it, one line each.
    fn finish(&mut self) -> Vec<String>;
}

/// The run-report lines both chips print the same way.
fn finish_lines(
    instructions: u64,
    unmapped: (u64, u64),
    flash: std::io::Result<bool>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match flash {
        Ok(true) => lines.push("flash image written back".to_string()),
        Ok(false) => {}
        Err(e) => lines.push(format!("could not write the flash image back: {e}")),
    }
    lines.push(format!("{instructions} instructions"));
    let (reads, writes) = unmapped;
    if reads + writes > 0 {
        lines.push(format!(
            "{reads} unmapped read(s), {writes} unmapped write(s) — each read zero and was \
             believed. Re-run with --strict-bus to fault on them."
        ));
    } else {
        lines.push("no unmapped accesses".to_string());
    }
    lines
}

/// The emulated C6, with an in-process queue as the host's OUT half. Build
/// the machine with `usb_sj_queue_source()` and an attached, draining host.
pub struct C6Board {
    pub machine: lp_emu_esp32c6::machine::Esp32C6Machine,
    queue: QueueHandle,
}

impl C6Board {
    pub fn new(machine: lp_emu_esp32c6::machine::Esp32C6Machine) -> Result<Self> {
        let Some(queue) = machine.usb_sj_host_handle() else {
            bail!("the machine was built without `usb_sj_queue_source()`: nothing to host");
        };
        Ok(Self { machine, queue })
    }
}

impl EmuUsbBoard for C6Board {
    fn run_for_us(&mut self, us: u64) -> Result<(), String> {
        use lp_emu_esp32c6::machine::{Outcome, StopCondition};
        let stop = StopCondition {
            stop_cycle: Some(self.machine.cycles() + us * lp_emu_esp32c6::memmap::CYCLES_PER_US),
            ..Default::default()
        };
        match self.machine.run_until(&stop) {
            Outcome::Deadline { .. } => Ok(()),
            other => Err(super::handler::describe(&other)),
        }
    }

    fn micros(&self) -> u64 {
        self.machine.micros()
    }

    fn take_usb_output(&mut self) -> Vec<u8> {
        self.machine.take_usb_sj_output()
    }

    fn push_usb_input(&mut self, bytes: &[u8]) {
        self.queue.push(bytes);
    }

    fn finish(&mut self) -> Vec<String> {
        let m = &mut self.machine;
        m.flush_frames();
        let flash = m.flush_flash();
        finish_lines(
            m.instructions(),
            (m.bus.unmapped_reads(), m.bus.unmapped_writes()),
            flash,
        )
    }
}

/// The emulated S3, with an in-process queue as the host's OUT half. On this
/// chip the console IS the link, so the machine's console log is what the
/// host takes.
pub struct S3Board {
    pub machine: lp_emu_esp32s3::machine::Machine,
    queue: QueueHandle,
}

impl S3Board {
    /// Install the host's queue on `builder`, build, and wrap. `usb_host` is
    /// the power-on USB host state — attached and draining
    /// (`UsbHost::Attached { draining: true }`) is every caller's default
    /// today; `run_s3`'s `--usb-host` is the one place that overrides it.
    pub fn build(
        builder: lp_emu_esp32s3::machine::Esp32S3Builder,
        usb_host: lp_emu_esp32s3::machine::UsbHost,
    ) -> Result<Self> {
        let (source, queue) = lp_emu_esp_common::QueueSource::new();
        let machine = builder
            .usb_host(usb_host)
            .usb_sj_source(Box::new(source))
            .build()
            .map_err(|e| anyhow::anyhow!("building the S3 machine: {e}"))?;
        Ok(Self { machine, queue })
    }
}

impl EmuUsbBoard for S3Board {
    fn run_for_us(&mut self, us: u64) -> Result<(), String> {
        use lp_emu_esp32s3::machine::{Outcome, StopCondition};
        let stop = StopCondition {
            stop_cycle: Some(self.machine.cycles() + us * lp_emu_esp32s3::memmap::CYCLES_PER_US),
            ..Default::default()
        };
        match self.machine.run_until(&stop) {
            Outcome::Deadline { .. } => Ok(()),
            other => Err(format!("{other:?}")),
        }
    }

    fn micros(&self) -> u64 {
        self.machine.micros()
    }

    fn take_usb_output(&mut self) -> Vec<u8> {
        let log = self.machine.console();
        let bytes = log.bytes();
        if !bytes.is_empty() {
            log.replace(&[]);
        }
        bytes
    }

    fn push_usb_input(&mut self, bytes: &[u8]) {
        self.queue.push(bytes);
    }

    fn finish(&mut self) -> Vec<String> {
        let m = &mut self.machine;
        m.flush_frames();
        let flash = m.flush_flash();
        finish_lines(
            m.instructions(),
            (m.bus().unmapped_reads(), m.bus().unmapped_writes()),
            flash,
        )
    }
}

/// The emulated classic (v3), with an in-process queue as the host's side of
/// UART0's cable. Since wire proto 32 its UART0 is an lp-link (plan
/// `classic-uart-on-lp-link`), so the host end takes [`LinkConfig::uart`].
/// Built to reboot on reset, so a Reboot request (a software system reset)
/// boots it again rather than leaving the guest in the ROM's reset path.
pub struct V3Board {
    pub machine: lp_emu_esp32v3::machine::Machine,
    queue: QueueHandle,
}

impl V3Board {
    /// Install the host's queue as UART0's RX (its TX kept in memory),
    /// build, and wrap.
    pub fn build(builder: lp_emu_esp32v3::machine::Esp32V3Builder) -> Result<Self> {
        let (source, queue) = lp_emu_esp_common::QueueSource::new();
        let machine = builder
            .uart0(lp_emu_esp32v3::machine::Uart0Sink::Memory)
            .uart0_source(Box::new(source))
            .reboot_on_reset(true)
            .build()
            .map_err(|e| anyhow::anyhow!("building the classic machine: {e}"))?;
        Ok(Self { machine, queue })
    }
}

impl EmuUsbBoard for V3Board {
    fn link_config(&self) -> LinkConfig {
        LinkConfig::uart()
    }

    fn link_name(&self) -> &'static str {
        "uart0"
    }

    fn run_for_us(&mut self, us: u64) -> Result<(), String> {
        use lp_emu_esp32v3::machine::{Outcome, StopCondition};
        let stop = StopCondition {
            stop_cycle: Some(self.machine.cycles() + us * lp_emu_esp32v3::memmap::CYCLES_PER_US),
            ..Default::default()
        };
        match self.machine.run_until(&stop) {
            Outcome::Deadline { .. } => Ok(()),
            other => Err(format!("{other:?}")),
        }
    }

    fn micros(&self) -> u64 {
        self.machine.micros()
    }

    fn take_usb_output(&mut self) -> Vec<u8> {
        let log = self.machine.uart0();
        let bytes = log.bytes();
        if !bytes.is_empty() {
            log.replace(&[]);
        }
        bytes
    }

    fn push_usb_input(&mut self, bytes: &[u8]) {
        self.queue.push(bytes);
    }

    fn finish(&mut self) -> Vec<String> {
        let m = &mut self.machine;
        m.flush_frames();
        let flash = m.flush_flash();
        let mut lines = finish_lines(
            m.instructions(),
            (m.bus().unmapped_reads(), m.bus().unmapped_writes()),
            flash,
        );
        if m.reboots() > 0 {
            lines.push(format!("{} reboot(s)", m.reboots()));
        }
        if m.control_lines() > 0 {
            // The cable's side, in the `state` verb's own words.
            let state = lp_emu_esp32v3::control::ControlReply::State {
                cycle: m.cycles(),
                report: m.cable_report(),
            };
            lines.push(format!("cable: {state}"));
        }
        if let Some((to_host, to_device)) = m.uart0_fault_counters() {
            lines.push(format!(
                "uart-faults: device→host {to_host}; host→device {to_device}"
            ));
        }
        lines
    }
}

/// One wire message a hosted link read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostedMessage {
    pub id: u64,
    /// It came as a learned packed payload (JSON Pack), not JSON.
    pub packed: bool,
    /// Bytes on the link's proto channel.
    pub wire_len: usize,
    /// Bytes of its JSON.
    pub json_len: usize,
    /// Emulated microseconds since power-on when it was read.
    pub at_us: u64,
}

/// A host on an emulated board's link. See the module docs.
pub struct EmuLinkHost<B: EmuUsbBoard> {
    pub board: B,
    pub port: WireLinkPort,
    start: u64,
    /// Wire messages not yet taken by [`ClientIo::receive`].
    pending: VecDeque<WireServerMessage>,
    /// Whether wire messages are queued for a client at all. A host that
    /// only watches (the heap ratchet) leaves it off so heartbeats do not
    /// pile up for nobody.
    queue_messages: bool,
    /// Every console line so far, in order (see [`Self::console`]).
    console: Vec<String>,
    console_sink: Option<Box<dyn Write>>,
    echo: bool,
    /// Link resets and messages that did not parse: an app error each.
    pub link_errors: u32,
    /// Every wire message read, in order: how it came and what it cost.
    pub messages: Vec<HostedMessage>,
    pub notes: Vec<String>,
    /// How long a `receive` waits for an answer, in emulated seconds.
    pub answer_budget_s: f64,
    wall_deadline: Option<Instant>,
    /// OTA split-link spike: firmware this host offers on the update channel.
    pub ota: Option<OtaServe>,
    /// Update-channel messages waiting for room in the link's send ring.
    ota_out: VecDeque<Vec<u8>>,
}

/// OTA split-link spike: a build this host offers, and serves chunk by chunk.
///
/// The update channel's messages, all little-endian:
/// - host → board `O` core_len:u32 engine_len:u32 build_id:[u8;48] — the offer
/// - board → host `Q` — "what do you have?" (answered with the offer)
/// - board → host `R` kind:u8 offset:u32 len:u32 — a request (`kind` `C`/`E`)
/// - host → board `D` kind:u8 offset:u32 bytes… — the answer
pub struct OtaServe {
    pub core: Vec<u8>,
    pub engine: Vec<u8>,
    pub build_id: [u8; 48],
    pub offers: u32,
    pub requests: u32,
    pub served_bytes: u64,
    /// Offers the board refused because that build already failed its trial
    /// on it (`F`).
    pub refusals: u32,
    /// Spike: end the run (a power cut) once this many requests are served.
    pub cut_after: Option<u32>,
    /// Spike: the ticket every offer carries (an untrusted link's
    /// authorization, see `fw-esp32c6`'s `ota/update_ticket.rs`).
    pub ticket: Option<[u8; 16]>,
    /// Offers refused for want of a login or a ticket (`A`).
    pub auth_refusals: u32,
    /// Sectors kept in flight ahead of the board's request (1: answer only
    /// what was asked). Over BLE the link is round-trip bound, and sending
    /// ahead is what keeps the radio busy while the board writes flash.
    pub ahead: u32,
    /// The chunk stream being sent ahead: (kind, next offset to send).
    stream: Option<(u8, usize)>,
    /// When each kind's first and latest chunk went out (for its rate).
    pub sent_at: [(Option<std::time::Instant>, Option<std::time::Instant>, u64); 2],
}

impl OtaServe {
    /// `core.bin` and `engine.bin` from `scripts/ota-spike/build-split.sh`;
    /// the build id is the engine header's own.
    pub fn from_dir(dir: &std::path::Path) -> Result<Self> {
        let core = std::fs::read(dir.join("core.bin"))?;
        let engine = std::fs::read(dir.join("engine.bin"))?;
        anyhow::ensure!(
            engine.len() > 56 && &engine[..8] == b"LPENGIN1",
            "engine.bin has no header"
        );
        let mut build_id = [0u8; 48];
        build_id.copy_from_slice(&engine[8..56]);
        Ok(Self {
            core,
            engine,
            build_id,
            offers: 0,
            requests: 0,
            served_bytes: 0,
            refusals: 0,
            cut_after: None,
            ticket: None,
            auth_refusals: 0,
            ahead: 1,
            stream: None,
            sent_at: [(None, None, 0); 2],
        })
    }

    pub fn offer(&mut self) -> Vec<u8> {
        self.offers += 1;
        let mut out = vec![b'O'];
        out.extend_from_slice(&(self.core.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.engine.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.build_id);
        if let Some(ticket) = &self.ticket {
            out.extend_from_slice(ticket);
        }
        out
    }

    /// The answer to one board message, if any: see [`Self::answer_all`].
    pub fn answer(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        self.answer_all(msg).into_iter().next()
    }

    /// Every message to send for one board message: an offer for `Q`; for a
    /// request `R`, the chunk asked for and, with `ahead > 1`, the chunks
    /// after it, so `ahead` stay in flight. A request behind what was sent
    /// (the board lost chunks with a link reset) restarts the stream there;
    /// the engine's header sector (offset 0, asked for last) goes alone.
    pub fn answer_all(&mut self, msg: &[u8]) -> Vec<Vec<u8>> {
        const SECTOR: usize = 4096;
        let Some(&first) = msg.first() else {
            return Vec::new();
        };
        match first {
            b'Q' => vec![self.offer()],
            b'F' => {
                self.refusals += 1;
                Vec::new()
            }
            b'A' => {
                self.auth_refusals += 1;
                Vec::new()
            }
            b'R' if msg.len() == 10 => {
                let kind = msg[1];
                let off = u32::from_le_bytes(msg[2..6].try_into().unwrap()) as usize;
                let total = if kind == b'C' {
                    self.core.len()
                } else {
                    self.engine.len()
                };
                if off >= total {
                    return Vec::new();
                }
                self.requests += 1;
                let ahead = self.ahead.max(1) as usize;
                let header_last = kind == b'E' && off == 0;
                let restart = match self.stream {
                    Some((k, next)) => {
                        k != kind || header_last || off > next || off + (ahead - 1) * SECTOR < next
                    }
                    None => true,
                };
                if restart {
                    self.stream = Some((kind, off));
                }
                let next = self.stream.map_or(off, |(_, n)| n).max(off);
                let to = if header_last {
                    SECTOR.min(total)
                } else {
                    (off + ahead * SECTOR).min(total)
                };
                let mut out = Vec::new();
                let mut at = next;
                while at < to {
                    let end = (at + SECTOR).min(total);
                    let src = if kind == b'C' { &self.core } else { &self.engine };
                    let mut m = vec![b'D', kind];
                    m.extend_from_slice(&(at as u32).to_le_bytes());
                    m.extend_from_slice(&src[at..end]);
                    self.served_bytes += (end - at) as u64;
                    let slot = &mut self.sent_at[usize::from(kind != b'C')];
                    let now = std::time::Instant::now();
                    slot.0.get_or_insert(now);
                    slot.1 = Some(now);
                    slot.2 += (end - at) as u64;
                    out.push(m);
                    at = end;
                }
                self.stream = Some((kind, if header_last { 0 } else { at.max(next) }));
                out
            }
            _ => Vec::new(),
        }
    }

    /// One line on what was served, per kind, and its rate.
    pub fn describe_rates(&self) -> String {
        let mut parts = Vec::new();
        for (name, (a, b, bytes)) in ["core", "engine"].iter().zip(self.sent_at.iter()) {
            if let (Some(a), Some(b)) = (a, b) {
                let secs = b.duration_since(*a).as_secs_f64().max(1e-3);
                parts.push(format!(
                    "{name} {bytes} B in {secs:.1} s ({:.1} KB/s)",
                    *bytes as f64 / 1024.0 / secs
                ));
            }
        }
        parts.join(", ")
    }
}

impl<B: EmuUsbBoard> EmuLinkHost<B> {
    /// A host with a fresh link port on the board's transport's preset
    /// ([`EmuUsbBoard::link_config`]; `want_packed`: ask the board to pack
    /// its replies, as every product host does).
    pub fn new(board: B, nonce: u32, want_packed: bool) -> Self {
        let start = board.micros();
        let config = board.link_config();
        Self {
            board,
            port: WireLinkPort::new(config, nonce, want_packed),
            start,
            pending: VecDeque::new(),
            queue_messages: true,
            console: Vec::new(),
            console_sink: None,
            echo: false,
            link_errors: 0,
            messages: Vec::new(),
            notes: Vec::new(),
            answer_budget_s: 60.0,
            wall_deadline: None,
            ota: None,
            ota_out: VecDeque::new(),
        }
    }

    /// Write every console line here as it arrives, newline-terminated.
    pub fn with_console_sink(mut self, sink: Box<dyn Write>) -> Self {
        self.console_sink = Some(sink);
        self
    }

    /// Echo every console line to stderr as it arrives.
    pub fn echo_to_stderr(mut self, echo: bool) -> Self {
        self.echo = echo;
        self
    }

    /// Queue wire messages for a client (on by default).
    pub fn queue_messages(mut self, on: bool) -> Self {
        self.queue_messages = on;
        self
    }

    /// Start or stop queueing wire messages for a client; stopping drops
    /// what is queued.
    pub fn set_queue_messages(&mut self, on: bool) {
        self.queue_messages = on;
        if !on {
            self.pending.clear();
        }
    }

    /// A wall-clock net: a step past it fails.
    pub fn wall_timeout(mut self, timeout: Duration) -> Self {
        self.wall_deadline = Some(Instant::now() + timeout);
        self
    }

    /// Emulated microseconds since the host was made.
    pub fn now_us(&self) -> u64 {
        self.board.micros() - self.start
    }

    /// Emulated seconds since power-on.
    pub fn board_seconds(&self) -> f64 {
        self.board.micros() as f64 / 1e6
    }

    /// Every console line so far: raw text, log records, `M!{json}` for each
    /// wire message, and `[link] …` notes, in the order they arrived.
    pub fn console(&self) -> &[String] {
        &self.console
    }

    pub fn counters(&self) -> LinkCounters {
        self.port.counters()
    }

    /// One slice of the board, then the host's end of the link.
    pub fn step(&mut self) -> Result<()> {
        if let Some(deadline) = self.wall_deadline
            && Instant::now() > deadline
        {
            bail!(
                "hit the wall-clock net at {:.3} s emulated",
                self.board_seconds()
            );
        }
        self.board
            .run_for_us(SLICE_US)
            .map_err(|why| anyhow::anyhow!("the emulated board stopped: {why}"))?;
        let now = self.now_us();
        let bytes = self.board.take_usb_output();
        if !bytes.is_empty() {
            self.port.on_bytes(now, &bytes);
        }
        while let Some(frame) = self.port.poll_transmit(now) {
            let frame = frame.to_vec();
            self.board.push_usb_input(&frame);
        }
        while let Some(read) = self.port.poll_read() {
            for line in console_lines(&read) {
                self.line(line);
            }
            match read {
                PortRead::Message(payload) => match payload.message {
                    Ok(message) => {
                        self.messages.push(HostedMessage {
                            id: message.id,
                            packed: payload.packed,
                            wire_len: payload.wire_len,
                            json_len: payload.json.len(),
                            at_us: self.board.micros(),
                        });
                        if self.queue_messages {
                            self.pending.push_back(message);
                        }
                    }
                    Err(_) => self.link_errors += 1,
                },
                PortRead::Up { .. } => {
                    if let Some(ota) = self.ota.as_mut() {
                        let offer = ota.offer();
                        let _ = self.port.send_update(&offer);
                    }
                }
                PortRead::Log(_) => {}
                PortRead::Reset { .. } => self.link_errors += 1,
                PortRead::Note(note) => self.notes.push(note),
            }
        }
        while let Some(msg) = self.port.poll_update() {
            let Some(ota) = self.ota.as_mut() else {
                continue;
            };
            self.ota_out.extend(ota.answer_all(&msg));
            if ota.cut_after.is_some_and(|n| ota.requests >= n) {
                bail!("ota: power cut after {} request(s)", ota.requests);
            }
        }
        // The send ring drains as frames go out; what does not fit waits.
        while let Some(next) = self.ota_out.front() {
            if self.port.send_update(next).is_err() {
                break;
            }
            self.ota_out.pop_front();
        }
        Ok(())
    }

    /// Step until the board has lived `until_us` emulated microseconds since
    /// power-on, or a new console line contains `exit_on`. `Ok(true)` is the
    /// match.
    pub fn run_until(&mut self, until_us: u64, exit_on: Option<&str>) -> Result<bool> {
        while self.board.micros() < until_us {
            let seen = self.console.len();
            self.step()?;
            if let Some(needle) = exit_on
                && self.console[seen..]
                    .iter()
                    .any(|line| line.contains(needle))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Step until a console line contains `needle`, for at most `budget_us`
    /// more emulated microseconds. The line, or `None` at the budget.
    pub fn wait_for_line(&mut self, needle: &str, budget_us: u64) -> Result<Option<String>> {
        let mut seen = 0;
        if let Some(line) = self.console.iter().find(|l| l.contains(needle)) {
            return Ok(Some(line.clone()));
        }
        let until = self.board.micros() + budget_us;
        while self.board.micros() < until {
            seen = seen.max(self.console.len());
            self.step()?;
            if let Some(line) = self.console[seen..].iter().find(|l| l.contains(needle)) {
                return Ok(Some(line.clone()));
            }
        }
        Ok(None)
    }

    /// Send one request (a fire-and-forget; its answer lands in the console
    /// and, when queued, in [`ClientIo::receive`]).
    pub fn send(&mut self, message: &ClientMessage) -> Result<()> {
        self.port
            .send_client(message)
            .map_err(|error| anyhow::anyhow!("the link refused the request: {error:?}"))
    }

    fn line(&mut self, line: String) {
        if self.echo {
            eprintln!("{line}");
        }
        if let Some(sink) = self.console_sink.as_mut() {
            let _ = writeln!(sink, "{line}");
        }
        self.console.push(line);
    }
}

/// Implemented on the borrow, so a caller keeps the host (and its console
/// and counters) after the client is done with it.
#[async_trait::async_trait(?Send)]
impl<B: EmuUsbBoard> lpa_client::ClientIo for &mut EmuLinkHost<B> {
    async fn send(&mut self, message: ClientMessage) -> Result<(), TransportError> {
        self.port
            .send_client(&message)
            .map_err(|error| TransportError::Other(format!("the link refused it: {error:?}")))
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let deadline = self.board_seconds() + self.answer_budget_s;
        loop {
            if let Some(message) = self.pending.pop_front() {
                return Ok(message);
            }
            if self.board_seconds() > deadline {
                return Err(TransportError::Other(format!(
                    "no answer within {} emulated seconds",
                    self.answer_budget_s
                )));
            }
            self.step()
                .map_err(|error| TransportError::Other(error.to_string()))?;
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// What one read off a host's link port looks like on a console: the line(s)
/// the board used to print before lp-link. Raw text and log records as they
/// came, each wire message as its `M!{json}` line (and a note when it did not
/// parse), and the link's own events as `[link] …`.
///
/// One renderer for every host that writes a console — this one, and `lp-cli
/// link capture` on a real port — so an emulated capture and a board's read
/// the same way, which is what lets `validate replay` compare them.
pub fn console_lines(read: &PortRead) -> Vec<String> {
    match read {
        PortRead::Message(payload) => {
            let mut lines = vec![format!("M!{}", payload.json)];
            if let Err(error) = &payload.message {
                lines.push(format!("[link] a message did not parse: {error}"));
            }
            lines
        }
        PortRead::Log(line) => vec![line.clone()],
        PortRead::Up { generation } => vec![format!("[link] up (session {generation})")],
        PortRead::Reset { reason } => vec![format!("[link] reset ({reason:?})")],
        PortRead::Note(note) => vec![format!("[link] {note}")],
    }
}

/// One line for a host link's counters: what it sent and read, and what it
/// had to recover from.
pub fn describe_link_counters(c: &LinkCounters) -> String {
    format!(
        "{} frames out / {} in, {} resent, {} damaged, {} stale partials, {} duplicates, \
         {} resets, {} payload errors",
        c.frames_tx,
        c.frames_rx,
        c.resends,
        c.damaged,
        c.stale_partials,
        c.duplicates,
        c.resets.total,
        c.payload_errors
    )
}

/// A random link nonce, as every product host draws one per port open.
pub fn fresh_nonce() -> u32 {
    lpa_client::transport_serial::fresh_link_nonce()
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: usize = 4096;

    #[test]
    fn one_ahead_answers_exactly_what_was_asked() {
        let mut ota = serve(10 * S, 5 * S + 100);
        assert_eq!(offsets(&ota.answer_all(&req(b'C', 0))), [0]);
        assert_eq!(offsets(&ota.answer_all(&req(b'C', S))), [S]);
        assert_eq!(offsets(&ota.answer_all(&req(b'C', 2 * S))), [2 * S]);
    }

    #[test]
    fn ahead_keeps_that_many_chunks_in_flight() {
        let mut ota = serve(10 * S, 0);
        ota.ahead = 4;
        assert_eq!(offsets(&ota.answer_all(&req(b'C', 0))), [0, S, 2 * S, 3 * S]);
        // Each request after the first adds one chunk at the far end.
        assert_eq!(offsets(&ota.answer_all(&req(b'C', S))), [4 * S]);
        assert_eq!(offsets(&ota.answer_all(&req(b'C', 2 * S))), [5 * S]);
        // Near the end nothing is sent past the image.
        for k in 3..=6 {
            ota.answer_all(&req(b'C', k * S));
        }
        assert!(ota.answer_all(&req(b'C', 8 * S)).is_empty());
        assert_eq!(ota.served_bytes, 10 * S as u64);
    }

    #[test]
    fn a_request_behind_the_stream_restarts_it_there() {
        let mut ota = serve(10 * S, 0);
        ota.ahead = 4;
        ota.answer_all(&req(b'C', 0));
        ota.answer_all(&req(b'C', S));
        // A link reset lost chunks 2..5: the board asks for 2 again.
        assert_eq!(
            offsets(&ota.answer_all(&req(b'C', 2 * S))),
            [5 * S],
            "in step: 2 + 3 ahead = 5 is the next new chunk"
        );
        assert_eq!(
            offsets(&ota.answer_all(&req(b'C', 2 * S))),
            [2 * S, 3 * S, 4 * S, 5 * S],
            "behind: resend from where the board is"
        );
    }

    #[test]
    fn the_engine_header_goes_alone_and_last() {
        let mut ota = serve(S, 3 * S + 10);
        ota.ahead = 4;
        assert_eq!(offsets(&ota.answer_all(&req(b'E', S))), [S, 2 * S, 3 * S]);
        assert!(ota.answer_all(&req(b'E', 2 * S)).is_empty());
        assert!(ota.answer_all(&req(b'E', 3 * S)).is_empty());
        let header = ota.answer_all(&req(b'E', 0));
        assert_eq!(offsets(&header), [0]);
        assert_eq!(header[0].len(), 6 + S);
    }

    fn serve(core: usize, engine: usize) -> OtaServe {
        let mut engine_bytes = vec![0u8; engine.max(64)];
        engine_bytes[..8].copy_from_slice(b"LPENGIN1");
        OtaServe {
            core: vec![1; core],
            engine: engine_bytes,
            build_id: [0; 48],
            offers: 0,
            requests: 0,
            served_bytes: 0,
            refusals: 0,
            cut_after: None,
            ticket: None,
            auth_refusals: 0,
            ahead: 1,
            stream: None,
            sent_at: [(None, None, 0); 2],
        }
    }

    fn req(kind: u8, off: usize) -> Vec<u8> {
        let mut m = vec![b'R', kind];
        m.extend_from_slice(&(off as u32).to_le_bytes());
        m.extend_from_slice(&(S as u32).to_le_bytes());
        m
    }

    fn offsets(msgs: &[Vec<u8>]) -> Vec<usize> {
        msgs.iter()
            .map(|m| u32::from_le_bytes(m[2..6].try_into().unwrap()) as usize)
            .collect()
    }
}
