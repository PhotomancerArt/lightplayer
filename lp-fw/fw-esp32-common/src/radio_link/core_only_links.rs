//! Core-only's radio and LAN links: the port's one reader while no engine
//! runs, feeding the board's update session (`lpc-update`).
//!
//! In core-only there is no server and no link mux, so this takes the
//! port's `Opened`/`Closed` notices and each open link's events itself, and
//! hands the session what it needs ([`CoreOnlySession`], the chip's update
//! edge on a board):
//!
//! - **A Bluetooth link** comes up [`LinkTrust::Untrusted`]; the session's
//!   own login (`L` over channel 3) is how it earns a tier.
//! - **A LAN link** (feature `wifi`) is a secure lp-link responder. Its
//!   handshake's key lookup is answered here by the session itself
//!   ([`CoreOnlySession::key_lookup`]: the engine's rule, from the device
//!   store's secrets, one backoff with `L`), and the key that verifies
//!   brings the link up [`LinkTrust::Keyed`] at its tier — or untrusted on
//!   the anonymous key, where the device's `open` decides. Its key is its
//!   login: the session refuses `L` on it.
//! - **A relayed link** (a cloud relay route) and a **challenger** for the
//!   held network slot are turned away: updates through the relay are their
//!   own change.
//!
//! Channel 3 goes to the session; channel 1 (the wire) has no server to
//! answer it here, and is dropped. A session that resets is the link going
//! down; its next `Up` (a new handshake, so a new key lookup) brings it up
//! again with whatever its key grants then.
//!
//! Nothing here logs a key, a salt or a PSK.

use alloc::vec::Vec;

use lp_link::{CH_UPDATE, LinkEvent};
use lpc_shared::transport::LinkId as PortLinkId;
use lpc_update::board::{LinkId, LinkTrust};
#[cfg(feature = "wifi")]
use lpc_update::board::CoreKeyAnswer;

use super::radio_link_port::{RADIO_LINK_SLOTS, RadioLinkEvent, RadioLinkPort};

/// What core-only's links need from the update session. The board's update
/// edge implements it over its `BoardSession` (reading the clock itself);
/// tests implement it over the update crate's model board.
pub trait CoreOnlySession {
    /// `link`'s session came up, trusted as `trust`.
    fn link_up(&mut self, link: LinkId, trust: LinkTrust);
    /// `link`'s session ended, or the link is gone.
    fn link_down(&mut self, link: LinkId);
    /// One channel-3 message from `link`.
    fn on_message(&mut self, link: LinkId, bytes: &[u8]);
    /// `link`'s handshake named the entry with `salt`.
    #[cfg(feature = "wifi")]
    fn key_lookup(&mut self, link: LinkId, salt: &[u8; 16]) -> CoreKeyAnswer;
    /// `link`'s handshake matched no candidate of a known salt.
    #[cfg(feature = "wifi")]
    fn key_wrong(&mut self, link: LinkId);
    /// `link` came up on `candidate` of its lookup: how to trust it.
    #[cfg(feature = "wifi")]
    fn key_authenticated(&mut self, link: LinkId, candidate: u8) -> LinkTrust;
}

/// A port link as the session names it.
#[must_use]
pub fn session_link(link: PortLinkId) -> LinkId {
    LinkId(link.raw())
}

/// One link core-only serves.
struct OpenLink {
    id: PortLinkId,
    slot: usize,
    /// A secure LAN link: its handshake asks for keys, and its key decides
    /// its trust.
    #[cfg(feature = "wifi")]
    keyed: bool,
}

/// See the module docs.
pub struct CoreOnlyLinks {
    port: &'static RadioLinkPort,
    open: Vec<OpenLink>,
}

impl CoreOnlyLinks {
    #[must_use]
    pub fn new(port: &'static RadioLinkPort) -> Self {
        Self {
            port,
            open: Vec::new(),
        }
    }

