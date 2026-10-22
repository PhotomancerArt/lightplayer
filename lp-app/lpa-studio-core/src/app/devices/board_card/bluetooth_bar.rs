//! The Bluetooth row: the board's own radio, the name a phone sees
//! (`docs/ux/anatomy/board-card.md`, "Bluetooth"). Its kind is the link's
//! name, "Bluetooth"; its name is the board's advertised name while
//! Bluetooth is on.
//!
//! | Board says | Name · state | Tone |
//! |---|---|---|
//! | nothing yet (no firmware, not read, a stand-in) | "—" | Neutral |
//! | on | its advertised name, `LP-<project>` or `LP-xxxx` ("on" while what it runs is not known, or Studio knows no MAC); "connected" when this tab's link is Bluetooth | Neutral |
//! | off | "disabled" | Neutral |
//!
//! The advertised name is the firmware's rule
//! (`fw-esp32-radio/src/ble/advertising.rs`): `LP-` and the loaded project's
//! folder name, cut to fit the advertising packet, else `LP-` and the last
//! two bytes of the MAC in lower-case hex. Studio knows the folder while the
//! board says what it runs, and the MAC form once it has said it runs
//! nothing; while it has not said (offline, or before its first report) the
//! name could be either, so the row says "on".
//!
//! "This tab is on Bluetooth" is the link's own kind, read off the endpoint
//! (`BoardCardInput::link`), never `UiDeviceAccess::over_bluetooth`, which
//! has called a LAN or relay board "Bluetooth" before.
//!
//! Hushing it is turning Bluetooth off with the switch at the top of its
//! details (the core offer `devices/<board>/bluetooth/enabled`, which
//! refuses off over Bluetooth — the link you are on — and restarts a board
//! on USB to apply it). Off, the row says "disabled" and the switch turns
//! it back on.
//!
//! Its details: the switch, then the name, what the name is for, and — on
//! a board open to anyone — that anyone nearby can edit it.

use lpa_devices::view::LoadedProject;
use lpc_access::{OpenTo, Tier};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{facts, help};
use super::link_bar::{LinkFacts, link_bar};
use super::link_session::{CONNECTED, DISABLED, NOT_KNOWN};
use super::row_switch::{SwitchOffer, row_switch};
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::access::{
    BLUETOOTH_ENABLED_PARAM, DISABLE_BLUETOOTH, ENABLE_BLUETOOTH, bluetooth_restart_words,
};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{BoardRef, RichLine};

/// What the name is for.
pub const BLUETOOTH_NAME_SENTENCE: &str =
    "A phone or laptop nearby can connect to it by this name.";

/// A board whose Bluetooth is off.
pub const BLUETOOTH_DISABLED_SENTENCE: &str =
    "Bluetooth is disabled on this board: nothing nearby can connect by it.";

/// On a board open to edit with no password.
pub const OPEN_BOARD_SENTENCE: &str =
    "No password yet: anyone nearby or on the same Wi\u{2011}Fi can connect and edit it.";

/// The row's name for a radio that is on when Studio can't say its name.
const ON: &str = "on";

/// The longest advertised name: the firmware's `ADV_NAME_MAX` (a 31-byte
/// advertisement less its flags and the name structure's own two bytes).
const ADV_NAME_MAX: usize = 26;

/// The Bluetooth row.
pub(crate) fn bluetooth_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let on = input.bluetooth_on().filter(|_| !input.stand_in());
    let name = advertised_name(input);
    let mut facts = match (on, &name) {
        (Some(true), name) => LinkFacts {
            aside: input
                .reached_over(UiLinkKind::Bluetooth)
                .then(|| CONNECTED.to_string()),
            ..LinkFacts::new("bluetooth", name.clone().unwrap_or_else(|| ON.to_string())).named()
        },
        (Some(false), _) => LinkFacts::new("bluetooth", DISABLED),
        _ => LinkFacts::new("bluetooth", NOT_KNOWN),
    };
    facts.stale = input.offline() && on.is_some();
    facts.details = details(input, on, name.as_deref());
    link_bar(input, BarLayer::Bluetooth, facts)
}

/// The name the board advertises: `LP-<project folder>` while it runs one,
/// `LP-xxxx` from its MAC (its board ref's, else the one it said) once it
/// has said it runs nothing; `None` while it has not said, which would make
/// either a guess, and for a board Studio knows no MAC of.
pub(crate) fn advertised_name(input: &BoardCardInput<'_>) -> Option<String> {
    match &input.view.loaded_project {
        LoadedProject::Running { label } => {
            let mut name = String::from("LP-");
            for c in label.chars() {
                if name.len() + c.len_utf8() > ADV_NAME_MAX {
                    break;
                }
                name.push(c);
            }
            return Some(name);
        }
        LoadedProject::Unknown => return None,
        LoadedProject::Empty => {}
    }
    let key = input
        .board
        .last()
        .and_then(|segment| BoardRef::parse(segment).ok())
        .and_then(|board| board.key())
        .or_else(|| {
            input
                .view
                .identity_label
                .as_deref()
                .and_then(|label| lpa_devices::BoardKey::parse(label).ok())
        })?;
    let octets = key.octets();
    Some(format!("LP-{:02x}{:02x}", octets[4], octets[5]))
}

