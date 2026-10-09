//! Test fixtures for the board card: a board's facts, owned, and its verbs
//! published by the same functions the controller calls
//! (`publish_device_offers`), so a test reads the card the builder makes
//! from real offers — never from actions a test built.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{ActivityView, DeviceView, Escape, FirmwareFace, LoadedProject};
use lpa_devices::{ActivityKind, DeviceId, FirmwareAge, WireVersion};

use super::board_card_input::BoardCardInput;
use crate::app::access::{UiDeviceAccess, UiUnlockOffer, device_unlock_offer};
use crate::app::devices::activity_ends::ActivityEnd;
use crate::app::devices::board_plays::BoardPlays;
use crate::app::devices::device_card_feed_view::{DeviceCardFeedView, FeedLiveness};
use crate::app::devices::device_layout_view::UiDeviceLayout;
use crate::app::devices::device_offers::{DeviceOfferFacts, device_offers};
use crate::app::devices::device_reset_reach::ResetReach;
use crate::app::devices::device_update_offers::UpdateOfferFacts;
use crate::app::devices::device_update_words::UiDeviceUpdate;
use crate::app::devices::devices_op::DeviceFace;
use crate::app::devices::edit_offer::device_edit_offer;
use crate::app::devices::lan_link_view::UiLanLink;
use crate::app::devices::relay_connect_offer::connect_relay_offer;
use crate::app::devices::runtime_band::UiRuntimeBand;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::devices::wifi_connect_offer::connect_wifi_offer;
use crate::app::devices::wifi_connects::UiWifiConnect;
use crate::app::home::{UiExampleCard, UiPackageCard};
use crate::app::network::UiDeviceWifi;
use crate::{OfferPath, UiOffer};

/// One board's facts, owned; [`Self::input`] publishes its verbs and lends
/// the builder its input.
#[derive(Clone, Debug)]
pub(crate) struct CardFixture {
    pub view: DeviceView,
    pub board: OfferPath,
    pub link: Option<UiLinkKind>,
    pub feed: Option<DeviceCardFeedView>,
    pub runtime: Option<UiRuntimeBand>,
    pub access: Option<UiDeviceAccess>,
    pub wifi: Option<UiDeviceWifi>,
    pub lan: Option<UiLanLink>,
    pub wifi_connect: Option<UiWifiConnect>,
    pub update: Option<UiDeviceUpdate>,
    /// The update standing the offers are published from.
    pub update_facts: UpdateOfferFacts,
    pub layout: Option<UiDeviceLayout>,
    pub plays: BoardPlays,
    pub sharing: usize,
    pub project: Option<UiPackageCard>,
    pub last_seen_at: Option<f64>,
    pub ended: Option<ActivityEnd>,
    pub editor_holds_it: bool,
    pub now: f64,
    /// The board's registry uid (its `edit` needs one).
    pub uid: Option<String>,
    /// How Reset reaches the board.
    pub reset: ResetReach,
    /// This browser remembers a Wi‑Fi address for it: `connect-wifi` on an
    /// offline board.
    pub wifi_address: bool,
    /// Someone is signed in: `connect-relay` on an offline board.
    pub relay: bool,
    /// More verbs the controller publishes beside the device's own (the
    /// layout verbs).
    pub extra_offers: Vec<UiOffer>,
    /// Published by [`Self::input`].
    offers: Vec<UiOffer>,
}

impl CardFixture {
    /// A Ready LightPlayer on an open USB port, idle, its board resolved,
    /// running `porch`, registered.
    pub fn ready() -> Self {
        Self {
            view: ready_view(),
            board: board(),
            link: Some(UiLinkKind::Usb),
            feed: None,
            runtime: None,
            access: None,
            wifi: None,
            lan: None,
            wifi_connect: None,
            update: None,
            update_facts: UpdateOfferFacts::default(),
            layout: None,
            plays: BoardPlays::Running {
                label: "porch".to_string(),
            },
            sharing: 0,
            project: None,
            last_seen_at: None,
            ended: None,
            editor_holds_it: false,
            now: 1_000_000.0,
            uid: Some("devporch0000000000".to_string()),
            reset: ResetReach::Lines,
            wifi_address: false,
            relay: false,
            extra_offers: Vec::new(),
            offers: Vec::new(),
        }
    }

    /// The same board, remembered and not here: no link, Reconnect and
    /// Forget.
    pub fn offline() -> Self {
        let mut fixture = Self::ready();
        fixture.view.status = DeviceStatus::Offline;
        fixture.view.state_label = "Offline".to_string();
        fixture.view.firmware_face = FirmwareFace::Unknown;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        fixture.view.escapes = vec![Escape::Reconnect, Escape::Forget];
        fixture.plays = BoardPlays::Unknown;
        fixture
    }

    /// The same board with `activity` running.
    pub fn with_activity(mut self, activity: ActivityView) -> Self {
        self.view.status = DeviceStatus::Busy;
        self.view.can_receive_project = false;
        self.view.can_remove_project = false;
        if activity.cancellable {
            self.view.escapes.insert(0, Escape::Cancel);
        }
        self.view.activity = Some(activity);
        self
    }

    /// The same board reached over `link` (a network link refuses
    /// firmware, as the model says).
    pub fn over(mut self, link: UiLinkKind) -> Self {
        self.link = Some(link);
        if link != UiLinkKind::Usb {
            self.view.firmware_blocked = Some(lpa_devices::view::FIRMWARE_NEEDS_USB.to_string());
        }
        self
    }

    /// The same board, Locked over its untrusted link.
    pub fn locked(mut self) -> Self {
        self.access = Some(UiDeviceAccess {
            unlock: Some(UiUnlockOffer::Locked),
            ..UiDeviceAccess::default()
        });
        self
    }

