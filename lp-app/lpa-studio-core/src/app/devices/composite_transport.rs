//! Two transports behind the one [`DeviceTransport`] the effects layer holds.
//!
//! [`DeviceEffects`](super::DeviceEffects) has exactly one transport per
//! build, and deliberately: a per-kind fork inside the effects layer is how a
//! second device flow grows. So a build that can reach both silicon and sims
//! installs ONE transport that owns both and routes by the only thing it
//! honestly can — the link's own endpoint.
//!
//! ```text
//!   discover_granted ──► serial ∪ sim          (both, every sweep)
//!   request_grant    ──► serial                (a chooser is a port chooser)
//!   run_effect       ──► endpoint "sim:…" ? sim : serial
//!   revoke_grant     ──► same rule
//!   lens_client_io   ──► same rule
//! ```
//!
//! # Why the endpoint and not a kind
//!
//! The endpoint is what the effects layer already routes by, what the model
//! binds identity to at its weakest rung, and what the registry column is
//! derived from ([`transport_label_for_endpoint`](super::device_records::transport_label_for_endpoint)).
//! Adding a `kind` field beside it would create a second answer to the same
//! question, and the two would eventually disagree.
//!
//! # A build with no serial transport still serves sims
//!
//! `BrowserSerialTransport::new` refuses to construct on a browser without
//! Web Serial (Safari, Firefox), and the host has none at all. The composite
//! takes `serial: Option<…>` for exactly that: the roster still fills with
//! sims, and the ONE thing that degrades is the chooser, which says why.

use std::rc::Rc;

use lpa_devices::link::LinkInfo;

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap,
};
use super::sim_record::uid_from_sim_endpoint;

/// Serial and sim, behind one trait.
pub struct CompositeDeviceTransport {
    /// `None` where this build cannot reach a serial port at all.
    serial: Option<Rc<dyn DeviceTransport>>,
    sim: Rc<dyn DeviceTransport>,
}

impl CompositeDeviceTransport {
    pub fn new(serial: Option<Rc<dyn DeviceTransport>>, sim: Rc<dyn DeviceTransport>) -> Self {
        Self { serial, sim }
    }

    /// Which half owns this endpoint. A `sim:` endpoint is the sim
    /// transport's; everything else belongs to serial, and a build without
    /// one says so rather than silently answering with the wrong half.
    fn route(&self, info: &LinkInfo) -> Result<Rc<dyn DeviceTransport>, String> {
        if uid_from_sim_endpoint(&info.endpoint.0).is_some() {
            return Ok(Rc::clone(&self.sim));
        }
        self.serial
            .clone()
            .ok_or_else(|| "this build cannot talk to USB devices".to_string())
    }
}

