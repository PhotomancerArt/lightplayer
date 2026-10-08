//! One browser port's lp-link end, and the queues its drainers read.
//!
//! A Web Serial port and a tab-hosted board each keep ONE of these for as
//! long as the port is open (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`,
//! D1/D2), and so does each Web Bluetooth connection (plan
//! `lp2025/2026-09-28-1445-ble-on-lp-link`, D1 — the same type, on
//! [`LinkConfig::ble`]'s datagrams). It is the sans-IO half of the
//! provider's per-port loop: the edge
//! (`providers/browser_serial_esp32/browser_serial.rs`,
//! `providers/emulator_tab/emulator_tab_link_port.rs`,
//! `providers/browser_ble/ble_link_port.rs`) pulls what the page read — bytes
//! for [`on_bytes`](LinkPortService::on_bytes), or one whole frame per
//! notification for [`on_datagram`](LinkPortService::on_datagram) — writes
//! what [`transmit`](LinkPortService::transmit) hands back, and wakes it
//! again within [`wake_in`](LinkPortService::wake_in). Everything else — the
//! handshake, resends, acknowledgements, the packed-reply opt-in, the dev
//! log-level rider — is the [`WireLinkPort`]'s.
//!
//! What the board said lands in two queues:
//!
//! - **reads** ([`take_reads`](LinkPortService::take_reads)): messages,
//!   console lines and link resets, for whoever drains the port — the
//!   model's link pump or, while it holds the wire, a borrowed conversation.
//!   One queue, many drainers, one at a time (the exclusive borrow, D2); a
//!   borrower that takes a message takes it whole, because the link already
//!   delivered it whole.
//! - **notes** ([`take_notes`](LinkPortService::take_notes)): the link's own
//!   story for the device journal (up, stalled, the opt-in's outcome), which
//!   only the pump reads.
//! - **updates** ([`take_updates`](LinkPortService::take_updates)): the
//!   board's channel-3 messages (the over-the-air update protocol, M7 P7),
//!   which only the pump reads, so a borrowed conversation never eats one.
//!   The queue is this session's: a link reset drops what an old session
//!   said.
//!
//! **Channel 3 only to a board that announced it** (DS9):
//! [`send_update`](LinkPortService::send_update) queues nothing until this
//! session's hello carried `firmware` or the board sent an update message
//! itself. A board without the channel would never acknowledge a reliable
//! frame there and the link would stall; a message refused this way is a
//! note, never a stalled link.
//!
//! A write that cannot be queued (the link's send budget is full, or the
//! message is larger than a link message may be) is an error the sender sees,
//! never a silent drop.

use std::collections::VecDeque;

use lpc_wire::lp_link::{LinkConfig, LinkState, Micros};
use lpc_wire::server::api::LogLevel;
use lpc_wire::{ClientMessage, LinkCounters, ServerMsgBody, WireLinkPort};

use crate::device_link::link_note::{
    LINK_ANSWERING_NOTE, LINK_STALLED_NOTE, UPDATE_NOT_ANNOUNCED_NOTE,
};
use crate::device_link::port_read_map::{MappedRead, map_port_read};
use crate::device_link::wire_reader::WireRead;

/// What a secure port's handshake said beyond `Up`.
#[cfg(feature = "secure-link")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecureLinkEvent {
    /// The board refused the key presented: answer with
    /// [`LinkPortService::retry_with`] (the key walk says which).
    Refused(crate::providers::network_link::KeyRefusal),
    /// The board runs a plain link: a secure port never comes up on it (there
    /// is no downgrade).
    PeerNotSecure,
}

/// A [`LinkKey`](crate::providers::network_link::LinkKey) as lp-link's own
/// key id and PSK.
#[cfg(feature = "secure-link")]
fn secure_key(
    key: &crate::providers::network_link::LinkKey,
) -> (
    lpc_wire::lp_link::secure_channel::KeyId,
    lpc_wire::lp_link::secure_channel::Psk,
) {
    use lpc_wire::lp_link::secure_channel::{KeyId, Psk};
    if key.is_anonymous() {
        return (KeyId::ANONYMOUS, Psk::ANONYMOUS);
    }
    (KeyId(key.key_id), Psk::new(key.psk))
}

/// A browser port's lp-link end. See the module docs.
pub struct LinkPortService {
    port: WireLinkPort,
    reads: VecDeque<WireRead>,
    notes: Vec<String>,
    /// This session's channel-3 messages from the board, for the pump.
    updates: VecDeque<Vec<u8>>,
    /// The board announced channel 3 this session (DS9).
    announced: bool,
    /// The board's base MAC, from the last hello that carried one (kept
    /// across sessions: a board's MAC does not change).
    base_mac: Option<String>,
    /// Whether the last look found the link stalled, so each edge is noted
    /// once.
    stalled: bool,
}

impl LinkPortService {
    /// A port on a link tuned by `config` (the transport's preset:
    /// `LinkConfig::usb()` for a USB-Serial-JTAG board, `LinkConfig::uart()`
    /// for a classic ESP32 behind a USB-UART bridge, `LinkConfig::ble()` for
    /// a Web Bluetooth connection — datagram framing, fed by
    /// [`Self::on_datagram`]), with a fresh `nonce`
    /// (random per open: it is how the board tells this open from the last
    /// one). `want_packed` asks the board to pack its replies; `device_log`
    /// is the dev log-level rider.
    pub fn new(
        config: LinkConfig,
        nonce: u32,
        want_packed: bool,
        device_log: Option<LogLevel>,
    ) -> Self {
        Self {
            port: WireLinkPort::new(config, nonce, want_packed).with_device_log_level(device_log),
            reads: VecDeque::new(),
            notes: Vec::new(),
            updates: VecDeque::new(),
            announced: false,
            base_mac: None,
            stalled: false,
        }
    }

