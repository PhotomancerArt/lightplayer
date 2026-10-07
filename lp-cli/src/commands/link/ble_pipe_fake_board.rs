//! A board double for the Bluetooth pipe's tests: the board's end of one
//! radio link (a real `lp_link::Link` on `LinkConfig::ble()`, Datagram
//! framing, one frame per GATT write and per notification), answering the
//! way the product's two modes do:
//!
//! - **an engine** says hello on channel 1 when its link comes up, runs the
//!   real login machine (`lpc_access::LoginState`) for `LoginBegin` /
//!   `LoginAnswer`, and answers `Q` on channel 3 with its manifest;
//! - **core-only** says `M` on channel 3 unprompted when its link comes up,
//!   and answers `Q` the same way.
//!
//! It never updates anything: the update itself is `lpc-update`'s and
//! `lpa-update`'s, tested there. What it checks is the pipe's half — what
//! goes on which channel, in which order, and in frames that fit.
//!
//! [`FakeBoard::over_usb`] is the same board on `LinkConfig::usb()`'s byte
//! stream, for the serial capture that shares the pipe's session above the
//! link ([`super::capture_session`]).

use lp_link::{CH_PROTO, CH_UPDATE, Link, LinkConfig, LinkEvent, SelectiveRepeat};
use lpc_access::{BeginOutcome, LoginOutcome, LoginState, SecretEntry, Tier};
use lpc_update::{BoardManifest, BoardState, sha256_to_hex};
use lpc_wire::server::{BuildFacts, HardwareFacts, HelloAuth, ServerHello};
use lpc_wire::{
    ClientRequest, WIRE_PROTO_VERSION, WireServerMessage, WireServerMsgBody, decode_client_payload,
    encode_server_payload,
};

use crate::commands::emu::link_host::fresh_nonce;
use crate::commands::firmware::ota_fixture::Fixture;

/// What runs on the board.
pub enum FakeMode {
    /// An engine whose access file holds `password` at edit (or nothing:
    /// every login refused).
    Engine {
        password: Option<&'static str>,
    },
    CoreOnly,
}

/// The board double. See the module docs.
pub struct FakeBoard {
    link: Link<SelectiveRepeat>,
    mode: FakeMode,
    manifest: BoardManifest,
    login: LoginState,
    answer_updates: bool,
    /// `Q`s it was asked.
    pub queries: u32,
    pub logins_begun: u32,
    /// The engine's login granted.
    pub granted: bool,
    /// Channel-3 messages that came before the engine's login was granted
    /// (an engine with a password only).
    pub update_messages_before_grant: u32,
    /// Every frame written to it fit one ATT value at the preset's payload.
    pub every_frame_fit: bool,
}

impl FakeBoard {
    /// A board in `mode` whose manifest is `manifest`, on a radio link.
    pub fn new(mode: FakeMode, manifest: BoardManifest) -> Self {
        Self::on_link(LinkConfig::ble(), mode, manifest)
    }

    /// The same board on a USB link's byte stream (feed [`Self::on_bytes`]).
    pub fn over_usb(mode: FakeMode, manifest: BoardManifest) -> Self {
        Self::on_link(LinkConfig::usb(), mode, manifest)
    }

    fn on_link(config: LinkConfig, mode: FakeMode, manifest: BoardManifest) -> Self {
        Self {
            link: Link::new(config, fresh_nonce()),
            mode,
            manifest,
            login: LoginState::new(),
            answer_updates: true,
            queries: 0,
            logins_begun: 0,
            granted: false,
            update_messages_before_grant: 0,
            every_frame_fit: true,
        }
    }

    /// The same board, but it never answers channel 3 (it is about to
    /// reset).
    pub fn silent_on_update(mut self) -> Self {
        self.answer_updates = false;
        self
    }

    /// One GATT write from the host: one frame.
    pub fn on_write(&mut self, now_us: u64, frame: &[u8]) {
        let max = usize::from(LinkConfig::ble().max_payload) + 8;
        self.every_frame_fit &= frame.len() <= max;
        self.link.on_datagram(now_us, frame);
    }

    /// Bytes from a USB host.
    pub fn on_bytes(&mut self, now_us: u64, bytes: &[u8]) {
        self.link.on_bytes(now_us, bytes);
    }

