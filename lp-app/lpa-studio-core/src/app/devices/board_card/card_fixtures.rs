//! Test fixtures for the board card: a board's facts, owned, and its verbs
//! published by the same functions the controller calls
//! (`publish_device_offers`), so a test reads the card the builder makes
//! from real offers — never from actions a test built.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{ActivityView, DeviceView, Escape, FirmwareFace, LoadedProject};
use lpa_devices::{
    ActivityKind, DeviceId, FirmwareAge, HeldElsewhere, HoldLevel, HoldVia, WireVersion,
};

use super::board_card_input::{BoardCardInput, link_icon};
use super::board_connection::BoardConnection;
use super::ui_board_panel::UiBoardPanel;
use crate::app::access::{UiDeviceAccess, UiUnlockOffer, device_unlock_offer};
use crate::app::devices::activity_ends::ActivityEnd;
use crate::app::devices::board_plays::BoardPlays;
use crate::app::devices::connect_offer::{ConnectFacts, device_connect_offer};
use crate::app::devices::device_card_feed_view::{DeviceCardFeedView, FeedLiveness};
use crate::app::devices::device_layout_view::UiDeviceLayout;
use crate::app::devices::device_offers::{DeviceOfferFacts, device_offers};
use crate::app::devices::device_reset_reach::ResetReach;
use crate::app::devices::device_update_offers::UpdateOfferFacts;
use crate::app::devices::device_update_words::UiDeviceUpdate;
use crate::app::devices::devices_op::DeviceFace;
use crate::app::devices::done_offer::device_done_offer;
use crate::app::devices::edit_offer::device_edit_offer;
use crate::app::devices::lan_link_view::UiLanLink;
use crate::app::devices::relay_connect_offer::connect_relay_offer;
use crate::app::devices::runtime_band::UiRuntimeBand;
use crate::app::devices::take_over_offer::{TAKE_OVER_ASKING, take_over_offer};
use crate::app::devices::take_over_state::UiTakeOver;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::devices::wifi_connect_offer::connect_wifi_offer;
use crate::app::devices::wifi_connects::UiWifiConnect;
use crate::app::home::{UiExampleCard, UiPackageCard};
use crate::app::network::UiDeviceWifi;
use crate::{ConnectReach, OfferPath, UiOffer};

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
    /// A take-over of the board from another tab, under way or failed.
    pub take_over: Option<UiTakeOver>,
    pub update: Option<UiDeviceUpdate>,
    /// The update standing the offers are published from.
    pub update_facts: UpdateOfferFacts,
    pub layout: Option<UiDeviceLayout>,
    pub plays: BoardPlays,
    pub sharing: usize,
    pub project: Option<UiPackageCard>,
    pub shared_with: Vec<String>,
    pub last_seen_at: Option<f64>,
    pub ended: Option<ActivityEnd>,
    /// This tab's session is on the board (its card, or the docked card).
    pub editor_holds_it: bool,
    /// …and the editor shows it (the docked card): no `edit`, as the
    /// controller publishes none there.
    pub docked: bool,
    /// Where the session stands on the board.
    pub connection: BoardConnection,
    /// The panel picks the controller hands a connected card.
    pub panel: Option<UiBoardPanel>,
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
    /// Verbs the fixture withholds from [`Self::publish`].
    hidden: Vec<String>,
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
            take_over: None,
            update: None,
            update_facts: UpdateOfferFacts::default(),
            layout: None,
            plays: BoardPlays::Running {
                label: "porch".to_string(),
            },
            sharing: 0,
            project: None,
            shared_with: Vec::new(),
            last_seen_at: None,
            ended: None,
            editor_holds_it: false,
            docked: false,
            connection: BoardConnection::Watched,
            panel: None,
            now: 1_000_000.0,
            uid: Some("devporch0000000000".to_string()),
            reset: ResetReach::Lines,
            wifi_address: false,
            relay: false,
            extra_offers: Vec::new(),
            hidden: Vec::new(),
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

    /// The same board, held by another tab of this browser: its port is
    /// there and this tab never opened it (status Attached), so nothing
    /// is known of what runs on it.
    pub fn held(level: HoldLevel) -> Self {
        let mut fixture = Self::ready();
        fixture.view.status = DeviceStatus::Attached;
        fixture.view.state_label = "Attached \u{2014} not listening".to_string();
        fixture.view.firmware_face = FirmwareFace::Unknown;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.engine_fps = None;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        fixture.view.held_elsewhere = Some(HeldElsewhere {
            via: HoldVia::Usb,
            level,
            taken_from_here: false,
        });
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
            take_over: self.take_over.as_ref(),
            update: self.update.as_ref(),
            layout: self.layout.as_ref(),
            plays: &self.plays,
            sharing: self.sharing,
            project: self.project.as_ref(),
            shared_with: &self.shared_with,
            last_seen_at: self.last_seen_at,
            ended: self.ended.as_ref(),
            editor_holds_it: self.editor_holds_it,
            connection: &self.connection,
            panel: self.panel.as_ref(),
            now: self.now,
        }
    }

    /// Every verb the board publishes now.
    pub fn offers(&mut self) -> Vec<UiOffer> {
        self.offers = self.publish();
        self.offers.clone()
    }

    /// The board's offer for `verb`, which must be published.
    #[track_caller]
    pub fn offers_at(&mut self, verb: &str) -> UiOffer {
        let path = self.board.clone().child(verb);
        self.offers()
            .into_iter()
            .find(|offer| offer.path == path)
            .unwrap_or_else(|| panic!("{path} is not offered"))
    }

    /// The board does not publish `verb` (the controller had no reason to).
    pub fn without_offer(&mut self, verb: &str) {
        self.hidden.push(verb.to_string());
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
        // The controller's `take-over`, on a board another tab holds.
        if let Some(held) = self.view.held_elsewhere.as_ref() {
            let asking = self
                .take_over
                .as_ref()
                .is_some_and(|over| over.words == TAKE_OVER_ASKING);
            offers.push(take_over_offer(
                &self.board,
                self.view.id,
                &held.level,
                asking,
                true,
            ));
        }
        offers.extend(device_unlock_offer(&self.board, &self.view, unlock));
        // The session's verbs, as the controller's `session_offers` publish
        // them: `connect`, `edit` (not where the editor shows the board) and
        // `done` (on the board the session is on).
        let reach = match offline_wire {
            true if self.wifi_address => Some(ConnectReach::Wifi),
            true if self.relay => Some(ConnectReach::Relay),
            true => (self.view.escapes.contains(&Escape::Reconnect)
                && !self
                    .view
                    .held_elsewhere
                    .as_ref()
                    .is_some_and(|held| held.via == HoldVia::Usb))
            .then_some(ConnectReach::Usb),
            false => None,
        };
        let connect = ConnectFacts {
            registered: self.uid.is_some(),
            session_on_it: self.editor_holds_it,
            granted: unlock != Some(UiUnlockOffer::Locked),
            icon: link_icon(self.link.unwrap_or_default()),
            reach,
            waiting: self.connection == BoardConnection::Connecting,
        };
        offers.extend(device_connect_offer(&self.board, &self.view, &connect));
        if !self.docked {
            offers.extend(device_edit_offer(
                &self.board,
                &self.view,
                self.uid.as_deref(),
            ));
        }
        if self.editor_holds_it {
            offers.push(device_done_offer(&self.board));
        }
        offers.extend(self.extra_offers.iter().cloned());
        offers.retain(|offer| {
            !self
                .hidden
                .iter()
                .any(|verb| offer.path == self.board.clone().child(verb))
        });
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
    DeviceView {
        id: DeviceId(7),
        title: "Porch".to_string(),
        status: DeviceStatus::Ready,
        state_label: "Ready".to_string(),
        detail: Some("LightPlayer · seeed/xiao-esp32-c6".to_string()),
        freshness_label: None,
        identity_label: Some("a0:f2:62:87:b4:8c".to_string()),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
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
        held_elsewhere: None,
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
        from_lens: false,
    }
}

/// The lens session's own live picture, as the card draws it while the
/// lens holds the wire (CD8): a moment old, at the lens's engine rate.
pub(crate) fn lens_feed() -> DeviceCardFeedView {
    DeviceCardFeedView {
        frame_age_secs: Some(0.2),
        engine_fps: Some(57),
        from_lens: true,
        ..feed(FeedLiveness::Live, true)
    }
}

/// A library project, last saved two hours before [`CardFixture::ready`]'s
/// now.
pub(crate) fn library_project(uid: &str, slug: &str) -> UiPackageCard {
    UiPackageCard {
        uid: uid.to_string(),
        kind: "Module".to_string(),
        project_kind: "General".to_string(),
        exports: Vec::new(),
        slug: slug.to_string(),
        last_saved_at: Some(1_000_000.0 - 7_200.0),
        provenance: None,
        on_boards: Vec::new(),
        open_elsewhere: false,
        target: None,
        health: crate::app::library::PackageHealth::Ready,
    }
}
