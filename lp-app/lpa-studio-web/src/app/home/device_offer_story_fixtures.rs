//! Story fixtures for the device surfaces' offer trees (M3).
//!
//! The device card, the pending card, the Connect a board section and the
//! stalled-open exits draw their verbs from the view's offer tree, which the shell
//! provides. A story mounts a surface on its own, so it builds the tree the
//! same way core publishes it — [`device_offers`], [`pending_link_offers`],
//! [`add_device_offers`], [`new_sim_offer`] over the story's own fixtures —
//! and hands it down with [`OffersProvider`]. Nothing here invents a verb:
//! a story whose card shows a button shows it because core offered it.
//!
//! Boards are addressed `devices/new-<handle>` here; the board ref is not
//! drawn anywhere, so a story has no reason to mint MACs for it.

use dioxus::prelude::*;
use lpa_studio_core::{
    BluetoothReach, BoardRef, DeviceFace, DeviceOfferFacts, DeviceRosterView, DeviceView,
    OfferPath, PendingLinkView, ResetReach, RosterCardsInput, UiDeviceSettingsView, UiExampleCard,
    UiHomeSections, UiHomeView, UiOfferTree, UiPackageCard, UiUnlockOffer, UpdateOfferFacts,
    WifiAddressReach, add_device_offers, build_home_sections, connect_relay_offer,
    connect_wifi_offer, device_offers, device_unlock_offer, home_offers, new_sim_offer,
    pending_link_offers, roster_board_cards, stamp_on_boards, unlock_offer,
};

use crate::app::board_card::BoardCard;
use crate::app::home::HomePage;
use crate::app::home::ble_reach::use_ble_reach;
use crate::app::home::connect_board::connect_board_section::ConnectStoryPins;
use crate::app::home::page::home_view_mode::HomeViewMode;
use crate::core::OffersProvider;

/// The tree core would publish for `devices`: the Connect a board section's
/// transports at `bluetooth`, `devices/new-sim` where a runtime can start, and every
/// pending link's and device's verbs — each device placed by its handle.
/// `wifi_addresses`: the remembered boards this browser knows a Wi‑Fi
/// address for (core's address book, as the story says it), each offered
/// "Connect over Wi‑Fi". `relay_boards`: the remembered boards offered
/// "Connect through lightplayer.app" (core: signed in, the board said its
/// MAC).
pub(crate) fn roster_tree(
    devices: &DeviceRosterView,
    projects: &[UiPackageCard],
    examples: &[UiExampleCard],
    bluetooth: BluetoothReach,
    wifi_addresses: &[(lpa_studio_core::DeviceId, String)],
    relay_boards: &[lpa_studio_core::DeviceId],
) -> UiOfferTree {
    let mut tree = UiOfferTree::new();
    for offer in add_device_offers(devices.usb_available, bluetooth, wifi_reach(devices)) {
        tree.publish(offer);
    }
    if devices.transport_available {
        tree.publish(new_sim_offer());
    }
    tree.append(pending_tree(&devices.roster.pending));
    for card in &devices.roster.devices {
        let unlock = devices
            .access
            .get(&card.id)
            .and_then(|access| access.unlock);
        let face = match devices.runtime_bands.contains_key(&card.id) {
            true => DeviceFace::Sim,
            false => DeviceFace::Wire,
        };
        tree.append(card_tree_unlocked(
            card,
            face,
            unlock,
            projects,
            examples,
            Default::default(),
        ));
        // Edit, where the board has an editor address (its registry uid),
        // as the controller publishes it.
        let prefix = OfferPath::board(&BoardRef::New(card.id.0 as u32));
        let uid = devices.open_addresses.get(&card.id.0).map(String::as_str);
        if let Some(edit) = lpa_studio_core::device_edit_offer(&prefix, card, uid) {
            tree.publish(edit);
        }
        // The Wi‑Fi verbs, under the same prefix.
        if let Some(wifi) = devices.wifi.get(&card.id) {
            for offer in lpa_studio_core::app::network::wifi_offers(&prefix, wifi) {
                tree.publish(offer);
            }
        }
        if let Some((_, ip)) = wifi_addresses.iter().find(|(device, _)| *device == card.id) {
            let connecting = devices
                .wifi_connects
                .get(&card.id)
                .is_some_and(|connect| !connect.through_relay && connect.connecting);
            tree.publish(connect_wifi_offer(&prefix, card.id, ip, connecting));
        }
        if relay_boards.contains(&card.id) {
            let connecting = devices
                .wifi_connects
                .get(&card.id)
                .is_some_and(|connect| connect.through_relay && connect.connecting);
            tree.publish(connect_relay_offer(&prefix, card.id, connecting));
        }
    }
    tree
}

