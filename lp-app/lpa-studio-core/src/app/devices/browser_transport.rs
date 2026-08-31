//! [`DeviceTransport`] over the browser's Web Serial provider (wasm only).
//!
//! A thin join, and deliberately so: the provider still owns the port, the
//! grant lifecycle and the JS read pump; `lpa-link`'s `BrowserSerialLink`
//! already turns that promise-shaped surface into the model's event queue.
//! What is left here is the three things the model cannot ask a link for —
//! which grants exist, one more grant please, and here is a grant back.
//!
//! ⚠️ **Brave revokes Web Serial grants on reload; Chrome persists them.** A
//! discovery that returns nothing is therefore an ordinary answer on some
//! browsers, and the page must read as "no devices yet", never as an error.

use std::rc::Rc;

use lpa_devices::link::LinkInfo;
use lpa_link::device_link::browser_serial::BrowserSerialLink;
use lpa_link::device_link::wire::link_info;
use lpa_link::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
use lpa_link::{LinkEndpointId, LinkManagementEvent, LinkManagementEventSink, LinkProvider};

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink,
};

/// Where the board runtime manifest lives on a device (read by the
/// firmware's loader at boot — effective next restart, board-selection D4).
const DEVICE_HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

/// The browser Web Serial transport.
pub struct BrowserSerialTransport {
    provider: Rc<BrowserSerialEsp32Provider>,
}

impl BrowserSerialTransport {
    /// Wrap a provider, or `None` when this browser has no Web Serial at all
    /// (Safari, Firefox). Refusing to construct is how the devices page ends
    /// up saying "this browser cannot talk to USB devices" instead of showing
    /// an empty roster that looks like "you have none".
    pub fn new(provider: Rc<BrowserSerialEsp32Provider>) -> Option<Self> {
        provider.is_serial_supported().then_some(Self { provider })
    }

    /// One granted endpoint as a closed link.
    fn granted(
        provider: &Rc<BrowserSerialEsp32Provider>,
        endpoint: &lpa_link::LinkEndpoint,
        usb_vid_pid: Option<(u16, u16)>,
    ) -> GrantedLink {
        let info = link_info(endpoint, usb_vid_pid);
        GrantedLink {
            link: Box::new(BrowserSerialLink::new(
                Rc::clone(provider),
                endpoint.id.clone(),
                info.clone(),
            )),
            info,
        }
    }
}

impl DeviceTransport for BrowserSerialTransport {
    fn label(&self) -> &'static str {
        "browser Web Serial"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        let provider = Rc::clone(&self.provider);
        Box::pin(async move {
            let granted = provider
                .discover_granted_endpoints_with_usb()
                .await
                .map_err(|error| error.to_string())?;
            Ok(granted
                .iter()
                // Under Brave's `SerialAllowAllPortsForUrls` policy,
                // `getPorts()` also surfaces Bluetooth serial nodes (no USB
                // ids at all). Only ports that could be an ESP32-class
                // bridge become links — the same allowlist the chooser
                // filter uses — so junk grants can never mint pending
                // cards. The chooser path is unfiltered on purpose: a
                // HUMAN pick through `requestPort` is already filtered by
                // the JS side, and honoring it beats second-guessing it.
                .filter(|grant| {
                    let keep = lpa_link::is_esp32_serial_candidate(grant.usb_vid_pid);
                    if !keep {
                        log::debug!(
                            "granted port {} is not an ESP32-class serial bridge; skipping",
                            grant.endpoint.id.as_str()
                        );
                    }
                    keep
                })
                .map(|grant| Self::granted(&provider, &grant.endpoint, grant.usb_vid_pid))
                .collect())
        })
    }

    fn run_effect(
        &self,
        info: LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        let provider = Rc::clone(&self.provider);
        Box::pin(async move {
            let endpoint = LinkEndpointId::new(info.endpoint.0.clone());
            // The connector-level event stream becomes the progress sink the
            // effects layer turns into `ActivityMarker::Progress` — the ONE
            // road to the screen (the 2026-07-28 defect's lesson). Logs ride
            // it too, as label-only progress, so the card's narration keeps
            // moving between percent ticks.
            let events = LinkManagementEventSink::new(move |event| match event {
                LinkManagementEvent::Log { message } => (progress)(message, None),
                LinkManagementEvent::Progress(update) => (progress)(
                    update.label,
                    update
                        .percent
                        .map(|percent| u8::try_from(percent.min(100)).unwrap_or(100)),
                ),
            });
            match call {
                DeviceEffectCall::FlashFirmware { build_id } => {
                    // `flashFirmware` in the JS releases the port's
                    // reader/writer and closes it before esptool builds its
                    // transport — the release half of the exclusive-borrow
                    // discipline; the effects layer paused the pump for the
                    // read half. The chip guard and the pre-write
                    // `readBaseMac` live in that same JS and are
                    // load-bearing — this path must never bypass them.
                    let result = provider
                        .flash_firmware_with_events(&endpoint, Some(&build_id), events)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: format!("wrote {}", result.manifest.display_name),
                        probed_mac: result.base_mac,
                        chip_name: result.chip_name,
                    })
                }
                DeviceEffectCall::EraseFlash => {
                    // Same exclusive-borrow discipline as the flash; the
                    // erase's own verification (completion line outranking
                    // the benign flash-id warning on C6 rev 2) lives in the
                    // shipped JS.
                    provider
                        .erase_device_flash_with_events(&endpoint, events)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: "flash erased".to_string(),
                        ..Default::default()
                    })
                }
                DeviceEffectCall::WriteHardwareManifest { manifest_json } => {
                    provider
                        .write_device_file(
                            &endpoint,
                            DEVICE_HARDWARE_MANIFEST_PATH,
                            manifest_json.as_bytes(),
                            events,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: "board manifest written".to_string(),
                        ..Default::default()
                    })
                }
                DeviceEffectCall::PushProject {
                    files,
                    expected_hash,
                    fallback_storage_id,
                } => {
                    // The conversation is `lpa-client`'s, on the raw `M!`
                    // line framing the JS controller already does. The
                    // effects layer paused this port's pump before calling
                    // us — two drainers would split the responses between
                    // them and both halves would look like a dead board.
                    let report = provider
                        .push_device_project(
                            &endpoint,
                            &files,
                            &expected_hash,
                            &fallback_storage_id,
                            events,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: format!("project sent to {}", report.storage_id),
                        ..Default::default()
                    })
                }
            }
        })
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        let provider = Rc::clone(&self.provider);
        Box::pin(async move {
            let endpoint = match provider.request_access().await {
                Ok(endpoint) => endpoint,
                // A dismissed chooser is not a failure, and the shape the
                // provider reports it in is a cancellation.
                Err(lpa_link::LinkError::Cancelled { .. }) => return Ok(None),
                Err(error) => return Err(error.to_string()),
            };
            // The chooser answers with an endpoint; the USB pair comes from
            // the same enumeration the sweep reads, so a picked port and a
            // swept one describe themselves identically.
            let usb_vid_pid = provider
                .discover_granted_endpoints_with_usb()
                .await
                .ok()
                .and_then(|granted| {
                    granted
                        .iter()
                        .find(|grant| grant.endpoint.id == endpoint.id)
                        .and_then(|grant| grant.usb_vid_pid)
                });
            Ok(Some(Self::granted(&provider, &endpoint, usb_vid_pid)))
        })
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        let provider = Rc::clone(&self.provider);
        Box::pin(async move {
            let endpoint = LinkEndpointId::new(info.endpoint.0);
            provider
                .forget_endpoint(&endpoint)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
}
