//! Test fixtures for the board card's pieces: a board's verbs published as
//! core publishes them, the card core's own builder makes over them, and the
//! card rendered to markup on the host (`dioxus-ssr`), so a test reads the
//! DOM a walk reads — never a class string standing in for it.

use dioxus::prelude::*;
use lpa_studio_core::{
    BoardCardInput, BoardPlays, BoardRef, DeviceEscape, DeviceFace, DeviceFirmwareAge,
    DeviceFirmwareFace, DeviceId, DeviceLoadedProject, DeviceOfferFacts, DeviceStatus, DeviceView,
    DeviceWireVersion, OfferPath, ResetReach, UiBoardCard, UiOfferTree, board_card,
    device_edit_offer, device_offers,
};

use super::{BoardCard, CardPart};
use crate::core::OffersProvider;

/// `devices/new-7`.
pub(crate) fn board() -> OfferPath {
    OfferPath::board(&BoardRef::New(7))
}

/// A Ready LightPlayer on its USB cable, idle, running `porch`, registered.
pub(crate) fn porch_view() -> DeviceView {
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
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 2026.10.05-2".to_string()),
            wire: DeviceWireVersion::Match,
            age: DeviceFirmwareAge::Current,
        },
        remembered_firmware: None,
        degraded: None,
        loaded_project: DeviceLoadedProject::Running {
            label: "porch".to_string(),
        },
        engine_fps: Some(58),
        link_counters: None,
        can_receive_project: true,
        can_remove_project: true,
        activity: None,
        last_outcome: None,
        terminal: Vec::new(),
        terminal_dropped: 0,
        firmware_blocked: None,
        held_elsewhere: None,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        update_blocked: None,
        last_update_outcome: None,
    }
}

/// `view`'s verbs as core publishes them, and the card core builds over
/// them.
pub(crate) fn card_and_tree(view: &DeviceView) -> (UiBoardCard, UiOfferTree) {
    let facts = DeviceOfferFacts {
        prefix: board(),
        face: DeviceFace::Wire,
        autoconnect: false,
        locked: false,
        reset: ResetReach::Lines,
        banked: false,
        projects: &[],
        examples: &[],
        update: Default::default(),
    };
    let mut tree = UiOfferTree::new();
    for offer in device_offers(view, &facts) {
        tree.publish(offer);
    }
    if let Some(edit) = device_edit_offer(&board(), view, Some("devporch0000000000")) {
        tree.publish(edit);
    }
    tree.place_device(view.id, board());
    let verbs: Vec<_> = tree.own_verbs_of(&board()).cloned().collect();
    let plays = BoardPlays::Running {
        label: "porch".to_string(),
    };
    let card = board_card(&BoardCardInput {
        view,
        board: &board(),
        offers: &verbs,
        link: None,
        feed: None,
        runtime: None,
        access: None,
        wifi: None,
        lan: None,
        wifi_connect: None,
        take_over: None,
        update: None,
        layout: None,
        plays: &plays,
        sharing: 0,
        project: None,
        shared_with: &[],
        last_seen_at: None,
        ended: None,
        editor_holds_it: false,
        now: 1_000_000.0,
        connection: &lpa_studio_core::BoardConnection::Watched,
        panel: None,
    });
    (card, tree)
}

/// `card` under `tree`, rendered to markup with `open`'s details open.
pub(crate) fn render_card(card: UiBoardCard, tree: UiOfferTree, open: Option<CardPart>) -> String {
    render(CardRoot, CardRootProps { card, tree, open })
}

/// A card under its board's tree, as the shell provides it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CardRoot(card: UiBoardCard, tree: UiOfferTree, open: Option<CardPart>) -> Element {
    rsx! {
        OffersProvider { offers: tree,
            BoardCard { card, details_open: open, on_action: |_| {} }
        }
    }
}

/// `root` with `props`, rendered to markup on the host.
pub(crate) fn render<P: Clone + 'static, M: 'static>(
    root: impl dioxus::prelude::dioxus_core::ComponentFunction<P, M>,
    props: P,
) -> String {
    let mut dom = VirtualDom::new_with_props(root, props);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    // Dropping the dom runs the popovers' drop hooks, which reach for the
    // browser's window — there is none on the host.
    std::mem::forget(dom);
    html
}

/// Every value of `attribute` in `html`, in order.
pub(crate) fn attribute_values(html: &str, attribute: &str) -> Vec<String> {
    let needle = format!("{attribute}=\"");
    html.match_indices(&needle)
        .map(|(at, _)| {
            let rest = &html[at + needle.len()..];
            rest[..rest.find('"').unwrap_or(rest.len())].to_string()
        })
        .collect()
}