/// The Connect a board section's Network entry as core reads a Chromium page: reachable, and
/// waiting while the roster says an address is being reached.
fn wifi_reach(devices: &DeviceRosterView) -> WifiAddressReach {
    WifiAddressReach {
        available: true,
        connecting: devices
            .wifi_address_connect
            .as_ref()
            .is_some_and(|connect| connect.connecting),
    }
}

/// One device card's verbs, as core publishes them for it.
pub(crate) fn card_tree(
    card: &DeviceView,
    face: DeviceFace,
    locked: bool,
    projects: &[UiPackageCard],
    examples: &[UiExampleCard],
) -> UiOfferTree {
    card_tree_with_update(card, face, locked, projects, examples, Default::default())
}

/// [`card_tree`] for a board with an update story: `update` is its
/// standing and route, read by core from the board's facts
/// ([`UpdateFixture::offer_facts`](lpa_studio_core::UpdateFixture::offer_facts)).
pub(crate) fn card_tree_with_update(
    card: &DeviceView,
    face: DeviceFace,
    locked: bool,
    projects: &[UiPackageCard],
    examples: &[UiExampleCard],
    update: UpdateOfferFacts,
) -> UiOfferTree {
    let unlock = locked.then_some(UiUnlockOffer::Locked);
    card_tree_unlocked(card, face, unlock, projects, examples, update)
}

/// [`card_tree_with_update`] read off the card's unlock state, as the
/// controller reads it: `Locked` is offered no push, and anything short of
/// the author tier (`Locked`, `PlayOnly`) is offered Reset disabled.
fn card_tree_unlocked(
    card: &DeviceView,
    face: DeviceFace,
    unlock: Option<UiUnlockOffer>,
    projects: &[UiPackageCard],
    examples: &[UiExampleCard],
    update: UpdateOfferFacts,
) -> UiOfferTree {
    let prefix = OfferPath::board(&BoardRef::New(card.id.0 as u32));
    let facts = DeviceOfferFacts {
        prefix: prefix.clone(),
        face,
        autoconnect: false,
        locked: unlock == Some(UiUnlockOffer::Locked),
        // A story card on a network link (it carries the firmware reason)
        // restarts by request, with the author tier when nothing asks it to
        // unlock.
        reset: match card.firmware_blocked.is_some() {
            true => ResetReach::Request {
                author: unlock.is_none(),
            },
            false => ResetReach::Lines,
        },
        banked: false,
        projects,
        examples,
        update,
    };
    let mut tree = UiOfferTree::new();
    for offer in device_offers(card, &facts) {
        tree.publish(offer);
    }
    // `<board>/unlock`, offered the way the controller offers it: while the
    // board is linked and idle.
    if let Some(offer) = device_unlock_offer(&prefix, card, unlock) {
        tree.publish(offer);
    }
    tree.place_device(card.id, prefix);
    tree
}

/// The tree core would publish for a board that needs unlocking, when a
/// story has the Unlock sheet and no card (the sheet finds its board's
/// `unlock` offer by the prompt's device).
pub(crate) fn unlock_sheet_tree(
    device: lpa_studio_core::DeviceId,
    unlock: UiUnlockOffer,
) -> UiOfferTree {
    let prefix = OfferPath::board(&BoardRef::New(device.0 as u32));
    let mut tree = UiOfferTree::new();
    tree.publish(unlock_offer(&prefix, device, unlock));
    tree.place_device(device, prefix);
    tree
}

