//! [`BoardCardInput`]: everything one board's card is built from — the
//! roster's projection, the side facts the controller joins beside it, and
//! the board's own verbs as the view's offer tree publishes them. Gathered by
//! the controller (`StudioController::view`) and by stories, so the builder
//! is a pure function of this.

use lpa_devices::DeviceView;
use lpa_devices::device::DeviceStatus;
use lpa_devices::view::Escape;

use crate::app::access::{UiDeviceAccess, UiUnlockOffer};
use crate::app::devices::activity_ends::ActivityEnd;
use crate::app::devices::board_plays::BoardPlays;
use crate::app::devices::device_card_feed_view::DeviceCardFeedView;
use crate::app::devices::device_layout_view::UiDeviceLayout;
use crate::app::devices::device_update_words::UiDeviceUpdate;
use crate::app::devices::lan_link_view::UiLanLink;
use crate::app::devices::runtime_band::UiRuntimeBand;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::devices::wifi_connects::UiWifiConnect;
use crate::app::home::UiPackageCard;
use crate::app::network::UiDeviceWifi;
use crate::{OfferPath, UiOffer};

/// One board's card, as facts. See the module docs.
#[derive(Clone, Copy, Debug)]
pub struct BoardCardInput<'a> {
    /// The roster's projection of the board.
    pub view: &'a DeviceView,
    /// `devices/<board ref>`: where the board's verbs live.
    pub board: &'a OfferPath,
    /// The board's own verbs, from the published tree: the card points only
    /// at these.
    pub offers: &'a [UiOffer],
    /// How the board is reached (the open link's kind, else the last one's),
    /// from the roster `Device`'s endpoint — never derived from
    /// `DeviceView::is_over_bluetooth`, which is true on every network link.
    pub link: Option<UiLinkKind>,
    /// The board's picture and its treatment.
    pub feed: Option<&'a DeviceCardFeedView>,
    /// The runtime band, for a stand-in (a simulated or in-tab emulated
    /// board); `None` for silicon.
    pub runtime: Option<&'a UiRuntimeBand>,
    pub access: Option<&'a UiDeviceAccess>,
    pub wifi: Option<&'a UiDeviceWifi>,
    /// The board reached on the LAN or through lightplayer.app right now.
    pub lan: Option<&'a UiLanLink>,
    /// A Wi‑Fi or relay connect under way, or why it failed.
    pub wifi_connect: Option<&'a UiWifiConnect>,
    /// The board's update story.
    pub update: Option<&'a UiDeviceUpdate>,
    /// The board's files across a layout change, and the project note.
    pub layout: Option<&'a UiDeviceLayout>,
    /// Which project the board plays ([`crate::BoardProjects`]).
    pub plays: &'a BoardPlays,
    /// How many boards play the same library project (this one included).
    pub sharing: usize,
    /// The library project `plays` names.
    pub project: Option<&'a UiPackageCard>,
    /// The registry row's `last_seen_at`, epoch seconds.
    pub last_seen_at: Option<f64>,
    /// How the board's last activity ended, and when.
    pub ended: Option<&'a ActivityEnd>,
    /// The editor's lens is on this board (the docked lens card).
    pub editor_holds_it: bool,
    /// Now, epoch seconds (the controller's clock).
    pub now: f64,
}

impl<'a> BoardCardInput<'a> {
    /// The board's offer for `verb` (`devices/<board>/<verb>`), when the
    /// tree publishes it.
    pub fn offer(&self, verb: &str) -> Option<&'a UiOffer> {
        offer_at(self.offers, self.board, verb)
    }

    /// The model holds a link to the board (Disconnect is offered exactly
    /// then).
    pub fn linked(&self) -> bool {
        self.view.escapes.contains(&Escape::Disconnect)
    }

    /// Nothing runs on the board.
    pub fn idle(&self) -> bool {
        self.view.activity.is_none()
    }

    /// Studio remembers the board and cannot reach it.
    pub fn offline(&self) -> bool {
        self.view.status == DeviceStatus::Offline
    }

    /// A stand-in: a simulated or in-tab emulated board, not silicon.
    pub fn stand_in(&self) -> bool {
        self.runtime.is_some()
    }

    /// The link holds nothing and no password Studio knows opened it.
    pub fn locked(&self) -> bool {
        self.access
            .is_some_and(|access| access.unlock == Some(UiUnlockOffer::Locked))
    }

    /// The link holds play only.
    pub fn play_only(&self) -> bool {
        self.access
            .is_some_and(|access| access.unlock == Some(UiUnlockOffer::PlayOnly))
    }

    /// The link's kind, USB when nothing has named one.
    pub fn link_kind(&self) -> UiLinkKind {
        self.link.unwrap_or_default()
    }
}

/// The offer for `verb` under `board` in `offers`.
pub(crate) fn offer_at<'a>(
    offers: &'a [UiOffer],
    board: &OfferPath,
    verb: &str,
) -> Option<&'a UiOffer> {
    let path = board.clone().child(verb);
    offers.iter().find(|offer| offer.path == path)
}

/// The icon token for a link: `usb`, `bluetooth`, `wifi`, or `cloud` for a
/// board reached through lightplayer.app.
pub fn link_icon(link: UiLinkKind) -> &'static str {
    match link {
        UiLinkKind::Usb => "usb",
        UiLinkKind::Bluetooth => "bluetooth",
        UiLinkKind::Wifi => "wifi",
        UiLinkKind::Relay => "cloud",
    }
}
