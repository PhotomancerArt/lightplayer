//! One simulated run: a host and a board, two pipes, a workload, faults for
//! `duration`, then a quiet tail with no faults and no new sends, in which
//! everything outstanding must arrive. Deterministic from `seed`.

use std::format;
use std::string::String;
use std::vec::Vec;

use crate::sim::checker::Checker;
use crate::sim::endpoint::Endpoint;
use crate::sim::pipe::{Faults, Pipe, PipeModel, PipeStats};
use crate::sim::sim_rng::SimRng;
use crate::sim::transport::Transport;
use crate::sim::workload::{Send, Workload, WorkloadState};
use crate::{Arq, LinkConfig, LinkCounters, LinkState, Micros};

/// A run that takes more steps than this is livelocked.
const MAX_STEPS: u64 = 50_000_000;

#[derive(Clone, Debug)]
pub struct Scenario {
    pub pipe: PipeModel,
    pub host_cfg: LinkConfig,
    pub board_cfg: LinkConfig,
    /// Host → board.
    pub faults_up: Faults,
    /// Board → host.
    pub faults_down: Faults,
    pub workload: Workload,
    pub duration: Micros,
    pub quiet_tail: Micros,
    pub board_reboots: Vec<Micros>,
    pub seed: u64,
}

impl Scenario {
    /// `transport`'s pipe and preset, with its fault shape at rate `p` both
    /// ways.
    pub fn new(
        transport: Transport,
        p: f64,
        workload: Workload,
        duration: Micros,
        seed: u64,
    ) -> Self {
        Scenario {
            pipe: transport.pipe(),
            host_cfg: transport.link_config(),
            board_cfg: transport.link_config(),
            faults_up: transport.faults(p),
            faults_down: transport.faults(p),
            workload,
            duration,
            quiet_tail: 20_000_000,
            board_reboots: Vec::new(),
            seed,
        }
    }

    pub fn with_configs(mut self, cfg: LinkConfig) -> Self {
        self.host_cfg = cfg.clone();
        self.board_cfg = cfg;
        self
    }
}

/// One direction's results.
#[derive(Default, Debug)]
pub struct DirReport {
    pub sent: u64,
    pub delivered: u64,
    pub delivered_bytes: u64,
    pub delivered_bytes_in_window: u64,
    pub latencies: Vec<Micros>,
    pub undetected_damage: u64,
    pub pipe: PipeStats,
}

#[derive(Debug)]
pub struct Report {
    pub arq: &'static str,
    /// Host → board.
    pub up: DirReport,
    /// Board → host.
    pub down: DirReport,
    pub host: LinkCounters,
    pub board: LinkCounters,
    pub host_peak_buffered: usize,
    pub board_peak_buffered: usize,
    pub host_peak_window: usize,
    pub board_peak_window: usize,
    pub board_peak_scratch: usize,
    pub host_resets: u64,
    pub board_resets: u64,
    pub logs_written: u64,
    pub logs_delivered: u64,
    pub logs_reported_dropped: u64,
    pub text_bytes: u64,
    pub duration: Micros,
    /// Broken delivery promises and liveness failures. Empty = correct.
    pub violations: Vec<String>,
}

impl Report {
    /// Board → host payload goodput during the fault window, bytes/s.
    pub fn goodput_down(&self) -> f64 {
        self.down.delivered_bytes_in_window as f64 / (self.duration as f64 / 1e6)
    }

    /// Wire bytes per delivered payload byte, board → host (1.0 = free).
    pub fn wire_per_payload_down(&self) -> f64 {
        self.down.pipe.bytes as f64 / self.down.delivered_bytes.max(1) as f64
    }

    /// Share of reliable frames that were resends, both ways.
    pub fn retransmit_share(&self) -> f64 {
        let r = (self.host.retransmits + self.board.retransmits) as f64;
        let d = (self.host.data_frames_tx + self.board.data_frames_tx) as f64;
        if d == 0.0 { 0.0 } else { r / d }
    }
}