/// Every pending link's verbs, as core publishes them.
pub(crate) fn pending_tree(pending: &[PendingLinkView]) -> UiOfferTree {
    let mut tree = UiOfferTree::new();
    for link in pending {
        let prefix = OfferPath::board(&BoardRef::New(link.device.0 as u32));
        let reset = match link.firmware_blocked.is_some() {
            true => ResetReach::Request { author: false },
            false => ResetReach::Lines,
        };
        for offer in pending_link_offers(link, &prefix, reset) {
            tree.publish(offer);
        }
        tree.place_device(link.device, prefix);
    }
    tree
}

/// `children` under the tree core would publish for one device card: a sim
/// when `sim` (the power verbs' words), locked when `locked` (no push).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardOffers(
    card: DeviceView,
    #[props(default)] sim: bool,
    #[props(default)] locked: bool,
    /// The card's unlock state, when its story tells one: what a play-only
    /// link is offered differs from an unlocked one (Reset needs the author
    /// tier). `locked` alone stands for `Locked`.
    #[props(default)]
    unlock: Option<UiUnlockOffer>,
    #[props(default)] projects: Vec<UiPackageCard>,
    #[props(default)] examples: Vec<UiExampleCard>,
    /// The board's update standing and route, when it tells an update
    /// story (core's, from the story's update fixture).
    #[props(default)]
    update: UpdateOfferFacts,
    children: Element,
) -> Element {
    let face = match sim {
        true => DeviceFace::Sim,
        false => DeviceFace::Wire,
    };
    let unlock = unlock.or(locked.then_some(UiUnlockOffer::Locked));
    let offers = card_tree_unlocked(&card, face, unlock, &projects, &examples, update);
    rsx! {
        OffersProvider { offers, {children} }
    }
}

/// `children` under the tree core would publish for a whole roster. The
/// Bluetooth half is `ble_reach` when a story pins it, else what this
/// browser answers — the same answer the Connect a board section's notes read, so the
/// button and the way forward under it never disagree.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn RosterOffers(
    devices: DeviceRosterView,
    #[props(default)] projects: Vec<UiPackageCard>,
    #[props(default)] examples: Vec<UiExampleCard>,
    #[props(default)] ble_reach: Option<BluetoothReach>,
    /// Remembered boards this browser knows a Wi‑Fi address for.
    #[props(default)]
    wifi_addresses: Vec<(lpa_studio_core::DeviceId, String)>,
    /// Remembered boards offered "Connect through lightplayer.app".
    #[props(default)]
    relay_boards: Vec<lpa_studio_core::DeviceId>,
    children: Element,
) -> Element {
    let asked = use_ble_reach();
    let reach = ble_reach.unwrap_or_else(|| asked());
    let offers = roster_tree(
        &devices,
        &projects,
        &examples,
        reach,
        &wifi_addresses,
        &relay_boards,
    );
    rsx! {
        OffersProvider { offers, {children} }
    }
}

