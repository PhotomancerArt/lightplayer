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

use crate::commands::ota_host::OtaHost;

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
        let outcome = self.machine.run_until(&stop);
        // A chip start's `SEAM …` lines, as they come (a ROM-up boot's arrive
        // once the app runs, inside the run).
        super::handler::print_seam_lines(&mut self.machine);
        match outcome {
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
        let mut lines = finish_lines(
            m.instructions(),
            (m.bus.unmapped_reads(), m.bus.unmapped_writes()),
            flash,
        );
        // `--flash-cut`: the cut and the in-range census, when one was armed.
        lines.extend(m.flash_cut_summary());
        lines
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
    /// An over-the-air update this host drives on the link's channel 3
    /// (`--ota-offer`).
    pub ota: Option<OtaHost>,
    /// `--ota-cut-after`'s request was answered: the run is to end here, as
    /// a power cut (the flash written back).
    pub ota_cut: bool,
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
            ota_cut: false,
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
                PortRead::Log(_) => {}
                PortRead::Up { .. } => {
                    if let Some(ota) = self.ota.as_mut() {
                        ota.link_up(now / 1_000);
                    }
                }
                PortRead::Reset { .. } => {
                    self.link_errors += 1;
                    if let Some(ota) = self.ota.as_mut() {
                        ota.link_down(now / 1_000);
                    }
                }
                PortRead::Note(note) => self.notes.push(note),
            }
        }
        self.service_ota(now);
        Ok(())
    }

    /// The update on channel 3: the board's messages in, the host's out,
    /// and the host's lines onto the console.
    fn service_ota(&mut self, now: u64) {
        let Some(ota) = self.ota.as_mut() else {
            return;
        };
        while let Some(message) = self.port.poll_update() {
            ota.on_board(now / 1_000, &message);
        }
        ota.tick(now / 1_000);
        while let Some(message) = ota.next_outgoing() {
            match self.port.send_update(message) {
                Ok(()) => ota.sent(),
                // The send ring is full: it drains as frames go out.
                Err(lpc_wire::lp_link::SendError::Full) => break,
                Err(_) => {
                    ota.sent();
                    self.link_errors += 1;
                }
            }
        }
        let cut = ota.cut_due();
        let lines = ota.take_lines();
        // Push what was queued out now: a cut ends the run at this slice.
        while let Some(frame) = self.port.poll_transmit(now) {
            let frame = frame.to_vec();
            self.board.push_usb_input(&frame);
        }
        for line in lines {
            self.line(line);
        }
        self.ota_cut |= cut;
    }

    /// Step until the board has lived `until_us` emulated microseconds since
    /// power-on, or a new console line contains `exit_on`. `Ok(true)` is the
    /// match.
    pub fn run_until(&mut self, until_us: u64, exit_on: Option<&str>) -> Result<bool> {
        while self.board.micros() < until_us && !self.ota_cut {
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
        while self.board.micros() < until && !self.ota_cut {
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
