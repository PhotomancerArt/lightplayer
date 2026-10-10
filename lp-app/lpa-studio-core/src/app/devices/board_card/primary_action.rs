//! The name bar's one primary action (`plan.md`, "The primary action").
//!
//! In order, first match wins:
//!
//! | Board | Primary | Offer | Icon |
//! |---|---|---|---|
//! | The editor holds it (the docked lens card) | none, until Done lands | — | — |
//! | Busy | the word it would be, disabled: "Busy: <the work>" | — | — |
//! | Needs firmware, `flash` offered | Install (the board pick) | `flash` | firmware |
//! | Locked | Unlock (the password sheet) | `unlock` | lock |
//! | Attached (the port is there, closed) | Connect | `connect` | the link's |
//! | Offline stand-in (a sim, an in-tab board) | Power on | `reconnect` | play |
//! | Offline, reachable | Connect: Wi‑Fi first, then the cloud, then the cable | `connect-wifi` / `connect-relay` / `reconnect` | wifi / cloud / the link's |
//! | Offline, nothing reaches it | Connect, disabled: "Offline · 2 weeks" | — | the last link's |
//! | Ready, play-only | Edit, with a lock | `unlock` (the sheet) | lock |
//! | Ready, running, the editor not on it | Edit | `edit` | edit |
//! | Ready, nothing on it or not said | Edit, disabled: "Nothing on it to edit yet" | — | edit |
//!
//! A new board's primary is [`pending_primary`]: Install once it has
//! settled on needing firmware, else Connect, disabled, saying how far it
//! has got ("Identifying…").
//!
//! "Connect" in this milestone means reach the board, never the editor; the
//! editor is Edit (director ruling Q1). The play-only row sits ahead of the
//! plain Edit row: a link that holds play only would open an editor that
//! cannot edit, so its Edit asks for the password first.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{LoadedProject, PendingLinkView};

use super::bar_work::{activity_bar, board_pick};
use super::board_card_input::{BoardCardInput, link_icon, offer_at};
use super::ui_bar_work::UiBarWork;
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_name_bar::UiPrimary;
use super::ui_stack_bar::BarLayer;
use crate::app::devices::age_words::duration_words;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{OfferPath, UiOffer};

/// Why Edit waits on a board that runs nothing (or has not said).
pub const NOTHING_TO_EDIT: &str = "Nothing on it to edit yet";

/// A board's primary. `work` is the running work's bar, for Busy's words.
pub(crate) fn primary_action(
    input: &BoardCardInput<'_>,
    work: Option<&UiBarWork>,
) -> Option<UiPrimary> {
    if input.editor_holds_it {
        return None;
    }
    let view = input.view;
    let link = input.link_kind();
    if let Some(activity) = &view.activity {
        let (word, icon) = match activity_bar(activity.kind) {
            BarLayer::Connection => ("Connect", link_icon(link)),
            _ => ("Edit", "edit"),
        };
        let words = work.map_or_else(|| activity.label.clone(), |work| work.words.clone());
        return Some(UiPrimary::Unavailable {
            word: word.to_string(),
            icon: icon.to_string(),
            reason: format!("Busy: {words}"),
        });
    }
    if view.needs_firmware()
        && let Some(flash) = input.offer("flash")
    {
        return Some(offer(install(flash).drawn(board_pick(input))));
    }
    if input.locked()
        && let Some(unlock) = input.offer("unlock")
    {
        return Some(offer(
            UiCardAction::press(unlock, "Unlock")
                .with_icon("lock")
                .drawn(UiActionDraw::Sheet),
        ));
    }
    if view.status == DeviceStatus::Attached
        && let Some(connect) = input.offer("connect")
    {
        return Some(offer(
            UiCardAction::press(connect, "Connect").with_icon(link_icon(link)),
        ));
    }
    if view.status == DeviceStatus::Offline {
        return Some(offline_primary(input, link));
    }
    if input.play_only()
        && let Some(unlock) = input.offer("unlock")
    {
        return Some(offer(
            UiCardAction::press(unlock, "Edit")
                .with_icon("lock")
                .drawn(UiActionDraw::Sheet),
        ));
    }
    if let Some(edit) = input.offer("edit") {
        return Some(offer(UiCardAction::press(edit, "Edit").with_icon("edit")));
    }
    let reason = match view.loaded_project {
        LoadedProject::Empty | LoadedProject::Unknown => NOTHING_TO_EDIT.to_string(),
        LoadedProject::Running { .. } => view.state_label.clone(),
    };
    Some(UiPrimary::Unavailable {
        word: "Edit".to_string(),
        icon: "edit".to_string(),
        reason,
    })
}