    /// Answer what arrived; the notifications to send, one frame each.
    pub fn poll(&mut self, now_us: u64) -> Vec<Vec<u8>> {
        while let Some(event) = self.link.recv() {
            match event {
                LinkEvent::Up { .. } => self.link_up(),
                LinkEvent::Message { channel, data } if channel == CH_PROTO => {
                    self.on_client(now_us, &data);
                }
                LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                    self.on_update(&data);
                }
                _ => {}
            }
        }
        let mut frames = Vec::new();
        while let Some(frame) = self.link.poll_transmit(now_us) {
            frames.push(frame.to_vec());
        }
        frames
    }

    fn link_up(&mut self) {
        match self.mode {
            FakeMode::Engine { .. } => self.send_server(WireServerMessage::new(0, hello_body())),
            FakeMode::CoreOnly => self.send_manifest(),
        }
    }

    fn on_client(&mut self, now_us: u64, data: &[u8]) {
        let Ok(message) = decode_client_payload(data) else {
            return;
        };
        let now_ms = now_us / 1_000;
        let body = match message.msg {
            ClientRequest::LoginBegin => {
                self.logins_begun += 1;
                let secrets = match self.mode {
                    FakeMode::Engine {
                        password: Some(password),
                    } => vec![SecretEntry::from_password(
                        "desk",
                        Tier::Edit,
                        password.as_bytes(),
                        [5; 16],
                        1_000,
                    )],
                    _ => Vec::new(),
                };
                match self.login.begin(now_ms, [7; 32], secrets) {
                    BeginOutcome::Challenge(c) => WireServerMsgBody::LoginChallenge {
                        nonce: c.nonce,
                        offers: c.offers,
                    },
                    BeginOutcome::Refused { retry_after_ms } => {
                        WireServerMsgBody::LoginResult(LoginOutcome::Refused { retry_after_ms })
                    }
                }
            }
            ClientRequest::LoginAnswer { macs } => {
                let outcome = self.login.answer(now_ms, &macs);
                self.granted |= matches!(outcome, LoginOutcome::Granted { .. });
                WireServerMsgBody::LoginResult(outcome)
            }
            ClientRequest::Hello => hello_body(),
            _ => return,
        };
        self.send_server(WireServerMessage::new(message.id, body));
    }

    fn on_update(&mut self, data: &[u8]) {
        if matches!(self.mode, FakeMode::Engine { password: Some(_) }) && !self.granted {
            self.update_messages_before_grant += 1;
        }
        if data.first() == Some(&b'Q') {
            self.queries += 1;
            if self.answer_updates {
                self.send_manifest();
            }
        }
    }

    fn send_manifest(&mut self) {
        let mut m = b"M".to_vec();
        m.extend_from_slice(&self.manifest.to_json());
        self.link.send(CH_UPDATE, &m).expect("the manifest fits");
    }

    fn send_server(&mut self, message: WireServerMessage) {
        let mut out = Vec::new();
        encode_server_payload(&message, None, &mut out);
        self.link.send(CH_PROTO, &out).expect("the reply fits");
    }
}

/// A board's hello body, as an engine sends it at link up.
pub fn hello_body() -> WireServerMsgBody {
    WireServerMsgBody::Hello(ServerHello {
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
        pack_format: 0,
        auth: HelloAuth::TRUSTED,
        firmware: None,
    })
}

/// A synthetic split package with its OTA files written: the offer.
pub fn offer_fixture() -> Fixture {
    let fx = Fixture::new("abc1234");
    fx.write().unwrap().expect("OTA files written");
    fx
}

/// A board that already runs the offered build, engine and all: the driver
/// decides there is nothing to do, which ends a run without moving a byte.
pub fn board_running(fx: &Fixture) -> BoardManifest {
    let (_, build) =
        crate::commands::ota_host::ota_offer_dir::load_offer(&fx.ota_dir(), false).unwrap();
    let id = &build.identity;
    BoardManifest {
        proto: 1,
        target: id.target.clone(),
        chip: id.chip.clone(),
        version: id.version.clone(),
        build_id: id.build_id.clone(),
        wire_proto: id.wire_proto,
        core_sha256: sha256_to_hex(&build.core.sha256),
        core_len: build.core.len,
        engine_sha256: sha256_to_hex(&build.engine.sha256),
        engine_len: Some(build.engine.len),
        layout: id.layout,
        loader: id.min_loader,
        region_len: 0x30_0000,
        state: BoardState::Running,
        refused_build: None,
        transfer: None,
    }
}
