//! [`BlePipeHost`]: the host's end of a board's Bluetooth link, carried by a
//! page that only moves frames — no sockets, no clock of its own.
//!
//! Web Bluetooth lives in a browser and lp-link lives here. The pipe page
//! (`spikes/ble-lab/pipe.html`) holds the GATT connection and hands over
//! [`PageMessage`]s: `up` once a connection is subscribed, one lp-link frame
//! per notification, `down` when the connection drops. Every frame this
//! returns is one GATT write (Datagram framing, `LinkConfig::ble()`: one
//! frame per ATT operation, never cut or joined).
//!
//! **Each connection is a new link.** The board opens its end of a radio
//! link at the subscribe and drops it with the connection, so an `up` makes
//! a fresh [`WireLinkPort`] under a fresh nonce, and a `down` drops it — the
//! update host sees the link go down, then a new one come up, and asks the
//! board `Q` again, which is how it resumes across the three resets an
//! update makes (each reset drops the connection; the page reconnects to
//! the same device with no chooser). The host's transmit window is
//! [`BLE_HOST_TX_WINDOW`] (S5c: loss rises sharply on Mac Chrome above ~16
//! frames in flight), the update's send-ahead `ServeConfig::BLE`.
//!
//! An update starts on a session only once [`EngineLoginGate`] opens it.

use anyhow::Result;
use lp_link::LinkConfig;
use lpc_wire::{PortRead, WireLinkPort};

use super::capture_session::CaptureSession;
use super::engine_login_gate::{EngineLoginGate, GateStep};
use crate::commands::emu::link_host::{describe_link_counters, fresh_nonce};

/// Frames the host keeps in flight before an acknowledgement (S5c's best on
/// Mac Chrome: window 16, four chunks ahead, unpaced). A core-only board
/// advertises a receive window of 32; a running engine, the preset's 8.
pub const BLE_HOST_TX_WINDOW: u8 = 16;

/// The host's link configuration on a Bluetooth pipe: the board's preset,
/// with [`BLE_HOST_TX_WINDOW`].
pub fn ble_host_link_config() -> LinkConfig {
    LinkConfig {
        tx_window: BLE_HOST_TX_WINDOW,
        ..LinkConfig::ble()
    }
}

/// One message from the pipe page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageMessage {
    /// A connection is up and subscribed: a new link session on the board.
    Up,
    /// The connection dropped.
    Down,
    /// One lp-link frame the board notified.
    Frame(Vec<u8>),
    /// The page's own story (connects, timeouts, write failures).
    Log(String),
}

impl PageMessage {
    /// A text message: `up`, `down`, or `log <line>` (anything else is
    /// kept as a log line).
    pub fn from_text(text: &str) -> Self {
        match text {
            "up" => Self::Up,
            "down" => Self::Down,
            other => Self::Log(other.strip_prefix("log ").unwrap_or(other).to_string()),
        }
    }
}

/// One Bluetooth connection, for the rate lines.
#[derive(Debug, Clone, Copy)]
struct Connection {
    index: u32,
    up_at_us: u64,
    /// Update bytes served before it came up.
    served_before: u64,
    /// Seconds it lasted, and what it served, once it is over.
    ended: Option<(f64, u64)>,
}

/// The host's end of the pipe. See the module docs.
pub struct BlePipeHost {
    session: CaptureSession,
    want_packed: bool,
    gate: EngineLoginGate,
    link: Option<WireLinkPort>,
    /// The current link's lp-link session is up.
    session_up: bool,
    connections: Vec<Connection>,
    /// Frames the page sent while no connection was up.
    stray_frames: u32,
}

impl BlePipeHost {
    /// A host for `session`, logging in with `password` where a running
    /// engine needs it, asking boards to pack their replies when
    /// `want_packed`.
    pub fn new(session: CaptureSession, password: Option<&str>, want_packed: bool) -> Self {
        Self {
            session,
            want_packed,
            gate: EngineLoginGate::new(password),
            link: None,
            session_up: false,
            connections: Vec::new(),
            stray_frames: 0,
        }
    }