impl DeviceTransport for CompositeDeviceTransport {
    fn label(&self) -> &'static str {
        match self.serial.is_some() {
            true => "browser Web Serial + sim",
            false => "sim only",
        }
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        let serial = self.serial.clone();
        let sim = Rc::clone(&self.sim);
        Box::pin(async move {
            // A serial discovery that FAILED says nothing about which boards
            // exist, and the departure sweep detaches on the answer — so the
            // failure propagates rather than being papered over with a
            // partial list that would read as "every serial board left".
            // The cost is one sweep's worth of latency on a new sim, which
            // the next sweep pays back.
            let mut granted = match &serial {
                Some(serial) => serial.discover_granted().await?,
                None => Vec::new(),
            };
            granted.extend(sim.discover_granted().await?);
            Ok(granted)
        })
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        // The chooser is a PORT chooser. Sims are made on the Devices page,
        // so a build with no serial transport lets the sim half answer —
        // which is a refusal naming the way in, not a silent nothing.
        match &self.serial {
            Some(serial) => serial.request_grant(),
            None => self.sim.request_grant(),
        }
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        match self.route(&info) {
            Ok(transport) => transport.revoke_grant(info),
            // Best effort by design, here as everywhere: a grant that cannot
            // be handed back is a log line, never a card that cannot be
            // dismissed.
            Err(error) => {
                log::debug!("grant not revoked: {error}");
                Box::pin(core::future::ready(Ok(())))
            }
        }
    }

    fn run_effect(
        &self,
        info: LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        match self.route(&info) {
            Ok(transport) => transport.run_effect(info, call, progress),
            Err(error) => Box::pin(core::future::ready(Err(error))),
        }
    }

    fn lens_client_io(
        &self,
        info: LinkInfo,
        tap: LensLineTap,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        self.route(&info)?.lens_client_io(info, tap)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::super::sim_record::sim_link_info;
    use super::*;

    /// A transport that records which of its methods were reached.
    struct SpyTransport {
        name: &'static str,
        calls: Rc<RefCell<Vec<String>>>,
        grants: Vec<String>,
        discovery_fails: bool,
    }

    impl SpyTransport {
        fn new(name: &'static str, calls: &Rc<RefCell<Vec<String>>>, grants: &[&str]) -> Self {
            Self {
                name,
                calls: Rc::clone(calls),
                grants: grants.iter().map(|grant| grant.to_string()).collect(),
                discovery_fails: false,
            }
        }

        fn note(&self, what: &str) {
            self.calls
                .borrow_mut()
                .push(format!("{}:{what}", self.name));
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

    fn link_at(endpoint: &str) -> GrantedLink {
        let info = LinkInfo {
            label: endpoint.to_string(),
            endpoint: lpa_devices::identity::EndpointKey(endpoint.to_string()),
            usb: None,
            serial_number: None,
        };
        GrantedLink {
            link: Box::new(SilentLink(info.clone())),
            info,
        }
    }

    impl DeviceTransport for SpyTransport {
        fn label(&self) -> &'static str {
            self.name
        }

        fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
            self.note("discover");
            if self.discovery_fails {
                return Box::pin(core::future::ready(Err("enumeration hiccup".to_string())));
            }
            let links: Vec<GrantedLink> = self.grants.iter().map(|at| link_at(at)).collect();
            Box::pin(core::future::ready(Ok(links)))
        }

        fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
            self.note("request_grant");
            Box::pin(core::future::ready(Ok(None)))
        }

        fn revoke_grant(&self, _info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
            self.note("revoke");
            Box::pin(core::future::ready(Ok(())))
        }

        fn run_effect(
            &self,
            _info: LinkInfo,
            _call: DeviceEffectCall,
            _progress: DeviceEffectProgress,
        ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
            self.note("effect");
            Box::pin(core::future::ready(Ok(DeviceEffectFacts::default())))
        }

        fn lens_client_io(
            &self,
            _info: LinkInfo,
            _tap: LensLineTap,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            self.note("lens");
            Err("no io in this spy".to_string())
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
        panic!("a composite future did not complete");
    }

    fn composite(calls: &Rc<RefCell<Vec<String>>>, with_serial: bool) -> CompositeDeviceTransport {
        let serial: Option<Rc<dyn DeviceTransport>> = with_serial.then(|| {
            Rc::new(SpyTransport::new("serial", calls, &["usb-1"])) as Rc<dyn DeviceTransport>
        });
        CompositeDeviceTransport::new(
            serial,
            Rc::new(SpyTransport::new("sim", calls, &["sim:dev1"])),
        )
    }

    #[test]
    fn discovery_unions_both_halves() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let transport = composite(&calls, true);

        let granted = block_on(transport.discover_granted()).expect("both answered");

        let endpoints: Vec<String> = granted
            .iter()
            .map(|grant| grant.info.endpoint.0.clone())
            .collect();
        assert_eq!(endpoints, vec!["usb-1".to_string(), "sim:dev1".to_string()]);
        assert_eq!(
            calls.borrow().as_slice(),
            ["serial:discover", "sim:discover"]
        );
    }

    /// A serial enumeration that failed says nothing about which boards
    /// exist. The departure sweep detaches on this answer, so a partial list
    /// would read as "every serial board left".
    #[test]
    fn a_failed_serial_discovery_is_not_papered_over() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let serial = SpyTransport {
            discovery_fails: true,
            ..SpyTransport::new("serial", &calls, &[])
        };
        let transport = CompositeDeviceTransport::new(
            Some(Rc::new(serial)),
            Rc::new(SpyTransport::new("sim", &calls, &["sim:dev1"])),
        );

        assert!(block_on(transport.discover_granted()).is_err());
    }

    #[test]
    fn effects_and_revocations_route_by_the_endpoint() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let transport = composite(&calls, true);

        block_on(transport.run_effect(
            sim_link_info("dev1", "Desktop"),
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect("the sim half answered");
        block_on(transport.run_effect(
            link_at("usb-1").info,
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect("the serial half answered");
        block_on(transport.revoke_grant(sim_link_info("dev1", "Desktop"))).unwrap();
        let _ = transport.lens_client_io(link_at("usb-1").info, Rc::new(|_| {}));

        assert_eq!(
            calls.borrow().as_slice(),
            ["sim:effect", "serial:effect", "sim:revoke", "serial:lens"]
        );
    }

    #[test]
    fn the_chooser_goes_to_serial_when_there_is_one() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let transport = composite(&calls, true);

        block_on(transport.request_grant()).unwrap();

        assert_eq!(calls.borrow().as_slice(), ["serial:request_grant"]);
    }

    /// The host, and every browser without Web Serial: the roster still
    /// fills with sims, and only the chooser degrades.
    #[test]
    fn a_build_with_no_serial_transport_still_serves_sims() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let transport = composite(&calls, false);

        let granted = block_on(transport.discover_granted()).expect("the sim half answered");
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].info.endpoint.0, "sim:dev1");
        assert_eq!(transport.label(), "sim only");

        block_on(transport.request_grant()).unwrap();
        assert_eq!(
            calls.borrow().as_slice(),
            ["sim:discover", "sim:request_grant"],
            "the chooser reaches the half that can explain itself"
        );

        let refused = block_on(transport.run_effect(
            link_at("usb-1").info,
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect_err("there is no serial half");
        assert!(refused.contains("USB"), "{refused}");
    }
}
