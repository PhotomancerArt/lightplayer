//! Management orchestration + reconnect-that-rebuilds.
//!
//! [`DeviceSession::manage`] owns the whole flash/erase/reset cycle:
//!
//! 1. Take [`DeviceMode::Management`] — refused while a management
//!    operation already holds the wire or an app-protocol request is
//!    mid-flight; the handed-out channel is invalidated by construction
//!    while the mode is held.
//! 2. Release the current link: the provider session closes, the old
//!    transport shuts down (host/fake: the serial framing thread ENDS; the
//!    browser connector closes its Web Serial port), and the port is free
//!    for the management tool.
//! 3. Run the connector's `manage_with_events`, folding the connector-level
//!    [`LinkManagementEventSink`] into [`DeviceEvent`] at the studio-facing
//!    surface (`Log` → `LogLine`, `Progress` → `Progress`).
//! 4. **Reconnect = rebuild**: a brand-new provider session + transport on
//!    the same endpoint, then the readiness engine re-runs from `Booting`.
//!    The state lands wherever the device's NEW firmware boots to —
//!    post-erase that is [`DeviceState::BlankFlash`], and for an erase that
//!    IS success.
//!
//! The same rebuild is exposed as [`DeviceSession::reconnect`] for
//! plain-disconnect recovery: terminal states (`Gone`, `Incompatible`, …)
//! are sticky under passive observation, and an explicit rebuild — which
//! replaces the whole link generation and re-runs readiness from scratch —
//! is the ONE way out of them.
//!
//! [`DeviceMode::Management`]: super::device_mode::DeviceMode::Management

use crate::provider::management_event::{LinkManagementEvent, LinkManagementEventSink};
use crate::provider::management_request::LinkManagementRequest;
use crate::provider::management_result::LinkManagementResult;
use crate::{LinkError, LinkProvider};

use super::device_event::{DeviceEvent, DeviceEventSink, DeviceLineOrigin};
use super::device_link_mode::DeviceLinkMode;
use super::device_session::DeviceSession;
use super::device_state::DeviceState;

/// Result of one management cycle: what the connector's operation produced
/// plus where the rebuilt link's readiness landed.
#[derive(Debug)]
pub struct DeviceManageOutcome {
    /// The connector-level management result (manifest, chip name, logs…).
    pub result: LinkManagementResult,
    /// Post-rebuild readiness outcome. Post-flash this should be
    /// [`DeviceState::Ready`]; post-erase it is [`DeviceState::BlankFlash`]
    /// — success for an erase. [`DeviceState::Gone`] means the OPERATION
    /// succeeded but the post-op rebuild failed: the result is still real
    /// (the device was flashed/erased), the link just needs a
    /// [`DeviceSession::reconnect`].
    pub state: DeviceState,
}

