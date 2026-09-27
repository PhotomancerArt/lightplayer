//! The host edge of the comms lab: one [`Link`] and one [`LabHost`] driven
//! over a pipe, either in wall-clock time (a serial device, an emulated
//! board's socket) or in the emulator's own time (the C6 machine in this
//! process, stepped slice by slice — deterministic, and the only honest clock
//! for an emulated number).

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lp_emu_esp_common::link_faults::{FaultCounters, LinkFaults};
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Outcome, StopCondition, TimeGrade, UsbHost,
};
use lp_link::lab::{LabHost, LabPlan, LabReport};
use lp_link::{Link, LinkConfig, LinkCounters, Micros, SelectiveRepeat};
use serde_json::{Value, json};

use super::lab_port::LabPort;

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

/// Run the plan over `port` in wall-clock time.
pub fn run_port(
    port: &mut LabPort,
    plan: LabPlan,
    cfg: LinkConfig,
    stall: HostStall,
    configuration: String,
) -> Result<LabOutcome> {
    let clock = Instant::now();
    let now = || clock.elapsed().as_micros() as Micros;
    let nonce = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(7))
        ^ std::process::id();
    let mut link = Link::<SelectiveRepeat>::new(cfg, nonce);
    let limit = total_budget(&plan);
    let mut host = LabHost::new(plan, now());
    let mut buf = vec![0u8; 16 * 1024];
    let mut next_stall = stall.every_ms;
    while !host.is_finished() {
        let t = now();
        if t > limit {
            bail!("the run overran its {} s budget", limit / 1_000_000);
        }
        if stall.every_ms > 0 && t / 1000 >= next_stall {
            std::thread::sleep(Duration::from_millis(stall.for_ms));
            next_stall += stall.every_ms;
        }
        let n = port.read(&mut buf).context("reading the link")?;
        let t = now();
        if n > 0 {
            link.on_bytes(t, &buf[..n]);
        }
        while let Some(ev) = link.recv() {
            host.on_event(t, ev);
        }
        host.drive(t, &mut link);
        while let Some(frame) = link.poll_transmit(t) {
            port.write_all(frame).context("writing the link")?;
        }
    }
    Ok(LabOutcome {
        report: host.report().clone(),
        host: link.counters().clone(),
        seconds: clock.elapsed().as_secs_f64(),
        faults: None,
        configuration,
    })
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
}

/// Run the plan against the C6 machine in this process, in emulated time.
pub fn run_emu(emu: &EmuLab<'_>, plan: LabPlan, cfg: LinkConfig) -> Result<LabOutcome> {
    let mut builder = Esp32C6Builder::new()
        .app(AppSource::Path(emu.elf.to_path_buf()))
        .time_grade(emu.grade)
        .reboot_on_reset(true)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .usb_in_free_lag_ns(emu.free_lag_ns);
    if let Some(f) = emu.faults.clone() {
        builder = builder.usb_faults(f);
    }
    let mut m = builder.build().context("building the emulated C6")?;
    let queue = m
        .usb_sj_host_handle()
        .context("the machine has no in-process USB host queue")?;
    let nonce = (plan.seed as u32).wrapping_mul(0x9E37_79B1) | 1;
    let mut link = Link::<SelectiveRepeat>::new(cfg, nonce);
    let limit = total_budget(&plan);
    let start = m.micros();
    let mut host = LabHost::new(plan, 0);
    let wall = Instant::now();
    while !host.is_finished() {
        let stop = StopCondition {
            stop_cycle: Some(m.cycles() + emu.slice_us * lp_emu_esp32c6::memmap::CYCLES_PER_US),
            ..Default::default()
        };
        match m.run_until(&stop) {
            Outcome::Deadline { .. } => {}
            other => bail!("the emulated board stopped: {other:?}"),
        }
        let t = m.micros() - start;
        if t > limit {
            bail!(
                "the run overran its {} s emulated budget",
                limit / 1_000_000
            );
        }
        let bytes = m.take_usb_sj_output();
        if !bytes.is_empty() {
            link.on_bytes(t, &bytes);
        }
        while let Some(ev) = link.recv() {
            host.on_event(t, ev);
        }
        host.drive(t, &mut link);
        while let Some(frame) = link.poll_transmit(t) {
            queue.push(frame);
        }
    }
    let seconds = (m.micros() - start) as f64 / 1e6;
    let faults = m.usb_fault_counters();
    let grade = match emu.grade {
        TimeGrade::T1 => "t1",
        TimeGrade::T2 => "t2",
        _ => "t3",
    };
    log::info!(
        "emulated {seconds:.1} s in {:.1} s wall",
        wall.elapsed().as_secs_f64()
    );
    Ok(LabOutcome {
        report: host.report().clone(),
        host: link.counters().clone(),
        seconds,
        faults,
        configuration: format!("lp-emu:esp32c6:{grade}@{}", emu_commit()),
    })
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