    /// A port whose link is a SECURE lp-link initiator (feature
    /// `secure-link`): the LAN link to a board on Wi-Fi, on
    /// [`LinkConfig::ws`]'s datagrams. It presents `key` (an access entry's
    /// salt and `link_psk(K)`, or the anonymous key) inside its SYN, and the
    /// board grants that entry's tier — its hello says which. `entropy`
    /// fills a buffer with fresh random bytes (32 per handshake). What the
    /// handshake says beyond `Up` is [`Self::poll_secure_event`]'s.
    #[cfg(feature = "secure-link")]
    pub fn new_secure(
        config: LinkConfig,
        nonce: u32,
        want_packed: bool,
        device_log: Option<LogLevel>,
        key: &crate::providers::network_link::LinkKey,
        entropy: fn(&mut [u8]),
    ) -> Self {
        let (key_id, psk) = secure_key(key);
        Self {
            port: WireLinkPort::new_secure(config, nonce, want_packed, key_id, psk, entropy)
                .with_device_log_level(device_log),
            reads: VecDeque::new(),
            notes: Vec::new(),
            updates: VecDeque::new(),
            announced: false,
            base_mac: None,
            stalled: false,
        }
    }

    /// The next thing a secure port's handshake said beyond `Up` (a refusal,
    /// a peer that runs a plain link), in this crate's words. Each also
    /// reached the journal as one note.
    #[cfg(feature = "secure-link")]
    pub fn poll_secure_event(&mut self) -> Option<SecureLinkEvent> {
        use crate::providers::network_link::KeyRefusal;
        use lpc_wire::lp_link::secure_channel::{RefusalReason, SecureEvent};
        loop {
            let event = self.port.poll_secure_event()?;
            self.collect_reads();
            return Some(match event {
                SecureEvent::Refused {
                    reason,
                    retry_after_ms,
                } => SecureLinkEvent::Refused(match reason {
                    RefusalReason::UnknownKey => KeyRefusal::UnknownKey,
                    RefusalReason::WrongKey => KeyRefusal::WrongKey,
                    RefusalReason::Backoff => KeyRefusal::Backoff { retry_after_ms },
                    RefusalReason::Busy => KeyRefusal::Busy,
                }),
                SecureEvent::PeerNotSecure => SecureLinkEvent::PeerNotSecure,
                // Responder events never reach an initiator.
                SecureEvent::KeyLookup { .. } | SecureEvent::WrongKey { .. } => continue,
            });
        }
    }

    /// A secure port, after a refusal: present `key` instead. The handshake
    /// starts again at once (the caller transmits after).
    #[cfg(feature = "secure-link")]
    pub fn retry_with(&mut self, key: &crate::providers::network_link::LinkKey) {
        let (key_id, psk) = secure_key(key);
        self.port.retry_with(key_id, psk);
    }

    /// A secure port that is UP on a key it would rather replace (a key that
    /// arrived while it was up — a password typed for a locked board): end
    /// this session and start a new one presenting `key`. The drainer reads
    /// the end as a link reset, then the new session's hello.
    #[cfg(feature = "secure-link")]
    pub fn rekey(&mut self, now: Micros, key: &crate::providers::network_link::LinkKey) {
        self.port.restart(now);
        self.retry_with(key);
        self.collect(now);
    }

