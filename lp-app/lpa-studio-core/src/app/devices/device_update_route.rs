//! Which way a board's firmware update goes: over the air (backed up first,
//! rolled back by itself on a failed start, the board's files kept) or the
//! USB flash.
//!
//! Over the air on **every** link, USB included, when the board can update
//! over its link, the link carries lp-link's update channel, **and this
//! Studio holds an update-capable build** of what it would install: one
//! path, and the safer one. That was a question for the product owner
//! (QY1); the answer is the one switch below, and a "no" turns only the USB
//! case back to the flash. N12 settled it: USB stays over the air.
//!
//! **The build is half the route** (the director's note on P8): a local or
//! dev Studio can carry a build with no update files — a single image, the
//! fast local build — and then no over-the-air install is possible, whatever
//! the board can do. Such a Studio has no own-build facts at all: the web's
//! own-build source yields them only for a package whose
//! `ota-manifest.json` and manifest core's `ota` field check out.
//!
//! | board can | build can | link | route |
//! |---|---|---|---|
//! | yes | yes | carries channel 3 | [`UpdateRoute::OverTheAir`] |
//! | yes | no | USB | [`UpdateRoute::Flash`] (today's flash, Lasting) |
//! | yes | no | Bluetooth, Wi‑Fi or the relay, carries channel 3 | [`UpdateRoute::NoWirelessBuild`]: no install offer, a plain line why |
//! | no, or the link carries no channel 3 | — | any | [`UpdateRoute::Flash`] |
//!
//! A restore or a finish needs none of this: it puts back the board's own
//! build, from the engine cache or the store, with no click.

use lpa_devices::identity::EndpointKey;
use lpc_access::Tier;

/// QY1: over a USB cable, a board that can update over the air does.
pub const USB_UPDATES_OVER_THE_AIR: bool = true;

/// The first release that serves the update channel over Bluetooth:
/// `2026.10.07-16`, the release of #1005's merge (`c0adadae7`), published
/// 2026-10-07T16:28:38Z. A release older than it, installed over Bluetooth,
/// cannot be updated over Bluetooth again until it has been connected by
/// USB once, so "Other version…" warns about it and arms. (`None` would
/// mean "not in a release yet": every choice older than the board's would
/// warn over Bluetooth.)
pub const FIRST_BLUETOOTH_UPDATE_RELEASE: Option<&str> = Some("2026.10.07-16");

/// The first release that serves the update channel over Wi‑Fi (the LAN):
/// `2026.10.08-2`, the release of #1035's merge (`3c1524500`, the board
/// taking updates over Wi‑Fi). A release older than it ignores the channel
/// on its LAN link, so "Other version…" warns about it and arms.
pub const FIRST_WIFI_UPDATE_RELEASE: Option<&str> = Some("2026.10.08-2");

/// The first release that serves the update channel through lightplayer.app's
/// relay: `2026.10.08-9`, the release of #1044's merge (`9aff4f66f`). An
/// older release is updated nearby once before the relay can update it.
pub const FIRST_RELAY_UPDATE_RELEASE: Option<&str> = Some("2026.10.08-9");

/// The link an update would ride, as the card's words name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateLink {
    Usb,
    Bluetooth,
    /// The board's secure link on the LAN (`lan:`).
    Wifi,
    /// The board's secure link through lightplayer.app's relay (`relay:`):
    /// Wi‑Fi in the card's words, but its own reach — a board whose
    /// firmware predates updates through the relay is updated nearby once
    /// (USB, Bluetooth or its own network).
    Relay,
}

impl UpdateLink {
    /// The link a board reached at `endpoint` updates over: Bluetooth for
    /// `ble:`, Wi‑Fi for `lan:`, the relay for `relay:`, USB for anything
    /// else (a serial port, the emulator's door, a sim — or no link at all).
    pub fn of_endpoint(endpoint: Option<&EndpointKey>) -> Self {
        match endpoint {
            Some(endpoint) if endpoint.is_bluetooth() => Self::Bluetooth,
            Some(endpoint) if endpoint.is_lan() => Self::Wifi,
            Some(endpoint) if endpoint.is_relay() => Self::Relay,
            _ => Self::Usb,
        }
    }