    /// The board's verbs, published as the controller publishes them, and
    /// the builder's input over them.
    pub fn input(&mut self) -> BoardCardInput<'_> {
        self.offers = self.publish();
        BoardCardInput {
            view: &self.view,
            board: &self.board,
            offers: &self.offers,
            link: self.link,
            feed: self.feed.as_ref(),
            runtime: self.runtime.as_ref(),
            access: self.access.as_ref(),
            wifi: self.wifi.as_ref(),
            lan: self.lan.as_ref(),
            wifi_connect: self.wifi_connect.as_ref(),
            update: self.update.as_ref(),
            layout: self.layout.as_ref(),
            plays: &self.plays,
            sharing: self.sharing,
            project: self.project.as_ref(),
            last_seen_at: self.last_seen_at,
            ended: self.ended.as_ref(),
            editor_holds_it: self.editor_holds_it,
            now: self.now,
        }
    }

    /// Every verb the board publishes now.
    pub fn offers(&mut self) -> Vec<UiOffer> {
        self.offers = self.publish();
        self.offers.clone()
    }

    fn publish(&self) -> Vec<UiOffer> {
        let face = match self.runtime.is_some() {
            true => DeviceFace::Sim,
            false => DeviceFace::Wire,
        };
        let unlock = self.access.as_ref().and_then(|access| access.unlock);
        let projects: Vec<UiPackageCard> = self.project.iter().cloned().collect();
        let examples = examples();
        let facts = DeviceOfferFacts {
            prefix: self.board.clone(),
            face,
            autoconnect: false,
            locked: unlock == Some(UiUnlockOffer::Locked),
            reset: self.reset,
            banked: self.project.is_some(),
            projects: &projects,
            examples: &examples,
            update: self.update_facts.clone(),
        };
        let mut offers = device_offers(&self.view, &facts);
        let offline_wire = face == DeviceFace::Wire && self.view.status == DeviceStatus::Offline;
        if self.wifi_address && offline_wire {
            offers.push(connect_wifi_offer(
                &self.board,
                self.view.id,
                "10.0.0.5",
                false,
            ));
        }
        if self.relay && offline_wire {
            offers.push(connect_relay_offer(&self.board, self.view.id, false));
        }
        offers.extend(device_unlock_offer(&self.board, &self.view, unlock));
        offers.extend(device_edit_offer(
            &self.board,
            &self.view,
            self.uid.as_deref(),
        ));
        offers.extend(self.extra_offers.iter().cloned());
        offers
    }
}

/// `devices/mac-a0f26287b48c`.
pub(crate) fn board() -> OfferPath {
    OfferPath::board(&crate::BoardRef::Mac(
        lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
    ))
}

/// An activity of `kind`, cancellable, saying `label` at `percent`.
pub(crate) fn activity(kind: ActivityKind, label: &str, percent: Option<u8>) -> ActivityView {
    ActivityView {
        kind,
        label: label.to_string(),
        percent,
        cancellable: true,
        cancel_requested: false,
        layout: None,
        update: None,
    }
}

/// The gallery's one example, so a push has something to offer.
fn examples() -> Vec<UiExampleCard> {
    vec![UiExampleCard {
        id: "catalog/plasma".to_string(),
        name: "Plasma".to_string(),
        kind: lpc_model::ProjectKind::General,
        description: String::new(),
    }]
}

/// A Ready LightPlayer on an open USB port, idle, running `porch`.
fn ready_view() -> DeviceView {
    let board = crate::flash_offer(Some("esp32c6")).candidates[0]
        .board_id
        .clone();
    DeviceView {
        id: DeviceId(7),
        title: "Porch".to_string(),
        status: DeviceStatus::Ready,
        state_label: "Ready".to_string(),
        detail: Some("LightPlayer · seeed/xiao-esp32-c6".to_string()),
        freshness_label: None,
        identity_label: Some("a0:f2:62:87:b4:8c".to_string()),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some(board),
        firmware_face: FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 2026.10.05-2".to_string()),
            wire: WireVersion::Match,
            age: FirmwareAge::Current,
        },
        remembered_firmware: None,
        degraded: None,
        loaded_project: LoadedProject::Running {
            label: "porch".to_string(),
        },
        engine_fps: Some(58),
        link_counters: None,
        can_receive_project: true,
        can_remove_project: true,
        activity: None,
        last_outcome: None,
        last_update_outcome: None,
        terminal: Vec::new(),
        terminal_dropped: 0,
        firmware_blocked: None,
        update_blocked: None,
        escapes: vec![Escape::Disconnect, Escape::Forget],
    }
}

/// A picture in `liveness`, with geometry when `with_layout` (a frame
/// without it is bytes the card names rather than draws).
pub(crate) fn feed(liveness: FeedLiveness, with_layout: bool) -> DeviceCardFeedView {
    let layout = with_layout.then(|| {
        std::rc::Rc::new(lpc_model::ControlDisplayLayout::Layout2d(
            lpc_model::ControlLayout2d::new(lpc_model::Revision::new(7), 4, 1, Vec::new()),
        ))
    });
    DeviceCardFeedView {
        frame: (liveness != FeedLiveness::Waiting).then(|| crate::UiControlProductPreview {
            revision: 3,
            extent: lpc_model::ControlExtent::new(1, 12),
            sample_format: crate::UiControlSampleFormat::Srgb8,
            sample_layout: lpc_model::ControlSampleLayout { spans: Vec::new() },
            display_layout: layout,
            bytes: std::rc::Rc::from(vec![0u8; 12]),
        }),
        frame_age_secs: Some(12.0),
        engine_fps: Some(43),
        liveness,
    }
}