    /// Links open now.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.open.len()
    }

    /// Take the port's notices and every open link's events into `session`.
    /// Whether anything happened.
    pub fn pump(&mut self, session: &mut impl CoreOnlySession) -> bool {
        let mut touched = self.take_notices(session);
        for link in &self.open {
            touched |= pump_link(self.port, link, session);
        }
        touched
    }

    fn take_notices(&mut self, session: &mut impl CoreOnlySession) -> bool {
        let mut touched = false;
        while let Some(event) = self.port.try_event() {
            touched = true;
            match event {
                RadioLinkEvent::Opened { link, slot } => self.opened(link, slot),
                RadioLinkEvent::Challenged { link, slot } => {
                    // One network session: a newcomer while it is held is
                    // turned away (the holder is updating).
                    log::info!("[OTA] core-only: network link {link} refused (the slot is held)");
                    #[cfg(feature = "wifi")]
                    self.port.slot(slot).refuse_challenge(link);
                    #[cfg(not(feature = "wifi"))]
                    let _ = slot;
                }
                RadioLinkEvent::Closed { link } => {
                    if let Some(at) = self.open.iter().position(|l| l.id == link) {
                        self.open.remove(at);
                        session.link_down(session_link(link));
                        log::info!("[OTA] core-only: link {link} closed");
                    }
                }
            }
        }
        touched
    }

    fn opened(&mut self, link: PortLinkId, slot: usize) {
        let port = self.port;
        if slot < RADIO_LINK_SLOTS {
            log::info!("[OTA] core-only: radio link {link} opened (slot {slot})");
            self.open.push(OpenLink {
                id: link,
                slot,
                #[cfg(feature = "wifi")]
                keyed: false,
            });
            return;
        }
        match port.link_on(slot, link).trust {
            #[cfg(feature = "wifi")]
            lpc_shared::transport::LinkTrust::Keyed => {
                log::info!("[OTA] core-only: LAN link {link} opened (slot {slot})");
                self.open.push(OpenLink {
                    id: link,
                    slot,
                    keyed: true,
                });
            }
            _ => {
                log::info!("[OTA] core-only: relayed link {link} refused (slot {slot})");
                port.slot(slot)
                    .revoke(link, "core-only serves no relayed link yet");
            }
        }
    }
}

/// One open link's handshake and events into `session`; whether anything
/// happened.
fn pump_link(
    port: &'static RadioLinkPort,
    link: &OpenLink,
    session: &mut impl CoreOnlySession,
) -> bool {
    let slot = port.slot(link.slot);
    let id = session_link(link.id);
    let mut touched = false;
    #[cfg(feature = "wifi")]
    if link.keyed {
        touched |= answer_keys(slot, link.id, session);
    }
    while let Some(event) = slot.recv(link.id) {
        touched = true;
        match event {
            LinkEvent::Up { .. } => {
                let trust = trust_at_up(slot, link, session);
                log::info!(
                    "[OTA] core-only: link {} up ({})",
                    link.id,
                    trust_word(trust)
                );
                session.link_up(id, trust);
            }
            LinkEvent::Reset { .. } => session.link_down(id),
            LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                session.on_message(id, &data);
            }
            // Channel 1 (the wire) has no server to answer it here.
            _ => {}
        }
    }
    touched
}