/// A device-card story's props, drawn as the board card: core's card over
/// the tree core would publish for it ([`StoryBoardCard`]). A runtime band
/// makes it a sim (the power verbs' words); a Locked access makes it locked
/// (no push), exactly as core reads the roster.
///
/// The props that opened a part of today's card open the details that
/// hold it now: `menu_initially_open` (Rename) the hardware details,
/// `access_panel_open` the access details, `install_picker_preview` the
/// firmware details with the Other version form, `armed_preview` the
/// hardware details with Forget armed, `armed_remove_preview` the project
/// details with Remove project armed.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryDeviceCard(
    card: DeviceView,
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    #[props(default)] armed_preview: bool,
    #[props(default)] armed_remove_preview: bool,
    #[props(default)] open_uid: Option<String>,
    #[props(default)] feed: Option<lpa_studio_core::DeviceCardFeedView>,
    #[props(default)] runtime: Option<lpa_studio_core::UiRuntimeBand>,
    #[props(default)] access: Option<lpa_studio_core::UiDeviceAccess>,
    #[props(default)] wifi: Option<lpa_studio_core::UiDeviceWifi>,
    #[props(default)] lan: Option<lpa_studio_core::UiLanLink>,
    /// How the board is reached; USB when unsaid.
    #[props(default)]
    link: Option<lpa_studio_core::UiLinkKind>,
    #[props(default)] access_panel_open: bool,
    #[props(default)] keys_open_preview: bool,
    #[props(default)] menu_initially_open: bool,
    /// The install verb's version list, mounted open (and picked/armed).
    #[props(default)]
    install_picker_preview: Option<
        crate::app::board_card::other_version_form::OfferPickerPreview,
    >,
    /// The card's update words (core's, from the story's update fixture).
    #[props(default)]
    update: Option<lpa_studio_core::UiDeviceUpdate>,
    /// The board's update standing and route, for its offers.
    #[props(default)]
    update_facts: UpdateOfferFacts,
    /// The board's files across a layout change (its verbs in
    /// `extra_offers`).
    #[props(default)]
    layout: Option<lpa_studio_core::UiDeviceLayout>,
    /// More verbs the controller publishes under the board's prefix.
    #[props(default)]
    extra_offers: Option<UiOfferTree>,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    use crate::app::board_card::CardPart;
    use lpa_studio_core::BarLayer;
    let prefix = story_board_prefix(card.id);
    let (details_open, armed) = if armed_remove_preview {
        (
            Some(CardPart::Bar(BarLayer::Project)),
            Some(prefix.child("remove-project")),
        )
    } else if armed_preview {
        (
            Some(CardPart::Bar(BarLayer::Hardware)),
            Some(prefix.child("forget")),
        )
    } else if install_picker_preview.is_some() {
        (Some(CardPart::Bar(BarLayer::Firmware)), None)
    } else if access_panel_open {
        (Some(CardPart::Bar(BarLayer::Access)), None)
    } else if menu_initially_open {
        (Some(CardPart::Bar(BarLayer::Hardware)), None)
    } else {
        (None, None)
    };
    let previews = crate::app::board_card::CardPreviews {
        access_keys_open: keys_open_preview,
        other_version: install_picker_preview,
    };
    rsx! {
        StoryBoardCard {
            card,
            projects,
            examples,
            open_uid,
            feed,
            runtime,
            access,
            wifi,
            lan,
            link,
            update,
            update_facts,
            layout,
            extra_offers,
            details_open,
            armed_preview: armed,
            previews,
            on_action,
        }
    }
}

/// The time every board-card story is told at (epoch seconds): a card's
/// ages ("Offline · 2 weeks", a done bar's few seconds) read against it.
pub(crate) const STORY_BOARD_NOW: f64 = 1_791_000_000.0;

/// `devices/new-<handle>`: where a story board's verbs live.
pub(crate) fn story_board_prefix(device: lpa_studio_core::DeviceId) -> OfferPath {
    OfferPath::board(&BoardRef::New(device.0 as u32))
}

