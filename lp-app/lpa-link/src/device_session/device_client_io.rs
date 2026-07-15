//! The readiness-gated app-protocol channel a session hands to consumers.
//!
//! The generalization of what M3's test-edge fake io proved: nothing is
//! written to the device before it is READY (the M5
//! pull-before-readiness hardware bug), device log lines keep flowing into
//! the event sink during protocol traffic, and gate failures carry the
//! classifiable no-firmware prefix. Unlike the M3 edge, readiness here is
//! hello-first and lives in the session — this adapter only asks.

use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use super::device_session::DeviceShared;

/// `ClientIo` over a device session's transport, gated on
/// `Ready` + `AppProtocol` by construction.
pub(crate) struct DeviceClientIo {
    shared: Rc<DeviceShared>,
}

impl DeviceClientIo {
    pub(crate) fn new(shared: Rc<DeviceShared>) -> Self {
        Self { shared }
    }
}

#[async_trait(?Send)]
impl ClientIo for DeviceClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.shared.ensure_app_protocol().await?;
        let result = {
            let _in_flight = self.shared.begin_channel_use();
            self.shared.send_frame(msg).await
        };
        if matches!(result, Err(TransportError::ConnectionLost)) {
            self.shared.mark_gone("device stream ended during send");
        }
        result
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        self.shared.ensure_app_protocol().await?;
        let budget = self.shared.timers().deadlines().request_idle;
        let received = {
            let _in_flight = self.shared.begin_channel_use();
            self.shared
                .timers()
                .with_deadline(budget, self.shared.recv_frame())
                .await
        };
        // Keep device log lines flowing into the console feed during pulls.
        self.shared.pump_console_lines();
        let result = match received {
            Some(result) => result,
            // The idle backstop: every request gets a response frame, so a
            // quiet gap this long means the wire died mid-request. Surfaced
            // to the caller; readiness-level Unresponsive is not re-entered.
            None => {
                return Err(TransportError::Other(format!(
                    "device did not respond within {:.1}s",
                    budget.as_secs_f64()
                )));
            }
        };
        if matches!(result, Err(TransportError::ConnectionLost)) {
            self.shared.mark_gone("device stream ended during receive");
        }
        result
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.shared.close_wire().await
    }
}