    /// The link's name in a sentence ("Updating over Bluetooth…"). The
    /// relay is Wi‑Fi too; the card's link line already says "via
    /// lightplayer.app".
    pub fn word(self) -> &'static str {
        match self {
            Self::Usb => "USB",
            Self::Bluetooth => "Bluetooth",
            Self::Wifi | Self::Relay => "Wi\u{2011}Fi",
        }
    }

    /// A link with no cable: what the user holds on it is a grant (a key's
    /// tier), not physical access.
    pub fn is_wireless(self) -> bool {
        matches!(self, Self::Bluetooth | Self::Wifi | Self::Relay)
    }

    /// The first release that serves the update channel over this link:
    /// `None` for USB, which always does, and for a link whose release is
    /// not named.
    pub fn first_update_release(self) -> Option<&'static str> {
        match self {
            Self::Usb => None,
            Self::Bluetooth => FIRST_BLUETOOTH_UPDATE_RELEASE,
            Self::Wifi => FIRST_WIFI_UPDATE_RELEASE,
            Self::Relay => FIRST_RELAY_UPDATE_RELEASE,
        }
    }

    /// A Wi‑Fi link, on the board's network or through the relay.
    pub fn is_wifi(self) -> bool {
        matches!(self, Self::Wifi | Self::Relay)
    }

    /// The user's tier an update decision reads over this link: the access
    /// layer's `granted` tier over Bluetooth and Wi‑Fi, so a play-only user
    /// is not offered an update the board would refuse; `None` (trusted)
    /// over a cable.
    pub fn update_tier(self, granted: Option<Tier>) -> Option<Tier> {
        granted.filter(|_| self.is_wireless())
    }
}

/// Which way the card's update goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateRoute {
    /// The update channel: Update, Install, Reinstall.
    OverTheAir,
    /// Today's USB flash (`update-firmware`, Lasting), or nothing over a
    /// link that cannot carry it.
    #[default]
    Flash,
    /// Over Bluetooth, a board that could update there, but this Studio's
    /// build cannot be installed over the air: no install offer, and the
    /// card says why.
    NoWirelessBuild,
}

/// The route for a board that `board_can` update over its link (its
/// manifest names a split layout and a chip this Studio knows), when this
/// Studio `build_can` install an update-capable build over the air,
/// reached over `link`, which `carries_update_channel` or not.
pub fn update_route(
    board_can: bool,
    build_can: bool,
    link: UpdateLink,
    carries_update_channel: bool,
) -> UpdateRoute {
    route_with(
        USB_UPDATES_OVER_THE_AIR,
        board_can,
        build_can,
        link,
        carries_update_channel,
    )
}

