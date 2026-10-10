//! One board as a row, for the home page's list view.
//!
//! ```text
//!   [pic] Desk C6            Ready      Open   Connect
//!         Porch lights
//! ```
//!
//! A small picture (the board's last frame when its feed has a layout, else
//! a poster swatch), the board's name and the project it plays, its status
//! in the card's own tone, an **Open** link to the editor when the card
//! would offer one, and at most one verb: the first of core's
//! [`UiHomeBoard::row_verbs`] that the board's published offers hold. The
//! row builds no action. The board card (M2) replaces the verb with its
//! name bar's primary offer (PQ17).

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceFeedOp, DeviceId, DeviceLoadedProject, DeviceRosterView, DeviceStatus, DeviceView,
    UiAction, UiHomeBoard, UiHomeBoardKind, UiOffer, UiStatus, device_status_kind,
};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::node::lamp_view::LampView;
use crate::core::{
    ActionButton, ActionButtonVariant, StatusChip, quiet_action_class, use_device_verbs, verb_named,
};

/// One board in the list.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BoardRow(
    entry: UiHomeBoard,
    /// The roster and its side maps: the board's view, its picture, its
    /// editor address.
    devices: DeviceRosterView,
    on_action: EventHandler<UiAction>,
) -> Element {
    let verbs = use_device_verbs(Some(entry.id))();
    let verb = row_verb(&verbs, &entry.row_verbs);
    let card = devices
        .roster
        .devices
        .iter()
        .find(|card| card.id == entry.id);
    let status = row_status(&entry, card);
    let open_href = row_open_href(
        card,
        devices.open_addresses.get(&entry.id.0).map(String::as_str),
    );
    let picture = devices
        .feeds
        .get(&entry.id)
        .and_then(|feed| feed.frame.as_ref())
        .filter(|frame| frame.display_layout.is_some())
        .cloned();
    let offline = entry.kind == UiHomeBoardKind::Remembered;
    let swatch = thumb_swatch_style(&entry.title, offline);

    rsx! {
        div { class: ROW_CLASS,
            if entry.kind == UiHomeBoardKind::Connected {
                // A row on screen wants its board's live picture, as a card
                // does; an offline row draws the frame it was left with.
                FeedLease { device: entry.id, on_action }
            }
            match picture {
                Some(picture) => rsx! {
                    div {
                        class: if offline { "ux-play-frame ux-play-frame-dim tw:flex-none" } else { "ux-play-frame tw:flex-none" },
                        style: PICTURE_SIZE,
                        aria_hidden: "true",
                        div { class: "ux-play-lamps",
                            LampView { preview: picture }
                        }
                    }
                },
                None => rsx! {
                    div {
                        class: "tw:flex-none tw:rounded-sm",
                        style: "{PICTURE_SIZE} {swatch}",
                        aria_hidden: "true",
                    }
                },
            }
            div { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                span {
                    class: "tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                    title: "{entry.title}",
                    "{entry.title}"
                }
                if let Some(project) = entry.project.as_deref() {
                    span {
                        class: "tw:truncate tw:text-xs tw:text-muted-foreground",
                        title: "{project}",
                        "{project}"
                    }
                }
            }
            if !status.label.is_empty() {
                span { class: "tw:flex-none", StatusChip { status } }
            }
            if let Some(href) = open_href {
                a {
                    class: quiet_action_class(),
                    href: "{href}",
                    title: "Open this board in the editor",
                    "Open"
                }
            }
            if let Some(verb) = verb {
                ActionButton {
                    action: verb.action,
                    running: false,
                    variant: ActionButtonVariant::Quiet,
                    on_action,
                }
            }
        }
    }
}

/// The board's picture lease while its row is mounted: the same plumbing a
/// card's mount sends (`DeviceFeedOp`, no verb and no offer).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FeedLease(device: DeviceId, on_action: EventHandler<UiAction>) -> Element {
    use_effect(move || on_action.call(DeviceFeedOp::action_for(device, true)));
    use_drop(move || on_action.call(DeviceFeedOp::action_for(device, false)));
    rsx! {}
}

/// The row: one line (the list draws the frame and the hairlines).
const ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-3 tw:px-3 tw:py-2";

/// The picture's box (inline, so the frame's own height rule cannot win).
const PICTURE_SIZE: &str = "width: 64px; height: 36px;";

/// The first of `row_verbs` the board's published offers hold, by its last
/// path segment; `None` when none is published.
fn row_verb(verbs: &[UiOffer], row_verbs: &[&str]) -> Option<UiOffer> {
    row_verbs.iter().find_map(|name| verb_named(verbs, name))
}

/// The status chip: core's words, in the tone the card gives that state. A
/// link still identifying is working; a remembered board is the roster's
/// own Offline tone.
fn row_status(entry: &UiHomeBoard, card: Option<&DeviceView>) -> UiStatus {
    let kind = match entry.kind {
        UiHomeBoardKind::Pending => lpa_studio_core::UiStatusKind::Working,
        UiHomeBoardKind::Connected => {
            device_status_kind(card.map_or(DeviceStatus::Attached, |card| card.status))
        }
        UiHomeBoardKind::Remembered => device_status_kind(DeviceStatus::Offline),
    };
    UiStatus {
        label: entry.status.clone(),
        kind,
    }
}

