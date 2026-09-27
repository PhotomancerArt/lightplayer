//! `link-bench`: the M2 comparison tables, as Markdown on stdout.
//!
//! ```text
//! cargo run -p lp-link --features sim --release --bin link-bench -- [compare|sweep|crc|logs|all]
//! ```
//!
//! Every number is simulated (the `sim` pipes in `lp_link::sim::transport`),
//! deterministic from the seeds below, and says nothing about silicon until
//! M3 repeats it on target.

use lp_link::sim::{Faults, Report, Scenario, Transport, Workload, run};
use lp_link::{Arq, CrcKind, Framing, GoBackN, LinkConfig, NoArq, SelectiveRepeat, StopAndWait};

type Gbn = GoBackN<127>;

const SEEDS: [u64; 3] = [11, 22, 33];
const RATES: [f64; 4] = [0.0, 0.001, 0.01, 0.05];

fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let all = what == "all";
    if all || what == "compare" {
        compare();
    }
    if all || what == "sweep" {
        sweep();
    }
    if all || what == "crc" {
        crc();
    }
    if all || what == "logs" {
        logs();
    }
}

fn bulk() -> Workload {
    Workload::Bulk { size: 1024 }
}

fn interactive() -> Workload {
    Workload::Interactive {
        interval: 100_000,
        up: 120,
        down: 1_500,
        log_every: 20_000,
    }
}

/// Results pooled over seeds for one cell.
struct Cell {
    goodput: Vec<f64>,
    overhead: Vec<f64>,
    retx: Vec<f64>,
    lat: Vec<u64>,
    delivered: u64,
    sent: u64,
    resets: u64,
    window_peak: usize,
    violations: usize,
}

