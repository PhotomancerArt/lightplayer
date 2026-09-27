//! `LabSession`: one [`Link`] and one [`LabHost`] for a browser page. The
//! page owns the pipe (a Web Serial port, a BLE characteristic) and the
//! clock; it feeds what arrives, calls `service`, and writes every frame
//! `take_frame` hands back, exactly as `lp-cli link lab` does natively.

use std::collections::VecDeque;
use std::fmt::Write as _;

use lp_link::lab::{LabHost, LabPlan, counters_kv, parse_kv};
use lp_link::{Framing, Link, LinkConfig, Micros, SelectiveRepeat};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct LabSession {
    link: Link<SelectiveRepeat>,
    host: LabHost,
    framing: Framing,
    frames: VecDeque<Vec<u8>>,
    transport: String,
}

fn micros(now_ms: f64) -> Micros {
    (now_ms * 1000.0).max(0.0) as Micros
}

#[wasm_bindgen]
impl LabSession {
    /// `transport`: `usb` (stream framing) or `ble` (one frame per
    /// datagram). `plan`: `k=v` words — echo, stream (seconds), min, max,
    /// inflight, seed, logs, loglen, stall (ms), payload (BLE frame payload,
    /// from the negotiated MTU), minrto (ms).
    #[wasm_bindgen(constructor)]
    pub fn new(transport: &str, plan: &str, nonce: u32, now_ms: f64) -> LabSession {
        let kv: Vec<(String, u64)> = parse_kv(plan);
        let get = |k: &str| kv.iter().find(|(key, _)| key == k).map(|(_, v)| *v);
        let mut cfg = match transport {
            "ble" => LinkConfig::ble(),
            _ => LinkConfig::usb(),
        };
        if let Some(p) = get("payload") {
            cfg.max_payload = p.clamp(20, 1024) as u16;
        }
        if let Some(ms) = get("minrto") {
            cfg.min_rto = ms * 1000;
        }
        // The A/B control: plain COBS against a `test_comms_lab_plain_cobs` board.
        if get("plaincobs") == Some(1) {
            cfg.escape_ff = false;
        }
        let d = LabPlan::default();
        let plan = LabPlan {
            echo_for: get("echo").map_or(d.echo_for, |s| s * 1_000_000),
            stream_for: get("stream").map_or(d.stream_for, |s| s * 1_000_000),
            min: get("min").map_or(d.min, |v| v as usize),
            max: get("max").map_or(d.max, |v| v as usize),
            in_flight: get("inflight").map_or(d.in_flight, |v| v.max(1) as usize),
            seed: get("seed").unwrap_or(d.seed),
            logs: get("logs").map_or(d.logs, |v| v as u32),
            log_len: get("loglen").map_or(d.log_len, |v| v as usize),
            stall_ms: get("stall").map_or(0, |v| v as u32),
            ..d
        };
        LabSession {
            framing: cfg.framing,
            link: Link::new(cfg, nonce),
            host: LabHost::new(plan, micros(now_ms)),
            frames: VecDeque::new(),
            transport: transport.into(),
        }
    }

    /// Bytes from a stream pipe (Web Serial).
    pub fn on_bytes(&mut self, now_ms: f64, bytes: &[u8]) {
        self.link.on_bytes(micros(now_ms), bytes);
    }

    /// One whole frame from a datagram pipe (a BLE notification).
    pub fn on_datagram(&mut self, now_ms: f64, frame: &[u8]) {
        self.link.on_datagram(micros(now_ms), frame);
    }

    /// Events into the plan, the plan's sends into the link, and the link's
    /// frames into the outbox. Returns how many frames wait there.
    pub fn service(&mut self, now_ms: f64) -> u32 {
        let t = micros(now_ms);
        while let Some(ev) = self.link.recv() {
            self.host.on_event(t, ev);
        }
        self.host.drive(t, &mut self.link);
        while let Some(f) = self.link.poll_transmit(t) {
            self.frames.push_back(f.to_vec());
        }
        self.frames.len() as u32
    }