/// [`update_route`] with QY1's switch as a parameter, so both answers are
/// tested.
fn route_with(
    usb_over_the_air: bool,
    board_can: bool,
    build_can: bool,
    link: UpdateLink,
    carries_update_channel: bool,
) -> UpdateRoute {
    let link_allows = match link {
        UpdateLink::Usb => usb_over_the_air,
        UpdateLink::Bluetooth | UpdateLink::Wifi | UpdateLink::Relay => true,
    };
    if !(board_can && carries_update_channel && link_allows) {
        return UpdateRoute::Flash;
    }
    match (build_can, link) {
        (true, _) => UpdateRoute::OverTheAir,
        (false, UpdateLink::Usb) => UpdateRoute::Flash,
        (false, UpdateLink::Bluetooth | UpdateLink::Wifi | UpdateLink::Relay) => {
            UpdateRoute::NoWirelessBuild
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Board × build capability × link, one test each (the director's note
    /// on P8). Over USB, both able: over the air.
    #[test]
    fn over_usb_a_board_and_a_build_that_can_go_over_the_air() {
        assert_eq!(
            update_route(true, true, UpdateLink::Usb, true),
            UpdateRoute::OverTheAir
        );
    }

    #[test]
    fn over_bluetooth_a_board_and_a_build_that_can_go_over_the_air() {
        assert_eq!(
            update_route(true, true, UpdateLink::Bluetooth, true),
            UpdateRoute::OverTheAir
        );
    }

    /// A single-image (or OTA-less) Studio build: over USB the board keeps
    /// today's flash.
    #[test]
    fn over_usb_a_build_that_cannot_keeps_the_flash() {
        assert_eq!(
            update_route(true, false, UpdateLink::Usb, true),
            UpdateRoute::Flash
        );
    }

    /// The same over Bluetooth and Wi‑Fi: no install at all, and the card
    /// says why.
    #[test]
    fn over_bluetooth_a_build_that_cannot_offers_no_install() {
        for link in [UpdateLink::Bluetooth, UpdateLink::Wifi, UpdateLink::Relay] {
            assert_eq!(
                update_route(true, false, link, true),
                UpdateRoute::NoWirelessBuild
            );
        }
    }

    /// Over Wi‑Fi (OTA M8), on the board's network or through the relay, a
    /// board and a build that can: over the air.
    #[test]
    fn over_wifi_a_board_and_a_build_that_can_go_over_the_air() {
        for link in [UpdateLink::Wifi, UpdateLink::Relay] {
            assert_eq!(
                update_route(true, true, link, true),
                UpdateRoute::OverTheAir
            );
        }
    }

    /// Each endpoint names its update link; the relay is its own, said
    /// Wi‑Fi.
    #[test]
    fn each_endpoint_names_its_update_link() {
        let link = |key: &str| UpdateLink::of_endpoint(Some(&EndpointKey(key.to_string())));
        assert_eq!(link("ble:QkxFLWlk"), UpdateLink::Bluetooth);
        assert_eq!(link("lan:ws://192.168.1.40/link"), UpdateLink::Wifi);
        assert_eq!(link("relay:a0f26287b48c"), UpdateLink::Relay);
        assert_eq!(link("serial:/dev/cu.usbmodem1"), UpdateLink::Usb);
        assert_eq!(UpdateLink::of_endpoint(None), UpdateLink::Usb);
        assert!(UpdateLink::Wifi.is_wireless() && UpdateLink::Bluetooth.is_wireless());
        assert!(UpdateLink::Relay.is_wireless() && UpdateLink::Relay.is_wifi());
        assert!(!UpdateLink::Usb.is_wireless());
    }

    /// A sim (or any link without lp-link's update channel) never does,
    /// whatever the build.
    #[test]
    fn a_link_without_the_update_channel_keeps_the_flash() {
        for link in [
            UpdateLink::Usb,
            UpdateLink::Bluetooth,
            UpdateLink::Wifi,
            UpdateLink::Relay,
        ] {
            for build_can in [true, false] {
                assert_eq!(
                    update_route(true, build_can, link, false),
                    UpdateRoute::Flash
                );
            }
        }
    }

    /// A pre-update board (a single image, no channel 3), whatever the
    /// build.
    #[test]
    fn a_board_that_cannot_update_over_its_link_keeps_the_flash() {
        for link in [
            UpdateLink::Usb,
            UpdateLink::Bluetooth,
            UpdateLink::Wifi,
            UpdateLink::Relay,
        ] {
            for build_can in [true, false] {
                assert_eq!(
                    update_route(false, build_can, link, true),
                    UpdateRoute::Flash
                );
            }
        }
    }

    /// QY1 answered "no": USB keeps the flash, and nothing else changes.
    #[test]
    fn the_no_answer_turns_only_usb_back_to_the_flash() {
        assert_eq!(
            route_with(false, true, true, UpdateLink::Usb, true),
            UpdateRoute::Flash
        );
        assert_eq!(
            route_with(false, true, true, UpdateLink::Bluetooth, true),
            UpdateRoute::OverTheAir
        );
    }

    /// Over Wi‑Fi, as over Bluetooth, the decision reads the user's granted
    /// tier (a play-only user is not offered an update); a cable is trusted.
    #[test]
    fn a_wireless_link_reads_the_granted_tier_and_a_cable_does_not() {
        for link in [UpdateLink::Bluetooth, UpdateLink::Wifi, UpdateLink::Relay] {
            assert_eq!(link.update_tier(Some(Tier::Play)), Some(Tier::Play));
            assert_eq!(link.update_tier(None), None);
        }
        assert_eq!(UpdateLink::Usb.update_tier(Some(Tier::Play)), None);
    }

    #[test]
    fn the_links_are_named_in_plain_words() {
        assert_eq!(UpdateLink::Usb.word(), "USB");
        assert_eq!(UpdateLink::Bluetooth.word(), "Bluetooth");
        assert_eq!(UpdateLink::Wifi.word(), "Wi\u{2011}Fi");
        assert_eq!(UpdateLink::Relay.word(), "Wi\u{2011}Fi");
    }
}
