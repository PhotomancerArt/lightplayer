//! The comms lab's two halves, [`LabHost`] and [`LabBoard`], against each
//! other over the simulator's faulty pipes: the same code the firmware, lp-cli
//! and the emulator test run, proved here before any of them.

use lp_link::lab::{BoardAction, LAB_LOG_MARK, LabBoard, LabHost, LabPlan, LabReport};
use lp_link::log_ring::{LEVEL_INFO, LogRing};
use lp_link::sim::Transport;
use lp_link::sim::pipe::Pipe;
use lp_link::sim::sim_rng::SimRng;
use lp_link::{CH_LOG, Framing, Link, LinkConfig, Micros, SelectiveRepeat};

#[test]
fn usb_clean_passes() {
    let r = run(Transport::Usb, 0.0, 0, 1);
    assert!(r.problems().is_empty(), "{r}");
    assert_eq!(r.lab_logs_rx, 50, "{r}");
}

#[test]
fn usb_with_one_percent_faults_loses_nothing() {
    for seed in 1..4 {
        let r = run(Transport::Usb, 0.01, 0, seed);
        assert!(r.problems().is_empty(), "seed {seed}: {r}");
    }
}

#[test]
fn usb_with_five_percent_faults_and_a_stall_loses_nothing() {
    let r = run(Transport::Usb, 0.05, 1500, 9);
    assert!(r.problems().is_empty(), "{r}");
    assert_eq!(r.stall_asked_ms, 1500);
}

#[test]
fn ble_with_one_percent_faults_loses_nothing() {
    let r = run(Transport::Ble, 0.01, 0, 3);
    assert!(r.problems().is_empty(), "{r}");
}

/// One lab run: host and board links, a pipe each way, stepped in simulated
/// time. `stall_ms` stalls the board mid-echo, as a shader compile would.
fn run(t: Transport, fault_rate: f64, stall_ms: u32, seed: u64) -> LabReport {
    let cfg: LinkConfig = t.link_config();
    let mut rng = SimRng::new(seed);
    let mut up = Pipe::new(t.pipe(), t.faults(fault_rate), rng.fork());
    let mut down = Pipe::new(t.pipe(), t.faults(fault_rate), rng.fork());
    let mut host_link = Link::<SelectiveRepeat>::new(cfg.clone(), rng.u32());
    let mut board_link = Link::<SelectiveRepeat>::new(cfg.clone(), rng.u32());
    let plan = LabPlan {
        echo_for: 3_000_000,
        stream_for: 3_000_000,
        logs: 50,
        stall_ms,
        seed,
        ..LabPlan::default()
    };
    let mut host = LabHost::new(plan, 0);
    let mut board = LabBoard::new("sim");
    let mut ring: LogRing<2048> = LogRing::new();
    let mut board_busy_until: Micros = 0;
    let mut logs_to_write: Option<(u32, u32, usize)> = None;

    let feed = |link: &mut Link<SelectiveRepeat>, now: Micros, data: &[u8]| match cfg.framing {
        Framing::Stream => link.on_bytes(now, data),
        Framing::Datagram => link.on_datagram(now, data),
    };

    let mut now: Micros = 0;
    while !host.is_finished() && now < 120_000_000 {
        while let Some(d) = down.pop_arrival(now) {
            feed(&mut host_link, now, &d);
        }
        while let Some(ev) = host_link.recv() {
            host.on_event(now, ev);
        }
        host.drive(now, &mut host_link);
        while up.can_accept(now) {
            match host_link.poll_transmit(now) {
                Some(f) => up.send(now, f),
                None => break,
            }
        }

        if now >= board_busy_until {
            while let Some(d) = up.pop_arrival(now) {
                feed(&mut board_link, now, &d);
            }
            while board.ready_for_event() {
                let Some(ev) = board_link.recv() else { break };
                match board.on_event(ev) {
                    Some(BoardAction::Stall { ms }) => {
                        board_busy_until = now + u64::from(ms) * 1000;
                    }
                    Some(BoardAction::Log { n, len }) => logs_to_write = Some((0, n, len)),
                    Some(BoardAction::Panic) => panic!("the plan never asks for a panic"),
                    None => {}
                }
            }
            // A few log lines per step, the way a busy board would write them.
            if let Some((i, n, len)) = logs_to_write.as_mut() {
                for _ in 0..4 {
                    if *i >= *n {
                        break;
                    }
                    *i += 1;
                    let pad = "x".repeat(len.saturating_sub(20));
                    ring.push(
                        LEVEL_INFO,
                        format!("{LAB_LOG_MARK}{}/{} {pad}", *i, *n).as_bytes(),
                    );
                    board.note_logs(1);
                }
                if *i >= *n {
                    logs_to_write = None;
                }
            }
            board.pump(&mut board_link, "");
            board_link.pump_log(now, &mut ring, CH_LOG);
            while down.can_accept(now) {
                match board_link.poll_transmit(now) {
                    Some(f) => down.send(now, f),
                    None => break,
                }
            }
        }
        now += 200;
    }
    eprintln!(
        "{} at {:.1}% (seed {seed}, stall {stall_ms} ms):\n{}\nhost {:?}",
        t.name(),
        fault_rate * 100.0,
        host.report(),
        host_link.counters()
    );
    host.report().clone()
}