/// An offline board's primary: a stand-in powers on; a board reconnects
/// over Wi‑Fi first, then through lightplayer.app, then by its cable; with
/// none of those, Connect is disabled and says how long it has been away.
fn offline_primary(input: &BoardCardInput<'_>, link: UiLinkKind) -> UiPrimary {
    if input.stand_in()
        && let Some(reconnect) = input.offer("reconnect")
    {
        return offer(UiCardAction::press(reconnect, "Power on").with_icon("play"));
    }
    for (verb, icon) in [
        ("connect-wifi", "wifi"),
        ("connect-relay", "cloud"),
        ("reconnect", link_icon(link)),
    ] {
        if let Some(found) = input.offer(verb) {
            return offer(UiCardAction::press(found, "Connect").with_icon(icon));
        }
    }
    UiPrimary::Unavailable {
        word: "Connect".to_string(),
        icon: link_icon(link).to_string(),
        reason: offline_words(input.last_seen_at, input.now),
    }
}

/// "Offline · 2 weeks", from the registry's last sighting; "Offline" when
/// Studio has none.
pub(crate) fn offline_words(last_seen_at: Option<f64>, now: f64) -> String {
    match last_seen_at {
        Some(seen) => format!("Offline · {}", duration_words(now - seen)),
        None => "Offline".to_string(),
    }
}

/// A new board's primary: Install once it settled on needing firmware
/// (the board pick, filtered by the chip its boot banner named), else
/// Connect, disabled, saying how far it has got.
pub(crate) fn pending_primary(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
    link: UiLinkKind,
) -> UiPrimary {
    if let Some(flash) = offer_at(offers, board, "flash") {
        return offer(install(flash).drawn(UiActionDraw::BoardPick {
            chip: pending.detected_chip.clone(),
            chip_from_banner: pending.detected_chip.is_some(),
        }));
    }
    UiPrimary::Unavailable {
        word: "Connect".to_string(),
        icon: link_icon(link).to_string(),
        reason: pending.state_label.clone(),
    }
}

/// Install: the `flash` offer, in the card's word.
fn install(flash: &UiOffer) -> UiCardAction {
    UiCardAction::press(flash, "Install").with_icon("firmware")
}

fn offer(action: UiCardAction) -> UiPrimary {
    UiPrimary::Offer(action)
}

#[cfg(test)]
pub(crate) mod tests {
    use lpa_devices::ActivityKind;
    use lpa_devices::view::{Escape, FirmwareFace};

    use super::super::card_fixtures::{CardFixture, activity};
    use super::*;
    use crate::app::access::{UiDeviceAccess, UiUnlockOffer};

