//! `lp-cli link lab`: the comms lab's host half against a real, socketed or
//! in-process emulated `test_comms_lab` board.

use anyhow::{Context, Result, bail};
use lp_emu_esp_common::link_faults::LinkFaults;
use lp_emu_esp32c6::machine::TimeGrade;
use lp_link::LinkConfig;
use lp_link::lab::LabPlan;

use super::args::LabArgs;
use super::lab_port::{LabPort, TermiosMode};
use super::lab_run::{
    EmuLab, EmuPipe, HostStall, LabOutcome, LabPipe, PortPipe, UdpPipe, panic_probe, run_plan,
};

pub fn lab(args: &LabArgs) -> Result<()> {
    let plan = plan_of(args);
    let mut port_holder: Option<LabPort> = None;
    let mut pipe: Box<dyn LabPipe + '_> = if let Some(elf) = args.target.strip_prefix("emu:") {
        let faults = match &args.faults {
            Some(spec) => Some(LinkFaults::parse(spec).map_err(anyhow::Error::msg)?),
            None => None,
        };
        let grade = match args.grade.as_str() {
            "t1" => TimeGrade::T1,
            "t2" => TimeGrade::T2,
            g => bail!("--grade must be t1 or t2, not {g}"),
        };
        Box::new(EmuPipe::new(&EmuLab {
            elf: std::path::Path::new(elf),
            faults,
            free_lag_ns: args.free_lag_ns,
            grade,
            slice_us: args.slice_us.max(10),
            blockprof: args.blockprof,
        })?)
    } else if let Some(addr) = args.target.strip_prefix("udp://") {
        let label = if args.label.is_empty() {
            args.target.clone()
        } else {
            args.label.clone()
        };
        Box::new(UdpPipe::open(
            addr,
            format!("{label}, reader: UDP over WiFi"),
        )?)
    } else {
        if args.faults.is_some() || args.free_lag_ns > 0 {
            bail!("--faults and --free-lag-ns act on the emulator's link: use an emu: target");
        }
        let reader = match args.termios {
            TermiosMode::Raw => "native raw termios",
            TermiosMode::Chrome => "native Chromium termios (PARMRK, 0xFF fold)",
        };
        let label = if args.label.is_empty() {
            args.target.clone()
        } else {
            args.label.clone()
        };
        let stall = HostStall {
            every_ms: args.host_stall_every_ms,
            for_ms: args.host_stall_ms,
        };
        let stalls = if stall.every_ms > 0 {
            format!(
                ", host stalls {} ms every {} ms",
                stall.for_ms, stall.every_ms
            )
        } else {
            String::new()
        };
        let port = port_holder.insert(LabPort::open(&args.target, args.termios)?);
        Box::new(PortPipe::new(
            port,
            stall,
            format!("{label}, reader: {reader}{stalls}"),
        ))
    };
    if args.panic_test {
        let p = panic_probe(pipe.as_mut(), host_config(args))?;
        println!("configuration: {}", pipe.configuration());
        println!(
            "panic probe: reset {:?}, link up again {} ({:.2} s after the command)",
            p.reset, p.up_again, p.seconds_to_up
        );
        println!("raw text after the command ({} B):", p.text.len());
        for l in p.text.lines().filter(|l| !l.trim().is_empty()).take(12) {
            println!("  | {l}");
        }
        if p.reset.is_none() || !p.up_again || !p.text.to_lowercase().contains("panic") {
            bail!("the panic did not arrive as text followed by a reset and a new session");
        }
        return Ok(());
    }
    let outcome = run_plan(pipe.as_mut(), plan, host_config(args))?;
    print_outcome(&outcome);
    if let Some(lines) = pipe.profile(40) {
        for l in lines {
            println!("{l}");
        }
    }
    if let Some(path) = &args.json {
        std::fs::write(path, serde_json::to_string_pretty(&outcome.to_json())?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    if !outcome.report.problems().is_empty() {
        bail!("the lab found problems (above)");
    }
    Ok(())
}

fn plan_of(args: &LabArgs) -> LabPlan {
    let us = |s: f64| (s * 1e6) as u64;
    LabPlan {
        echo_for: us(args.echo_secs),
        stream_for: us(args.stream_secs),
        min: args.min,
        max: args.max,
        in_flight: args.in_flight.max(1),
        seed: args.seed,
        logs: args.logs,
        log_len: args.log_len,
        stall_ms: args.stall_ms,
        ..LabPlan::default()
    }
}

/// The host link's settings: the transport's preset (UDP for `udp://`, else
/// USB), with any tuning override.
fn host_config(args: &LabArgs) -> LinkConfig {
    let mut cfg = if args.target.starts_with("udp://") {
        LinkConfig::udp()
    } else {
        LinkConfig::usb()
    };
    cfg.escape_ff = !args.plain_cobs;
    if let Some(ms) = args.min_rto_ms {
        cfg.min_rto = ms * 1000;
        cfg.initial_rto = cfg.initial_rto.max(cfg.min_rto);
    }
    cfg
}

fn print_outcome(o: &LabOutcome) {
    println!("configuration: {} ({:.1} s)", o.configuration, o.seconds);
    print!("{}", o.report);
    let h = &o.host;
    println!(
        "host link: {} frames out, {} in; {} resent ({} timer, {} early, {} probes); \
         {} damaged frames dropped, {} duplicates, {} out of order, {} stale partials; \
         {} log datagrams lost",
        h.frames_tx,
        h.frames_rx,
        h.retransmits,
        h.timeouts,
        h.fast_retransmits,
        h.probes,
        h.bad_frames,
        h.duplicates,
        h.out_of_order,
        h.stale_partials,
        h.datagrams_lost
    );
    if let Some((to_host, to_device)) = &o.faults {
        println!("injected: device→host {to_host}; host→device {to_device}");
    }
    if !o.report.text_head.is_empty() {
        let text = String::from_utf8_lossy(&o.report.text_head);
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        println!("raw text ({} B), first lines:", o.report.text_bytes);
        for l in lines.iter().take(6) {
            println!("  | {l}");
        }
    }
    if !o.report.log_tail.is_empty() {
        println!("last log lines:");
        for l in o.report.log_tail.iter().rev().take(4).rev() {
            let cut: String = l.chars().take(110).collect();
            println!("  | {cut}");
        }
    }
}