/// The editor address the row's Open link goes to: the rule the card's
/// Edit offer follows (`device_edit_offer`), restated. Only a
/// READY board (identified, port open, idle) that runs a project, and has
/// a registry address, can be opened; Degraded is a refinement of Ready
/// and keeps it.
fn row_open_href(card: Option<&DeviceView>, open_uid: Option<&str>) -> Option<String> {
    let card = card?;
    let ready = matches!(card.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    let linked = card
        .escapes
        .contains(&lpa_studio_core::DeviceEscape::Disconnect);
    let idle = card.activity.is_none();
    let running = matches!(card.loaded_project, DeviceLoadedProject::Running { .. });
    match (ready && linked && idle && running, open_uid) {
        (true, Some(uid)) => Some(format!("/device/{uid}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        ActionMeta, ActionPriority, BoardRef, DeviceEscape, OfferPath, UiStatusKind,
    };

    #[test]
    fn the_open_link_needs_an_address() {
        let card = running_card();
        assert_eq!(
            row_open_href(Some(&card), Some("dev0000000000000001")),
            Some("/device/dev0000000000000001".to_string())
        );
        assert_eq!(row_open_href(Some(&card), None), None);
        assert_eq!(row_open_href(None, Some("dev0000000000000001")), None);
    }

    #[test]
    fn the_open_link_follows_the_cards_rule() {
        let address = Some("dev0000000000000001");
        let nothing_loaded = DeviceView {
            loaded_project: DeviceLoadedProject::Empty,
            ..running_card()
        };
        assert_eq!(row_open_href(Some(&nothing_loaded), address), None);
        let unlinked = DeviceView {
            escapes: vec![DeviceEscape::Forget],
            ..running_card()
        };
        assert_eq!(row_open_href(Some(&unlinked), address), None);
        let attached = DeviceView {
            status: DeviceStatus::Attached,
            ..running_card()
        };
        assert_eq!(row_open_href(Some(&attached), address), None);
        let degraded = DeviceView {
            status: DeviceStatus::Degraded,
            ..running_card()
        };
        assert!(row_open_href(Some(&degraded), address).is_some());
    }

    #[test]
    fn the_verb_shown_is_the_first_published_of_row_verbs() {
        let published = [offer("reconnect"), offer("forget"), offer("connect-wifi")];
        let shown = row_verb(&published, &["connect-wifi", "reconnect"]);
        assert_eq!(
            shown.and_then(|offer| offer.path.last().map(str::to_string)),
            Some("connect-wifi".to_string()),
            "core's order wins, not the tree's"
        );
        let shown = row_verb(&[offer("reconnect")], &["connect-wifi", "reconnect"]);
        assert_eq!(
            shown.and_then(|offer| offer.path.last().map(str::to_string)),
            Some("reconnect".to_string())
        );
    }

    #[test]
    fn no_verb_when_none_is_published() {
        assert!(row_verb(&[offer("forget")], &["connect-wifi", "reconnect"]).is_none());
        assert!(row_verb(&[offer("connect")], &[]).is_none());
        assert!(row_verb(&[], &["connect"]).is_none());
    }

    #[test]
    fn the_status_wears_the_cards_tone() {
        let mut entry = UiHomeBoard {
            id: DeviceId(1),
            kind: UiHomeBoardKind::Connected,
            title: "Desk C6".to_string(),
            status: "Ready".to_string(),
            project: None,
            row_verbs: Vec::new(),
        };
        assert_eq!(
            row_status(&entry, Some(&running_card())).kind,
            UiStatusKind::Good
        );
        assert_eq!(row_status(&entry, Some(&running_card())).label, "Ready");
        entry.kind = UiHomeBoardKind::Pending;
        assert_eq!(row_status(&entry, None).kind, UiStatusKind::Working);
        entry.kind = UiHomeBoardKind::Remembered;
        assert_eq!(row_status(&entry, None).kind, UiStatusKind::Neutral);
    }

    fn offer(verb: &str) -> UiOffer {
        let path = OfferPath::board(&BoardRef::New(1)).child(verb);
        UiOffer::new(
            path,
            "link",
            UiAction::from_op(
                lpa_studio_core::ControllerId::new("test"),
                DeviceFeedOp {
                    device: DeviceId(1),
                    wanted: true,
                },
            )
            .with_meta(ActionMeta::new(verb, verb, ActionPriority::Tertiary)),
        )
    }

    fn running_card() -> DeviceView {
        DeviceView {
            id: DeviceId(1),
            title: "Desk C6".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: lpa_studio_core::DeviceFirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: DeviceLoadedProject::Running {
                label: "Porch lights".to_string(),
            },
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
}
