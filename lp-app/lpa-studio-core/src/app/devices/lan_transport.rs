//! [`DeviceTransport`] over the LAN (a browser WebSocket to a board on
//! Wi-Fi): a CONTROL-ONLY, secure link (Wi-Fi M6 P07).
//!
//! Studio reaches a board on the LAN as it reaches one over Bluetooth — the
//! same wire messages over the same lp-link, the same device fold, the same
//! identity merge by base MAC — with the same honesty about what a socket
//! cannot do: there is no reset line and no ROM downloader on the far side
//! of one, so **firmware cannot be written over it**. Flash and factory
//! reset are refused here by name.
//!
//! | effect | over Wi-Fi |
//! |---|---|
//! | Flash firmware, Factory reset | refused: "Firmware updates need USB" |
//! | Write the board manifest, push / remove a project | the REAL `lpa-client` conversation (`wire_conversation.rs`), over the link |
//!
//! The link is SECURE: each connection presents this browser's access keys
//! (`access/network_link_keys.rs`), and a locked board's password is asked
//! for the way the Bluetooth flow asks (`access/keyed_login.rs`).
//!
//! There is no chooser here. A board is reached at an address: one Studio
//! remembered for it (`wifi_address_book.rs`, "Connect over Wi‑Fi" on its
//! card), one typed into "Connect a board on Wi‑Fi", or one the `?lan=` dev
//! shortcut names at page load. Everything platform-shaped arrives through
//! [`LanLinkSource`] — the page's WebSockets in production
//! (`browser_lan_source.rs`), a double in the tests below — so routing,
//! refusals and the connect's plain words are `just test`-covered on the
//! host.

use std::rc::Rc;

use lpa_devices::link::LinkInfo;
use lpa_devices::view::FIRMWARE_NEEDS_USB;
use lpa_link::providers::network_link::{lan_host, url_from_lan_endpoint};

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent,
};
use super::wifi_connect_failure::WifiConnectFailure;
use super::wire_conversation::{is_wire_conversation, run_wire_conversation};

/// Where LAN boards come from on this platform.
pub trait LanLinkSource {
    /// The boards Studio should hold a link for right now (connected, or
    /// closed by request), each as a fresh CLOSED link. A dropped board is
    /// absent until it reconnects — which is how the departure sweep learns
    /// it went away.
    fn present(&self) -> Vec<GrantedLink>;

    /// Stop reaching the board at `url` (the card's Forget): its socket is
    /// closed and no longer reconnected.
    fn forget(&self, url: &str) -> DeviceTransportFuture<Result<(), String>>;

    /// An `lpa-client` io on this board's link, for a borrowing conversation.
    /// `tap` receives every message, link note and link error the io drains.
    fn client_io(
        &self,
        url: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String>;

    /// Reach the board at `url` now, because someone asked: open (or keep)
    /// its session and wait until its first connection says how it went —
    /// the board answered, or the socket failed, timed out, or was turned
    /// away. `Err` carries the socket's own words, and a failed session is
    /// not kept (nothing keeps redialling an address that did not answer).
    /// Once it is `Ok` the board is present, and the next sweep links it.
    fn connect(&self, url: &str) -> DeviceTransportFuture<Result<(), String>>;

    /// Whether this page is served over https, where Chrome's Local Network
    /// check turns away a socket to a private address before it opens.
    fn secure_page(&self) -> bool {
        false
    }
}

/// The LAN half of the composite.
pub struct LanDeviceTransport {
    source: Rc<dyn LanLinkSource>,
}

impl LanDeviceTransport {
    pub fn new(source: Rc<dyn LanLinkSource>) -> Self {
        Self { source }
    }