    /// A console line contained `--exit-on`.
    pub fn matched(&self) -> bool {
        self.session.matched()
    }

    /// Bluetooth connections the page has brought up so far.
    pub fn connections(&self) -> u32 {
        self.connections.len() as u32
    }

    /// The current connection's link, if one is up.
    #[cfg(test)]
    pub fn link(&self) -> Option<&WireLinkPort> {
        self.link.as_ref()
    }

    /// The capture's session (its update host).
    #[cfg(test)]
    pub fn session(&self) -> &CaptureSession {
        &self.session
    }

    /// One message from the page, at `now_us` (since the run started).
    pub fn on_page(&mut self, now_us: u64, message: PageMessage) -> Result<()> {
        match message {
            PageMessage::Up => {
                if self.link.is_some() {
                    // An `up` with no `down` between: the old link is gone.
                    self.drop_connection(now_us, "a new connection came up over it")?;
                }
                let index = self.connections() + 1;
                self.connections.push(Connection {
                    index,
                    up_at_us: now_us,
                    served_before: self.served(),
                    ended: None,
                });
                self.link = Some(WireLinkPort::new(
                    ble_host_link_config(),
                    fresh_nonce(),
                    self.want_packed,
                ));
                self.note(
                    now_us,
                    &format!("Bluetooth connection {index} up; a new link"),
                )?;
            }
            PageMessage::Down => {
                self.drop_connection(now_us, "the page lost the Bluetooth connection")?;
            }
            PageMessage::Frame(frame) => match self.link.as_mut() {
                Some(link) => link.on_datagram(now_us, &frame),
                None => self.stray_frames += 1,
            },
            PageMessage::Log(text) => self.session.line(&format!("[pipe] {text}"))?,
        }
        Ok(())
    }

    /// The page itself went away (its WebSocket closed): the connection it
    /// carried is gone with it.
    pub fn page_gone(&mut self, now_us: u64) -> Result<()> {
        self.drop_connection(now_us, "the pipe page went away")
    }

    /// Run the link and everything above it at `now_us`; the frames to
    /// write, one per GATT write, are appended to `out`.
    pub fn poll(&mut self, now_us: u64, out: &mut Vec<Vec<u8>>) -> Result<()> {
        let now_ms = now_us / 1_000;
        let Some(link) = self.link.as_mut() else {
            return Ok(());
        };
        while let Some(frame) = link.poll_transmit(now_us) {
            out.push(frame.to_vec());
        }
        while let Some(read) = link.poll_read() {
            let step = match &read {
                PortRead::Up { generation } => {
                    self.session_up = true;
                    self.gate.session_up(now_ms);
                    eprintln!(
                        "link capture: link up (session {generation}) at {:.3} s, board nonce {}",
                        secs(now_us),
                        link.link()
                            .peer_nonce()
                            .map_or_else(|| "unknown".into(), |n| format!("{n:#010x}"))
                    );
                    GateStep::Nothing
                }
                PortRead::Reset { reason } => {
                    if self.session_up && self.gate.is_open() {
                        self.session.ota_down(now_ms);
                    }
                    self.session_up = false;
                    self.gate.session_down();
                    eprintln!(
                        "link capture: link reset ({reason:?}) at {:.3} s",
                        secs(now_us)
                    );
                    GateStep::Nothing
                }
                PortRead::Message(payload) => match &payload.message {
                    Ok(message) => self.gate.on_server(now_ms, &message.msg),
                    Err(_) => GateStep::Nothing,
                },
                _ => GateStep::Nothing,
            };
            self.session.on_read(&read)?;
            gate_step(step, link, &mut self.session, now_us)?;
            if self.session.matched() {
                return Ok(());
            }
        }
        // Until the gate opens, channel 3 is only the board saying what
        // runs (core-only's unprompted `M`): the update has not asked yet,
        // so nothing here is an answer to it.
        if self.session_up && !self.gate.is_open() {
            while link.poll_update().is_some() {
                let step = self.gate.on_update_message();
                gate_step(step, link, &mut self.session, now_us)?;
            }
            let step = self.gate.tick(now_ms);
            gate_step(step, link, &mut self.session, now_us)?;
        }
        if self.gate.is_open() {
            self.session.pump_ota(link, now_ms)?;
            if self.session.matched() {
                return Ok(());
            }
            self.session.pump_requests(link)?;
        }
        while let Some(frame) = link.poll_transmit(now_us) {
            out.push(frame.to_vec());
        }
        Ok(())
    }

