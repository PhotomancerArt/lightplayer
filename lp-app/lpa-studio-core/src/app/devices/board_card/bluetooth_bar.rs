//! The Bluetooth row: the board's own radio, the name a phone sees
//! (`docs/ux/anatomy/board-card.md`, "Bluetooth"). Its kind is the link's
//! name, "Bluetooth"; its name is the board's advertised name while
//! Bluetooth is on.
//!
//! | Board says | Name · state | Tone |
//! |---|---|---|
//! | nothing yet (no firmware, not read, a stand-in) | "—" | Neutral |
//! | on | its advertised name, `LP-<project>` or `LP-xxxx`; "connected" when this tab's link is Bluetooth | Neutral |
//! | off | "disabled" | Neutral |
//!
//! The advertised name is the firmware's rule
//! (`fw-esp32c6/src/ble/advertising.rs`): `LP-` and the loaded project's
//! folder name, cut to fit the advertising packet, else `LP-` and the last
//! two bytes of the MAC in lower-case hex. Studio knows the folder while the
//! board says what it runs; an offline board's row says the MAC form.
//!
//! "This tab is on Bluetooth" is the link's own kind, read off the endpoint
//! (`BoardCardInput::link`), never `UiDeviceAccess::over_bluetooth`, which
//! has called a LAN or relay board "Bluetooth" before.
//!
//! Its details: the switch, the name, what the name is for, and — on a
//! board open to anyone — that anyone nearby can edit it.

use lpa_devices::view::LoadedProject;
use lpc_access::{OpenTo, Tier};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{facts, help};
use super::link_bar::{LinkFacts, link_bar};
use super::link_session::{CONNECTED, DISABLED, NOT_KNOWN};
use super::ui_bluetooth_switch::bluetooth_switch;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{BoardRef, RichLine};

/// What the name is for.
pub const BLUETOOTH_NAME_SENTENCE: &str =
    "A phone or laptop nearby can connect to it by this name.";

/// On a board open to edit with no password.
pub const OPEN_BOARD_SENTENCE: &str =
    "No password yet: anyone nearby or on the same Wi\u{2011}Fi can connect and edit it.";

/// The longest advertised name: the firmware's `ADV_NAME_MAX` (a 31-byte
/// advertisement less its flags and the name structure's own two bytes).
const ADV_NAME_MAX: usize = 26;

/// The Bluetooth row.
pub(crate) fn bluetooth_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let on = input.bluetooth_on().filter(|_| !input.stand_in());
    let name = advertised_name(input);
    let mut facts = match (on, &name) {
        (Some(true), Some(name)) => LinkFacts {
            aside: input
                .reached_over(UiLinkKind::Bluetooth)
                .then(|| CONNECTED.to_string()),
            ..LinkFacts::new("bluetooth", name.clone()).named()
        },
        (Some(false), _) => LinkFacts::new("bluetooth", DISABLED),
        _ => LinkFacts::new("bluetooth", NOT_KNOWN),
    };
    facts.stale = input.offline() && on.is_some();
    facts.details = details(input, on, name.as_deref());
    link_bar(input, BarLayer::Bluetooth, facts)
}

/// The name the board advertises: `LP-<project folder>`, else `LP-xxxx`
/// from its MAC; `None` for a board Studio knows no MAC of.
pub(crate) fn advertised_name(input: &BoardCardInput<'_>) -> Option<String> {
    if let LoadedProject::Running { label } = &input.view.loaded_project {
        let mut name = String::from("LP-");
        for c in label.chars() {
            if name.len() + c.len_utf8() > ADV_NAME_MAX {
                break;
            }
            name.push(c);
        }
        return Some(name);
    }
    let key = input
        .board
        .last()
        .and_then(|segment| BoardRef::parse(segment).ok())
        .and_then(|board| board.key())?;
    let octets = key.octets();
    Some(format!("LP-{:02x}{:02x}", octets[4], octets[5]))
}

/// The row's details: the switch, the name, and what it means.
fn details(input: &BoardCardInput<'_>, on: Option<bool>, name: Option<&str>) -> UiBarDetails {
    let mut details = UiBarDetails::default();
    let over_bluetooth = input.reached_over(UiLinkKind::Bluetooth);
    if let Some(access) = input
        .access
        .filter(|access| access.panel.is_some() || over_bluetooth)
    {
        details
            .panels
            .push(UiDetailPanel::Bluetooth(bluetooth_switch(
                access,
                over_bluetooth,
            )));
    }
    if on == Some(true)
        && let Some(name) = name
    {
        details
            .sections
            .push(facts("Bluetooth", vec![RichLine::new("Name", name)]));
        details
            .sections
            .push(help("Bluetooth", BLUETOOTH_NAME_SENTENCE));
        if open_to_edit(input) {
            details
                .sections
                .push(help("Anyone can edit", OPEN_BOARD_SENTENCE));
        }
    }
    details
}

