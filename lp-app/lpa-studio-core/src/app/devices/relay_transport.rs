//! [`DeviceTransport`] through lightplayer.app's relay: a board on Wi‑Fi
//! reached from anywhere, as a CONTROL-ONLY, secure link (the network
//! transport's P05, behind `?relay=1`).
//!
//! The relay's browser leg carries bare lp-link frames, one per WebSocket
//! message — what a board serves on its LAN `/link` — so this is the LAN
//! transport ([`super::lan_transport`]) with another address: the socket is
//! `wss://<this page's host>/relay/board/<mac>` and the endpoint is
//! `relay:<mac>`. Everything above the socket is the LAN path's: the same
//! provider (`browser-websocket`), the same secure link, the same wire
//! conversation, the same refusal of firmware.
//!
//! What differs is who may get in. Through the relay a link presents only
//! the keys this browser holds — the account's, then the browser's own — and
//! never the anonymous key or a typed password (`access/network_link_keys.rs`
//! and `KeyWalk::held_only`); a board none of them opens is given up, and the
//! person is told to sign in and plug it in once
//! ([`RelayConnectFailure::NoHeldKey`]).
//!
//! Discovery never fails: signed out, offline, or with no board dialled, the
//! relay half answers what is present — often nothing — never `Err`, because
//! one failing half fails the whole sweep (`composite_transport.rs`), and a
//! partial list reads as "every board left" (the plan's R5).
//!
//! Platform-shaped work arrives through [`RelayLinkSource`]: the page's
//! WebSockets in production (`browser_relay_source.rs`), a double in the
//! tests below.

use std::rc::Rc;

use lpa_devices::link::LinkInfo;
use lpa_devices::view::FIRMWARE_NEEDS_USB;
use lpa_link::providers::network_link::board_from_relay_endpoint;

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent,
};
use super::relay_connect_failure::RelayConnectFailure;
use super::wire_conversation::{is_wire_conversation, run_wire_conversation};

/// Where relay boards come from on this platform. Boards are named by their
/// relay id: the MAC as twelve lowercase hex digits (`a0f26287b48c`).
pub trait RelayLinkSource {
    /// The boards Studio should hold a link for right now, each as a fresh
    /// CLOSED link. A board whose session dropped is absent until it comes
    /// back.
    fn present(&self) -> Vec<GrantedLink>;

    /// Stop reaching `board` through the relay (the card's Forget).
    fn forget(&self, board: &str) -> DeviceTransportFuture<Result<(), String>>;

    /// An `lpa-client` io on `board`'s link, for a borrowing conversation.
    fn client_io(
        &self,
        board: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String>;

    /// Reach `board` through the relay now, because someone asked: open its
    /// session and wait until its secure link is up, or until it is turned
    /// away (the relay's close code, or no held key opening it). `Err`
    /// carries the session's own words, and a failed session is not kept.
    fn connect(&self, board: &str) -> DeviceTransportFuture<Result<(), String>>;
}

/// The relay half of the composite.
pub struct RelayDeviceTransport {
    source: Rc<dyn RelayLinkSource>,
}

impl RelayDeviceTransport {
    pub fn new(source: Rc<dyn RelayLinkSource>) -> Self {
        Self { source }
    }

    /// Reach `board` through lightplayer.app now, answering why not in plain
    /// words.
    pub fn connect(&self, board: &str) -> DeviceTransportFuture<Result<(), RelayConnectFailure>> {
        let attempt = self.source.connect(board);
        Box::pin(async move {
            attempt
                .await
                .map_err(|raw| RelayConnectFailure::from_socket(&raw))
        })
    }
}

/// The refusal a firmware effect gets through the relay, in the card's words.
fn firmware_refusal() -> String {
    format!("{FIRMWARE_NEEDS_USB}: connect this board with a USB cable to change its firmware")
}

impl DeviceTransport for RelayDeviceTransport {
    fn label(&self) -> &'static str {
        "lightplayer.app relay"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        Box::pin(core::future::ready(Ok(self.source.present())))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        // Never reached through the composite (the chooser is serial's).
        Box::pin(core::future::ready(Err(
            "a board is reached through lightplayer.app by its id, not the port chooser"
                .to_string(),
        )))
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        match board_from_relay_endpoint(&info.endpoint.0) {
            Some(board) => self.source.forget(board),
            None => Box::pin(core::future::ready(Ok(()))),
        }
    }