    /// End the run with the connections' account.
    pub fn finish(mut self, elapsed_s: f64) -> Result<()> {
        let now_us = (elapsed_s * 1e6) as u64;
        if let Some(link) = &self.link {
            eprintln!(
                "link capture: at the end, the last connection's link — {}",
                describe_link_counters(&link.counters())
            );
            self.end_connection(now_us);
        }
        // A fold, not `sum()`: an empty f64 sum is -0.0.
        let connected_s = self
            .connections
            .iter()
            .filter_map(|c| c.ended)
            .fold(0.0, |total, c| total + c.0);
        let served: u64 = self
            .connections
            .iter()
            .filter_map(|c| c.ended)
            .map(|c| c.1)
            .sum();
        let host_link = format!(
            "{} Bluetooth connection(s), {connected_s:.1} s connected",
            self.connections.len()
        );
        let mut notes: Vec<String> = self
            .connections
            .iter()
            .filter_map(|c| {
                let (lasted, bytes) = c.ended?;
                Some(format!(
                    "connection {} up at {:.3} s for {lasted:.1} s: {}",
                    c.index,
                    secs(c.up_at_us),
                    describe_rate(bytes, lasted)
                ))
            })
            .collect();
        if self.session.ota().is_some() {
            notes.push(format!(
                "update over Bluetooth: {} while connected, {} reconnect(s)",
                describe_rate(served, connected_s),
                self.connections.len().saturating_sub(1)
            ));
        }
        if self.stray_frames > 0 {
            notes.push(format!(
                "{} frame(s) arrived with no connection up and were dropped",
                self.stray_frames
            ));
        }
        self.session.finish(elapsed_s, &host_link, &notes)
    }

    // ---- Helpers -----------------------------------------------------------

    /// The connection is gone: its link with it, and the update's session.
    fn drop_connection(&mut self, now_us: u64, why: &str) -> Result<()> {
        let Some(link) = self.link.take() else {
            return Ok(());
        };
        if self.session_up {
            self.session.session_lost();
            if self.gate.is_open() {
                self.session.ota_down(now_us / 1_000);
            }
        }
        self.session_up = false;
        self.gate.session_down();
        let rate = self.end_connection(now_us);
        let index = self.connections();
        self.note(
            now_us,
            &format!(
                "Bluetooth connection {index} down ({why}); {rate}; its link — {}",
                describe_link_counters(&link.counters())
            ),
        )
    }

    /// Close the current connection's account; its rate line.
    fn end_connection(&mut self, now_us: u64) -> String {
        let served = self.served();
        let Some(c) = self.connections.last_mut() else {
            return String::new();
        };
        if c.ended.is_none() {
            let lasted = now_us.saturating_sub(c.up_at_us) as f64 / 1e6;
            c.ended = Some((lasted, served.saturating_sub(c.served_before)));
        }
        let (lasted, bytes) = c.ended.unwrap_or_default();
        format!("{lasted:.1} s up, {}", describe_rate(bytes, lasted))
    }

    /// Update bytes served so far, across connections.
    fn served(&self) -> u64 {
        self.session.ota().map_or(0, |ota| {
            let c = ota.served();
            c.bytes_raw + c.bytes_encoded
        })
    }

    /// A line of the host's own, on stderr and in the console.
    fn note(&mut self, now_us: u64, text: &str) -> Result<()> {
        eprintln!("link capture: {text} at {:.3} s", secs(now_us));
        self.session.line(&format!("[host-ble] {text}"))
    }
}

