//! What the add slot says under a transport button this browser cannot use
//! (BLE G3): the reason, and a way to continue — never a dead end.
//!
//! Both of the slot's buttons are always drawn. One this browser cannot
//! drive is DISABLED, with its reason under it; the way forward (a link to
//! Bluefy, the Brave flag, this page's address to open elsewhere) is text to
//! select and copy, never folded behind a click — the repo's rule for
//! pasteable text.

/// Bluefy – Web BLE Browser, on the App Store (PNN SOFT, app id
/// 1492822055). Region-neutral: apps.apple.com redirects to the visitor's
/// own storefront. Verified 2026-09-24 (200, title "Bluefy – Web BLE
/// Browser App - App Store"; the iTunes lookup for the id names the app).
pub const BLUEFY_APP_STORE_URL: &str =
    "https://apps.apple.com/app/bluefy-web-ble-browser/id1492822055";

/// What a disabled transport button says, and how to go on from here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReachNote {
    /// Why the button is disabled — one short sentence.
    pub reason: &'static str,
    /// Somewhere to go first (the App Store, for Bluefy).
    pub link: Option<ReachLink>,
    /// The line that introduces [`Self::copy`] ("Then open this page in
    /// Bluefy:").
    pub copy_lead: Option<&'static str>,
    /// Text to select and copy.
    pub copy: Option<ReachCopy>,
}

/// A link out of the page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReachLink {
    pub label: &'static str,
    pub href: &'static str,
}

/// What the select-and-copy line holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReachCopy {
    /// Fixed text (the Brave flag's address).
    Text(&'static str),
    /// This page's own address, to open in a browser that can.
    ThisPage,
}

impl ReachNote {
    /// A bare reason, no way forward beyond what it says.
    pub const fn reason(reason: &'static str) -> Self {
        Self {
            reason,
            link: None,
            copy_lead: None,
            copy: None,
        }
    }
}

/// Where the USB button stands when this browser has no Web Serial (iPhone,
/// iPad, Bluefy, Firefox, Safari): the reason, and this page's address to
/// open in a browser that has it.
pub const USB_UNAVAILABLE: ReachNote = ReachNote {
    reason: "USB needs Chrome or Edge on a computer.",
    link: None,
    copy_lead: Some("Open this page there:"),
    copy: Some(ReachCopy::ThisPage),
};

/// This page's address, as the copy line shows it. The host build (tests,
/// a story with no override) has no window and answers the product's own
/// address.
pub fn this_page_url() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(href) = web_sys::window().and_then(|window| window.location().href().ok()) {
            return href;
        }
    }
    "https://lightplayer.app/devices".to_string()
}