    #[test]
    fn the_editor_holding_the_board_leaves_no_primary() {
        let mut fixture = CardFixture::ready();
        fixture.editor_holds_it = true;
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    #[test]
    fn a_busy_board_says_what_it_is_doing_disabled() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Update,
            "Updating firmware",
            Some(42),
        ));
        let work = UiBarWork {
            words: "Updating · 42%".to_string(),
            percent: Some(42),
            state: super::super::ui_bar_work::BarWorkState::Running,
            cancel: None,
            other_device: false,
        };
        assert_eq!(
            primary_action(&fixture.input(), Some(&work)),
            Some(UiPrimary::Unavailable {
                word: "Edit".to_string(),
                icon: "edit".to_string(),
                reason: "Busy: Updating · 42%".to_string(),
            })
        );
    }

    #[test]
    fn a_board_that_needs_firmware_installs_with_the_board_pick() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::Blank;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        fixture.view.status = DeviceStatus::NeedsAttention;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Install");
        assert_eq!(action.icon.as_deref(), Some("firmware"));
        assert_eq!(path(&action), "flash");
        assert_eq!(
            action.draw,
            UiActionDraw::BoardPick {
                chip: Some("esp32c6".to_string()),
                chip_from_banner: true
            }
        );
    }

    #[test]
    fn a_locked_board_unlocks_with_the_sheet() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth).locked();
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Unlock");
        assert_eq!(action.icon.as_deref(), Some("lock"));
        assert_eq!(path(&action), "unlock");
        assert_eq!(action.draw, UiActionDraw::Sheet);
    }

    #[test]
    fn an_attached_board_connects_over_its_link() {
        let mut fixture = CardFixture::ready();
        fixture.view.status = DeviceStatus::Attached;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Connect");
        assert_eq!(action.icon.as_deref(), Some("usb"));
        assert_eq!(path(&action), "connect");
    }

    #[test]
    fn an_offline_board_reconnects_wifi_first_then_the_cloud_then_its_cable() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_address = true;
        fixture.relay = true;
        let action = offered(&mut fixture);
        assert_eq!(
            (action.word.as_str(), action.icon.as_deref(), path(&action)),
            ("Connect", Some("wifi"), "connect-wifi".to_string())
        );
        fixture.wifi_address = false;
        let action = offered(&mut fixture);
        assert_eq!(
            (action.icon.as_deref(), path(&action)),
            (Some("cloud"), "connect-relay".to_string())
        );
        fixture.relay = false;
        let action = offered(&mut fixture);
        assert_eq!(
            (action.icon.as_deref(), path(&action)),
            (Some("usb"), "reconnect".to_string())
        );
    }

    #[test]
    fn an_offline_stand_in_powers_on() {
        let mut fixture = CardFixture::offline();
        fixture.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Power on");
        assert_eq!(action.icon.as_deref(), Some("play"));
        assert_eq!(path(&action), "reconnect");
    }

    #[test]
    fn an_offline_board_nothing_reaches_says_how_long_it_has_been_away() {
        let mut fixture = CardFixture::offline().over(UiLinkKind::Bluetooth);
        fixture.view.escapes = vec![Escape::Forget];
        fixture.last_seen_at = Some(fixture.now - 15.0 * 86_400.0);
        assert_eq!(
            primary_action(&fixture.input(), None),
            Some(UiPrimary::Unavailable {
                word: "Connect".to_string(),
                icon: "bluetooth".to_string(),
                reason: "Offline · 2 weeks".to_string(),
            })
        );
        fixture.last_seen_at = None;
        assert!(matches!(
            primary_action(&fixture.input(), None),
            Some(UiPrimary::Unavailable { reason, .. }) if reason == "Offline"
        ));
    }

    #[test]
    fn a_ready_running_board_edits() {
        let mut fixture = CardFixture::ready();
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Edit");
        assert_eq!(action.icon.as_deref(), Some("edit"));
        assert_eq!(path(&action), "edit");
        assert_eq!(action.draw, UiActionDraw::Press);
    }

    #[test]
    fn a_play_only_board_edits_with_a_lock_through_unlock() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.access = Some(UiDeviceAccess {
            unlock: Some(UiUnlockOffer::PlayOnly),
            ..UiDeviceAccess::default()
        });
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Edit");
        assert_eq!(action.icon.as_deref(), Some("lock"));
        assert_eq!(path(&action), "unlock");
        assert_eq!(action.draw, UiActionDraw::Sheet);
    }

    #[test]
    fn a_ready_board_with_nothing_on_it_cannot_edit_yet() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        assert_eq!(
            primary_action(&fixture.input(), None),
            Some(UiPrimary::Unavailable {
                word: "Edit".to_string(),
                icon: "edit".to_string(),
                reason: NOTHING_TO_EDIT.to_string(),
            })
        );
    }

    #[test]
    fn a_new_board_connects_disabled_until_it_says_it_needs_firmware() {
        let mut pending = pending_view();
        let board = super::super::card_fixtures::board();
        assert_eq!(
            pending_primary(&pending, &board, &[], UiLinkKind::Usb),
            UiPrimary::Unavailable {
                word: "Connect".to_string(),
                icon: "usb".to_string(),
                reason: "Identifying…".to_string(),
            }
        );
        pending.firmware_face = FirmwareFace::Blank;
        let offers = crate::pending_link_offers(
            &pending,
            &board,
            crate::app::devices::device_reset_reach::ResetReach::Lines,
        );
        let UiPrimary::Offer(action) = pending_primary(&pending, &board, &offers, UiLinkKind::Usb)
        else {
            panic!("Install once it settles");
        };
        assert_eq!(action.word, "Install");
        assert_eq!(path(&action), "flash");
    }

    fn offered(fixture: &mut CardFixture) -> UiCardAction {
        match primary_action(&fixture.input(), None) {
            Some(UiPrimary::Offer(action)) => action,
            other => panic!("an offer, not {other:?}"),
        }
    }

    fn path(action: &UiCardAction) -> String {
        let path = action.offer.to_string();
        let prefix = "devices/mac-a0f26287b48c/";
        assert!(path.starts_with(prefix), "{path}");
        path[prefix.len()..].to_string()
    }

    pub(crate) fn pending_view() -> PendingLinkView {
        PendingLinkView {
            link: lpa_devices::LinkId(1),
            device: lpa_devices::DeviceId(9),
            title: "New board".to_string(),
            state_label: "Identifying…".to_string(),
            detail: None,
            can_adopt: true,
            firmware_face: FirmwareFace::Unknown,
            detected_chip: Some("esp32c6".to_string()),
            mac: None,
            firmware_blocked: None,
            held_by_tab: false,
            escapes: vec![Escape::Forget],
        }
    }
}
