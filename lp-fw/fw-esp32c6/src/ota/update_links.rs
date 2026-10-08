//! The host links the update session answers on: the USB link and, when the
//! image has Bluetooth, each radio link (and with Wi-Fi the LAN link) — by
//! the [`LinkId`] the session names it with.
//!
//! The session's link ids are the transport's: [`USB_LINK`] is the USB
//! cable (`lpc_shared`'s `LinkId::PRIMARY`, 0), and a radio link is the id
//! its connection was minted (`RadioLinkPort::mint_link`, from 1, never
//! reused).
//!
//! **One frame buffer.** A large answer (a read-back `D`) goes out of the
//! static frame buffer as the link's external message, and every link —
//! USB and radio — may still be reading its own last one from there. Each
//! link checks only itself, so this is where both are asked before a large
//! answer is handed to either: a link still reading makes it wait
//! (`UpdateSend::Later`), and the outbox keeps it.

use fw_esp32_common::update_send::UpdateSend;
use fw_esp32_common::usb_link::UsbLinkShared;
use fw_esp32_common::usb_link::usb_update_channel::RING_MAX;
use lpc_update::board::LinkId;

/// The one USB link, as the session names it.
pub const USB_LINK: LinkId = LinkId(0);

/// Where channel-3 answers go.
#[derive(Clone, Copy)]
pub struct UpdateLinks {
    pub usb: &'static UsbLinkShared,
    /// The radio links' port, when the image has Bluetooth (whether or not
    /// the device store started it: a port with no links answers nothing).
    #[cfg(feature = "ble")]
    pub radio: &'static fw_esp32_common::radio_link::RadioLinkPort,
}

impl UpdateLinks {
    /// Queue `bytes` on `link`'s channel 3.
    pub fn send(&self, link: LinkId, bytes: &[u8]) -> UpdateSend {
        if bytes.len() > RING_MAX && self.frame_buf_in_use() {
            return UpdateSend::Later;
        }
        if link == USB_LINK {
            return self.usb.send_update(bytes);
        }
        #[cfg(feature = "ble")]
        {
            self.radio.send_update(transport_link(link), bytes)
        }
        #[cfg(not(feature = "ble"))]
        UpdateSend::NoSession
    }

    /// The lp-link session `link` is in now; `None` when it is not open.
    pub fn generation(&self, link: LinkId) -> Option<u32> {
        if link == USB_LINK {
            return Some(self.usb.with_link(|l| l.generation()));
        }
        #[cfg(feature = "ble")]
        {
            self.radio.link_generation(transport_link(link))
        }
        #[cfg(not(feature = "ble"))]
        None
    }

    /// Some link still reads a long message out of the frame buffer.
    fn frame_buf_in_use(&self) -> bool {
        #[cfg(feature = "ble")]
        if self.radio.frame_buf_in_use() {
            return true;
        }
        self.usb.frame_buf_in_use()
    }
}

/// A transport link as the session names it.
#[cfg(feature = "ble")]
pub use fw_esp32_common::radio_link::core_only_links::session_link;

/// A session link as the transport names it.
#[cfg(feature = "ble")]
fn transport_link(link: LinkId) -> lpc_shared::transport::LinkId {
    lpc_shared::transport::LinkId::new(link.0)
}