/// [`BoardCard`] for a story board: the tree core would publish for it
/// (its device verbs, Unlock, Edit, the Wi‑Fi verbs, the reconnects an
/// offline board is offered, and any `extra_offers` — the layout verbs a
/// story built with `device_layout_view`), and the card core's own
/// [`board_card`] builds from the story's facts over that tree. So a story
/// card says and offers exactly what core decides; nothing here invents a
/// word or a verb. `StoryDeviceCard`'s props, plus the card's own facts:
/// `plays`, `ended`, `last_seen_at`, `now`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryBoardCard(
    card: DeviceView,
    #[props(default)] projects: Vec<UiPackageCard>,
    #[props(default)] examples: Vec<UiExampleCard>,
    #[props(default)] feed: Option<lpa_studio_core::DeviceCardFeedView>,
    #[props(default)] runtime: Option<lpa_studio_core::UiRuntimeBand>,
    #[props(default)] access: Option<lpa_studio_core::UiDeviceAccess>,
    #[props(default)] wifi: Option<lpa_studio_core::UiDeviceWifi>,
    #[props(default)] lan: Option<lpa_studio_core::UiLanLink>,
    /// A Wi‑Fi or relay connect under way, or why it failed.
    #[props(default)]
    wifi_connect: Option<lpa_studio_core::UiWifiConnect>,
    #[props(default)] update: Option<lpa_studio_core::UiDeviceUpdate>,
    #[props(default)] update_facts: UpdateOfferFacts,
    /// The board's files across a layout change (its verbs in
    /// `extra_offers`).
    #[props(default)]
    layout: Option<lpa_studio_core::UiDeviceLayout>,
    /// More verbs the controller publishes under the board's prefix.
    #[props(default)]
    extra_offers: Option<UiOfferTree>,
    /// How the board is reached; USB when unsaid.
    #[props(default)]
    link: Option<lpa_studio_core::UiLinkKind>,
    /// The board's registry uid: Edit on a ready, running board.
    #[props(default)]
    open_uid: Option<String>,
    /// This browser remembers the board's Wi‑Fi address: "Connect over
    /// Wi‑Fi" while it is offline.
    #[props(default)]
    wifi_address: Option<String>,
    /// Someone is signed in: "Connect through lightplayer.app" while it is
    /// offline.
    #[props(default)]
    relay: bool,
    /// Which project it plays. Left `Unknown`, it is core's join over the
    /// board's own report ([`lpa_studio_core::board_projects`] with no
    /// registry and no lens): "Nothing on it yet" for an empty board, the
    /// running label for a running one, as the controller would say.
    #[props(default = lpa_studio_core::BoardPlays::Unknown)]
    plays: lpa_studio_core::BoardPlays,
    #[props(default)] sharing: usize,
    #[props(default)] project: Option<UiPackageCard>,
    #[props(default)] shared_with: Vec<String>,
    #[props(default)] ended: Option<lpa_studio_core::ActivityEnd>,
    #[props(default)] last_seen_at: Option<f64>,
    #[props(default = STORY_BOARD_NOW)] now: f64,
    #[props(default)] editor_holds_it: bool,
    #[props(default)] details_open: Option<crate::app::board_card::CardPart>,
    #[props(default)] armed_preview: Option<OfferPath>,
    #[props(default)] previews: crate::app::board_card::CardPreviews,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    let face = match runtime.is_some() {
        true => DeviceFace::Sim,
        false => DeviceFace::Wire,
    };
    let unlock = access.as_ref().and_then(|access| access.unlock);
    let mut tree = card_tree_unlocked(&card, face, unlock, &projects, &examples, update_facts);
    let prefix = story_board_prefix(card.id);
    let offline_wire =
        face == DeviceFace::Wire && card.status == lpa_studio_core::DeviceStatus::Offline;
    let connecting = |through_relay: bool| {
        wifi_connect
            .as_ref()
            .is_some_and(|connect| connect.through_relay == through_relay && connect.connecting)
    };
    if let Some(ip) = wifi_address.as_deref().filter(|_| offline_wire) {
        tree.publish(connect_wifi_offer(&prefix, card.id, ip, connecting(false)));
    }
    if relay && offline_wire {
        tree.publish(connect_relay_offer(&prefix, card.id, connecting(true)));
    }
    if let Some(offer) = lpa_studio_core::device_edit_offer(&prefix, &card, open_uid.as_deref()) {
        tree.publish(offer);
    }
    if let Some(wifi) = wifi.as_ref() {
        for offer in lpa_studio_core::app::network::wifi_offers(&prefix, wifi) {
            tree.publish(offer);
        }
    }
    if let Some(extra) = extra_offers {
        tree.append(extra);
    }
    let verbs: Vec<lpa_studio_core::UiOffer> = tree.own_verbs_of(&prefix).cloned().collect();
    // How it is reached, when the story does not say: what the controller
    // reads off the board's endpoint — a LAN or relay link names itself, a
    // card carrying the network reason is over Bluetooth, else USB.
    let link = link.or_else(|| match (&lan, card.is_over_bluetooth()) {
        (Some(lan), _) => Some(lan.kind),
        (None, true) => Some(lpa_studio_core::UiLinkKind::Bluetooth),
        (None, false) => None,
    });
    let plays = match plays {
        lpa_studio_core::BoardPlays::Unknown => {
            own_report_plays(std::slice::from_ref(&card), &projects)
                .plays(card.id)
                .clone()
        }
        told => told,
    };
    let built = lpa_studio_core::board_card(&lpa_studio_core::BoardCardInput {
        view: &card,
        board: &prefix,
        offers: &verbs,
        link,
        feed: feed.as_ref(),
        runtime: runtime.as_ref(),
        access: access.as_ref(),
        wifi: wifi.as_ref(),
        lan: lan.as_ref(),
        wifi_connect: wifi_connect.as_ref(),
        update: update.as_ref(),
        layout: layout.as_ref(),
        plays: &plays,
        sharing,
        project: project.as_ref(),
        shared_with: &shared_with,
        last_seen_at,
        ended: ended.as_ref(),
        editor_holds_it,
        now,
    });
    // A story that opens a part's details (or whose layout question raises
    // them) keeps the room they float in, so a capture holds the whole
    // details card and a grid of such cards never stacks one over the next.
    let opens = details_open.is_some() || built.bars.iter().any(|bar| bar.details.raised);
    rsx! {
        OffersProvider { offers: tree,
            div { class: if opens { DETAILS_ROOM_CLASS } else { "tw:contents" },
                BoardCard {
                    card: built,
                    projects,
                    examples,
                    details_open,
                    armed_preview,
                    previews,
                    on_action,
                }
            }
        }
    }
}