    /// Reach the board at `url` now (a user's "Connect over Wi‑Fi", or an
    /// address typed into Connect a board's Network row), answering why not in plain words.
    pub fn connect(&self, url: &str) -> DeviceTransportFuture<Result<(), WifiConnectFailure>> {
        let host = lan_host(url).to_string();
        let secure = self.source.secure_page();
        let attempt = self.source.connect(url);
        Box::pin(async move {
            attempt
                .await
                .map_err(|raw| WifiConnectFailure::from_socket(&raw, &host, secure))
        })
    }
}

/// The refusal a firmware effect gets over Wi-Fi, in the card's words.
fn firmware_refusal() -> String {
    format!("{FIRMWARE_NEEDS_USB}: connect this board with a USB cable to change its firmware")
}

impl DeviceTransport for LanDeviceTransport {
    fn label(&self) -> &'static str {
        "Wi-Fi"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        Box::pin(core::future::ready(Ok(self.source.present())))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        // Never reached through the composite (the chooser is serial's).
        Box::pin(core::future::ready(Err(
            "a Wi\u{2011}Fi board is reached by its address, not the port chooser".to_string(),
        )))
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        match url_from_lan_endpoint(&info.endpoint.0) {
            Some(url) => self.source.forget(url),
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
        let Some(url) = url_from_lan_endpoint(&info.endpoint.0) else {
            return Box::pin(core::future::ready(Err("not a Wi-Fi endpoint".to_string())));
        };
        // The board's own words during the conversation ride the progress
        // narration, as over Bluetooth: the pump is paused for the borrow.
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
        let io = match self.source.client_io(url, Some(narrate)) {
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
        let url = url_from_lan_endpoint(&info.endpoint.0)
            .ok_or_else(|| "not a Wi-Fi endpoint".to_string())?;
        self.source.client_io(url, Some(tap))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use lpa_link::providers::network_link::lan_link_info;
    use lpc_wire::{ClientMessage, ClientRequest, TransportError, WireServerMessage};

    use super::*;

    const URL: &str = "ws://10.0.0.5/link";

    #[test]
    fn firmware_effects_are_refused_with_the_cards_reason() {
        let source = Rc::new(DoubleSource::default());
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);

        for call in [
            DeviceEffectCall::FlashFirmware {
                build_id: "esp32c6-xiao".to_string(),
                plan: None,
            },
            DeviceEffectCall::EraseFlash,
        ] {
            let refused =
                block_on(transport.run_effect(lan_link_info(URL), call, Rc::new(|_, _| {})))
                    .expect_err("firmware cannot go over Wi-Fi");
            assert!(refused.starts_with(FIRMWARE_NEEDS_USB), "{refused}");
        }
        assert!(source.asked.borrow().is_empty(), "nothing touched the wire");
    }

    #[test]
    fn a_manifest_write_runs_the_conversation_over_the_link() {
        let source = Rc::new(DoubleSource::default());
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);

        block_on(transport.run_effect(
            lan_link_info(URL),
            DeviceEffectCall::WriteHardwareManifest {
                manifest_json: "{\"id\":\"x\"}".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("the manifest write succeeds");

        assert_eq!(
            source.asked.borrow()[..2],
            [format!("io {URL} tap"), "listLoadedProjects".to_string()]
        );
    }

    #[test]
    fn discovery_is_what_is_present_and_there_is_no_chooser() {
        let source = Rc::new(DoubleSource {
            present: vec![URL.to_string()],
            ..Default::default()
        });
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);

        let granted = block_on(transport.discover_granted()).expect("discovery answers");
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].info.endpoint.0, format!("lan:{URL}"));
        assert_eq!(granted[0].info.label, "10.0.0.5");
        assert!(block_on(transport.request_grant()).is_err());
        assert!(block_on(transport.request_ble_grant()).is_err());
        assert_eq!(transport.label(), "Wi-Fi");
    }

    #[test]
    fn revoke_and_the_lens_address_the_board_the_endpoint_names() {
        let source = Rc::new(DoubleSource::default());
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);

        block_on(transport.revoke_grant(lan_link_info(URL))).unwrap();
        assert!(
            transport
                .lens_client_io(lan_link_info(URL), Rc::new(|_| {}))
                .is_ok()
        );
        assert_eq!(
            source.asked.borrow().as_slice(),
            [format!("forget {URL}"), format!("io {URL} tap")]
        );
    }

    #[test]
    fn a_connect_answers_in_plain_words_for_the_address_it_dialled() {
        let source = Rc::new(DoubleSource::default());
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);
        block_on(transport.connect(URL)).expect("the board answered");

        let source = Rc::new(DoubleSource {
            refuse: Some("wi-fi connect to ws://10.0.0.5/link was closed (code 1006)".to_string()),
            secure: true,
            ..Default::default()
        });
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);
        let blocked = block_on(transport.connect(URL)).expect_err("the page was turned away");
        assert_eq!(blocked, WifiConnectFailure::Blocked);
        assert_eq!(source.asked.borrow().as_slice(), [format!("connect {URL}")]);

        let source = Rc::new(DoubleSource {
            refuse: Some("wi-fi connect timed out after 10 s".to_string()),
            ..Default::default()
        });
        let transport = LanDeviceTransport::new(Rc::clone(&source) as Rc<dyn LanLinkSource>);
        assert_eq!(
            block_on(transport.connect(URL)).unwrap_err().words(),
            "Couldn't reach the board at 10.0.0.5. Is it on this network?"
        );
    }

    // --- the doubles -----------------------------------------------------

    #[derive(Default)]
    struct DoubleSource {
        present: Vec<String>,
        asked: Rc<RefCell<Vec<String>>>,
        /// The socket's words a connect fails with, or `None` to answer.
        refuse: Option<String>,
        secure: bool,
    }

    impl LanLinkSource for DoubleSource {
        fn present(&self) -> Vec<GrantedLink> {
            self.present
                .iter()
                .map(|url| {
                    let info = lan_link_info(url);
                    GrantedLink {
                        link: Box::new(SilentLink(info.clone())),
                        info,
                    }
                })
                .collect()
        }

        fn forget(&self, url: &str) -> DeviceTransportFuture<Result<(), String>> {
            self.asked.borrow_mut().push(format!("forget {url}"));
            Box::pin(core::future::ready(Ok(())))
        }

        fn client_io(
            &self,
            url: &str,
            tap: Option<LensLineTap>,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            let note = match tap {
                Some(_) => format!("io {url} tap"),
                None => format!("io {url}"),
            };
            self.asked.borrow_mut().push(note);
            Ok(Box::new(CountingIo {
                asked: Rc::clone(&self.asked),
                answer: None,
            }))
        }

        fn connect(&self, url: &str) -> DeviceTransportFuture<Result<(), String>> {
            self.asked.borrow_mut().push(format!("connect {url}"));
            Box::pin(core::future::ready(match &self.refuse {
                Some(words) => Err(words.clone()),
                None => Ok(()),
            }))
        }

        fn secure_page(&self) -> bool {
            self.secure
        }
    }

    /// Records each request and answers it with the smallest successful
    /// reply (the Bluetooth transport's double, for the same reason).
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
                ClientRequest::Filesystem(FsRequest::DeleteFile { path }) => (
                    format!("delete {}", path.as_str()),
                    ServerMsgBody::Filesystem(FsResponse::DeleteFile { path, error: None }),
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
        panic!("a Wi-Fi transport future did not complete");
    }
}