/// Answer `id`'s key lookups from the session, and charge its wrong keys.
#[cfg(feature = "wifi")]
fn answer_keys(
    slot: &super::RadioLinkSlot,
    id: PortLinkId,
    session: &mut impl CoreOnlySession,
) -> bool {
    use lp_link::secure_channel::SecureEvent;
    use lpc_shared::transport::KeyAnswer;

    let mut touched = false;
    while let Some(event) = slot.poll_key_event(id) {
        touched = true;
        match event {
            SecureEvent::KeyLookup { key_id } => {
                let answer = match session.key_lookup(session_link(id), &key_id.0) {
                    CoreKeyAnswer::Keys(psks) => KeyAnswer::Keys(psks),
                    CoreKeyAnswer::Unknown => {
                        log::info!("[OTA] core-only: link {id} named a key this board does not hold");
                        KeyAnswer::Unknown
                    }
                    CoreKeyAnswer::Backoff { retry_after_ms } => {
                        log::info!(
                            "[OTA] core-only: link {id} refused — too many wrong keys, \
                             {retry_after_ms} ms to wait"
                        );
                        KeyAnswer::Backoff { retry_after_ms }
                    }
                };
                slot.answer_key(id, key_id, answer);
            }
            SecureEvent::WrongKey { .. } => {
                log::warn!("[OTA] core-only: link {id} had a wrong key");
                session.key_wrong(session_link(id));
            }
            SecureEvent::Refused { .. } | SecureEvent::PeerNotSecure => {}
        }
    }
    touched
}

/// How `link` is trusted as its session comes up.
fn trust_at_up(
    slot: &super::RadioLinkSlot,
    link: &OpenLink,
    session: &mut impl CoreOnlySession,
) -> LinkTrust {
    #[cfg(feature = "wifi")]
    if link.keyed {
        return match slot.session_auth(link.id) {
            Some(auth) => session.key_authenticated(session_link(link.id), auth.candidate),
            None => LinkTrust::Untrusted,
        };
    }
    let _ = (slot, link, session);
    LinkTrust::Untrusted
}

/// A trust in words, for the log line (never `{:?}`: the C6 prints a Debug
/// as nothing).
fn trust_word(trust: LinkTrust) -> &'static str {
    match trust {
        LinkTrust::Trusted => "trusted",
        LinkTrust::Untrusted => "untrusted",
        LinkTrust::Keyed(lpc_access::Tier::Play) => "key at play",
        LinkTrust::Keyed(lpc_access::Tier::Edit) => "key at edit",
    }
}