/// Do what the gate asked, on `link`.
fn gate_step(
    step: GateStep,
    link: &mut WireLinkPort,
    session: &mut CaptureSession,
    now_us: u64,
) -> Result<()> {
    match step {
        GateStep::Nothing => {}
        GateStep::Send(message) => {
            if let Err(error) = link.send_client(&message) {
                // The gate's own wait ends it: the update is asked anyway.
                eprintln!("link capture: the engine's login could not be sent: {error:?}");
            }
        }
        GateStep::Open(why) => {
            eprintln!("link capture: {why} at {:.3} s", secs(now_us));
            session.line(&format!("[host-ble] {why}"))?;
            session.ota_up(now_us / 1_000);
        }
    }
    Ok(())
}

/// `n B served (r KiB/s)`.
fn describe_rate(bytes: u64, seconds: f64) -> String {
    let rate = if seconds > 0.0 {
        bytes as f64 / 1024.0 / seconds
    } else {
        0.0
    };
    format!("{bytes} B served ({rate:.1} KiB/s)")
}

fn secs(us: u64) -> f64 {
    us as f64 / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::link::ble_pipe_fake_board::{FakeBoard, FakeMode};
    use crate::commands::link::ble_pipe_fake_board::{board_running, offer_fixture};

    #[test]
    fn page_text_is_up_down_or_a_log_line() {
        assert_eq!(PageMessage::from_text("up"), PageMessage::Up);
        assert_eq!(PageMessage::from_text("down"), PageMessage::Down);
        assert_eq!(
            PageMessage::from_text("log 1.234 ble up (LP-x)"),
            PageMessage::Log("1.234 ble up (LP-x)".into())
        );
    }

    #[test]
    fn the_host_window_is_s5c_s_sixteen_on_the_ble_preset() {
        let cfg = ble_host_link_config();
        assert_eq!(cfg.tx_window, 16);
        assert!(matches!(cfg.framing, lp_link::Framing::Datagram));
        assert_eq!(cfg.max_payload, LinkConfig::ble().max_payload);
    }

    #[test]
    fn a_locked_engine_is_logged_in_to_before_the_update_asks_anything() {
        let fx = offer_fixture();
        let (mut host, _console) = rig::host(&fx, Some("desk-lab"));
        let mut board = FakeBoard::new(
            FakeMode::Engine {
                password: Some("desk-lab"),
            },
            board_running(&fx),
        );
        let mut now = 0;
        host.on_page(now, PageMessage::Up).unwrap();
        rig::run_until(&mut host, &mut board, &mut now, |h, _| {
            h.session().ota().is_some_and(|o| o.done())
        });
        assert!(board.granted, "the engine's login was granted");
        assert_eq!(
            board.update_messages_before_grant, 0,
            "nothing on channel 3 before the engine's login"
        );
        assert_eq!(board.queries, 1, "then the update asked who it is");
        let ota = host.session().ota().unwrap();
        assert_eq!(ota.finish, Some(lpa_update::Finish::UpToDate));
        assert!(
            board.every_frame_fit,
            "one frame per write, never over the payload"
        );
    }

    #[test]
    fn core_only_is_asked_at_once_and_its_unprompted_manifest_is_not_an_answer() {
        let fx = offer_fixture();
        let (mut host, _console) = rig::host(&fx, Some("desk-lab"));
        let mut board = FakeBoard::new(FakeMode::CoreOnly, board_running(&fx));
        let mut now = 0;
        host.on_page(now, PageMessage::Up).unwrap();
        rig::run_until(&mut host, &mut board, &mut now, |h, _| {
            h.session().ota().is_some_and(|o| o.done())
        });
        assert_eq!(
            board.logins_begun, 0,
            "core-only has no engine to log in to"
        );
        assert_eq!(board.queries, 1);
        let ota = host.session().ota().unwrap();
        assert_eq!(
            ota.manifests.len(),
            1,
            "only the answer to Q reached the driver, not the unprompted M"
        );
        assert!(now < 3_000_000, "no 3 s wait for a hello that never comes");
    }

    #[test]
    fn every_connection_is_a_new_link_and_the_update_asks_again_on_it() {
        let fx = offer_fixture();
        let (mut host, _console) = rig::host(&fx, Some("desk-lab"));
        // Connection 1: an engine, logged in, asked — and the board resets
        // (the connection drops) before it answers.
        let mut first = FakeBoard::new(
            FakeMode::Engine {
                password: Some("desk-lab"),
            },
            board_running(&fx),
        )
        .silent_on_update();
        let mut now = 0;
        host.on_page(now, PageMessage::Up).unwrap();
        rig::run_until(&mut host, &mut first, &mut now, |_, b| b.queries == 1);
        let first_nonce = host.link().unwrap().link().peer_nonce();
        host.on_page(now, PageMessage::Down).unwrap();
        assert!(host.link().is_none(), "the link went with the connection");
        // Frames still in the air from the old connection go nowhere.
        host.on_page(now, PageMessage::Frame(vec![0; 12])).unwrap();

        // Connection 2: the board came back core-only (the update's reset).
        let mut second = FakeBoard::new(FakeMode::CoreOnly, board_running(&fx));
        host.on_page(now, PageMessage::Up).unwrap();
        rig::run_until(&mut host, &mut second, &mut now, |h, _| {
            h.session().ota().is_some_and(|o| o.done())
        });
        assert_eq!(host.connections(), 2);
        assert_eq!(second.queries, 1, "the new link starts with Q");
        assert_eq!(second.logins_begun, 0);
        assert_ne!(host.link().unwrap().link().peer_nonce(), first_nonce);
        assert_eq!(
            host.session().ota().unwrap().finish,
            Some(lpa_update::Finish::UpToDate)
        );
        host.finish(secs(now)).unwrap();
    }

    /// The host and fake boards, stepped in a shared clock.
    mod rig {
        use super::*;
        use crate::commands::link::args::CaptureArgs;
        use crate::commands::ota_host::{OtaArgs, OtaHost};
        use lpa_update::ServeConfig;

        pub fn host(
            fx: &crate::commands::firmware::ota_fixture::Fixture,
            password: Option<&str>,
        ) -> (BlePipeHost, tempfile::TempDir) {
            let dir = tempfile::tempdir().unwrap();
            let ota = OtaArgs {
                ota_offer: Some(fx.ota_dir()),
                ota_password: password.map(str::to_string),
                ..OtaArgs::default()
            };
            let args = CaptureArgs {
                target: "blepipe:0".into(),
                board_password: Default::default(),
                console: dir.path().join("console.txt"),
                exit_on: None,
                seconds: 60,
                json_replies: true,
                request: Vec::new(),
                ota: ota.clone(),
            };
            let ota = OtaHost::from_args_over(&ota, ServeConfig::BLE).unwrap();
            let session = CaptureSession::create(&args, ota).unwrap();
            (BlePipeHost::new(session, password, false), dir)
        }

        /// Step both ends 5 ms at a time, moving frames between them the
        /// way the page does, until `done` (or 60 emulated seconds).
        pub fn run_until(
            host: &mut BlePipeHost,
            board: &mut FakeBoard,
            now: &mut u64,
            done: impl Fn(&BlePipeHost, &FakeBoard) -> bool,
        ) {
            let mut out = Vec::new();
            for _ in 0..12_000 {
                if done(host, board) {
                    return;
                }
                *now += 5_000;
                out.clear();
                host.poll(*now, &mut out).unwrap();
                for frame in out.drain(..) {
                    board.on_write(*now, &frame);
                }
                for frame in board.poll(*now) {
                    host.on_page(*now, PageMessage::Frame(frame)).unwrap();
                }
            }
            panic!("the run did not get there in 60 s");
        }
    }
}
