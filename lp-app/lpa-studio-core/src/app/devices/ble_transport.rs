//! [`DeviceTransport`] over Bluetooth (Web Bluetooth + NUS): a CONTROL-ONLY
//! link to a LightPlayer board (M5 of the BLE remote-control plan).
//!
//! Studio reaches a board over BLE exactly as it reaches one over USB — the
//! same `M!{json}` lines, the same device fold, the same identity merge by
//! base MAC — with one difference the card has to be honest about: there is
//! no reset line and no ROM downloader on the far side of a GATT service, so
//! **firmware cannot be written over it**. Flash and factory reset are
//! refused here by name, and the card says "Firmware updates need USB"
//! (`DeviceView::firmware_blocked`) before anyone presses anything.
//!
//! | effect | over Bluetooth |
//! |---|---|
//! | Flash firmware, Factory reset | refused: "Firmware updates need USB" |
//! | Write the board manifest, push / remove a project | the REAL `lpa-client` conversation (`wire_conversation.rs`), over the link |
//!
//! Everything platform-shaped arrives through [`BleLinkSource`] — the
//! browser's `navigator.bluetooth` in production (`browser_ble_source.rs`),
//! a double in the tests below — so the routing, the refusals and the grant
//! vocabulary are `just test`-covered on the host.

use std::rc::Rc;

use lpa_devices::link::LinkInfo;
use lpa_devices::view::FIRMWARE_NEEDS_USB;

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap,
};
use super::sim_record::device_id_from_ble_endpoint;
use super::wire_conversation::{is_wire_conversation, run_wire_conversation};

/// Where Bluetooth devices come from on this platform.
pub trait BleLinkSource {
    /// The devices Studio should hold a link for right now (connected, or
    /// closed by request), each as a fresh CLOSED link. A dropped device is
    /// absent until it reconnects — which is how the departure sweep learns
    /// it went away.
    fn present(&self) -> Vec<GrantedLink>;

    /// Re-connect, quietly and once per page, the devices this origin was
    /// already granted. Returns at once; successes arrive through the
    /// platform's presence edge and the next sweep.
    fn restore(&self);

