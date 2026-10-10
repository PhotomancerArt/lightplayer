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
    OfferPath, PendingLinkView, ResetReach, UiDeviceSettingsView, UiExampleCard, UiHomeSections,
    UiHomeView, UiLensCard, UiOfferTree, UiPackageCard, UiUnlockOffer, UpdateOfferFacts,
    WifiAddressReach, add_device_offers, build_home_sections, connect_relay_offer,
    connect_wifi_offer, device_offers, home_offers, new_sim_offer, pending_link_offers,
    stamp_on_boards,
};

use crate::app::home::HomePage;
use crate::app::home::ble_reach::use_ble_reach;
use crate::app::home::connect_board::connect_board_section::ConnectStoryPins;
use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};
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
        if let Some((_, ip)) = wifi_addresses.iter().find(|(device, _)| *device == card.id) {
            let connecting = devices
                .wifi_connects
                .get(&card.id)
                .is_some_and(|connect| !connect.through_relay && connect.connecting);
            let prefix = OfferPath::board(&BoardRef::New(card.id.0 as u32));
            tree.publish(connect_wifi_offer(&prefix, card.id, ip, connecting));
        }
        if relay_boards.contains(&card.id) {
            let connecting = devices
                .wifi_connects
                .get(&card.id)
                .is_some_and(|connect| connect.through_relay && connect.connecting);
            let prefix = OfferPath::board(&BoardRef::New(card.id.0 as u32));
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
    tree.place_device(card.id, prefix);
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

/// `children` under the tree core would publish for these pending links.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PendingOffers(pending: Vec<PendingLinkView>, children: Element) -> Element {
    let offers = pending_tree(&pending);
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

/// [`DeviceRosterCard`] under the tree core would publish for its card —
/// the card's own props, passed straight through. A runtime band makes it
/// a sim (the power verbs' words); a Locked access makes it locked (no
/// push), exactly as core reads the roster.
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
    #[props(default)] access_panel_open: bool,
    #[props(default)] keys_open_preview: bool,
    #[props(default)] menu_initially_open: bool,
    /// The install verb's version list, mounted open (and picked/armed).
    #[props(default)]
    install_picker_preview: Option<crate::app::home::device_roster_card::OfferPickerPreview>,
    /// The card's update words (core's, from the story's update fixture).
    #[props(default)]
    update: Option<lpa_studio_core::UiDeviceUpdate>,
    /// The board's update standing and route, for its offers.
    #[props(default)]
    update_facts: UpdateOfferFacts,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    let unlock = access.as_ref().and_then(|access| access.unlock);
    rsx! {
        CardOffers {
            card: card.clone(),
            sim: runtime.is_some(),
            unlock,
            projects: projects.clone(),
            examples: examples.clone(),
            update: update_facts,
            DeviceRosterCard {
                update,
                card,
                projects,
                examples,
                armed_preview,
                armed_remove_preview,
                open_uid,
                feed,
                runtime,
                access,
                wifi,
                lan,
                access_panel_open,
                keys_open_preview,
                menu_initially_open,
                install_picker_preview,
                on_action,
            }
        }
    }
}

/// [`PendingLinkCard`] under the tree core would publish for its link.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryPendingCard(
    pending: PendingLinkView,
    on_action: EventHandler<lpa_studio_core::UiAction>,
) -> Element {
    rsx! {
        PendingOffers { pending: vec![pending.clone()],
            PendingLinkCard { pending, on_action }
        }
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
/// `StudioController::home_view` writes each project's boards onto its card
/// ([`stamp_on_boards`]) and fills [`UiHomeView::sections`] with
/// [`build_home_sections`]; a story that left the sections at their default
/// gets the same two calls, so a story never hand-builds which board or
/// project sits in which section, or which boards a project says it is on,
/// and cannot show a page core would not produce. A story that pins sections
/// keeps its own (a test of the page's drawing, not of core's membership).
pub(crate) fn with_core_sections(mut home: UiHomeView) -> UiHomeView {
    if home.sections == UiHomeSections::default() {
        stamp_on_boards(&mut home.projects, &home.devices);
        home.sections = build_home_sections(&home.projects, &home.devices);
    }
    home
}

/// The tree core would publish for a docked lens card (D43): the gallery's
/// own `card_tree`, read off the `UiLensCard` the view already carries, so
/// a `StudioShell`/`WorkbenchFrame` story needs only `simulator_lens_card()`
/// (or whatever lens it docks) to build both the card and its offers.
///
/// `StudioShell` always re-publishes the offer context from `view.offers`
/// (never an ancestor's — see its `use_provide_offers` call), so a story
/// that docks a lens card must fold this into `UiStudioView::offers`
/// itself; wrapping the shell in [`OffersProvider`] from outside has no
/// effect on anything under it. A bare `WorkbenchFrame` story (no
/// `StudioShell`) has no such shadowing and may use this with
/// [`OffersProvider`] directly.
pub(crate) fn lens_card_offer_tree(card: &UiLensCard) -> UiOfferTree {
    let UiLensCard::Device { card, runtime } = card;
    let face = match runtime.is_some() {
        true => DeviceFace::Sim,
        false => DeviceFace::Wire,
    };
    card_tree(card, face, false, &[], &[])
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
        assert_eq!(
            filled.sections,
            build_home_sections(&home.projects, &home.devices)
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
