//! [`UiStackBar`]: one of the card's eight rows — project, the four links
//! (USB, Wi‑Fi, Bluetooth, lightplayer.app), access, firmware, hardware,
//! always in that order (`docs/ux/anatomy/board-card.md`, "The row"). For a
//! new board they are its setup list: a row with something left to do says
//! so ([`UiStackBar::todo`]). One line each, one grammar: **icon · kind ·
//! name · state · action**. The icon is the kind's own; the kind is a muted word that
//! teaches what the row is about; the name is the thing itself, there in
//! every state; the state is said only when it is not normal, its tone
//! saying how bad; and at most one action fixes it, flush at the row's end.
//! Its work when it is doing some; everything the line leaves out in its
//! details.
//!
//! The four link rows' kind is the link's own name ("USB", "Wi‑Fi",
//! "Bluetooth", "lightplayer.app"), drawn a step brighter than the other
//! kinds: the link is the thing, as a version number is
//! ([`BarLayer::is_link`]).

use crate::{RichSection, RichWeight, UiStatusKind};

use super::ui_bar_work::UiBarWork;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use crate::app::devices::ui_link_kind::UiLinkKind;

/// One row.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStackBar {
    pub layer: BarLayer,
    /// An icon token, as offers use (`project`, `usb`, `lock`, `firmware`,
    /// `chip`, …): the kind's one icon, the same in every state.
    pub icon: String,
    /// The kind word ("Project", "Access", "Firmware", "Hardware"; on a
    /// link row the link's name, "USB", "Wi‑Fi", "Bluetooth",
    /// "lightplayer.app", or a stand-in's "Simulated"), drawn as a fixed
    /// column after the icon so names line up down the card
    /// ([`BarLayer::kind_word`]).
    pub kind: Option<&'static str>,
    /// The name: the thing itself — "studio", "edit · USB", "v2026.10.16-1",
    /// "XIAO ESP32-C6"; on a link row what the link reaches or how it
    /// stands: "connected", "Cedar Lane", "LP-b48c", "Yona's account",
    /// "not connected", "disabled", "—" while nothing is known. Empty while
    /// the row's work says it ("connecting").
    pub summary: String,
    /// The state, only when it is not normal: "out of date", "crashing",
    /// "2 boards", "model not set", "editor open".
    pub aside: Option<String>,
    /// An icon alone at the row's end, after the state, saying one fact
    /// without words. No row draws one today; the account mark ("From your
    /// account") is the secrets vault's, once a store can say a value came
    /// from it.
    pub mark: Option<UiBarMark>,
    /// The row's tint, in ui.md's status families: Neutral; Attention
    /// (orange) when a device needs you; Error (red, striped) when it is
    /// broken. Never Warning (an unsaved edit's yellow), never Live (blue
    /// is the lit cloud's alone).
    pub tone: UiStatusKind,
    /// The one offer flush at the row's end: the fix.
    pub action: Option<UiCardAction>,
    /// The way out beside the fix (P7): Reset beside Restore, on a board
    /// whose data was lost.
    pub way_out: Option<UiCardAction>,
    /// A word on a flush section at the row's end that opens the row's own
    /// details instead of pressing an offer: "Review", on a confirmation
    /// the details ask for. Opening details is view state, not an offer.
    pub opens_details: Option<&'static str>,
    /// Last known rather than current (an offline board's project, links,
    /// access and firmware): drawn dimmed, its words unchanged (P13).
    pub stale: bool,
    /// Something is left to do here, and the row's action does it: a new
    /// board's setup list (Project none · Add, Wi‑Fi not connected · Set up,
    /// lightplayer.app not connected · Sign in, Firmware none · Install).
    /// Drawn lit: the whole row on a lighter flat ground. Never on a toned
    /// row, a disabled one, one whose fix is another row's, or one at work.
    pub todo: bool,
    /// Work this row is doing, or has just done; while it runs the spinner
    /// takes the icon's place and the work's words are the row's state.
    pub work: Option<UiBarWork>,
    pub details: UiBarDetails,
}

impl UiStackBar {
    /// A quiet `layer` row naming `summary`, with the layer's kind word and
    /// nothing else yet.
    pub fn new(layer: BarLayer, icon: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            layer,
            icon: icon.into(),
            kind: layer.kind_word(),
            summary: summary.into(),
            aside: None,
            mark: None,
            tone: UiStatusKind::Neutral,
            action: None,
            way_out: None,
            opens_details: None,
            stale: false,
            todo: false,
            work: None,
            details: UiBarDetails::default(),
        }
    }
}

/// An icon alone at a bar's end, saying one fact without words on the
/// bar: its label is the accessible name and the hover. Its shape says the
/// fact too, never its colour alone (P14).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBarMark {
    /// An icon token (`cloud`).
    pub icon: String,
    /// What the mark means, in words: "From your account".
    pub label: String,
    /// Lit when the fact holds right now — drawn filled, in the live tone;
    /// else set up and not holding — drawn struck through.
    pub lit: bool,
}

/// Which bar, in the card's fixed order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BarLayer {
    Project,
    /// The USB cable (or a stand-in's runtime, which stands in for one).
    Usb,
    Wifi,
    Bluetooth,
    /// lightplayer.app: the board reached through the cloud relay.
    Cloud,
    Access,
    Firmware,
    Hardware,
}