/// Run `sc` with reliability variant `A`.
pub fn run<A: Arq>(sc: &Scenario) -> Report {
    let mut rng = SimRng::new(sc.seed);
    let mut host = Endpoint::<A>::new(sc.host_cfg.clone(), rng.u32());
    let mut board = Endpoint::<A>::new(sc.board_cfg.clone(), rng.u32());
    let mut up = Pipe::new(sc.pipe.clone(), sc.faults_up.clone(), rng.fork());
    let mut down = Pipe::new(sc.pipe.clone(), sc.faults_down.clone(), rng.fork());
    let mut wl_host = WorkloadState::new(&sc.workload, false, rng.fork());
    let mut wl_board = WorkloadState::new(&sc.workload, true, rng.fork());
    let mut check_up = Checker::default();
    let mut check_down = Checker::default();
    let mut reboots = sc.board_reboots.clone();
    reboots.sort_unstable();
    let mut reboot_i = 0;
    let window = sc.duration;
    let end = sc.duration + sc.quiet_tail;
    let mut faults_on = true;
    let mut now: Micros = 0;
    let mut steps = 0u64;
    loop {
        steps += 1;
        assert!(
            steps < MAX_STEPS,
            "livelock: {steps} steps by t={now} (seed {})",
            sc.seed
        );
        if faults_on && now >= window {
            up.faults = Faults::none();
            down.faults = Faults::none();
            faults_on = false;
        }
        while let Some(d) = up.pop_arrival(now) {
            board.feed(now, &d);
        }
        while let Some(d) = down.pop_arrival(now) {
            host.feed(now, &d);
        }
        while reboots.get(reboot_i).is_some_and(|&t| t <= now) {
            board.reboot(rng.u32());
            reboot_i += 1;
        }
        if now < window {
            offer(&mut wl_host, &mut host, now);
            offer(&mut wl_board, &mut board, now);
        }
        host.drain(now, window, &mut check_down);
        board.drain(now, window, &mut check_up);
        host.service(now, &mut up);
        board.service(now, &mut down);
        host.drain(now, window, &mut check_down);
        board.drain(now, window, &mut check_up);

        let mut next = end + 1;
        let mut consider = |t: Option<Micros>, floor: Micros| {
            if let Some(t) = t {
                next = next.min(t.max(floor));
            }
        };
        consider(up.next_arrival(), now + 1);
        consider(down.next_arrival(), now + 1);
        consider(host.wake(&up), now + 1);
        consider(board.wake(&down), now + 1);
        consider(reboots.get(reboot_i).copied(), now + 1);
        if now < window {
            consider(Some(window), now + 1);
            for t in [wl_host.next_at(), wl_board.next_at()]
                .into_iter()
                .flatten()
            {
                if t > now {
                    consider(Some(t), now + 1);
                }
            }
        }
        if next > end {
            break;
        }
        now = next;
    }

    let mut violations = check_up.violations.clone();
    violations.extend(check_down.violations.iter().cloned());
    for (name, st) in [("host", host.link.state()), ("board", board.link.state())] {
        if st != LinkState::Established {
            violations.push(format!(
                "liveness: the {name} link is not up after the quiet tail"
            ));
        }
    }
    if A::RELIABLE {
        liveness(&host, &check_up, "host→board", &mut violations);
        liveness(&board, &check_down, "board→host", &mut violations);
    }

    let dir = |e: &Endpoint<A>, c: &Checker, pipe: &Pipe| DirReport {
        sent: e.sent_count,
        delivered: c.delivered,
        delivered_bytes: c.delivered_bytes,
        delivered_bytes_in_window: c.delivered_bytes_in_window,
        latencies: c.latencies.clone(),
        undetected_damage: c.undetected_damage,
        pipe: pipe.stats.clone(),
    };
    Report {
        arq: A::NAME,
        up: dir(&host, &check_up, &up),
        down: dir(&board, &check_down, &down),
        host: host.total_counters(),
        board: board.total_counters(),
        host_peak_buffered: host.peak_buffered,
        board_peak_buffered: board.peak_buffered,
        host_peak_window: host.peak_window,
        board_peak_window: board.peak_window,
        board_peak_scratch: board.peak_scratch,
        host_resets: host.resets,
        board_resets: board.resets,
        logs_written: board.logs_written,
        logs_delivered: check_down.logs,
        logs_reported_dropped: check_down.logs_reported_dropped,
        text_bytes: host.text_bytes + board.text_bytes,
        duration: sc.duration,
        violations,
    }
}

/// Offer everything the workload has due; stop at the first refusal.
fn offer<A: Arq>(wl: &mut WorkloadState, ep: &mut Endpoint<A>, now: Micros) {
    for _ in 0..1_000 {
        match wl.due(now) {
            None => return,
            Some(Send::Log) => ep.log(now),
            Some(Send::Message { channel, size }) => {
                if !ep.send_probe(now, channel, size) {
                    return;
                }
                wl.accepted(now);
            }
        }
    }
}

/// The sender's last generation must have been delivered in full.
fn liveness<A: Arq>(sender: &Endpoint<A>, checker: &Checker, dir: &str, v: &mut Vec<String>) {
    let key = (sender.inc, sender.link.generation());
    let sent = sender.sent.get(&key).copied().unwrap_or(0);
    let got = checker.delivered_of(key.0, key.1);
    if got != sent {
        v.push(format!(
            "liveness {dir}: generation {key:?} sent {sent}, delivered {got} after the quiet tail"
        ));
    }
    if !sender.link.is_idle() {
        v.push(format!(
            "liveness {dir}: the sender still holds unacknowledged data"
        ));
    }
}
