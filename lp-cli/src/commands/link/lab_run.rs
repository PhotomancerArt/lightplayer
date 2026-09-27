//! The host edge of the comms lab: one [`Link`] and one [`LabHost`] driven
//! over a pipe, either in wall-clock time (a serial device, an emulated
//! board's socket) or in the emulator's own time (the C6 machine in this
//! process, stepped slice by slice — deterministic, and the only honest clock
//! for an emulated number).

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lp_emu_esp_common::QueueHandle;
use lp_emu_esp_common::link_faults::{FaultCounters, LinkFaults};
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade, UsbHost,
};
use lp_link::lab::{LabCommand, LabHost, LabPlan, LabReport};
use lp_link::{
    CH_CONTROL, Link, LinkConfig, LinkCounters, LinkEvent, Micros, ResetReason, SelectiveRepeat,
};
use serde_json::{Value, json};

use super::lab_port::LabPort;

type HostLink = Link<SelectiveRepeat>;

/// What one lab run found, with what the host's link and (on the emulator)
/// the fault injector counted.
pub struct LabOutcome {
    pub report: LabReport,
    pub host: LinkCounters,
    /// Seconds of the run's own clock: wall for a port, emulated for `emu:`.
    pub seconds: f64,
    /// The injector's counters: (device → host, host → device).
    pub faults: Option<(FaultCounters, FaultCounters)>,
    /// Where the numbers came from (`silicon:esp32c6 …`, `lp-emu:esp32c6:t1@…`).
    pub configuration: String,
}

/// A host that stops reading now and then, as a busy page does.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostStall {
    pub every_ms: u64,
    pub for_ms: u64,
}

/// How the in-process emulator run is set up.
pub struct EmuLab<'a> {
    pub elf: &'a Path,
    pub faults: Option<LinkFaults>,
    pub free_lag_ns: u64,
    pub grade: TimeGrade,
    /// Emulated microseconds per slice: the host services the link between
    /// slices, so this bounds the host's reaction time.
    pub slice_us: u64,
    /// Record where the board's instructions went (the emulator's block
    /// census); [`LabPipe::profile`] then names the hottest blocks.
    pub blockprof: bool,
}

/// The pipe under a host link: move what has arrived into the link and say
/// what time it is; take the link's frames out.
pub trait LabPipe {
    fn step(&mut self, link: &mut HostLink) -> Result<Micros>;
    fn send(&mut self, frame: &[u8]) -> Result<()>;
    /// Seconds on the pipe's own clock since it opened.
    fn seconds(&self) -> f64;
    fn configuration(&self) -> String;
    fn faults(&mut self) -> Option<(FaultCounters, FaultCounters)> {
        None
    }
    /// Runs on this pipe reproduce from their seed (the emulator), so the
    /// host's nonce must come from the seed too.
    fn deterministic(&self) -> bool {
        false
    }
    /// The board's hottest code, when the pipe can see it (the emulator's
    /// block census).
    fn profile(&self, _top: usize) -> Option<Vec<String>> {
        None
    }
}

/// A serial device or socket, in wall-clock time.
pub struct PortPipe<'a> {
    port: &'a mut LabPort,
    clock: Instant,
    stall: HostStall,
    next_stall: u64,
    buf: Vec<u8>,
    configuration: String,
}

impl<'a> PortPipe<'a> {
    pub fn new(port: &'a mut LabPort, stall: HostStall, configuration: String) -> Self {
        PortPipe {
            port,
            clock: Instant::now(),
            stall,
            next_stall: stall.every_ms,
            buf: vec![0u8; 16 * 1024],
            configuration,
        }
    }

    fn now(&self) -> Micros {
        self.clock.elapsed().as_micros() as Micros
    }
}

