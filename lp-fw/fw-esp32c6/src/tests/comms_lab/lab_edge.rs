//! What every pipe's task does around its [`Link`], whatever the pipe: take
//! events into the [`LabBoard`], do what the board asks (stall, panic, log
//! lines), pump the board's replies and the log ring into the link, and keep
//! the time spent in the link and the lab apart so the report can say what
//! the link layer costs in CPU.

use alloc::format;
use alloc::string::String;

use embassy_time::Instant;
use lp_link::lab::{BoardAction, LAB_LOG_MARK, LabBoard};
use lp_link::{Link, LinkConfig, Micros, SelectiveRepeat};

use super::lab_logger;

/// Log lines a `log` command writes per loop pass: enough to keep the link
/// busy, few enough that the ring never has to drop one.
const LOG_LINES_PER_PASS: u32 = 4;

pub struct LabEdge {
    pub link: Link<SelectiveRepeat>,
    pub board: LabBoard,
    pipe: &'static str,
    /// A `log` command in progress: (written, asked, length).
    logs: Option<(u32, u32, usize)>,
    /// Time inside `Link` calls (framing, CRC, ARQ, queues).
    pub link_us: u64,
    /// Time inside the lab's own work (soak verify / echo / stream encode).
    pub app_us: u64,
    /// Pipe writes that timed out (USB: no host draining).
    pub write_timeouts: u32,
    /// Pipe writes that failed outright (BLE: notify refused).
    pub write_errors: u32,
}

pub fn now_us() -> Micros {
    Instant::now().as_micros()
}

impl LabEdge {
    pub fn new(pipe: &'static str, cfg: LinkConfig, nonce: u32) -> Self {
        LabEdge {
            link: Link::new(cfg, nonce),
            board: LabBoard::new(identity(pipe)),
            pipe,
            logs: None,
            link_us: 0,
            app_us: 0,
            write_timeouts: 0,
            write_errors: 0,
        }
    }

    /// Bytes from a stream pipe.
    pub fn on_bytes(&mut self, bytes: &[u8]) {
        let t = now_us();
        self.link.on_bytes(t, bytes);
        self.link_us += now_us() - t;
    }

    /// One datagram from a message pipe.
    #[cfg_attr(
        not(feature = "test_comms_lab_ble"),
        allow(dead_code, reason = "only the BLE pipe is a message pipe")
    )]
    pub fn on_datagram(&mut self, frame: &[u8]) {
        let t = now_us();
        self.link.on_datagram(t, frame);
        self.link_us += now_us() - t;
    }

    /// Everything but the pipe: events, actions, replies, stream, logs.
    pub fn service(&mut self) {
        let t = now_us();
        while self.board.ready_for_event() {
            let Some(ev) = self.link.recv() else { break };
            if let Some(action) = self.board.on_event(ev) {
                self.act(action);
            }
        }
        self.write_some_logs();
        let extra = if self.board.stats_due() {
            self.extra()
        } else {
            String::new()
        };
        self.board.pump(&mut self.link, &extra);
        let t2 = now_us();
        self.app_us += t2 - t;
        lab_logger::pump(&mut self.link, t2);
        self.link_us += now_us() - t2;
    }

    /// The link's next frame, timed as link work. The slice borrows the
    /// link; write it before calling anything else.
    pub fn next_frame(&mut self) -> Option<&[u8]> {
        let t = now_us();
        let frame = self.link.poll_transmit(t);
        self.link_us += now_us() - t;
        frame
    }

    /// When the link next needs a pass for a timer, capped so the log ring is
    /// pumped at least every `cap_us`.
    pub fn wake_at(&self, cap_us: Micros) -> Micros {
        let now = now_us();
        self.link
            .poll_timeout()
            .unwrap_or(Micros::MAX)
            .min(now + cap_us)
            .max(now)
    }

    fn act(&mut self, action: BoardAction) {
        match action {
            BoardAction::Stall { ms } => {
                // The whole executor stops, as it does for a shader compile.
                log::info!("{}: stalling the executor {} ms", self.pipe, ms);
                let until = now_us() + u64::from(ms) * 1000;
                while now_us() < until {}
            }
            BoardAction::Panic => panic!("comms lab: the host asked for a panic"),
            BoardAction::Log { n, len } => self.logs = Some((0, n, len)),
        }
    }

    fn write_some_logs(&mut self) {
        let Some((written, asked, len)) = self.logs.as_mut() else {
            return;
        };
        for _ in 0..LOG_LINES_PER_PASS {
            if *written >= *asked {
                break;
            }
            *written += 1;
            let pad = (*len).saturating_sub(LAB_LOG_MARK.len() + 16).min(160);
            log::info!(
                "{}{}/{} {:x<pad$}",
                LAB_LOG_MARK,
                *written,
                *asked,
                "",
                pad = pad
            );
            self.board.note_logs(1);
        }
        if *written >= *asked {
            self.logs = None;
        }
    }

    /// The edge's own facts for a `stats` reply.
    fn extra(&self) -> String {
        format!(
            "edge.link_us={} edge.app_us={} edge.write_timeouts={} edge.write_errors={} \
             edge.uptime_ms={} edge.heap_free={} edge.heap_used={} edge.log_ring_dropped={}",
            self.link_us,
            self.app_us,
            self.write_timeouts,
            self.write_errors,
            Instant::now().as_millis(),
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used(),
            critical_section::with(|cs| lab_logger::LOG_RING.borrow_ref(cs).dropped_total()),
        )
    }
}

fn identity(pipe: &str) -> String {
    format!(
        "lab=comms pipe={pipe} chip=esp32c6 fw={}{} link=selective-repeat",
        env!("LP_BUILD_COMMIT"),
        if env!("LP_BUILD_DIRTY") == "true" {
            "-dirty"
        } else {
            ""
        },
    )
}