/// The board is open to edit with no password: its access list says so,
/// or this link was granted edit with no key's name.
fn open_to_edit(input: &BoardCardInput<'_>) -> bool {
    let access = input.access.filter(|_| !input.offline());
    access
        .and_then(|access| access.panel.as_ref())
        .is_some_and(|panel| panel.open == OpenTo::Edit)
        || access
            .and_then(|access| access.grant.as_ref())
            .is_some_and(|grant| grant.tier == Tier::Edit && grant.key.is_none())
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;

    use super::super::card_fixtures::CardFixture;
    use super::*;
    use crate::app::access::{UiAccessPanel, UiDeviceAccess};

    #[test]
    fn the_bluetooth_row_says_its_name_or_disabled() {
        for (ble, summary) in [
            (None, "\u{2014}"),
            (Some(true), "LP-porch"),
            (Some(false), "disabled"),
        ] {
            let mut fixture = CardFixture::ready();
            fixture.access = Some(usb_access(ble));
            let bar = bluetooth_bar(&fixture.input());
            assert_eq!(bar.kind, Some("Bluetooth"));
            assert_eq!(bar.summary, summary, "{ble:?}");
            assert_eq!(bar.aside, None, "USB is the link in use");
        }
    }

    /// Over Bluetooth the radio is on whatever the list says, and the row
    /// says it is the link in use.
    #[test]
    fn over_bluetooth_it_is_on_and_connected() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        let bar = bluetooth_bar(&fixture.input());
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            ("LP-porch", Some("connected"))
        );
        assert!(
            bar.details
                .sections
                .iter()
                .any(|section| section.title == "Reach"),
            "the link in use's reach"
        );
    }

    /// The name is the firmware's rule: the project folder, cut to the
    /// advertising packet; else the MAC's last two bytes.
    #[test]
    fn the_advertised_name_follows_the_firmwares_rule() {
        let mut fixture = CardFixture::ready();
        assert_eq!(
            advertised_name(&fixture.input()).as_deref(),
            Some("LP-porch")
        );
        fixture.view.loaded_project = LoadedProject::Running {
            label: "a-very-long-project-folder-name".to_string(),
        };
        let name = advertised_name(&fixture.input()).unwrap();
        assert_eq!(name, "LP-a-very-long-project-fol");
        assert_eq!(name.len(), ADV_NAME_MAX);
        fixture.view.loaded_project = LoadedProject::Empty;
        assert_eq!(
            advertised_name(&fixture.input()).as_deref(),
            Some("LP-b48c")
        );
    }

    /// Offline, the switch the board last said, dimmed, by its MAC name.
    #[test]
    fn an_offline_board_keeps_its_last_switch() {
        let mut fixture = CardFixture::offline();
        fixture.remembered_bluetooth = Some(true);
        let bar = bluetooth_bar(&fixture.input());
        assert_eq!(bar.summary, "LP-b48c");
        assert!(bar.stale);
        fixture.remembered_bluetooth = None;
        let bar = bluetooth_bar(&fixture.input());
        assert_eq!(bar.summary, "\u{2014}");
    }

    /// Its details: the switch, the name and what it is for, and on an
    /// open board that anyone nearby can edit it.
    #[test]
    fn its_details_hold_the_switch_and_say_what_the_name_is_for() {
        let mut fixture = CardFixture::ready();
        let mut access = usb_access(Some(true));
        access.panel.as_mut().unwrap().open = OpenTo::Edit;
        fixture.access = Some(access);
        let bar = bluetooth_bar(&fixture.input());
        assert!(
            bar.details
                .panels
                .iter()
                .any(|panel| matches!(panel, UiDetailPanel::Bluetooth(_)))
        );
        let sentences: Vec<&str> = bar
            .details
            .sections
            .iter()
            .filter_map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [BLUETOOTH_NAME_SENTENCE, OPEN_BOARD_SENTENCE]);
    }

    #[test]
    fn a_stand_in_says_a_dash() {
        let mut fixture = CardFixture::ready();
        fixture.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        fixture.access = Some(usb_access(Some(true)));
        assert_eq!(bluetooth_bar(&fixture.input()).summary, "\u{2014}");
    }

    fn usb_access(ble_enabled: Option<bool>) -> UiDeviceAccess {
        UiDeviceAccess {
            panel: Some(UiAccessPanel {
                ble_enabled,
                ..UiAccessPanel::reading(DeviceId(7))
            }),
            ..UiDeviceAccess::default()
        }
    }
}