impl LabPipe for PortPipe<'_> {
    fn step(&mut self, link: &mut HostLink) -> Result<Micros> {
        if self.stall.every_ms > 0 && self.now() / 1000 >= self.next_stall {
            std::thread::sleep(Duration::from_millis(self.stall.for_ms));
            self.next_stall += self.stall.every_ms;
        }
        let n = self.port.read(&mut self.buf).context("reading the link")?;
        let t = self.now();
        if n > 0 {
            link.on_bytes(t, &self.buf[..n]);
        }
        Ok(t)
    }

    fn send(&mut self, frame: &[u8]) -> Result<()> {
        self.port.write_all(frame).context("writing the link")
    }

    fn seconds(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    fn configuration(&self) -> String {
        self.configuration.clone()
    }
}

/// A UDP socket to a board's WiFi pipe (one frame per datagram), in
/// wall-clock time.
pub struct UdpPipe {
    sock: std::net::UdpSocket,
    clock: Instant,
    buf: Vec<u8>,
    configuration: String,
}

impl UdpPipe {
    pub fn open(addr: &str, configuration: String) -> Result<Self> {
        let sock = std::net::UdpSocket::bind("0.0.0.0:0").context("binding a UDP socket")?;
        sock.connect(addr)
            .with_context(|| format!("udp connect {addr}"))?;
        sock.set_read_timeout(Some(Duration::from_millis(1)))?;
        Ok(UdpPipe {
            sock,
            clock: Instant::now(),
            buf: vec![0u8; 2048],
            configuration,
        })
    }
}

impl LabPipe for UdpPipe {
    fn step(&mut self, link: &mut HostLink) -> Result<Micros> {
        let mut t = self.clock.elapsed().as_micros() as Micros;
        for _ in 0..64 {
            match self.sock.recv(&mut self.buf) {
                Ok(n) => {
                    t = self.clock.elapsed().as_micros() as Micros;
                    link.on_datagram(t, &self.buf[..n]);
                    // Drain what is queued without waiting again.
                    self.sock.set_nonblocking(true)?;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                // A board not listening yet answers with ICMP unreachable.
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => break,
                Err(e) => return Err(e).context("reading the UDP link"),
            }
        }
        self.sock.set_nonblocking(false)?;
        Ok(t.max(self.clock.elapsed().as_micros() as Micros))
    }

    fn send(&mut self, frame: &[u8]) -> Result<()> {
        match self.sock.send(frame) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => Ok(()),
            Err(e) => Err(e).context("writing the UDP link"),
        }
    }

    fn seconds(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    fn configuration(&self) -> String {
        self.configuration.clone()
    }
}

/// The C6 machine in this process, in emulated time.
pub struct EmuPipe {
    m: Esp32C6Machine,
    queue: QueueHandle,
    slice_us: u64,
    start: Micros,
    grade: TimeGrade,
    wall: Instant,
}

impl EmuPipe {
    pub fn new(emu: &EmuLab<'_>) -> Result<Self> {
        let mut builder = Esp32C6Builder::new()
            .app(AppSource::Path(emu.elf.to_path_buf()))
            .time_grade(emu.grade)
            .reboot_on_reset(true)
            .usb_host(UsbHost::Attached { draining: true })
            .usb_sj_queue_source()
            .usb_in_free_lag_ns(emu.free_lag_ns)
            .blockprof(emu.blockprof);
        if let Some(f) = emu.faults.clone() {
            builder = builder.usb_faults(f);
        }
        let m = builder.build().context("building the emulated C6")?;
        let queue = m
            .usb_sj_host_handle()
            .context("the machine has no in-process USB host queue")?;
        let start = m.micros();
        Ok(EmuPipe {
            m,
            queue,
            slice_us: emu.slice_us.max(10),
            start,
            grade: emu.grade,
            wall: Instant::now(),
        })
    }
}

impl LabPipe for EmuPipe {
    fn step(&mut self, link: &mut HostLink) -> Result<Micros> {
        let stop = StopCondition {
            stop_cycle: Some(
                self.m.cycles() + self.slice_us * lp_emu_esp32c6::memmap::CYCLES_PER_US,
            ),
            ..Default::default()
        };
        match self.m.run_until(&stop) {
            Outcome::Deadline { .. } => {}
            other => bail!("the emulated board stopped: {other:?}"),
        }
        let t = self.m.micros() - self.start;
        let bytes = self.m.take_usb_sj_output();
        if !bytes.is_empty() {
            link.on_bytes(t, &bytes);
        }
        Ok(t)
    }

