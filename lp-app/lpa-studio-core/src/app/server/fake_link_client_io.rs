//! Test-edge `ClientIo` over a fake-device link connection (host tests only).
//!
//! The host analogue of `browser_serial_client_io`: it gates the first
//! request on serial readiness using the SAME studio-side classifier
//! (`browser_serial_readiness`), then speaks the server protocol through the
//! real serial framing transport carried by the link connection.
//!
//! This module is `#[cfg(test)]` because lpa-studio-core is sans-IO: the
//! fake device and any blocking (the readiness loop sleeps between polls)
//! live in test edges. The PRODUCT host path (a future desktop studio) will
//! grow a real adapter in M4 (DeviceSession); until then this is the wire
//! that lets the StudioController e2e matrix run the real link path.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpa_link::LinkProviderKind;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::LinkProviderInstance;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use super::browser_serial_readiness::BrowserSerialReadinessClassifier;
use super::device_log_line::parse_device_log_line;
use crate::{SharedLinkRegistry, UiLogDraft, UiLogOrigin, UiLogSource};

/// Bounded readiness wait: POLL_LIMIT polls × POLL_SLEEP. Mirrors the
/// browser io's bounded poll loop; the product layers below still have NO
/// timeout of their own (an M4 input — see the e2e matrix's stall row).
const READINESS_POLL_LIMIT: usize = 1500;
const READINESS_POLL_SLEEP: Duration = Duration::from_millis(2);

pub(crate) struct FakeLinkClientIo {
    registry: SharedLinkRegistry,
    session_id: LinkSessionId,
    transport: lpa_link::LinkServerConnection,
    logs: Rc<RefCell<Vec<UiLogDraft>>>,
    classifier: BrowserSerialReadinessClassifier,
    protocol_frame_seen: bool,
    protocol_ready: bool,
}

impl FakeLinkClientIo {
    pub(crate) fn new(
        registry: SharedLinkRegistry,
        session_id: LinkSessionId,
        transport: lpa_link::LinkServerConnection,
        logs: Rc<RefCell<Vec<UiLogDraft>>>,
    ) -> Self {
        Self {
            registry,
            session_id,
            transport,
            logs,
            classifier: BrowserSerialReadinessClassifier::new(),
            protocol_frame_seen: false,
            protocol_ready: false,
        }
    }

    /// Block until the device is READY to be spoken to: the server-start
    /// boot marker was observed AND the first `M!` frame arrived — the
    /// exact ordering whose absence was the M5 pull-before-readiness
    /// hardware bug. Boot output that classifies as no-firmware fails fast
    /// with the classifier's message (prefix-matched upstream into
    /// `UiError::NoFirmwareDetected`).
    ///
    /// Blocking sleeps are fine here: this io is a test edge, driven from a
    /// host test's executor.
    fn ensure_protocol_ready(&mut self) -> Result<(), TransportError> {
        if self.protocol_ready {
            return Ok(());
        }
        for _ in 0..READINESS_POLL_LIMIT {
            self.drain_lines()?;
            if self.classifier.no_firmware_detected() {
                return Err(TransportError::Other(
                    self.classifier.classify_timeout().message(),
                ));
            }
            if self.classifier.server_started() && self.protocol_frame_seen {
                self.protocol_ready = true;
                return Ok(());
            }
            std::thread::sleep(READINESS_POLL_SLEEP);
        }
        // Same shape as the browser io's readiness timeout: classify what
        // was (or wasn't) seen. There is deliberately no product-level
        // timeout below this — this bound belongs to the test edge.
        Err(TransportError::Other(
            self.classifier.classify_timeout().message(),
        ))
    }

    /// Drain serial lines observed by the transport (via the fake
    /// provider's line buffer): feed the readiness classifier, note `M!`
    /// frames, and turn device log lines into pending console drafts.
    fn drain_lines(&mut self) -> Result<(), TransportError> {
        let lines = {
            let mut registry = self.registry.borrow_mut();
            let provider = fake_provider_mut(&mut registry)?;
            provider
                .take_lines(&self.session_id)
                .map_err(|error| TransportError::Other(error.to_string()))?
        };
        for line in lines {
            if line.starts_with("M!") {
                self.protocol_frame_seen = true;
                continue;
            }
            self.classifier.observe_line(line.as_str());
            let parsed = parse_device_log_line(&line);
            self.logs.borrow_mut().push(UiLogDraft::new(
                parsed.level,
                match parsed.module {
                    Some(module) => UiLogSource::with_detail(UiLogOrigin::Device, module),
                    None => UiLogSource::new(UiLogOrigin::Device),
                },
                parsed.message,
            ));
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl ClientIo for FakeLinkClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.ensure_protocol_ready()?;
        let mut transport = self.transport.lock().await;
        lpa_client::ClientTransport::send(&mut **transport, msg).await
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let result = {
            let mut transport = self.transport.lock().await;
            lpa_client::ClientTransport::receive(&mut **transport).await
        };
        // Keep device log lines flowing into the console during pulls.
        self.drain_lines()?;
        result
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let mut transport = self.transport.lock().await;
        lpa_client::ClientTransport::close(&mut **transport).await
    }
}

fn fake_provider_mut(
    registry: &mut lpa_link::providers::LinkProviderRegistry,
) -> Result<&mut lpa_link::providers::fake::FakeProvider, TransportError> {
    match registry.provider_mut(LinkProviderKind::Fake) {
        Some(LinkProviderInstance::Fake(provider)) => Ok(provider),
        // Reachable only when other lpa-link provider features are unified
        // into the test build (e.g. a full-workspace test compiles lp-cli's
        // host providers in); with fake-device alone the enum has one
        // variant and this arm is dead.
        #[allow(
            unreachable_patterns,
            reason = "provider variants are feature-gated; this arm exists for feature-unified builds"
        )]
        Some(_) => Err(TransportError::Other(
            "fake registry entry has the wrong provider type".to_string(),
        )),
        None => Err(TransportError::Other(
            "fake provider is not available".to_string(),
        )),
    }
}