impl BarLayer {
    /// The eight, in the card's order: the links in the middle.
    pub const ALL: [BarLayer; 8] = [
        BarLayer::Project,
        BarLayer::Usb,
        BarLayer::Wifi,
        BarLayer::Bluetooth,
        BarLayer::Cloud,
        BarLayer::Access,
        BarLayer::Firmware,
        BarLayer::Hardware,
    ];

    /// The layer's name in hooks and logs (`data-bar="firmware"`).
    pub fn as_str(self) -> &'static str {
        match self {
            BarLayer::Project => "project",
            BarLayer::Usb => "usb",
            BarLayer::Wifi => "wifi",
            BarLayer::Bluetooth => "bluetooth",
            BarLayer::Cloud => "cloud",
            BarLayer::Access => "access",
            BarLayer::Firmware => "firmware",
            BarLayer::Hardware => "hardware",
        }
    }

    /// The row's kind word: "Project", "Access", "Firmware", "Hardware",
    /// and on a link row the link's own name ("USB", "Wi‑Fi",
    /// "Bluetooth", "lightplayer.app").
    pub fn kind_word(self) -> Option<&'static str> {
        Some(match self {
            BarLayer::Project => "Project",
            BarLayer::Usb => UiLinkKind::Usb.label(),
            BarLayer::Wifi => UiLinkKind::Wifi.label(),
            BarLayer::Bluetooth => UiLinkKind::Bluetooth.label(),
            BarLayer::Cloud => UiLinkKind::Relay.label(),
            BarLayer::Access => "Access",
            BarLayer::Firmware => "Firmware",
            BarLayer::Hardware => "Hardware",
        })
    }

    /// One of the four link rows, whose kind is the link's name.
    pub fn is_link(self) -> bool {
        self.link().is_some()
    }

    /// The link a link row is about.
    pub fn link(self) -> Option<UiLinkKind> {
        match self {
            BarLayer::Usb => Some(UiLinkKind::Usb),
            BarLayer::Wifi => Some(UiLinkKind::Wifi),
            BarLayer::Bluetooth => Some(UiLinkKind::Bluetooth),
            BarLayer::Cloud => Some(UiLinkKind::Relay),
            BarLayer::Project | BarLayer::Access | BarLayer::Firmware | BarLayer::Hardware => None,
        }
    }

    /// The row of `link`: a board reached through lightplayer.app is the
    /// lightplayer.app row's.
    pub fn of_link(link: UiLinkKind) -> Self {
        match link {
            UiLinkKind::Usb => BarLayer::Usb,
            UiLinkKind::Wifi => BarLayer::Wifi,
            UiLinkKind::Bluetooth => BarLayer::Bluetooth,
            UiLinkKind::Relay => BarLayer::Cloud,
        }
    }
}

/// A bar's details: Studio's detail card, merged with the bar that opened
/// it. A notice first, a Danger section last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiBarDetails {
    /// Facts and verbs (`RichSection<UiCardAction>`): the bar's notice
    /// first (Actionable, in the bar's tone), its facts, its verbs, and a
    /// [`RichWeight::Danger`] section last.
    pub sections: Vec<RichSection<UiCardAction>>,
    /// Today's panels, named ([`UiDetailPanel`]).
    pub panels: Vec<UiDetailPanel>,
    /// A question that must be answered now (the layout question, Q40): the
    /// web opens these details while it is set.
    pub raised: bool,
}

impl UiBarDetails {
    /// The bar's notice: its first section, when that section is
    /// Actionable — what the status corner rolls up.
    pub fn notice(&self) -> Option<&RichSection<UiCardAction>> {
        self.sections
            .first()
            .filter(|section| section.weight == RichWeight::Actionable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight rows, the links in the middle; every row names its kind, and
    /// a link row's kind is the link's own name.
    #[test]
    fn eight_rows_each_naming_its_kind() {
        let kinds: Vec<Option<&str>> = BarLayer::ALL.into_iter().map(BarLayer::kind_word).collect();
        assert_eq!(
            kinds,
            [
                Some("Project"),
                Some("USB"),
                Some("Wi\u{2011}Fi"),
                Some("Bluetooth"),
                Some("lightplayer.app"),
                Some("Access"),
                Some("Firmware"),
                Some("Hardware")
            ]
        );
        let hooks: Vec<&str> = BarLayer::ALL.into_iter().map(BarLayer::as_str).collect();
        assert_eq!(
            hooks,
            [
                "project",
                "usb",
                "wifi",
                "bluetooth",
                "cloud",
                "access",
                "firmware",
                "hardware"
            ]
        );
        let links: Vec<BarLayer> = BarLayer::ALL
            .into_iter()
            .filter(|layer| layer.is_link())
            .collect();
        assert_eq!(
            links,
            [
                BarLayer::Usb,
                BarLayer::Wifi,
                BarLayer::Bluetooth,
                BarLayer::Cloud
            ]
        );
        for layer in links {
            assert_eq!(BarLayer::of_link(layer.link().unwrap()), layer);
        }
        let row = UiStackBar::new(BarLayer::Firmware, "firmware", "v2026.10.16-1");
        assert_eq!(row.kind, Some("Firmware"));
        assert_eq!(row.tone, UiStatusKind::Neutral);
        assert!(!row.stale);
        assert!(!row.todo);
    }
}