    fn send(&mut self, frame: &[u8]) -> Result<()> {
        self.queue.push(frame);
        Ok(())
    }

    fn seconds(&self) -> f64 {
        (self.m.micros() - self.start) as f64 / 1e6
    }

    fn configuration(&self) -> String {
        let grade = match self.grade {
            TimeGrade::T1 => "t1",
            TimeGrade::T2 => "t2",
            _ => "t3",
        };
        log::info!(
            "emulated {:.1} s in {:.1} s wall",
            self.seconds(),
            self.wall.elapsed().as_secs_f64()
        );
        format!("lp-emu:esp32c6:{grade}@{}", emu_commit())
    }

    fn faults(&mut self) -> Option<(FaultCounters, FaultCounters)> {
        self.m.usb_fault_counters()
    }

    fn deterministic(&self) -> bool {
        true
    }

    fn profile(&self, top: usize) -> Option<Vec<String>> {
        self.m.blockprof_report(top)
    }
}

/// A host link: its nonce from the seed on a deterministic pipe, else fresh
/// per run.
pub fn host_link(cfg: LinkConfig, seed: u64, deterministic: bool) -> HostLink {
    if deterministic {
        return Link::new(cfg, (seed as u32).wrapping_mul(0x9E37_79B1) | 1);
    }
    let nonce = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(7))
        ^ std::process::id()
        ^ (seed as u32).wrapping_mul(0x9E37_79B1);
    Link::new(cfg, nonce | 1)
}

/// Run the plan to its end over `pipe`.
pub fn run_plan(pipe: &mut dyn LabPipe, plan: LabPlan, cfg: LinkConfig) -> Result<LabOutcome> {
    let mut link = host_link(cfg, plan.seed, pipe.deterministic());
    let limit = total_budget(&plan);
    let t0 = pipe.step(&mut link)?;
    let mut host = LabHost::new(plan, t0);
    while !host.is_finished() {
        let t = pipe.step(&mut link)?;
        if t > t0 + limit {
            bail!("the run overran its {} s budget", limit / 1_000_000);
        }
        while let Some(ev) = link.recv() {
            host.on_event(t, ev);
        }
        host.drive(t, &mut link);
        while let Some(frame) = link.poll_transmit(t) {
            pipe.send(frame)?;
        }
    }
    Ok(LabOutcome {
        report: host.report().clone(),
        host: link.counters().clone(),
        seconds: pipe.seconds(),
        faults: pipe.faults(),
        configuration: pipe.configuration(),
    })
}

/// Run the plan against the C6 machine in this process, in emulated time.
#[allow(
    dead_code,
    reason = "the emulator soak test's entry point (tests/emu_link_lab.rs, through the lib)"
)]
pub fn run_emu(emu: &EmuLab<'_>, plan: LabPlan, cfg: LinkConfig) -> Result<LabOutcome> {
    let mut pipe = EmuPipe::new(emu)?;
    run_plan(&mut pipe, plan, cfg)
}

/// What happened when the board was told to panic.
#[derive(Debug, Default)]
pub struct PanicProbe {
    /// The raw text that arrived outside frames after the command.
    pub text: String,
    pub reset: Option<ResetReason>,
    /// The link came back up with the rebooted board.
    pub up_again: bool,
    pub seconds_to_up: f64,
}