    /// The platform's Bluetooth chooser, then a bounded connect. `Ok(None)`
    /// = the user closed it.
    fn request(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>>;

    /// Revoke the browser's permission for this device.
    fn forget(&self, device_id: &str) -> DeviceTransportFuture<Result<(), String>>;

    /// An `lpa-client` io on this device's link, for a borrowing
    /// conversation. `tap` receives every whole line (and every link error)
    /// the io drains, so the fold keeps hearing the board.
    fn client_io(
        &self,
        device_id: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String>;
}

/// The Bluetooth half of the composite.
pub struct BleDeviceTransport {
    source: Rc<dyn BleLinkSource>,
}

impl BleDeviceTransport {
    pub fn new(source: Rc<dyn BleLinkSource>) -> Self {
        Self { source }
    }
}

/// The refusal a firmware effect gets over Bluetooth. It carries the card's
/// sentence so an activity that somehow got this far ends saying the same
/// thing the disabled verb said.
fn firmware_refusal() -> String {
    format!("{FIRMWARE_NEEDS_USB}: connect this board with a USB cable to change its firmware")
}

impl DeviceTransport for BleDeviceTransport {
    fn label(&self) -> &'static str {
        "Bluetooth"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        self.source.restore();
        Box::pin(core::future::ready(Ok(self.source.present())))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        // Never reached through the composite (the port chooser is
        // serial's); a direct caller is told which verb it meant.
        Box::pin(core::future::ready(Err(
            "a Bluetooth device is added with Add over Bluetooth, not the port chooser".to_string(),
        )))
    }

    fn request_ble_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        self.source.request()
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        match device_id_from_ble_endpoint(&info.endpoint.0) {
            Some(device_id) => self.source.forget(device_id),
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
        let Some(device_id) = device_id_from_ble_endpoint(&info.endpoint.0) else {
            return Box::pin(core::future::ready(Err(
                "not a Bluetooth endpoint".to_string()
            )));
        };
        // Built before the future: an io is a borrow of the wire, and a
        // failure to take one is the effect's failure, not a step inside it.
        let io = match self.source.client_io(device_id, None) {
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
        let device_id = device_id_from_ble_endpoint(&info.endpoint.0)
            .ok_or_else(|| "not a Bluetooth endpoint".to_string())?;
        self.source.client_io(device_id, Some(tap))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use lpc_wire::{ClientMessage, ClientRequest, TransportError, WireServerMessage};

    use super::super::sim_record::ble_link_info;
    use super::*;

    /// Flash and factory reset are refused by name over Bluetooth, and the
    /// refusal carries the card's sentence. Nothing reaches the wire.
    #[test]
    fn firmware_effects_are_refused_with_the_cards_reason() {
        let source = Rc::new(DoubleSource::default());
        let transport = BleDeviceTransport::new(Rc::clone(&source) as Rc<dyn BleLinkSource>);

        for call in [
            DeviceEffectCall::FlashFirmware {
                build_id: "esp32c6-xiao".to_string(),
            },
            DeviceEffectCall::EraseFlash,
        ] {
            let refused = block_on(transport.run_effect(
                ble_link_info("QkxFLWlk", "LP-b48c"),
                call,
                Rc::new(|_, _| {}),
            ))
            .expect_err("firmware cannot go over Bluetooth");
            assert!(refused.starts_with(FIRMWARE_NEEDS_USB), "{refused}");
        }
        assert!(source.asked.borrow().is_empty(), "nothing touched the wire");
    }

    /// A manifest write is the real conversation over the link: ready
    /// first, then the chunked write — the serial arm's order.
    #[test]
    fn a_manifest_write_runs_the_conversation_over_the_link() {
        let source = Rc::new(DoubleSource::default());
        let transport = BleDeviceTransport::new(Rc::clone(&source) as Rc<dyn BleLinkSource>);

        block_on(transport.run_effect(
            ble_link_info("QkxFLWlk", "LP-b48c"),
            DeviceEffectCall::WriteHardwareManifest {
                manifest_json: "{\"id\":\"x\"}".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("the manifest write succeeds");

        assert_eq!(
            source.asked.borrow().as_slice(),
            [
                "io QkxFLWlk".to_string(),
                "listLoadedProjects".to_string(),
                "write /hardware.json {\"id\":\"x\"}".to_string()
            ]
        );
    }

    /// Discovery kicks the page-load restore and answers what is present;
    /// the Bluetooth chooser reaches the source, the port chooser does not.
    #[test]
    fn discovery_restores_and_the_chooser_is_the_bluetooth_one() {
        let source = Rc::new(DoubleSource {
            present: vec![("QkxFLWlk".to_string(), "LP-b48c".to_string())],
            ..Default::default()
        });
        let transport = BleDeviceTransport::new(Rc::clone(&source) as Rc<dyn BleLinkSource>);

        let granted = block_on(transport.discover_granted()).expect("discovery answers");
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].info.endpoint.0, "ble:QkxFLWlk");
        assert_eq!(granted[0].info.label, "LP-b48c");
        assert_eq!(source.restores.get(), 1);

        assert!(block_on(transport.request_grant()).is_err());
        assert!(matches!(block_on(transport.request_ble_grant()), Ok(None)));
        assert_eq!(source.requests.get(), 1);
    }

    /// Revoking a `ble:` grant forgets that device id; the lens io is built
    /// for the device the endpoint names, with the tap attached.
    #[test]
    fn revoke_and_the_lens_address_the_device_the_endpoint_names() {
        let source = Rc::new(DoubleSource::default());
        let transport = BleDeviceTransport::new(Rc::clone(&source) as Rc<dyn BleLinkSource>);

        block_on(transport.revoke_grant(ble_link_info("QkxFLWlk", "LP-b48c"))).unwrap();
        assert!(
            transport
                .lens_client_io(ble_link_info("QkxFLWlk", "LP-b48c"), Rc::new(|_| {}))
                .is_ok()
        );

        assert_eq!(
            source.asked.borrow().as_slice(),
            ["forget QkxFLWlk".to_string(), "io QkxFLWlk tap".to_string()]
        );
    }

    // --- the doubles -----------------------------------------------------

    #[derive(Default)]
    struct DoubleSource {
        present: Vec<(String, String)>,
        restores: Cell<usize>,
        requests: Cell<usize>,
        asked: Rc<RefCell<Vec<String>>>,
    }

    impl BleLinkSource for DoubleSource {
        fn present(&self) -> Vec<GrantedLink> {
            self.present
                .iter()
                .map(|(id, name)| {
                    let info = ble_link_info(id, name);
                    GrantedLink {
                        link: Box::new(SilentLink(info.clone())),
                        info,
                    }
                })
                .collect()
        }

        fn restore(&self) {
            self.restores.set(self.restores.get() + 1);
        }

        fn request(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
            self.requests.set(self.requests.get() + 1);
            Box::pin(core::future::ready(Ok(None)))
        }

        fn forget(&self, device_id: &str) -> DeviceTransportFuture<Result<(), String>> {
            self.asked.borrow_mut().push(format!("forget {device_id}"));
            Box::pin(core::future::ready(Ok(())))
        }

        fn client_io(
            &self,
            device_id: &str,
            tap: Option<LensLineTap>,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            let note = match tap {
                Some(_) => format!("io {device_id} tap"),
                None => format!("io {device_id}"),
            };
            self.asked.borrow_mut().push(note);
            Ok(Box::new(CountingIo {
                asked: Rc::clone(&self.asked),
                answer: None,
            }))
        }
    }

    /// Records each request and answers it with the smallest successful
    /// reply (the emu transport's double, for the same reason).
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
        panic!("a Bluetooth transport future did not complete");
    }
}