/// The room a story card's open details float in: the card, and the
/// tallest details card below its lowest bar.
const DETAILS_ROOM_CLASS: &str = "tw:grid tw:min-h-[900px] tw:content-start";

/// [`BoardCard`] for a new board: core's [`pending_board_card`] over the
/// verbs core publishes for the link (`link`: how it arrived).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryNewBoardCard(
    pending: PendingLinkView,
    #[props(default)] link: lpa_studio_core::UiLinkKind,
    #[props(default)] details_open: Option<crate::app::board_card::CardPart>,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    let tree = pending_tree(std::slice::from_ref(&pending));
    let prefix = story_board_prefix(pending.device);
    let verbs: Vec<lpa_studio_core::UiOffer> = tree.own_verbs_of(&prefix).cloned().collect();
    let built = lpa_studio_core::pending_board_card(&pending, &prefix, &verbs, link);
    rsx! {
        OffersProvider { offers: tree,
            div { class: if details_open.is_some() { DETAILS_ROOM_CLASS } else { "tw:contents" },
                BoardCard { card: built, details_open, on_action }
            }
        }
    }
}

/// A new board's card for a pending-card story: [`StoryNewBoardCard`],
/// arrived over `link` — when unsaid, Bluetooth for a link carrying the
/// network reason, else USB, as the link's endpoint would say.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryPendingCard(
    pending: PendingLinkView,
    #[props(default)] link: Option<lpa_studio_core::UiLinkKind>,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    let link = link.unwrap_or(match pending.firmware_blocked.is_some() {
        true => lpa_studio_core::UiLinkKind::Bluetooth,
        false => lpa_studio_core::UiLinkKind::Usb,
    });
    rsx! {
        StoryNewBoardCard { pending, link, on_action }
    }
}

/// The Connect a board section's offers alone — `devices/connect-usb`,
/// `connect-ble` and `devices/new-sim` — for a story that mounts the section
/// (or a page that draws one of its verbs) on its own.
pub(crate) fn add_slot_tree(usb_available: bool, bluetooth: BluetoothReach) -> UiOfferTree {
    let mut tree = UiOfferTree::new();
    let wifi = WifiAddressReach {
        available: true,
        connecting: false,
    };
    for offer in add_device_offers(usb_available, bluetooth, wifi) {
        tree.publish(offer);
    }
    tree.publish(new_sim_offer());
    tree
}