/// Bring the link up, tell the board to panic, and watch for the panic's
/// raw text, the reset, and the link's return.
pub fn panic_probe(pipe: &mut dyn LabPipe, cfg: LinkConfig) -> Result<PanicProbe> {
    let mut link = host_link(cfg, 99, pipe.deterministic());
    let t0 = pipe.step(&mut link)?;
    let mut up = false;
    let mut asked_at: Option<Micros> = None;
    let mut probe = PanicProbe::default();
    loop {
        let t = pipe.step(&mut link)?;
        while let Some(ev) = link.recv() {
            match ev {
                LinkEvent::Up { .. } if asked_at.is_none() => up = true,
                LinkEvent::Up { .. } => {
                    probe.up_again = true;
                    probe.seconds_to_up = (t - asked_at.unwrap_or(t)) as f64 / 1e6;
                }
                LinkEvent::Reset { reason, .. } if asked_at.is_some() => {
                    probe.reset.get_or_insert(reason);
                }
                LinkEvent::Text(bytes) if asked_at.is_some() => {
                    probe.text.push_str(&String::from_utf8_lossy(&bytes));
                }
                _ => {}
            }
        }
        if up && asked_at.is_none() && t > t0 + 500_000 {
            link.send(CH_CONTROL, LabCommand::Panic.to_text().as_bytes())
                .map_err(|e| anyhow::anyhow!("sending panic: {e:?}"))?;
            asked_at = Some(t);
        }
        while let Some(frame) = link.poll_transmit(t) {
            pipe.send(frame)?;
        }
        if probe.up_again || t > t0 + 20_000_000 {
            return Ok(probe);
        }
    }
}

/// The whole run's ceiling: every phase, plus its reply timeouts.
fn total_budget(plan: &LabPlan) -> Micros {
    plan.up_timeout + plan.echo_for + plan.stream_for + 6 * plan.reply_timeout
}

/// The commit the emulator was built from (this checkout's HEAD).
fn emu_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

impl LabOutcome {
    pub fn to_json(&self) -> Value {
        let r = &self.report;
        let board: serde_json::Map<String, Value> =
            r.board.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        let fault = |c: &FaultCounters| {
            json!({
                "packets": c.packets_seen, "dropped": c.packets_dropped,
                "tails": c.tails_cut, "flips": c.bits_flipped, "runs": c.runs_started,
                "bytes_lost": c.bytes_dropped,
            })
        };
        json!({
            "configuration": self.configuration,
            "seconds": self.seconds,
            "hello": r.hello,
            "echo": {
                "msgs": r.echo_msgs, "bytes_each_way": r.echo_bytes, "ms": r.echo_micros / 1000,
                "bps_each_way": r.echo_rate(), "rtt_p50_us": r.rtt_percentile(50),
                "rtt_p99_us": r.rtt_percentile(99), "rtt_max_us": r.rtt_percentile(100),
                "errors": r.echo_errors,
            },
            "stream": {
                "msgs": r.stream_msgs, "board_sent": r.stream_board_sent, "bytes": r.stream_bytes,
                "ms": r.stream_micros / 1000, "bps": r.stream_rate(), "errors": r.stream_errors,
                "gaps": r.stream_gaps,
            },
            "logs": {"asked": r.logs_asked, "lab_rx": r.lab_logs_rx, "other_rx": r.other_logs_rx},
            "text_bytes": r.text_bytes,
            "ups": r.ups, "resets": r.resets, "lost_to_reset": r.lost_to_reset,
            "stall_ms": r.stall_asked_ms,
            "board": board,
            "host_link": counters_json(&self.host),
            "faults": self.faults.as_ref().map(|(a, b)| json!({"to_host": fault(a), "to_device": fault(b)})),
            "problems": r.problems(),
            "failure": r.failure,
        })
    }
}

fn counters_json(c: &LinkCounters) -> Value {
    let mut s = String::new();
    lp_link::lab::counters_kv(c, &mut s);
    let map: serde_json::Map<String, Value> = lp_link::lab::parse_kv(&s)
        .into_iter()
        .map(|(k, v)| (k.trim_start_matches("link.").to_string(), json!(v)))
        .collect();
    Value::Object(map)
}