/// The row's details: the board's Bluetooth switch at the top while it
/// answers ([`super::row_switch`]); then the name and what it means, or,
/// off, that it is off; and while a change applies, the restart's words.
fn details(input: &BoardCardInput<'_>, on: Option<bool>, name: Option<&str>) -> UiBarDetails {
    let mut details = UiBarDetails::default();
    details.switch = on.filter(|_| !input.offline()).map(|on| {
        row_switch(
            input,
            BarLayer::Bluetooth,
            "bluetooth",
            on,
            SwitchOffer {
                under: &["bluetooth", "enabled"],
                param: BLUETOOTH_ENABLED_PARAM,
                turn_off: DISABLE_BLUETOOTH,
                turn_on: ENABLE_BLUETOOTH,
            },
        )
    });
    let applying = input
        .access
        .and_then(|access| access.panel.as_ref())
        .filter(|panel| panel.restart_pending)
        .and_then(|panel| {
            Some(bluetooth_restart_words(
                panel.ble_enabled?,
                panel.can_restart,
            ))
        });
    match (on, name) {
        (Some(true), name) => {
            // One section: the name a phone sees, and what it is for.
            let mut named = facts(
                "",
                name.map(|name| RichLine::new("Name", name))
                    .into_iter()
                    .collect(),
            );
            named.sentence = Some(BLUETOOTH_NAME_SENTENCE.to_string());
            details.sections.push(named);
            if open_to_edit(input) {
                details
                    .sections
                    .push(help("No password", OPEN_BOARD_SENTENCE));
            }
            details
                .sections
                .extend(applying.map(|words| help("Restarting", words)));
        }
        (Some(false), _) => {
            details.sections.push(help("", BLUETOOTH_DISABLED_SENTENCE));
            details
                .sections
                .extend(applying.map(|words| help("Restarting", words)));
        }
        _ => {}
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
        fixture.view.loaded_project = LoadedProject::Unknown;
        assert_eq!(advertised_name(&fixture.input()), None);
    }

    /// Offline, the switch the board last said, dimmed — "on", never a
    /// name: what it runs is not known, so neither is the name.
    #[test]
    fn an_offline_board_keeps_its_last_switch() {
        let mut fixture = CardFixture::offline();
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.remembered_bluetooth = Some(true);
        let bar = bluetooth_bar(&fixture.input());
        assert_eq!(bar.summary, "on");
        assert!(bar.stale);
        fixture.remembered_bluetooth = None;
        let bar = bluetooth_bar(&fixture.input());
        assert_eq!(bar.summary, "\u{2014}");
    }

    /// Its details: the switch at the top, on, flipping off; then the name
    /// and what it is for, and on an open board that anyone nearby can
    /// edit it. Off, the switch stands off and flips on. Nothing in the
    /// sections presses the switch's offer: the switch is the one way.
    #[test]
    fn its_details_lead_with_the_switch_then_say_what_the_name_is_for() {
        let mut fixture = CardFixture::ready();
        let mut access = usb_access(Some(true));
        access.panel.as_mut().unwrap().open = OpenTo::Edit;
        fixture.access = Some(access);
        let bar = bluetooth_bar(&fixture.input());
        let sentences: Vec<&str> = bar
            .details
            .sections
            .iter()
            .filter_map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [BLUETOOTH_NAME_SENTENCE, OPEN_BOARD_SENTENCE]);
        let switch = bar.details.switch.clone().expect("a switch at the top");
        assert_eq!(
            (switch.label.as_str(), switch.icon.as_str()),
            ("Bluetooth", "bluetooth")
        );
        assert!(switch.on);
        let flip = switch.flip.expect("this link may flip it");
        assert_eq!(flip.word, "Disable Bluetooth");
        assert!(flip.offer.to_string().ends_with("/bluetooth/enabled"));
        assert_eq!(flip.args.get(BLUETOOTH_ENABLED_PARAM), Some("false"));
        assert_eq!(flip.refused, None);
        assert_eq!(section_presses(&bar), 0, "no Disable beside the switch");

        fixture.access = Some(usb_access(Some(false)));
        let bar = bluetooth_bar(&fixture.input());
        assert_eq!(bar.summary, "disabled");
        assert!(!bar.todo, "disabled wants nothing");
        let switch = bar.details.switch.clone().expect("a switch at the top");
        assert!(!switch.on);
        assert_eq!(switch.flip.expect("flips on").word, "Enable Bluetooth");
        assert_eq!(section_presses(&bar), 0, "no Enable beside the switch");
    }

    /// Over Bluetooth, the switch says why it won't turn off: it is the
    /// link you are on.
    #[test]
    fn over_bluetooth_the_switch_says_why_it_waits() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.access = Some(usb_access(Some(true)));
        let bar = bluetooth_bar(&fixture.input());
        let flip = bar.details.switch.and_then(|switch| switch.flip);
        assert_eq!(
            flip.and_then(|flip| flip.refused).as_deref(),
            Some(crate::OFF_OVER_BLUETOOTH)
        );
    }

    /// Offline, there is no switch to flip: the board can't hear it.
    #[test]
    fn an_offline_board_has_no_switch() {
        let mut fixture = CardFixture::offline();
        fixture.remembered_bluetooth = Some(true);
        assert!(bluetooth_bar(&fixture.input()).details.switch.is_none());
    }

    #[test]
    fn a_stand_in_says_a_dash() {
        let mut fixture = CardFixture::ready();
        fixture.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        fixture.access = Some(usb_access(Some(true)));
        assert_eq!(bluetooth_bar(&fixture.input()).summary, "\u{2014}");
    }

    /// How many of the details' section buttons press the switch's offer.
    fn section_presses(bar: &UiStackBar) -> usize {
        bar.details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .filter(|action| action.offer.to_string().contains("/bluetooth/"))
            .count()
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