/// [`HomePage`] under the tree core would publish for its view: the
/// roster's verbs and Home's own (`project/new`, `project/open`).
///
/// A story that leaves `home.sections` at its default gets the sections
/// core would build from the story's own library and roster
/// ([`build_home_sections`]), as `StudioController::home_view` does; a story
/// that pins them keeps its own.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryHomePage(
    home: lpa_studio_core::UiHomeView,
    /// A fixed clock ("edited 3 days ago" is read against it).
    #[props(default)]
    now_secs: Option<f64>,
    /// The tab the page starts on.
    #[props(default)]
    initial_tab: Option<lpa_studio_core::UiHomeTab>,
    /// Cards or rows (the page otherwise reads the browser's choice).
    #[props(default)]
    initial_mode: Option<HomeViewMode>,
    /// The Connect a board section's pins.
    #[props(default)]
    connect_pins: ConnectStoryPins,
    /// Remembered boards this browser knows a Wi‑Fi address for.
    #[props(default)]
    wifi_addresses: Vec<(lpa_studio_core::DeviceId, String)>,
    /// Remembered boards offered "Connect through lightplayer.app".
    #[props(default)]
    relay_boards: Vec<lpa_studio_core::DeviceId>,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    let home = with_core_sections(home);
    // The Bluetooth half is the pinned answer, else this browser's — the
    // same answer the section's notes read, so the square and the way
    // forward under it never disagree.
    let asked = use_ble_reach();
    let reach = connect_pins.ble_reach.unwrap_or_else(|| asked());
    let mut offers = roster_tree(
        &home.devices,
        &home.projects,
        &home.examples,
        reach,
        &wifi_addresses,
        &relay_boards,
    );
    for offer in home_offers(&home) {
        offers.publish(offer);
    }
    let home = with_core_cards(home, &offers, now_secs.unwrap_or(STORY_BOARD_NOW));
    rsx! {
        OffersProvider { offers,
            HomePage {
                home: Some(home),
                now_secs,
                on_action: Some(on_action),
                initial_tab,
                // A capture never reads the story server's own storage:
                // cards unless the story asks for rows.
                initial_mode: Some(initial_mode.unwrap_or_default()),
                connect_pins,
                // The app reads these from its access context, which a story
                // has none of: pin them so the closed fold is drawn.
                keys_settings: Some(UiDeviceSettingsView {
                    browser_name: Some("Luna's laptop".to_string()),
                    remembered_passwords: 0,
                }),
            }
        }
    }
}

/// `home` with the sections core would build for its library and roster.
///
/// `StudioController::home_view` joins boards to projects, writes each
/// project's boards onto its card ([`stamp_on_boards`]) and fills
/// [`UiHomeView::sections`] with
/// [`build_home_sections`]; a story that left the sections at their default
/// gets the same two calls, so a story never hand-builds which board or
/// project sits in which section, or which boards a project says it is on,
/// and cannot show a page core would not produce. A story that pins sections
/// keeps its own (a test of the page's drawing, not of core's membership).
pub(crate) fn with_core_sections(mut home: UiHomeView) -> UiHomeView {
    // The join every section and card reads: core's, over the boards' own
    // reports, unless the story pins its own.
    if home.devices.board_projects == lpa_studio_core::BoardProjects::default() {
        home.devices.board_projects =
            own_report_plays(&home.devices.roster.devices, &home.projects);
    }
    if home.sections == UiHomeSections::default() {
        stamp_on_boards(&mut home.projects, &home.devices);
        home.sections = build_home_sections(&home.projects, &home.devices);
    }
    home
}

/// Which project each board plays, by core's join over the boards' own
/// reports alone ([`lpa_studio_core::board_projects`] with no registry
/// rows and no lens) — what the controller answers before the library has
/// a record of what it gave a board.
fn own_report_plays(
    boards: &[DeviceView],
    projects: &[UiPackageCard],
) -> lpa_studio_core::BoardProjects {
    lpa_studio_core::board_projects(&lpa_studio_core::BoardProjectInputs {
        boards,
        registry_keys: &Default::default(),
        registry: &[],
        projects,
        project_heads: &Default::default(),
        lens: None,
    })
}

/// `home` with the board cards core would build for its roster over
/// `offers` ([`roster_board_cards`]), as `StudioController::view` does once
/// the view's offers are published — so a story's page draws exactly the
/// cards core would, and never hand-builds one. A story that pins its own
/// cards keeps them.
pub(crate) fn with_core_cards(mut home: UiHomeView, offers: &UiOfferTree, now: f64) -> UiHomeView {
    if home.devices.cards.is_empty() {
        let cards = roster_board_cards(&RosterCardsInput {
            roster: &home.devices,
            offers,
            projects: &home.projects,
            lens: None,
            now,
        });
        home.devices.cards = cards;
    }
    home
}