#[cfg(all(test, feature = "wifi"))]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::collections::VecDeque;
    use alloc::vec;

    use lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole};
    use lp_link::{Link, LinkConfig, SelectiveRepeat};
    use lpc_access::{OpenTo, SecretEntry, Tier, link_psk};
    use lpc_shared::transport::LinkTrust as PortTrust;
    use lpc_update::board::{AccessFacts, Outgoing, SessionConfig};
    use lpc_update::code_table::{CHIP_ESP32C6, LAYOUT_1, LOADER_1, PROTO_V1};
    use lpc_update::testing::{BoardRig, FakeBoard, MODEL_REGION_START, ModelBuild};
    use lpc_update::{BoardMessage, CHUNK, HostMessage, Offer, Refusal};

    use super::super::{RadioLinkMode, SlotEdge};
    use crate::serial::server_msg::frame_buf_turn;
    use crate::update_send::UpdateSend;

    extern crate std;

    const PLAY_SALT: u8 = 1;
    const EDIT_SALT: u8 = 2;

    /// A play key comes up keyed at play: the board sends its manifest on
    /// up, answers `Q`, and refuses a core install; an edit key may install
    /// one (the board asks for its first chunk).
    #[test]
    fn a_lan_link_comes_up_with_its_keys_tier_and_the_tier_decides_a_core_install() {
        let _turn = frame_buf_turn();
        let (x, y) = builds();
        let mut board = Board::engineless(OpenTo::Nobody, &x, &y);

        let mut play = board.connect(&key(Tier::Play, PLAY_SALT));
        board.settle(&mut play);
        assert_eq!(
            board.session.ups,
            vec![(session_link(play.id), LinkTrust::Keyed(Tier::Play))]
        );
        assert_eq!(play.next_board_message().unwrap()[0], b'M', "M on up");
        play.send(&HostMessage::Query { proto: 1 }.encode());
        board.settle(&mut play);
        assert_eq!(play.next_board_message().unwrap()[0], b'M');
        play.send(&offer_of(&y).encode());
        board.settle(&mut play);
        assert_eq!(play.refusal(), Some(Refusal::Access));
        board.disconnect(&play);

        let mut edit = board.connect(&key(Tier::Edit, EDIT_SALT));
        board.settle(&mut edit);
        assert_eq!(
            board.session.ups.last(),
            Some(&(session_link(edit.id), LinkTrust::Keyed(Tier::Edit)))
        );
        let _manifest = edit.next_board_message();
        edit.send(&offer_of(&y).encode());
        board.settle(&mut edit);
        let answer = edit.next_board_message().expect("an answer");
        assert!(
            matches!(BoardMessage::decode(&answer), Ok(BoardMessage::Request(_))),
            "an edit key's offer is taken: the board asks for chunks"
        );
    }

    /// On a board open at edit the anonymous key comes up untrusted and
    /// `open` lets it install; on one open to nobody it is refused.
    #[test]
    fn the_anonymous_key_holds_what_open_gives() {
        let _turn = frame_buf_turn();
        let (x, y) = builds();
        for (open, refused) in [(OpenTo::Edit, false), (OpenTo::Nobody, true)] {
            let mut board = Board::engineless(open, &x, &y);
            let mut anyone = board.connect(&anonymous());
            board.settle(&mut anyone);
            assert_eq!(
                board.session.ups,
                vec![(session_link(anyone.id), LinkTrust::Untrusted)]
            );
            let _manifest = anyone.next_board_message();
            anyone.send(&offer_of(&y).encode());
            board.settle(&mut anyone);
            assert_eq!(
                anyone.refusal() == Some(Refusal::Access),
                refused,
                "{open:?}"
            );
        }
    }

    /// Wrong keys put the board in backoff, and in backoff the next
    /// handshake is refused with its wait — before anything is read.
    #[test]
    fn wrong_keys_put_the_board_in_backoff_and_the_next_handshake_is_refused() {
        let _turn = frame_buf_turn();
        let (x, y) = builds();
        let mut board = Board::engineless(OpenTo::Nobody, &x, &y);
        let mut wrong = key(Tier::Edit, EDIT_SALT);
        wrong.k = [0xee; 32];
        for _ in 0..4 {
            let mut guess = board.connect(&wrong);
            board.settle(&mut guess);
            assert!(board.session.ups.is_empty(), "a wrong key never comes up");
            board.disconnect(&guess);
        }
        let mut right = board.connect(&key(Tier::Edit, EDIT_SALT));
        board.settle(&mut right);
        assert!(board.session.ups.is_empty());
        let refused = core::iter::from_fn(|| right.link.poll_secure_event()).find_map(|e| match e {
            SecureEvent::Refused {
                reason,
                retry_after_ms,
            } => Some((reason, retry_after_ms)),
            _ => None,
        });
        let Some((RefusalReason::Backoff, wait)) = refused else {
            panic!("refused for backoff: {refused:?}");
        };
        assert!(wait > 0);
    }

    /// A relayed link is turned away in core-only (updates through the
    /// relay are their own change).
    #[test]
    fn a_relayed_link_is_turned_away() {
        let _turn = frame_buf_turn();
        let (x, y) = builds();
        let mut board = Board::engineless(OpenTo::Edit, &x, &y);
        let index = RADIO_LINK_SLOTS;
        let id = board.port.mint_link();
        board
            .port
            .slot(index)
            .open_network(id, 1, fill, PortTrust::Relayed, SlotEdge::Relay)
            .unwrap();
        block(board.port.announce(RadioLinkEvent::Opened {
            link: id,
            slot: index,
        }));
        board.links.pump(&mut board.session);
        assert_eq!(board.links.open_count(), 0);
        assert_eq!(board.port.slot(index).link_id(), None, "revoked");
    }

    // ---- helpers ----

    /// The update session on the model board, recording what the links
    /// told it.
    struct RigSession {
        rig: BoardRig,
        now: u64,
        ups: Vec<(LinkId, LinkTrust)>,
        out: VecDeque<Outgoing>,
    }

    impl CoreOnlySession for RigSession {
        fn link_up(&mut self, link: LinkId, trust: LinkTrust) {
            self.now += 1;
            self.ups.push((link, trust));
            let out = self.rig.link_up(self.now, link, trust);
            self.out.extend(out);
        }

        fn link_down(&mut self, link: LinkId) {
            self.now += 1;
            self.rig.link_down(self.now, link);
        }

        fn on_message(&mut self, link: LinkId, bytes: &[u8]) {
            self.now += 1;
            let out = self.rig.deliver(self.now, link, None, bytes);
            self.out.extend(out);
        }

        fn key_lookup(&mut self, link: LinkId, salt: &[u8; 16]) -> CoreKeyAnswer {
            self.now += 1;
            let now = self.now;
            self.rig
                .session
                .as_mut()
                .unwrap()
                .key_lookup(now, link, salt)
        }

        fn key_wrong(&mut self, link: LinkId) {
            self.now += 1;
            let now = self.now;
            self.rig.session.as_mut().unwrap().key_wrong(now, link);
        }

        fn key_authenticated(&mut self, link: LinkId, candidate: u8) -> LinkTrust {
            self.rig
                .session
                .as_mut()
                .unwrap()
                .key_authenticated(link, candidate)
        }
    }

    /// A board in core-only: its port (update mode), its links and its
    /// session.
    struct Board {
        port: &'static RadioLinkPort,
        links: CoreOnlyLinks,
        session: RigSession,
    }

    impl Board {
        fn engineless(open: OpenTo, x: &ModelBuild, y: &ModelBuild) -> Self {
            let mut flash = FakeBoard::flashed_with(vec![x.clone(), y.clone()], 0, 40 * 4096);
            let engine = (MODEL_REGION_START + x.core.len() as u32).div_ceil(CHUNK) * CHUNK;
            flash.flash.flash_image(engine, &[0xFF; CHUNK as usize]);
            let access = AccessFacts {
                secrets: vec![key(Tier::Play, PLAY_SALT), key(Tier::Edit, EDIT_SALT)],
                open,
                core_install_follows_open_to: true,
            };
            let rig = BoardRig::new(
                flash,
                access,
                SessionConfig {
                    entropy: Some(fill),
                    ..SessionConfig::default()
                },
            )
            .unwrap();
            let port: &'static RadioLinkPort = Box::leak(Box::new(RadioLinkPort::new()));
            port.decide_mode(RadioLinkMode::Update);
            Self {
                port,
                links: CoreOnlyLinks::new(port),
                session: RigSession {
                    rig,
                    now: 0,
                    ups: Vec::new(),
                    out: VecDeque::new(),
                },
            }
        }

        /// What the LAN endpoint does with a new socket, and the client's
        /// end holding `key`.
        fn connect(&mut self, key: &SecretEntry) -> Client {
            let index = RADIO_LINK_SLOTS;
            let id = self.port.mint_link();
            self.port
                .slot(index)
                .open_network(id, 0x5eed, fill, PortTrust::Keyed, SlotEdge::Local)
                .expect("the slot is free");
            block(self.port.announce(RadioLinkEvent::Opened {
                link: id,
                slot: index,
            }));
            Client::new(id, key)
        }

        fn disconnect(&mut self, client: &Client) {
            self.port.slot(RADIO_LINK_SLOTS).close_link(client.id);
            block(self.port.announce(RadioLinkEvent::Closed { link: client.id }));
            self.links.pump(&mut self.session);
        }

        /// Move frames both ways and run core-only until quiet.
        fn settle(&mut self, client: &mut Client) {
            let slot = self.port.slot(RADIO_LINK_SLOTS);
            let mut quiet = 0;
            for _ in 0..2_000 {
                client.now += 5_000;
                let t = client.now;
                let mut moved = false;
                while let Ok(Some(frame)) = slot.poll_frame_for(client.id, t, <[u8]>::to_vec) {
                    client.link.on_datagram(t, &frame);
                    moved = true;
                }
                while let Some(frame) = client.link.poll_transmit(t).map(<[u8]>::to_vec) {
                    let _ = slot.on_datagram_for(client.id, t, &frame);
                    moved = true;
                }
                moved |= self.links.pump(&mut self.session);
                while let Some(out) = self.session.out.front() {
                    let link = lpc_shared::transport::LinkId::new(out.link.0);
                    if self.port.send_update(link, &out.bytes) == UpdateSend::Later {
                        break;
                    }
                    self.session.out.pop_front();
                    moved = true;
                }
                while let Some(event) = client.link.recv() {
                    client.events.push_back(event);
                }
                quiet = if moved { 0 } else { quiet + 1 };
                if quiet >= 20 {
                    break;
                }
            }
        }
    }

    /// A host on the board's LAN endpoint: a secure initiator at `ws()`.
    struct Client {
        id: lpc_shared::transport::LinkId,
        link: Link<SelectiveRepeat>,
        events: VecDeque<LinkEvent>,
        now: u64,
    }

    impl Client {
        fn new(id: lpc_shared::transport::LinkId, key: &SecretEntry) -> Self {
            let (key_id, psk) = if key.salt == [0; 16] {
                (KeyId::ANONYMOUS, Psk::ANONYMOUS)
            } else {
                (KeyId(key.salt), Psk::new(link_psk(&key.k)))
            };
            Self {
                id,
                link: Link::new_secure(
                    LinkConfig::ws(),
                    0x0c11_e470,
                    SecureRole::Initiator { key_id, psk },
                    fill,
                ),
                events: VecDeque::new(),
                now: 1_000_000_000,
            }
        }

        fn send(&mut self, bytes: &[u8]) {
            self.link.send(CH_UPDATE, bytes).unwrap();
        }

        fn next_board_message(&mut self) -> Option<Vec<u8>> {
            while let Some(event) = self.events.pop_front() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == CH_UPDATE
                {
                    return Some(data);
                }
            }
            None
        }

        fn refusal(&mut self) -> Option<Refusal> {
            let bytes = self.next_board_message()?;
            match BoardMessage::decode(&bytes) {
                Ok(BoardMessage::Refusal(r)) => Some(r),
                _ => None,
            }
        }
    }

    fn builds() -> (ModelBuild, ModelBuild) {
        (
            ModelBuild::synthetic("2026.10.05-1", 1, 5 * 4096 + 300, 8 * 4096 + 77),
            ModelBuild::synthetic("2026.10.06-1", 2, 6 * 4096 + 11, 9 * 4096 + 1000),
        )
    }

    fn offer_of(build: &ModelBuild) -> Offer {
        Offer {
            proto: PROTO_V1,
            flags: 0,
            chip: CHIP_ESP32C6,
            layout: LAYOUT_1,
            min_loader: LOADER_1,
            core_len: build.core.len() as u32,
            engine_len: build.engine.len() as u32,
            core_sha256: build.core_sha256(),
            engine_sha256: build.engine_sha256(),
            build_id: build.build_id_field(),
        }
    }

    fn key(tier: Tier, salt: u8) -> SecretEntry {
        SecretEntry {
            label: alloc::string::String::from("test key"),
            kind: lpc_access::SecretKind::Browser,
            tier,
            salt: [salt; 16],
            iterations: 1,
            k: [salt.wrapping_add(100); 32],
            added_at: None,
        }
    }

    fn anonymous() -> SecretEntry {
        key(Tier::Play, 0)
    }

    fn fill(buf: &mut [u8]) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(31).wrapping_add(7);
        }
    }

    fn block<F: core::future::Future>(future: F) -> F::Output {
        embassy_futures::block_on(future)
    }
}