    /// Bytes the page read from the port at `now`, in any split.
    pub fn on_bytes(&mut self, now: Micros, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.port.on_bytes(now, bytes);
        }
        self.collect(now);
    }

    /// One whole frame the page read at `now` from a datagram transport (one
    /// Bluetooth notification). A notification that is not a whole, intact
    /// frame is dropped and counted by the link, like any damaged frame.
    pub fn on_datagram(&mut self, now: Micros, frame: &[u8]) {
        self.port.on_datagram(now, frame);
        self.collect(now);
    }

    /// Hand every frame the link has to send now to `write`, in order.
    pub fn transmit(&mut self, now: Micros, mut write: impl FnMut(&[u8])) {
        while let Some(frame) = self.port.poll_transmit(now) {
            write(frame);
        }
        self.collect(now);
    }

    /// [`Self::transmit`], but at most `room` frames: for a transport that
    /// takes one awaited write at a time (a GATT write), where a frame handed
    /// over is a frame queued in the page, not one on the air. What is left
    /// stays in the link, where a resend or a newer acknowledgement can
    /// still replace it. Answers how many frames went to `write`.
    pub fn transmit_up_to(
        &mut self,
        now: Micros,
        room: usize,
        mut write: impl FnMut(&[u8]),
    ) -> usize {
        let mut written = 0;
        while written < room {
            let Some(frame) = self.port.poll_transmit(now) else {
                break;
            };
            write(frame);
            written += 1;
        }
        self.collect(now);
        written
    }

    /// Queue a request already serialized as JSON (no `M!`, no newline).
    /// The caller transmits after.
    pub fn send_client_json(&mut self, json: &str) -> Result<(), String> {
        self.port
            .send_client_json(json)
            .map_err(|error| format!("the link would not take the request: {error:?}"))
    }

    /// [`Self::send_client_json`] for a request not yet serialized.
    pub fn send_client(&mut self, message: &ClientMessage) -> Result<(), String> {
        self.port
            .send_client(message)
            .map_err(|error| format!("the link would not take the request: {error:?}"))
    }

    /// Queue one channel-3 (update) message. The caller transmits after.
    ///
    /// Before the board announced the channel this session (DS9, see the
    /// module docs) nothing is queued: the message is dropped with a note
    /// and `Ok(false)` says so. `Ok(true)`: queued.
    pub fn send_update(&mut self, message: &[u8]) -> Result<bool, String> {
        if !self.announced {
            self.notes.push(UPDATE_NOT_ANNOUNCED_NOTE.to_string());
            return Ok(false);
        }
        self.port
            .send_update(message)
            .map(|()| true)
            .map_err(|error| format!("the link would not take the update message: {error:?}"))
    }

    /// The board's channel-3 messages since the last take, this session's
    /// only, in order. Only the model's link pump drains them.
    pub fn take_updates(&mut self) -> Vec<Vec<u8>> {
        self.updates.drain(..).collect()
    }

    /// Whether the board announced the update channel this session.
    pub fn update_channel_announced(&self) -> bool {
        self.announced
    }

    /// The board's base MAC, once a hello on this link said it.
    pub fn base_mac(&self) -> Option<&str> {
        self.base_mac.as_deref()
    }

    /// How long until the link next needs [`Self::transmit`] for a timer,
    /// at most `cap` (new bytes and new sends need one too, and the edge
    /// polls the page for bytes on the same tick).
    pub fn wake_in(&self, now: Micros, cap: Micros) -> Micros {
        match self.port.poll_timeout() {
            Some(at) => at.saturating_sub(now).min(cap),
            None => cap,
        }
    }

    /// Everything the board said since the last take, in order.
    pub fn take_reads(&mut self) -> Vec<WireRead> {
        self.reads.drain(..).collect()
    }

    /// What the link said about itself since the last take.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Up (the handshake is done) or not.
    pub fn is_up(&self) -> bool {
        self.port.state() == LinkState::Established
    }

    /// Up, but the board has been silent past the link's stall time.
    pub fn is_stalled(&self, now: Micros) -> bool {
        self.port.is_stalled(now)
    }

    /// This end's link counters.
    pub fn counters(&self) -> LinkCounters {
        self.port.counters()
    }

    /// The link's smoothed round-trip time, as its retransmit timer uses it.
    pub fn srtt(&self) -> Micros {
        self.port.link().srtt()
    }

    /// The preset this port's link was built with.
    pub fn config(&self) -> &LinkConfig {
        self.port.link().config()
    }

    /// Sort what the port has read onto the queues
    /// ([`Self::collect_reads`]), and note a stall's edges.
    fn collect(&mut self, now: Micros) {
        self.collect_reads();
        let stalled = self.port.is_stalled(now);
        if stalled != self.stalled {
            self.stalled = stalled;
            self.notes.push(
                if stalled {
                    LINK_STALLED_NOTE
                } else {
                    LINK_ANSWERING_NOTE
                }
                .to_string(),
            );
        }
    }

    /// Sort what the port has read onto the queues, and track the update
    /// session: every read first, because a link event (`Up`, `Reset`) clears
    /// the port's own update queue, so the update messages polled after are
    /// this session's, and a reset drops ours too. Every path that drains the
    /// port comes through here (a secure port's handshake events included),
    /// so the tracking holds on every link, the LAN link's too.
    fn collect_reads(&mut self) {
        while let Some(read) = self.port.poll_read() {
            if matches!(read, lpc_wire::PortRead::Up { .. }) {
                self.forget_session();
            }
            match map_port_read(read) {
                MappedRead::Read(read) => {
                    match &read {
                        WireRead::LinkReset(_) => self.forget_session(),
                        WireRead::Frame(frame) => {
                            if let Ok(message) = &frame.message
                                && let ServerMsgBody::Hello(hello) = &message.msg
                            {
                                self.announced |= hello.firmware.is_some();
                                if let Some(mac) = &hello.hardware.base_mac {
                                    self.base_mac = Some(mac.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                    self.reads.push_back(read);
                }
                MappedRead::Note(note) => self.notes.push(note),
            }
        }
        while let Some(update) = self.port.poll_update() {
            self.announced = true;
            self.updates.push_back(update);
        }
    }

    /// A session ended or began: what the board said on channel 3, and
    /// whether it announced it, belonged to the old one.
    fn forget_session(&mut self) {
        self.updates.clear();
        self.announced = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_update::BoardManifest;
    use lpc_wire::lp_link::{CH_PROTO, CH_UPDATE, Link, LinkConfig, LinkEvent, SelectiveRepeat};
    use lpc_wire::server::hello::{BuildFacts, HardwareFacts, ServerHello};
    use lpc_wire::{
        ClientRequest, PACK_FORMAT_VERSION, ServerMsgBody, WIRE_PROTO_VERSION, WireServerMessage,
    };

    #[test]
    fn the_link_comes_up_and_the_hello_is_a_frame() {
        let mut bench = Bench::new();
        bench.run(40);

        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.starts_with("link: up")),
            "{notes:?}"
        );
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.json.contains("\"hello\"") && !frame.packed
            ),
            "{reads:?}"
        );
        assert!(bench.host.is_up());
    }

    #[test]
    fn a_request_is_answered_through_the_link() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();

        let json = lpc_wire::json::to_string(&ClientMessage {
            id: 7,
            msg: ClientRequest::StopAllProjects,
        })
        .unwrap();
        bench.host.send_client_json(&json).expect("queued");
        bench.run(40);

        assert_eq!(bench.board.requests, [7]);
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.message.as_ref().is_ok_and(|m| m.id == 7)
            ),
            "{reads:?}"
        );
    }

    #[test]
    fn raw_text_outside_frames_is_console_lines() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();

        bench.host.on_bytes(bench.now, b"[INIT] boot marker\r\n");
        let reads = bench.host.take_reads();
        assert!(
            matches!(reads.as_slice(), [WireRead::Line(line)] if line == "[INIT] boot marker"),
            "{reads:?}"
        );
    }

    /// Plan D9: a board that restarts ends the session, and the drainer
    /// hears it at once — before the new session's hello.
    #[test]
    fn a_board_restart_is_a_link_reset_then_a_new_hello() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();
        bench.host.take_notes();

        bench.board = BoardDouble::new(0xB0A2_0002);
        bench.run(60);

        let reads = bench.host.take_reads();
        let reset_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::LinkReset(note) if note.contains("restarted")),
            )
            .unwrap_or_else(|| panic!("no reset read: {reads:?}"));
        let hello_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\"")),
            )
            .unwrap_or_else(|| panic!("no new hello: {reads:?}"));
        assert!(reset_at < hello_at, "{reads:?}");
    }

    #[test]
    fn a_silent_board_is_noted_as_a_stall_and_its_return_as_news() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_notes();

        // Two seconds of the host alone: the board says nothing.
        for _ in 0..2_000 {
            bench.host.transmit(bench.now, |_| {});
            bench.now += 1_000;
        }
        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.contains("stalled")),
            "{notes:?}"
        );
        bench.run(40);
        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.contains("answering again")),
            "{notes:?}"
        );
    }

    /// The classic's pair (plan `classic-uart-on-lp-link`, P4): a Web Serial
    /// port behind the CH340 runs `uart()`, the board its own cut of it. The
    /// port above the link is the one every other board gets.
    #[test]
    fn a_classic_board_comes_up_and_answers_on_the_uart_preset() {
        let mut bench = Bench::classic();
        assert_eq!(
            format!("{:?}", bench.host.config()),
            format!("{:?}", LinkConfig::uart())
        );
        bench.run(40);
        assert!(bench.host.is_up());
        let reads = bench.host.take_reads();
        assert!(
            matches!(reads.as_slice(), [WireRead::Frame(frame)] if frame.json.contains("\"hello\"")),
            "{reads:?}"
        );

        let json = lpc_wire::json::to_string(&ClientMessage {
            id: 9,
            msg: ClientRequest::StopAllProjects,
        })
        .unwrap();
        bench.host.send_client_json(&json).expect("queued");
        bench.run(40);
        assert_eq!(bench.board.requests, [9]);
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.message.as_ref().is_ok_and(|m| m.id == 9)
            ),
            "{reads:?}"
        );
    }

    /// Plan D9 is not a USB rule: a classic that restarts (a Reboot request,
    /// its new nonce salted by the boot count) is a link reset the drainer
    /// hears before the new hello, over the classic's own preset.
    #[test]
    fn a_classic_board_restart_is_a_link_reset_then_a_new_hello() {
        let mut bench = Bench::classic();
        bench.run(40);
        bench.host.take_reads();
        bench.host.take_notes();

        bench.board = BoardDouble::with_config(classic_board(), 0xB0A2_0002);
        bench.run(60);

        let reads = bench.host.take_reads();
        let reset_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::LinkReset(note) if note.contains("restarted")),
            )
            .unwrap_or_else(|| panic!("no reset read: {reads:?}"));
        let hello_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\"")),
            )
            .unwrap_or_else(|| panic!("no new hello: {reads:?}"));
        assert!(reset_at < hello_at, "{reads:?}");
    }

    /// What D9 exists for: a request in flight when the classic restarts
    /// is never answered, and the drainer reads a reset in its place — so
    /// the lens io (`port_client_io`) fails it at once instead of waiting
    /// out its budget.
    #[test]
    fn a_request_in_flight_when_a_classic_restarts_reads_as_a_reset() {
        let mut bench = Bench::classic();
        bench.run(40);
        bench.host.take_reads();

        let json = lpc_wire::json::to_string(&ClientMessage {
            id: 11,
            msg: ClientRequest::StopAllProjects,
        })
        .unwrap();
        bench.host.send_client_json(&json).expect("queued");
        // The board goes down before it hears the request, and comes back as
        // a new session.
        bench.board = BoardDouble::with_config(classic_board(), 0xB0A2_0003);
        bench.run(60);

        let reads = bench.host.take_reads();
        assert!(
            reads
                .iter()
                .any(|read| matches!(read, WireRead::LinkReset(_))),
            "{reads:?}"
        );
        assert!(
            !reads.iter().any(
                |read| matches!(read, WireRead::Frame(frame) if frame.message.as_ref().is_ok_and(|m| m.id == 11))
            ),
            "the lost request was answered: {reads:?}"
        );
        assert!(
            bench.board.requests.is_empty(),
            "the new session carried the old request: {:?}",
            bench.board.requests
        );
    }

    // ---- The update channel (M7 P7, DS9) -------------------------------

    /// A pre-update board (its hello carries no `firmware`) never hears
    /// channel 3: the message is refused with a note, and the link stays up
    /// and answering — no frame waits for an acknowledgement that would
    /// never come.
    #[test]
    fn nothing_goes_out_on_channel_3_to_a_board_that_did_not_announce_it() {
        let mut bench = Bench::new();
        bench.run(40);
        bench.host.take_reads();
        bench.host.take_notes();
        assert!(!bench.host.update_channel_announced());

        assert_eq!(bench.host.send_update(b"Q\x01"), Ok(false));
        bench.run(40);
        assert!(bench.board.updates.is_empty());
        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note == UPDATE_NOT_ANNOUNCED_NOTE),
            "{notes:?}"
        );

        // The link is not stalled behind it: a request still goes through.
        let json = lpc_wire::json::to_string(&ClientMessage {
            id: 5,
            msg: ClientRequest::StopAllProjects,
        })
        .unwrap();
        bench.host.send_client_json(&json).expect("queued");
        bench.run(40);
        assert_eq!(bench.board.requests, [5]);
        assert_eq!(bench.host.counters().resends, 0);
    }

    /// A split image's hello announces the channel: a message goes out on
    /// channel 3, and the board's answer comes back on the update queue —
    /// never among the reads a borrowed conversation drains.
    #[test]
    fn a_hello_with_firmware_announces_the_channel_and_updates_flow_both_ways() {
        let mut bench = Bench::with_board(BoardDouble::split(0xB0A2_0001, false));
        bench.run(40);
        assert!(bench.host.update_channel_announced());
        assert!(
            bench.host.take_updates().is_empty(),
            "a running board sends no M unasked"
        );
        bench.host.take_reads();

        assert_eq!(bench.host.send_update(b"Q\x01"), Ok(true));
        bench.run(40);
        assert_eq!(bench.board.updates, vec![b"Q\x01".to_vec()]);
        let updates = bench.host.take_updates();
        assert!(
            matches!(updates.as_slice(), [m] if m[0] == b'M'),
            "{updates:?}"
        );
        assert!(bench.host.take_reads().is_empty());
    }

    /// A core-only board says no hello: its own `M` on link-up is the
    /// announcement.
    #[test]
    fn a_core_only_boards_manifest_announces_the_channel() {
        let mut bench = Bench::with_board(BoardDouble::split(0xB0A2_0001, true));
        bench.run(40);
        assert!(bench.host.update_channel_announced());
        let updates = bench.host.take_updates();
        assert!(
            matches!(updates.as_slice(), [m] if m[0] == b'M'),
            "{updates:?}"
        );
        assert_eq!(bench.host.send_update(b"Q\x01"), Ok(true));
    }

    /// A board that restarts as a pre-update image (a USB flash of a single
    /// image, say) is not announced on the new session, and nothing the old
    /// session said survives the reset.
    #[test]
    fn a_reset_forgets_the_announcement_and_the_old_sessions_messages() {
        let mut bench = Bench::with_board(BoardDouble::split(0xB0A2_0001, true));
        bench.run(40);
        assert!(bench.host.update_channel_announced());

        bench.board = BoardDouble::new(0xB0A2_0002);
        bench.run(60);
        assert!(!bench.host.update_channel_announced());
        assert!(bench.host.take_updates().is_empty());
        assert_eq!(bench.host.send_update(b"Q\x01"), Ok(false));
    }

    /// The LAN link (a secure port on `ws()` datagrams) keeps the update
    /// session like every other link: a split image's hello announces
    /// channel 3, a message goes out and the board's answer comes back on
    /// the update queue (OTA over Wi-Fi rides this), even though the port is
    /// also drained by the secure handshake's own event poll.
    #[cfg(feature = "secure-link")]
    #[test]
    fn a_lan_link_announces_the_update_channel_and_updates_flow_both_ways() {
        let mut bench = SecureBench::new(BoardDouble::secure_split(0xB0A2_0001));
        bench.run(200);
        assert!(bench.host.is_up());
        assert!(bench.host.update_channel_announced());
        bench.host.take_reads();

        assert_eq!(bench.host.send_update(b"Q\x01"), Ok(true));
        bench.run(200);
        assert_eq!(bench.board.updates, vec![b"Q\x01".to_vec()]);
        let updates = bench.host.take_updates();
        assert!(
            matches!(updates.as_slice(), [m] if m[0] == b'M'),
            "{updates:?}"
        );
        assert!(bench.host.take_reads().is_empty());
    }

    /// A LAN link that rekeys (a password typed for a locked board) starts a
    /// new session: the old one's announcement and messages are forgotten,
    /// and the new session's hello announces again.
    #[cfg(feature = "secure-link")]
    #[test]
    fn a_lan_rekey_forgets_the_old_sessions_update_state() {
        let mut bench = SecureBench::new(BoardDouble::secure_split(0xB0A2_0001));
        bench.run(200);
        assert!(bench.host.update_channel_announced());

        // The board's next session says no `firmware`: a pre-update image.
        bench.board.manifest = None;
        bench.host.rekey(
            bench.now,
            &crate::providers::network_link::LinkKey::ANONYMOUS,
        );
        assert!(
            !bench.host.update_channel_announced(),
            "the rekey's reset forgets at once"
        );
        bench.run(200);
        assert!(bench.host.is_up());
        assert!(!bench.host.update_channel_announced());
        assert!(bench.host.take_updates().is_empty());
        assert_eq!(bench.host.send_update(b"Q\x01"), Ok(false));
    }

    #[test]
    fn the_wake_is_capped() {
        let bench = Bench::new();
        assert!(bench.host.wake_in(0, 10_000) <= 10_000);
    }

    // ---- Web Bluetooth: the same service on `LinkConfig::ble()` datagrams --
    //
    // The board end is sized the way the C6 sizes a radio link
    // (`fw-esp32-common/src/radio_link/radio_link_config.rs`): the `ble()`
    // preset with `max_payload` cut to the connection's ATT MTU. 174 B is an
    // iOS central's (MTU 185); the host never learns the MTU — the board's
    // SYN carries its payload size and the host's frames shrink to it.

    /// The board's `max_payload` at an iOS central's ATT MTU (185 − 11).
    const IOS_PAYLOAD: u16 = 174;

    #[test]
    fn a_bluetooth_link_comes_up_over_datagrams_and_the_hello_is_a_frame() {
        let mut bench = DatagramBench::new(IOS_PAYLOAD);
        bench.run(1_500);

        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.starts_with("link: up")),
            "{notes:?}"
        );
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.json.contains("\"hello\"") && !frame.packed
            ),
            "{reads:?}"
        );
        assert!(bench.host.is_up());
    }

    /// One frame per write: every frame the host hands the page fits the
    /// board's advertised payload (header and CRC on top), a request larger
    /// than many frames arrives whole, and so does its answer.
    #[test]
    fn a_bluetooth_request_goes_out_in_frames_the_board_can_take() {
        let mut bench = DatagramBench::new(IOS_PAYLOAD);
        bench.run(1_500);
        bench.host.take_reads();

        let json = big_write(9, 2_000);
        bench.host.send_client_json(&json).expect("queued");
        bench.run(3_000);

        assert_eq!(bench.board.requests, [9]);
        assert!(
            bench.largest_host_frame <= usize::from(IOS_PAYLOAD) + 8,
            "a host frame of {} B does not fit a {IOS_PAYLOAD} B payload",
            bench.largest_host_frame
        );
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.message.as_ref().is_ok_and(|m| m.id == 9)
            ),
            "{reads:?}"
        );
    }

    /// A notification that is not an intact frame is counted and dropped —
    /// never read as a message — and the board's resend delivers the reply.
    #[test]
    fn a_damaged_notification_is_counted_and_the_resend_delivers() {
        let mut bench = DatagramBench::new(IOS_PAYLOAD);
        bench.run(1_500);
        bench.host.take_reads();
        let bad_before = bench.host.counters().damaged;

        bench.damage_next_board_frame = true;
        let json = lpc_wire::json::to_string(&ClientMessage {
            id: 11,
            msg: ClientRequest::StopAllProjects,
        })
        .unwrap();
        bench.host.send_client_json(&json).expect("queued");
        bench.run(2_000);

        assert!(!bench.damage_next_board_frame, "a frame was damaged");
        assert_eq!(bench.host.counters().damaged, bad_before + 1);
        let reads = bench.host.take_reads();
        assert!(
            matches!(
                reads.as_slice(),
                [WireRead::Frame(frame)] if frame.message.as_ref().is_ok_and(|m| m.id == 11)
            ),
            "{reads:?}"
        );
    }

    /// Plan D9 over Bluetooth: a session that ends while the GATT connection
    /// stays up (the board's link gave up on a frame, or restarted its end) is
    /// a link reset the drainer hears at once, before the new hello.
    #[test]
    fn a_bluetooth_session_reset_is_a_link_reset_then_a_new_hello() {
        let mut bench = DatagramBench::new(IOS_PAYLOAD);
        bench.run(1_500);
        bench.host.take_reads();

        bench.board.link.restart(bench.now);
        bench.run(3_000);

        let reads = bench.host.take_reads();
        let reset_at = reads
            .iter()
            .position(|read| matches!(read, WireRead::LinkReset(_)))
            .unwrap_or_else(|| panic!("no reset read: {reads:?}"));
        let hello_at = reads
            .iter()
            .position(
                |read| matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\"")),
            )
            .unwrap_or_else(|| panic!("no new hello: {reads:?}"));
        assert!(reset_at < hello_at, "{reads:?}");
    }

    /// A GATT write is awaited, so the page takes a frame only when it has
    /// room; what it cannot take waits in the link, not in the page.
    #[test]
    fn transmit_up_to_hands_over_no_more_than_the_room() {
        let mut bench = DatagramBench::new(IOS_PAYLOAD);
        bench.run(1_500);
        bench.host.take_reads();

        let json = big_write(12, 1_000);
        bench.host.send_client_json(&json).expect("queued");
        let mut handed = Vec::new();
        let written = bench
            .host
            .transmit_up_to(bench.now, 1, |frame| handed.push(frame.to_vec()));
        assert_eq!((written, handed.len()), (1, 1));
        let more = bench.host.transmit_up_to(bench.now, 8, |_| {});
        assert!(more > 1, "the rest was still in the link: {more}");
    }

    /// Bluetooth's own stall time (`ble()`'s 3.5 s, not USB's 1 s): a quiet
    /// connection interval or two is not a stall, three and a half silent
    /// seconds is, and the board's return is news.
    #[test]
    fn a_silent_bluetooth_board_is_a_stall_only_past_its_stall_time() {
        let mut bench = DatagramBench::new(IOS_PAYLOAD);
        bench.run(1_500);
        bench.host.take_notes();

        let mut stalled_at = None;
        for ms in 0..5_000 {
            bench.host.transmit(bench.now, |_| {});
            if stalled_at.is_none() && bench.host.is_stalled(bench.now) {
                stalled_at = Some(ms);
            }
            bench.now += 1_000;
        }
        let stalled_at = stalled_at.expect("a stall");
        assert!(stalled_at >= 3_000, "stalled after only {stalled_at} ms");
        bench.run(1_500);
        let notes = bench.host.take_notes();
        assert!(
            notes.iter().any(|note| note.contains("stalled")),
            "{notes:?}"
        );
        assert!(
            notes.iter().any(|note| note.contains("answering again")),
            "{notes:?}"
        );
    }

    /// Both ends, a millisecond at a time.
    struct Bench {
        host: LinkPortService,
        board: BoardDouble,
        now: Micros,
    }

    impl Bench {
        fn new() -> Self {
            Self::with_board(BoardDouble::new(0xB0A2_0001))
        }

        fn with_board(board: BoardDouble) -> Self {
            Self {
                host: LinkPortService::new(LinkConfig::usb(), 0xAAAA_0001, false, None),
                board,
                now: 0,
            }
        }

        /// A classic behind a CH340: the port's `uart()` host, the board's own
        /// cut ([`classic_board`]).
        fn classic() -> Self {
            Self {
                host: LinkPortService::new(LinkConfig::uart(), 0xAAAA_0002, false, None),
                board: BoardDouble::with_config(classic_board(), 0xB0A2_0001),
                now: 0,
            }
        }

        fn run(&mut self, steps: u32) {
            for _ in 0..steps {
                let board = &mut self.board.link;
                let now = self.now;
                self.host.transmit(now, |frame| board.on_bytes(now, frame));
                self.board.serve();
                let mut out = Vec::new();
                while let Some(frame) = self.board.link.poll_transmit(now) {
                    out.extend_from_slice(frame);
                }
                self.host.on_bytes(now, &out);
                self.now += 1_000;
            }
        }
    }

    /// Both ends over datagrams (one frame per GATT write or notification),
    /// a millisecond at a time, with the page's write room modelled as
    /// Bluetooth's: two frames queued, the board's side unbounded.
    struct DatagramBench {
        host: LinkPortService,
        board: BoardDouble,
        now: Micros,
        largest_host_frame: usize,
        damage_next_board_frame: bool,
    }

    impl DatagramBench {
        fn new(board_payload: u16) -> Self {
            let board = LinkConfig {
                max_payload: board_payload,
                ..LinkConfig::ble()
            };
            Self {
                host: LinkPortService::new(LinkConfig::ble(), 0xAAAA_0001, false, None),
                board: BoardDouble::with_config(board, 0xB0A2_0001),
                now: 0,
                largest_host_frame: 0,
                damage_next_board_frame: false,
            }
        }

        fn run(&mut self, steps: u32) {
            for _ in 0..steps {
                let now = self.now;
                let mut sent = Vec::new();
                self.host
                    .transmit_up_to(now, 2, |frame| sent.push(frame.to_vec()));
                for frame in sent {
                    self.largest_host_frame = self.largest_host_frame.max(frame.len());
                    self.board.link.on_datagram(now, &frame);
                }
                self.board.serve();
                let mut out = Vec::new();
                while let Some(frame) = self.board.link.poll_transmit(now) {
                    out.push(frame.to_vec());
                }
                for mut frame in out {
                    // Damage a data frame (one long enough to carry the
                    // reply), never a bare acknowledgement.
                    if self.damage_next_board_frame && frame.len() > 40 {
                        self.damage_next_board_frame = false;
                        let middle = frame.len() / 2;
                        frame[middle] ^= 0x5A;
                    }
                    self.host.on_datagram(now, &frame);
                }
                self.now += 1_000;
            }
        }
    }

    /// The LAN link: a secure initiator on `ws()` datagrams (one frame per
    /// WebSocket message) against a secure responder, the way the C6's LAN
    /// endpoint builds its slot, a millisecond at a time. The host polls its
    /// handshake events every step, as the WebSocket provider does.
    #[cfg(feature = "secure-link")]
    struct SecureBench {
        host: LinkPortService,
        board: BoardDouble,
        now: Micros,
    }

    #[cfg(feature = "secure-link")]
    impl SecureBench {
        fn new(board: BoardDouble) -> Self {
            Self {
                host: LinkPortService::new_secure(
                    LinkConfig::ws(),
                    0xAAAA_0003,
                    false,
                    None,
                    &crate::providers::network_link::LinkKey::ANONYMOUS,
                    test_entropy,
                ),
                board,
                now: 0,
            }
        }

        fn run(&mut self, steps: u32) {
            for _ in 0..steps {
                let now = self.now;
                let mut sent = Vec::new();
                self.host.transmit(now, |frame| sent.push(frame.to_vec()));
                for frame in sent {
                    self.board.link.on_datagram(now, &frame);
                }
                self.board.answer_key_lookups();
                self.board.serve();
                let mut out = Vec::new();
                while let Some(frame) = self.board.link.poll_transmit(now) {
                    out.push(frame.to_vec());
                }
                for frame in out {
                    self.host.on_datagram(now, &frame);
                }
                while self.host.poll_secure_event().is_some() {}
                self.now += 1_000;
            }
        }
    }

    #[cfg(feature = "secure-link")]
    fn test_entropy(buf: &mut [u8]) {
        thread_local! {
            static NEXT: std::cell::Cell<u8> = const { std::cell::Cell::new(1) };
        }
        NEXT.with(|next| {
            for byte in buf.iter_mut() {
                *byte = next.get();
                next.set(next.get().wrapping_add(1));
            }
        });
    }

    /// A request carrying `bytes` of file data: many frames' worth.
    fn big_write(id: u64, bytes: usize) -> String {
        lpc_wire::json::to_string(&ClientMessage {
            id,
            msg: ClientRequest::Filesystem(lpc_wire::server::FsRequest::Write {
                path: "/big.txt".into(),
                data: vec![b'z'; bytes],
            }),
        })
        .unwrap()
    }

    /// A board's end: hello on every `Up`, and a reply to every request.
    struct BoardDouble {
        link: Link<SelectiveRepeat>,
        requests: Vec<u64>,
        /// What the board says about its firmware: `None` is a pre-update
        /// board (no channel 3); `Some` a split image, whose hello carries
        /// it unless [`Self::core_only`].
        manifest: Option<BoardManifest>,
        /// Core-only: no hello, an `M` on every link-up instead.
        core_only: bool,
        /// Channel-3 messages the board heard.
        updates: Vec<Vec<u8>>,
    }

    impl BoardDouble {
        fn new(nonce: u32) -> Self {
            Self::with_config(LinkConfig::usb(), nonce)
        }

        fn with_config(config: LinkConfig, nonce: u32) -> Self {
            Self {
                link: Link::new(config, nonce),
                requests: Vec::new(),
                manifest: None,
                core_only: false,
                updates: Vec::new(),
            }
        }

        /// A split image that speaks channel 3.
        fn split(nonce: u32, core_only: bool) -> Self {
            Self {
                manifest: Some(board_manifest()),
                core_only,
                ..Self::new(nonce)
            }
        }

        /// A split image's LAN slot: a secure responder on `ws()` that
        /// admits the anonymous key ([`Self::answer_key_lookups`]).
        #[cfg(feature = "secure-link")]
        fn secure_split(nonce: u32) -> Self {
            use lpc_wire::lp_link::secure_channel::SecureRole;
            Self {
                link: Link::new_secure(
                    LinkConfig::ws(),
                    nonce,
                    SecureRole::Responder,
                    test_entropy,
                ),
                manifest: Some(board_manifest()),
                ..Self::new(nonce)
            }
        }

        /// Answer a secure handshake's key lookups: the anonymous key only.
        #[cfg(feature = "secure-link")]
        fn answer_key_lookups(&mut self) {
            use lpc_wire::lp_link::secure_channel::{Psk, RefusalReason, SecureEvent};
            while let Some(event) = self.link.poll_secure_event() {
                if let SecureEvent::KeyLookup { key_id } = event {
                    if key_id.is_anonymous() {
                        self.link.provide_keys(key_id, &[Psk::ANONYMOUS]);
                    } else {
                        self.link.refuse(key_id, RefusalReason::UnknownKey, 0);
                    }
                }
            }
        }

        fn serve(&mut self) {
            while let Some(event) = self.link.recv() {
                match event {
                    LinkEvent::Up { .. } if self.core_only => self.send_manifest(),
                    LinkEvent::Up { .. } => {
                        let mut message = hello();
                        if let ServerMsgBody::Hello(hello) = &mut message.msg {
                            hello.firmware = self.manifest.clone();
                        }
                        self.send(&message);
                    }
                    LinkEvent::Message {
                        channel: CH_UPDATE,
                        data,
                    } => {
                        self.updates.push(data);
                        self.send_manifest();
                    }
                    LinkEvent::Message {
                        channel: CH_PROTO,
                        data,
                    } => {
                        let request = lpc_wire::decode_client_payload(&data).expect("a request");
                        self.requests.push(request.id);
                        self.send(&WireServerMessage::new(
                            request.id,
                            ServerMsgBody::StopAllProjects,
                        ));
                    }
                    _ => {}
                }
            }
        }

        fn send(&mut self, message: &WireServerMessage) {
            let mut payload = Vec::new();
            lpc_wire::encode_server_payload(message, None, &mut payload);
            self.link.send(CH_PROTO, &payload).expect("board send");
        }

        /// `M`: the board's manifest on channel 3.
        fn send_manifest(&mut self) {
            let Some(manifest) = &self.manifest else {
                return;
            };
            let mut m = vec![b'M'];
            m.extend_from_slice(&manifest.to_json());
            self.link.send(CH_UPDATE, &m).expect("board send");
        }
    }

    fn board_manifest() -> BoardManifest {
        BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".to_string(),
            chip: "esp32c6".to_string(),
            version: "2026.10.06-1".to_string(),
            build_id: "2026.10.06-1+abc123456789".to_string(),
            wire_proto: WIRE_PROTO_VERSION,
            core_sha256: "11".repeat(32),
            core_len: 4096,
            engine_sha256: "22".repeat(32),
            engine_len: Some(8192),
            layout: 1,
            loader: 1,
            region_len: 65_536,
            state: lpc_update::BoardState::Running,
            refused_build: None,
            transfer: None,
        }
    }

    /// The classic board's link timings on the `uart()` preset — what
    /// `fw_esp32_common::uart_link::uart_board_link_config` sets (a 200 ms
    /// resend floor, SYNs backed off to 1.6 s), which this crate cannot
    /// depend on. The same double as the fake board's classic test.
    fn classic_board() -> LinkConfig {
        LinkConfig {
            min_rto: 200_000,
            syn_backoff: 4,
            ..LinkConfig::uart()
        }
    }

    fn hello() -> WireServerMessage {
        WireServerMessage::new(
            0,
            ServerMsgBody::Hello(ServerHello {
                proto: WIRE_PROTO_VERSION,
                build: BuildFacts {
                    features: vec![],
                    package: "fw-esp32c6".to_string(),
                    version: "unknown".into(),
                    commit: "unknown".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: HardwareFacts::default(),
                device_uid: None,
                pack_format: PACK_FORMAT_VERSION,
                auth: lpc_wire::HelloAuth::TRUSTED,
                firmware: None,
            }),
        )
    }
}
