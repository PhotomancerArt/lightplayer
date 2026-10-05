//! Which way a board's firmware update goes: over the air (backed up first,
//! rolled back by itself on a failed start, the board's files kept) or the
//! USB flash.
//!
//! Over the air on **every** link, USB included, when the board can update
//! over its link and the link carries lp-link's update channel: one path,
//! and the safer one. Everything else keeps the flash. That was a question
//! for the product owner (QY1); the answer is the one switch below, and a
//! "no" turns only the USB case back to the flash.

/// QY1: over a USB cable, a board that can update over the air does.
pub const USB_UPDATES_OVER_THE_AIR: bool = true;

/// The link an update would ride, as the card's words name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateLink {
    Usb,
    Bluetooth,
}

impl UpdateLink {
    /// The link's name in a sentence ("Updating over Bluetooth…").
    pub fn word(self) -> &'static str {
        match self {
            Self::Usb => "USB",
            Self::Bluetooth => "Bluetooth",
        }
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
}

/// The route for a board that `can_update_over_link` (its manifest names a
/// split layout and a chip this Studio knows) reached over `link`, which
/// `carries_update_channel` or not.
pub fn update_route(
    can_update_over_link: bool,
    link: UpdateLink,
    carries_update_channel: bool,
) -> UpdateRoute {
    route_with(
        USB_UPDATES_OVER_THE_AIR,
        can_update_over_link,
        link,
        carries_update_channel,
    )
}

/// [`update_route`] with QY1's switch as a parameter, so both answers are
/// tested.
fn route_with(
    usb_over_the_air: bool,
    can_update_over_link: bool,
    link: UpdateLink,
    carries_update_channel: bool,
) -> UpdateRoute {
    let link_allows = match link {
        UpdateLink::Usb => usb_over_the_air,
        UpdateLink::Bluetooth => true,
    };
    if can_update_over_link && carries_update_channel && link_allows {
        UpdateRoute::OverTheAir
    } else {
        UpdateRoute::Flash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_usb_a_board_that_can_goes_over_the_air() {
        assert_eq!(
            update_route(true, UpdateLink::Usb, true),
            UpdateRoute::OverTheAir
        );
    }

    #[test]
    fn over_bluetooth_a_board_that_can_goes_over_the_air() {
        assert_eq!(
            update_route(true, UpdateLink::Bluetooth, true),
            UpdateRoute::OverTheAir
        );
    }

    /// A sim (or any link without lp-link's update channel) never does.
    #[test]
    fn a_link_without_the_update_channel_keeps_the_flash() {
        for link in [UpdateLink::Usb, UpdateLink::Bluetooth] {
            assert_eq!(update_route(true, link, false), UpdateRoute::Flash);
        }
    }

    #[test]
    fn a_board_that_cannot_update_over_its_link_keeps_the_flash() {
        for link in [UpdateLink::Usb, UpdateLink::Bluetooth] {
            assert_eq!(update_route(false, link, true), UpdateRoute::Flash);
        }
    }

    /// QY1 answered "no": USB keeps the flash, and nothing else changes.
    #[test]
    fn the_no_answer_turns_only_usb_back_to_the_flash() {
        assert_eq!(
            route_with(false, true, UpdateLink::Usb, true),
            UpdateRoute::Flash
        );
        assert_eq!(
            route_with(false, true, UpdateLink::Bluetooth, true),
            UpdateRoute::OverTheAir
        );
    }

    #[test]
    fn the_links_are_named_in_plain_words() {
        assert_eq!(UpdateLink::Usb.word(), "USB");
        assert_eq!(UpdateLink::Bluetooth.word(), "Bluetooth");
    }
}
