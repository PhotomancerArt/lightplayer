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
use lpa_link::{LinkEndpointId, LinkProvider};

use super::device_transport::{DeviceTransport, DeviceTransportFuture, GrantedLink};

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
                .map(|grant| Self::granted(&provider, &grant.endpoint, grant.usb_vid_pid))
                .collect())
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