    /// The next frame to write (one Web Serial write, one BLE write).
    pub fn take_frame(&mut self) -> Option<Vec<u8>> {
        self.frames.pop_front()
    }

    /// When `service` next has timer work, in ms (the page's clock).
    pub fn next_wake_ms(&self) -> f64 {
        let a = self.link.poll_timeout();
        let b = self.host.next_deadline();
        match (a, b) {
            (Some(a), Some(b)) => a.min(b) as f64 / 1000.0,
            (Some(x), None) | (None, Some(x)) => x as f64 / 1000.0,
            (None, None) => f64::INFINITY,
        }
    }

    pub fn finished(&self) -> bool {
        self.host.is_finished()
    }

    pub fn phase(&self) -> String {
        format!("{:?}", self.host.phase())
    }

    pub fn is_stream(&self) -> bool {
        self.framing == Framing::Stream
    }

    /// The report as the native tool prints it.
    pub fn report_text(&self) -> String {
        format!("{}", self.host.report())
    }

    /// The report as JSON (the same fields `lp-cli link lab --json` writes).
    pub fn report_json(&self, configuration: &str) -> String {
        let r = self.host.report();
        let mut board = String::new();
        for (i, (k, v)) in r.board.iter().enumerate() {
            let _ = write!(board, "{}\"{}\":{}", if i > 0 { "," } else { "" }, k, v);
        }
        let mut host_kv = String::new();
        counters_kv(self.link.counters(), &mut host_kv);
        let mut host = String::new();
        for (i, (k, v)) in parse_kv(&host_kv).iter().enumerate() {
            let k = k.trim_start_matches("link.");
            let _ = write!(host, "{}\"{}\":{}", if i > 0 { "," } else { "" }, k, v);
        }
        let problems: Vec<String> = r
            .problems()
            .iter()
            .map(|p| format!("\"{}\"", p.replace('"', "'")))
            .collect();
        format!(
            "{{\"configuration\":\"{}\",\"transport\":\"{}\",\"hello\":\"{}\",\
             \"echo\":{{\"msgs\":{},\"bytes_each_way\":{},\"ms\":{},\"bps_each_way\":{},\
             \"rtt_p50_us\":{},\"rtt_p99_us\":{},\"rtt_max_us\":{},\"errors\":{}}},\
             \"stream\":{{\"msgs\":{},\"board_sent\":{},\"bytes\":{},\"ms\":{},\"bps\":{},\
             \"errors\":{},\"gaps\":{}}},\
             \"logs\":{{\"asked\":{},\"lab_rx\":{},\"other_rx\":{}}},\"text_bytes\":{},\
             \"ups\":{},\"resets\":{},\"lost_to_reset\":{},\"stall_ms\":{},\
             \"board\":{{{}}},\"host_link\":{{{}}},\"problems\":[{}],\"failure\":{}}}",
            configuration.replace('"', "'"),
            self.transport,
            r.hello.replace('"', "'"),
            r.echo_msgs,
            r.echo_bytes,
            r.echo_micros / 1000,
            r.echo_rate(),
            r.rtt_percentile(50),
            r.rtt_percentile(99),
            r.rtt_percentile(100),
            r.echo_errors,
            r.stream_msgs,
            r.stream_board_sent
                .map_or("null".to_string(), |v| v.to_string()),
            r.stream_bytes,
            r.stream_micros / 1000,
            r.stream_rate(),
            r.stream_errors,
            r.stream_gaps,
            r.logs_asked,
            r.lab_logs_rx,
            r.other_logs_rx,
            r.text_bytes,
            r.ups,
            r.resets,
            r.lost_to_reset,
            r.stall_asked_ms,
            board,
            host,
            problems.join(","),
            r.failure
                .as_ref()
                .map_or("null".to_string(), |f| format!("\"{}\"", f.replace('"', "'"))),
        )
    }
}