impl DeviceSession {
    /// Run one management operation (flash/erase/reset) with exclusive
    /// ownership of the wire, then rebuild the link and re-run readiness.
    ///
    /// Progress/log events from the connector arrive on `sink` folded into
    /// the [`DeviceEvent`] vocabulary; state transitions keep flowing to the
    /// sink installed at [`DeviceSession::connect`].
    ///
    /// On OPERATION failure the session record's status becomes
    /// `LinkSessionStatus::Error`, the state lands on [`DeviceState::Gone`]
    /// (the wire was released), and the mode returns to `AppProtocol`; the
    /// channel becomes usable again after a successful
    /// [`Self::reconnect`]. A rebuild failure AFTER a successful operation
    /// is NOT an error: the outcome is returned with `state: Gone` so the
    /// operation's result survives (the caller degrades to a notice and
    /// offers reconnect).
    pub async fn manage(
        &self,
        request: LinkManagementRequest,
        sink: DeviceEventSink,
    ) -> Result<DeviceManageOutcome, LinkError> {
        let _mode = self.try_begin_management()?;
        let session_id = self.shared.session_id();
        self.shared.release_link().await;
        let result = self
            .shared
            .connector()
            .manage_with_events(&session_id, request, fold_into_device_events(&sink))
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.shared
                    .record_link_failure(&format!("device management failed: {error}"));
                return Err(error);
            }
        };
        // Identity evidence rides the operation that was going to open the
        // bootloader anyway (A2): the flash preflight reads the base MAC
        // between the chip guard and the write. Recorded BEFORE the rebuild
        // so it survives one that fails, and additive by construction —
        // `record_probed_mac` drops anything it cannot validate, so a
        // provider that reads nothing changes nothing here.
        if let LinkManagementResult::FlashFirmware(flashed) = &result {
            self.shared.record_probed_mac(flashed.probed_mac.as_deref());
        }
        // The operation SUCCEEDED; a failed rebuild must not swallow that
        // result (P5 regression). Degrade: report the outcome with the state
        // `rebuild_link` recorded (`Gone`) so the caller can surface the
        // success and offer a reconnect.
        if let Err(error) = self.shared.rebuild_link().await {
            sink.emit(DeviceEvent::LogLine {
                line: format!("device link rebuild failed after management: {error}"),
                origin: DeviceLineOrigin::Link,
            });
            return Ok(DeviceManageOutcome {
                result,
                state: self.shared.state(),
            });
        }
        let state = self.shared.drive_readiness().await;
        Ok(DeviceManageOutcome { result, state })
    }

    /// Escalate to an **authoritative** link-mode determination by asking
    /// the device directly.
    ///
    /// Runs the esptool SYNC handshake, which only a ROM/stub bootloader
    /// answers. This is the one way to distinguish "in download mode" from
    /// "not responding": USB-Serial-JTAG parts present identical enumeration
    /// data in both states, so nothing short of talking to the chip can tell
    /// them apart.
    ///
    /// **This reboots the device.** The handshake drives DTR/RTS to enter
    /// download mode, and on USB-Serial-JTAG that reset drops USB
    /// enumeration. It therefore takes [`DeviceMode::Management`] and
    /// rebuilds the link afterwards, exactly like [`Self::manage`] — never
    /// call it speculatively on a healthy board. `DeviceLinkMode::App`
    /// already answers `probe_would_help() == false` for this reason.
    ///
    /// A probe that goes unanswered is NOT proof of `Unknown`: a device
    /// happily running the app ignores SYNC too. So on probe failure this
    /// reports whatever the rebuilt link's PASSIVE classification says,
    /// which is the honest answer rather than a guess.
    ///
    /// [`DeviceMode::Management`]: super::device_mode::DeviceMode::Management
    pub async fn probe_link_mode(
        &self,
        sink: DeviceEventSink,
    ) -> Result<DeviceLinkMode, LinkError> {
        let _mode = self.try_begin_management()?;
        let session_id = self.shared.session_id();
        self.shared.release_link().await;
        let probed = self
            .shared
            .connector()
            .probe_target(&session_id, fold_into_device_events(&sink))
            .await;

        // The probe rebooted the device whether or not it answered, so the
        // link must be rebuilt either way before anything can be said about
        // the device's state.
        if let Err(error) = self.shared.rebuild_link().await {
            sink.emit(DeviceEvent::LogLine {
                line: format!("device link rebuild failed after probe: {error}"),
                origin: DeviceLineOrigin::Link,
            });
            // A probe that answered is still authoritative; the rebuild
            // failing does not unsay it.
            return probed.map(DeviceLinkMode::from_sync_probe);
        }
        let state = self.shared.drive_readiness().await;

        match probed {
            Ok(chip_name) => Ok(DeviceLinkMode::from_sync_probe(chip_name)),
            Err(error) => {
                sink.emit(DeviceEvent::LogLine {
                    line: format!("no bootloader answered ({error}); classifying passively"),
                    origin: DeviceLineOrigin::Link,
                });
                let _ = state;
                Ok(self.shared.passive_link_mode())
            }
        }
    }

    /// Rebuild the link on the same endpoint and re-run readiness: recovery
    /// from `Gone` (device unplugged/rebooted) and the way out of every
    /// sticky terminal state. Refused while a management operation holds the
    /// wire or a request is in flight.
    pub async fn reconnect(&self) -> Result<DeviceState, LinkError> {
        // The mode guard makes the rebuild exclusive: the channel errors
        // cleanly instead of racing the transport swap.
        let _mode = self.try_begin_management()?;
        self.shared.release_link().await;
        self.shared.rebuild_link().await?;
        Ok(self.shared.drive_readiness().await)
    }
}

/// The fold from the connector-facing management event vocabulary into the
/// studio-facing [`DeviceEvent`] one. `LinkManagementEventSink` stays as the
/// lpa-link-internal bridge type the providers feed; consumers of a
/// [`DeviceSession`] only ever see `DeviceEvent`.
fn fold_into_device_events(sink: &DeviceEventSink) -> LinkManagementEventSink {
    let sink = sink.clone();
    LinkManagementEventSink::new(move |event| match event {
        LinkManagementEvent::Log { message } => sink.emit(DeviceEvent::LogLine {
            line: message,
            origin: DeviceLineOrigin::Link,
        }),
        LinkManagementEvent::Progress(progress) => sink.emit(DeviceEvent::Progress {
            label: progress.label,
            percent: progress
                .percent
                .map(|percent| u8::try_from(percent.min(100)).unwrap_or(100)),
        }),
    })
}
