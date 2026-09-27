//! `link-bench`: the M2 comparison tables, as Markdown on stdout.
//!
//! ```text
//! cargo run -p lp-link --features sim --release --bin link-bench -- [compare|sweep|crc|logs|all]
//! ```
//!
//! Every number is simulated (the `sim` pipes in `lp_link::sim::transport`),
//! deterministic from the seeds below, and says nothing about silicon until
//! M3 repeats it on target.

use lp_link::frame;
use lp_link::sim::sim_rng::SimRng;
use lp_link::sim::{Report, Scenario, Transport, Workload, run};
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
    if all || what == "ram" {
        ram();
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

/// How often damage gets past the receiver, measured directly: frames built
/// exactly as the link builds them (header, 256 random payload bytes,
/// keyed checksum, COBS, delimiters), damaged the ways our pipes damage
/// them, then put through the receiver's own checks (COBS decode, header,
/// checksum). "none" is the same with no checksum: what the wire has today.
/// A frame counts as passed only if what it decodes to differs from both
/// originals (so real damage, not a harmless cut).
fn crc() {
    let trials: u64 = std::env::var("CRC_TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4_000_000);
    println!(
        "\n### Checksum: damaged frames that passed every receiver check ({trials} damaged frames per row)\n"
    );
    println!(
        "| checksum | bytes/frame | torn (tail lost, next frame spliced on) | bytes lost mid-frame | 1–3 bits flipped | all |"
    );
    println!("|---|---|---|---|---|---|");
    for crc in [None, Some(CrcKind::Crc16), Some(CrcKind::Crc32c)] {
        let mut rng = SimRng::new(0xC0FFEE);
        let mut passed = [0u64; 3];
        let mut tried = [0u64; 3];
        let (mut raw, mut a, mut b, mut dec) = (vec![], vec![], vec![], vec![]);
        for i in 0..trials {
            let key = rng.u32();
            let kind = (i % 3) as usize;
            make_frame(&mut rng, crc, key, &mut raw, &mut a);
            let a_raw = raw.clone();
            make_frame(&mut rng, crc, key, &mut raw, &mut b);
            let b_raw = raw.clone();
            // `a` and `b` are `00 body 00`; work on the bodies.
            let a_body = &a[1..a.len() - 1];
            let b_body = &b[1..b.len() - 1];
            let body: Vec<u8> = match kind {
                0 => {
                    let cut = rng.below(a_body.len() as u64 - 1) as usize + 1;
                    let from = rng.below(b_body.len() as u64 - 1) as usize;
                    [&a_body[..cut], &b_body[from..]].concat()
                }
                1 => {
                    let len = 1 + rng.below(64) as usize;
                    let at = rng.below((a_body.len() - len.min(a_body.len() - 1)) as u64) as usize;
                    let mut v = a_body.to_vec();
                    v.drain(at..(at + len).min(v.len() - 1));
                    v
                }
                _ => {
                    let mut v = a_body.to_vec();
                    for _ in 0..1 + rng.below(3) {
                        let bit = rng.below(v.len() as u64 * 8);
                        v[(bit / 8) as usize] ^= 1 << (bit % 8);
                    }
                    if v.contains(&0) {
                        // A flip to 0x00 splits the frame at the deframer:
                        // the first half is what gets checked.
                        let z = v.iter().position(|&x| x == 0).unwrap_or(v.len());
                        v.truncate(z);
                    }
                    v
                }
            };
            tried[kind] += 1;
            dec.clear();
            if body.is_empty() || frame::unwrap_stream(&body, &mut dec).is_err() {
                continue;
            }
            let ok = match crc {
                None => frame::Header::parse(&dec).is_some(),
                Some(c) => {
                    frame::Header::parse(&dec).is_some() && frame::verify(c, key, &dec).is_some()
                }
            };
            // Damage that decodes back to an intact frame (a redundant
            // trailing COBS code byte cut off) is not an error.
            if ok && dec != a_raw && dec != b_raw {
                passed[kind] += 1;
            }
        }
        let name = crc.map_or("none".to_string(), |c| format!("{c:?}"));
        let cell = |k: usize| per_million(passed[k], tried[k]);
        let all = per_million(passed.iter().sum(), tried.iter().sum());
        println!(
            "| {name} | {} | {} | {} | {} | {all} |",
            crc.map_or(0, |c| c.len()),
            cell(0),
            cell(1),
            cell(2)
        );
    }
}

fn per_million(n: u64, of: u64) -> String {
    if n == 0 {
        format!("0 of {of}")
    } else {
        format!("{n} ({:.1} per million)", n as f64 * 1e6 / of as f64)
    }
}

fn make_frame(
    rng: &mut SimRng,
    crc: Option<CrcKind>,
    key: u32,
    raw: &mut Vec<u8>,
    out: &mut Vec<u8>,
) {
    let payload: Vec<u8> = (0..256).map(|_| rng.next_u64() as u8).collect();
    let hdr = frame::Header {
        kind: frame::FrameKind::Data,
        fin: rng.chance(0.5),
        first: rng.chance(0.5),
        chan: (rng.below(3)) as u8,
        seq: rng.next_u64() as u8,
        ack: rng.next_u64() as u8,
        win: 8,
    };
    match crc {
        Some(c) => frame::encode(c, key, &hdr, &payload, raw, out),
        None => {
            raw.clear();
            raw.extend_from_slice(&hdr.to_bytes());
            raw.extend_from_slice(&payload);
            frame::wrap_stream(raw, out);
        }
    }
}

/// Logs are best effort, but every line is accounted for: delivered, lost on
/// the wire (the host sees the datagram sequence gap), dropped by the board's
/// ring while the link was down (the ring says so in its next record), or
/// lost with the ring at a reboot.
fn logs() {
    println!(
        "\n### Logs: a line every 20 ms for 20 s; the board reboots at 5 s and 12 s; the cable is out 14–17 s\n"
    );
    println!(
        "| transport | fault rate | written | delivered | lost on the wire (seen) | dropped by the ring (reported) | lost at reboot | unaccounted |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for t in [Transport::Usb, Transport::Ble] {
        for p in [0.0, 0.01, 0.05] {
            let mut sc = Scenario::new(t, p, interactive(), 20_000_000, 7);
            sc.board_reboots = vec![5_000_000, 12_000_000];
            sc.outage = Some((14_000_000, 17_000_000));
            let r = run::<SelectiveRepeat>(&sc);
            let unaccounted = r.logs_written as i64
                - r.logs_delivered as i64
                - r.logs_seen_lost as i64
                - r.logs_reported_dropped as i64
                - r.logs_lost_in_ring as i64;
            println!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                t.name(),
                rate(p),
                r.logs_written,
                r.logs_delivered,
                r.logs_seen_lost,
                r.logs_reported_dropped,
                r.logs_lost_in_ring,
                unaccounted
            );
        }
    }
}

/// What one link holds on the board, measured: the peak of the reliability
/// buffers (sent-unacknowledged plus held-out-of-order payload), the frame
/// scratch buffers, and everything queued (which includes the application's
/// own send queue, bounded by `send_budget`).
fn ram() {
    println!(
        "\n### RAM per link on the board (peaks over bulk + interactive runs at 1% faults, 3 seeds)\n"
    );
    println!(
        "| transport | variant | reliability buffers B | frame scratch B | all queued B (incl. app send queue) |"
    );
    println!("|---|---|---|---|---|");
    for t in [Transport::Usb, Transport::Ble, Transport::Udp] {
        ram_row::<SelectiveRepeat>(t);
        ram_row::<Gbn>(t);
    }
}

fn ram_row<A: Arq>(t: Transport) {
    let (mut window, mut scratch, mut queued) = (0, 0, 0);
    for seed in SEEDS {
        for (w, d) in [(bulk(), 10_000_000), (interactive(), 20_000_000)] {
            let r = run::<A>(&Scenario::new(t, 0.01, w, d, seed));
            window = window.max(r.board_peak_window);
            scratch = scratch.max(r.board_peak_scratch);
            queued = queued.max(r.board_peak_buffered);
        }
    }
    println!(
        "| {} | {} | {window} | {scratch} | {queued} |",
        t.name(),
        A::NAME
    );
}