fn cell<A: Arq>(t: Transport, p: f64, cfg: Option<&LinkConfig>) -> Cell {
    let mut c = Cell {
        goodput: vec![],
        overhead: vec![],
        retx: vec![],
        lat: vec![],
        delivered: 0,
        sent: 0,
        resets: 0,
        window_peak: 0,
        violations: 0,
    };
    for seed in SEEDS {
        let mk = |w: Workload, dur: u64| {
            let mut sc = Scenario::new(t, p, w, dur, seed);
            sc.quiet_tail = 15_000_000;
            if let Some(cfg) = cfg {
                sc = sc.with_configs(cfg.clone());
            }
            sc
        };
        let b: Report = run::<A>(&mk(bulk(), 10_000_000));
        c.goodput.push(b.goodput_down());
        c.overhead.push(b.wire_per_payload_down());
        c.retx.push(b.retransmit_share());
        c.window_peak = c.window_peak.max(b.board_peak_window);
        c.resets += b.host_resets + b.board_resets;
        c.violations += b.violations.len();
        let i: Report = run::<A>(&mk(interactive(), 20_000_000));
        c.lat.extend(i.down.latencies.iter().copied());
        c.delivered += i.down.delivered + i.up.delivered;
        c.sent += i.down.sent + i.up.sent;
        c.resets += i.host_resets + i.board_resets;
        c.violations += if A::RELIABLE { i.violations.len() } else { 0 };
    }
    c
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn pct(v: &mut [u64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_unstable();
    v[((v.len() - 1) as f64 * q).round() as usize] as f64 / 1000.0
}

fn row(name: &str, t: Transport, p: f64, mut c: Cell) {
    println!(
        "| {} | {} | {} | {:.1} | {:.2} | {:.1} | {:.1} | {:.1} | {:.2} | {} | {} | {} |",
        t.name(),
        name,
        rate(p),
        median(&mut c.goodput) / 1024.0,
        median(&mut c.overhead),
        pct(&mut c.lat, 0.5),
        pct(&mut c.lat, 0.99),
        median(&mut c.retx) * 100.0,
        100.0 * c.delivered as f64 / c.sent.max(1) as f64,
        c.resets,
        c.window_peak,
        if c.violations == 0 {
            "ok".to_string()
        } else {
            format!("**{}**", c.violations)
        },
    );
}

fn rate(p: f64) -> String {
    if p == 0.0 {
        "0".into()
    } else {
        format!("{}%", p * 100.0)
    }
}

fn header(title: &str) {
    println!("\n### {title}\n");
    println!(
        "| transport | variant | fault rate | goodput KB/s | wire/payload | p50 ms | p99 ms | resent % | delivered % | resets | board window B | promise |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
}

fn compare() {
    header(
        "Variants × transports × fault rate (3 seeds; goodput = median of 10 s bulk board→host; latency = 1.5 KB replies every 100 ms, pooled)",
    );
    for t in [
        Transport::Usb,
        Transport::Ble,
        Transport::BleStream,
        Transport::Udp,
    ] {
        for p in RATES {
            row("none", t, p, cell::<NoArq>(t, p, None));
            row("stop-and-wait", t, p, cell::<StopAndWait>(t, p, None));
            row("go-back-N", t, p, cell::<Gbn>(t, p, None));
            row(
                "selective-repeat",
                t,
                p,
                cell::<SelectiveRepeat>(t, p, None),
            );
        }
    }
    row(
        "none",
        Transport::Ws,
        0.0,
        cell::<NoArq>(Transport::Ws, 0.0, None),
    );
    row(
        "selective-repeat",
        Transport::Ws,
        0.0,
        cell::<SelectiveRepeat>(Transport::Ws, 0.0, None),
    );
}

fn sweep() {
    header("USB: frame payload × window, at 1% faults");
    for payload in [128u16, 256, 512, 1024] {
        for window in [2u8, 4, 8, 16] {
            let cfg = LinkConfig {
                max_payload: payload,
                tx_window: window,
                rx_window: window,
                ..LinkConfig::usb()
            };
            let name = format!("SR p{payload} w{window}");
            row(
                &name,
                Transport::Usb,
                0.01,
                cell::<SelectiveRepeat>(Transport::Usb, 0.01, Some(&cfg)),
            );
            let name = format!("GBN p{payload} w{window}");
            row(
                &name,
                Transport::Usb,
                0.01,
                cell::<Gbn>(Transport::Usb, 0.01, Some(&cfg)),
            );
        }
    }
    header("BLE (one frame per notification): window, at 1% faults");
    for window in [2u8, 4, 8, 16] {
        let cfg = LinkConfig {
            tx_window: window,
            rx_window: window,
            ..LinkConfig::ble()
        };
        row(
            &format!("SR w{window}"),
            Transport::Ble,
            0.01,
            cell::<SelectiveRepeat>(Transport::Ble, 0.01, Some(&cfg)),
        );
        row(
            &format!("GBN w{window}"),
            Transport::Ble,
            0.01,
            cell::<Gbn>(Transport::Ble, 0.01, Some(&cfg)),
        );
    }
    header("BLE as a byte stream: frame payload × window, at 1% faults");
    for payload in [236u16, 480, 960] {
        for window in [4u8, 8] {
            let cfg = LinkConfig {
                framing: Framing::Stream,
                max_payload: payload,
                tx_window: window,
                rx_window: window,
                ..LinkConfig::ble()
            };
            let name = format!("SR p{payload} w{window}");
            row(
                &name,
                Transport::BleStream,
                0.01,
                cell::<SelectiveRepeat>(Transport::BleStream, 0.01, Some(&cfg)),
            );
        }
    }
}

fn crc() {
    println!(
        "\n### Checksum: damage that got through (USB, bulk 20 s, 5 seeds, 30% of packets with a flipped bit, 10% with lost bytes)\n"
    );
    println!("| checksum | bytes per frame | frames rejected | damaged messages delivered |");
    println!("|---|---|---|---|");
    for crc in [CrcKind::Crc16, CrcKind::Crc32c] {
        let (mut rejected, mut missed) = (0u64, 0u64);
        for seed in 0..5 {
            let cfg = LinkConfig {
                crc,
                ..LinkConfig::usb()
            };
            let mut sc =
                Scenario::new(Transport::Usb, 0.0, bulk(), 20_000_000, seed).with_configs(cfg);
            sc.faults_down = Faults {
                corrupt: 0.3,
                drop_span: 0.1,
                ..Faults::none()
            };
            let r = run::<SelectiveRepeat>(&sc);
            rejected += r.host.bad_frames as u64;
            missed += r.down.undetected_damage;
        }
        println!("| {crc:?} | {} | {rejected} | {missed} |", crc.len());
    }
}

fn logs() {
    println!(
        "\n### Logs: best effort, with the board rebooting twice (a log line every 20 ms, 20 s)\n"
    );
    println!(
        "| transport | fault rate | lines written | delivered | reported dropped | unaccounted |"
    );
    println!("|---|---|---|---|---|---|");
    for t in [Transport::Usb, Transport::Ble] {
        for p in [0.0, 0.01, 0.05] {
            let mut sc = Scenario::new(t, p, interactive(), 20_000_000, 7);
            sc.board_reboots = vec![5_000_000, 12_000_000];
            let r = run::<SelectiveRepeat>(&sc);
            let unaccounted =
                r.logs_written as i64 - r.logs_delivered as i64 - r.logs_reported_dropped as i64;
            println!(
                "| {} | {} | {} | {} | {} | {} |",
                t.name(),
                rate(p),
                r.logs_written,
                r.logs_delivered,
                r.logs_reported_dropped,
                unaccounted
            );
        }
    }
}
