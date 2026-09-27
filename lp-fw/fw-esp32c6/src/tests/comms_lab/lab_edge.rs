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
    /// Pipe writes that timed out (USB: no host draining, or a lost wake).
    pub write_timeouts: u32,
    /// Of those, the ones while the link was up and hearing its peer: a host
    /// WAS draining, so the write should not have timed out.
    pub write_timeouts_live: u32,
    /// Of the live ones, those that ended with the send buffer free: the
    /// host took the packet and the write future never woke (the esp-hal
    /// interrupt-handler defect, docs/defects/2026-09-26-esp-hals-usb-isr-…).
    pub lost_wakes: u32,
    /// Pipe writes that failed outright (BLE: notify refused).
    pub write_errors: u32,
    /// Peaks since boot: payload bytes the link held (queued, unacknowledged,
    /// reordering, unread), and of those the reliability windows alone.
    peak_buffered: usize,
    peak_window: usize,
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
            write_timeouts_live: 0,
            lost_wakes: 0,
            write_errors: 0,
            peak_buffered: 0,
            peak_window: 0,
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
        not(any(feature = "test_comms_lab_ble", feature = "test_comms_lab_wifi")),
        allow(dead_code, reason = "only the BLE and WiFi pipes are message pipes")
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
        self.peak_buffered = self.peak_buffered.max(self.link.buffered_bytes());
        self.peak_window = self.peak_window.max(self.link.window_bytes());
    }

    /// A pipe write timed out; `buffer_free`: the pipe's send buffer was
    /// empty when it did.
    pub fn note_write_timeout(&mut self, buffer_free: bool) {
        self.write_timeouts += 1;
        let now = now_us();
        if self.link.state() == lp_link::LinkState::Established && !self.link.is_stalled(now) {
            self.write_timeouts_live += 1;
            if buffer_free {
                self.lost_wakes += 1;
            }
        }
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
            "edge.link_us={} edge.app_us={} edge.write_timeouts={} edge.write_timeouts_live={} \
             edge.lost_wakes={} edge.write_errors={} \
             edge.uptime_ms={} edge.heap_free={} edge.heap_used={} edge.heap_max={} \
             edge.peak_link_buffered={} edge.peak_link_window={} edge.link_scratch={} \
             edge.log_ring_dropped={}",
            self.link_us,
            self.app_us,
            self.write_timeouts,
            self.write_timeouts_live,
            self.lost_wakes,
            self.write_errors,
            Instant::now().as_millis(),
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used(),
            esp_alloc::HEAP.stats().max_usage,
            self.peak_buffered,
            self.peak_window,
            self.link.scratch_bytes(),
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