    fn run_effect(
        &self,
        info: LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        if !is_wire_conversation(&call) {
            return Box::pin(core::future::ready(Err(firmware_refusal())));
        }
        let Some(board) = board_from_relay_endpoint(&info.endpoint.0) else {
            return Box::pin(core::future::ready(Err("not a relay endpoint".to_string())));
        };
        // The board's own words during the conversation ride the progress
        // narration, as on the LAN.
        let narrate: LensLineTap = {
            let progress = Rc::clone(&progress);
            Rc::new(move |event| {
                if let LensTapEvent::Line(line) = event
                    && !line.starts_with("M!")
                {
                    progress(line, None);
                }
            })
        };
        let io = match self.source.client_io(board, Some(narrate)) {
            Ok(io) => io,
            Err(error) => return Box::pin(core::future::ready(Err(error))),
        };
        Box::pin(run_wire_conversation(io, call, progress))
    }

    fn lens_client_io(
        &self,
        info: LinkInfo,
        tap: LensLineTap,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let board = board_from_relay_endpoint(&info.endpoint.0)
            .ok_or_else(|| "not a relay endpoint".to_string())?;
        self.source.client_io(board, Some(tap))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use lpa_link::providers::network_link::{relay_link_info, relay_socket_url};
    use lpc_wire::{ClientMessage, ClientRequest, TransportError, WireServerMessage};

    use super::*;

    const BOARD: &str = "a0f26287b48c";

    fn info() -> LinkInfo {
        relay_link_info(&relay_socket_url("https://lightplayer.app", BOARD)).unwrap()
    }

    #[test]
    fn firmware_effects_are_refused_with_the_cards_reason() {
        let source = Rc::new(DoubleSource::default());
        let transport = RelayDeviceTransport::new(Rc::clone(&source) as Rc<dyn RelayLinkSource>);
        for call in [
            DeviceEffectCall::FlashFirmware {
                build_id: "esp32c6-xiao".to_string(),
                plan: None,
            },
            DeviceEffectCall::EraseFlash,
        ] {
            let refused = block_on(transport.run_effect(info(), call, Rc::new(|_, _| {})))
                .expect_err("firmware cannot go through the relay");
            assert!(refused.starts_with(FIRMWARE_NEEDS_USB), "{refused}");
        }
        assert!(source.asked.borrow().is_empty(), "nothing touched the wire");
    }

    #[test]
    fn a_manifest_write_runs_the_conversation_over_the_link() {
        let source = Rc::new(DoubleSource::default());
        let transport = RelayDeviceTransport::new(Rc::clone(&source) as Rc<dyn RelayLinkSource>);
        block_on(transport.run_effect(
            info(),
            DeviceEffectCall::WriteHardwareManifest {
                manifest_json: "{\"id\":\"x\"}".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("the manifest write succeeds");
        assert_eq!(
            source.asked.borrow()[..2],
            [format!("io {BOARD} tap"), "listLoadedProjects".to_string()]
        );
    }

    /// R5: signed out, offline, or nothing dialled, the relay half answers
    /// an empty list, never an error that would fail the whole sweep.
    #[test]
    fn discovery_is_what_is_present_and_never_fails() {
        let empty = RelayDeviceTransport::new(Rc::new(DoubleSource::default()));
        assert_eq!(
            block_on(empty.discover_granted())
                .expect("an empty relay half still answers")
                .len(),
            0
        );

        let source = Rc::new(DoubleSource {
            present: vec![BOARD.to_string()],
            ..Default::default()
        });
        let transport = RelayDeviceTransport::new(Rc::clone(&source) as Rc<dyn RelayLinkSource>);
        let granted = block_on(transport.discover_granted()).expect("discovery answers");
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].info.endpoint.0, "relay:a0f26287b48c");
        assert_eq!(granted[0].info.label, "lightplayer.app");
        assert!(block_on(transport.request_grant()).is_err());
        assert!(block_on(transport.request_ble_grant()).is_err());
    }

    #[test]
    fn revoke_and_the_lens_address_the_board_the_endpoint_names() {
        let source = Rc::new(DoubleSource::default());
        let transport = RelayDeviceTransport::new(Rc::clone(&source) as Rc<dyn RelayLinkSource>);
        block_on(transport.revoke_grant(info())).unwrap();
        assert!(transport.lens_client_io(info(), Rc::new(|_| {})).is_ok());
        assert_eq!(
            source.asked.borrow().as_slice(),
            [format!("forget {BOARD}"), format!("io {BOARD} tap")]
        );
    }

    #[test]
    fn a_connect_answers_in_plain_words() {
        let source = Rc::new(DoubleSource::default());
        let transport = RelayDeviceTransport::new(Rc::clone(&source) as Rc<dyn RelayLinkSource>);
        block_on(transport.connect(BOARD)).expect("the board answered");
        assert_eq!(
            source.asked.borrow().as_slice(),
            [format!("connect {BOARD}")]
        );

        for (raw, words) in [
            (
                "relay link lost: no key this browser holds opens this board",
                "Sign in to Studio and plug this board in once to reach it through \
                 lightplayer.app.",
            ),
            (
                "relay link lost: the board closed the link (code 4404: board-offline)",
                "The board isn't online.",
            ),
            (
                "relay link lost: the board closed the link (code 4429: busy)",
                "Busy with another connection \u{2014} try again",
            ),
        ] {
            let transport = RelayDeviceTransport::new(Rc::new(DoubleSource {
                refuse: Some(raw.to_string()),
                ..Default::default()
            }));
            assert_eq!(
                block_on(transport.connect(BOARD)).unwrap_err().words(),
                words,
                "{raw}"
            );
        }
    }

    // --- the doubles -----------------------------------------------------

    #[derive(Default)]
    struct DoubleSource {
        present: Vec<String>,
        asked: Rc<RefCell<Vec<String>>>,
        /// The session's words a connect fails with, or `None` to answer.
        refuse: Option<String>,
    }

    impl RelayLinkSource for DoubleSource {
        fn present(&self) -> Vec<GrantedLink> {
            self.present
                .iter()
                .map(|board| {
                    let info = relay_link_info(&relay_socket_url("https://lightplayer.app", board))
                        .unwrap();
                    GrantedLink {
                        link: Box::new(SilentLink(info.clone())),
                        info,
                    }
                })
                .collect()
        }

        fn forget(&self, board: &str) -> DeviceTransportFuture<Result<(), String>> {
            self.asked.borrow_mut().push(format!("forget {board}"));
            Box::pin(core::future::ready(Ok(())))
        }

        fn client_io(
            &self,
            board: &str,
            tap: Option<LensLineTap>,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            let note = match tap {
                Some(_) => format!("io {board} tap"),
                None => format!("io {board}"),
            };
            self.asked.borrow_mut().push(note);
            Ok(Box::new(CountingIo {
                asked: Rc::clone(&self.asked),
                answer: None,
            }))
        }

        fn connect(&self, board: &str) -> DeviceTransportFuture<Result<(), String>> {
            self.asked.borrow_mut().push(format!("connect {board}"));
            Box::pin(core::future::ready(match &self.refuse {
                Some(words) => Err(words.clone()),
                None => Ok(()),
            }))
        }
    }

    /// Records each request and answers it with the smallest successful
    /// reply (the LAN transport's double, for the same reason).
    struct CountingIo {
        asked: Rc<RefCell<Vec<String>>>,
        answer: Option<WireServerMessage>,
    }

    #[async_trait::async_trait(?Send)]
    impl lpa_client::ClientIo for CountingIo {
        async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
            use lpc_wire::server::ServerMsgBody;
            use lpc_wire::server::{FsRequest, FsResponse};
            let (note, body) = match msg.msg {
                ClientRequest::ListLoadedProjects => (
                    "listLoadedProjects".to_string(),
                    ServerMsgBody::ListLoadedProjects {
                        projects: Vec::new(),
                    },
                ),
                ClientRequest::Filesystem(FsRequest::Write { path, data }) => (
                    format!("write {} {}", path.as_str(), String::from_utf8_lossy(&data)),
                    ServerMsgBody::Filesystem(FsResponse::Write { path, error: None }),
                ),
                other => (format!("{other:?}"), ServerMsgBody::UnloadProject),
            };
            self.asked.borrow_mut().push(note);
            self.answer = Some(WireServerMessage::new(msg.id, body));
            Ok(())
        }

        async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
            self.answer
                .take()
                .ok_or_else(|| TransportError::Other("nothing was asked".to_string()))
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    struct SilentLink(LinkInfo);

    impl lpa_devices::link::Link for SilentLink {
        fn info(&self) -> &LinkInfo {
            &self.0
        }

        fn submit(&mut self, _command: lpa_devices::link::LinkCommand) {}

        fn poll_event(&mut self) -> Option<lpa_devices::link::LinkEvent> {
            None
        }
    }

    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        use core::task::{Context, Poll};
        use std::sync::Arc;
        use std::task::Wake;

        struct Noop;
        impl Wake for Noop {
            fn wake(self: Arc<Self>) {}
        }
        let waker = core::task::Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut future = core::pin::pin!(future);
        for _ in 0..1_000 {
            if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                return output;
            }
        }
        panic!("a relay transport future did not complete");
    }
}