/// The tree a session's device lens is offered under: the device's own
/// verbs (Rename among them), for a story of the header session panel.
pub(crate) fn session_device_tree(
    device: Option<lpa_studio_core::DeviceId>,
    title: &str,
) -> UiOfferTree {
    let Some(device) = device else {
        return UiOfferTree::new();
    };
    card_tree(
        &ready_device(device, title),
        DeviceFace::Wire,
        false,
        &[],
        &[],
    )
}

/// A connected, idle LightPlayer with nothing reported loaded.
fn ready_device(id: lpa_studio_core::DeviceId, title: &str) -> DeviceView {
    use lpa_studio_core::{DeviceEscape, DeviceFirmwareFace, DeviceLoadedProject, DeviceStatus};
    DeviceView {
        id,
        title: title.to_string(),
        status: DeviceStatus::Ready,
        state_label: "Ready".to_string(),
        detail: None,
        freshness_label: None,
        identity_label: None,
        detected_chip: None,
        board_id: None,
        firmware_face: DeviceFirmwareFace::Unknown,
        remembered_firmware: None,
        degraded: None,
        loaded_project: DeviceLoadedProject::Unknown,
        engine_fps: None,
        link_counters: None,
        can_receive_project: false,
        can_remove_project: false,
        activity: None,
        last_outcome: None,
        terminal: Vec::new(),
        terminal_dropped: 0,
        firmware_blocked: None,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        update_blocked: None,
        last_update_outcome: None,
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{BoardPlays, BoardProjects, DeviceId, UiHomeBoardKind};

    use super::*;
    use crate::app::home::home_gallery_stories::{examples, packages, roster_page_fixture};

    #[test]
    fn a_story_that_leaves_the_sections_default_gets_the_ones_core_builds() {
        let home = story_home(UiHomeSections::default());
        let filled = with_core_sections(home.clone());
        // The join is core's, over the boards' own reports.
        assert_eq!(
            filled.devices.board_projects,
            own_report_plays(&home.devices.roster.devices, &home.projects)
        );
        assert_ne!(
            filled.devices.board_projects,
            BoardProjects::default(),
            "the roster's boards say what they play"
        );
        assert_eq!(
            filled.sections,
            build_home_sections(&home.projects, &filled.devices)
        );
        // Not the default by accident: the roster is on the page.
        assert!(!filled.sections.online.is_empty());
        assert!(
            filled
                .sections
                .offline
                .iter()
                .all(|board| board.kind == UiHomeBoardKind::Remembered)
        );
    }

    #[test]
    fn a_projects_boards_are_the_ones_the_join_says_play_it() {
        let mut home = story_home(UiHomeSections::default());
        // The library fixture carries a hand-written "On Luna's porch sign"
        // that no join backs; core would not say it.
        assert_eq!(home.projects[0].on_boards, ["Luna's porch sign"]);
        let porch = home.projects[0].uid.clone();
        home.devices.board_projects = BoardProjects::from_answers([(
            DeviceId(5),
            BoardPlays::Given {
                project_uid: porch,
                at_head: true,
            },
        )]);
        let shown = with_core_sections(home);
        assert_eq!(shown.projects[0].on_boards, ["Garage strip"]);
        assert!(shown.projects[1..].iter().all(|c| c.on_boards.is_empty()));
    }

    #[test]
    fn a_story_that_pins_its_sections_keeps_them() {
        let pinned = UiHomeSections {
            newcomer: true,
            ..UiHomeSections::default()
        };
        let kept = with_core_sections(story_home(pinned.clone()));
        assert_eq!(kept.sections, pinned);
    }

    fn story_home(sections: UiHomeSections) -> UiHomeView {
        UiHomeView {
            projects: packages(),
            examples: examples(),
            devices: roster_page_fixture(),
            sections,
            library_available: true,
            opening: None,
            issue: None,
        }
    }
}
