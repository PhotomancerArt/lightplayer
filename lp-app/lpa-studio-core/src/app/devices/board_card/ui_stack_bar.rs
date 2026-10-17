//! [`UiStackBar`]: one of the card's five rows — project, link, access,
//! firmware, hardware, always in that order (`docs/ux/anatomy/board-card.md`,
//! "The row"). One line each, one grammar: **icon · kind · name · state ·
//! action**. The icon is the kind's own; the kind is a muted word that
//! teaches what the row is about; the name is the thing itself, there in
//! every state; the state is said only when it is not normal, its tone
//! saying how bad; and at most one action fixes it, flush at the row's end.
//! Its work when it is doing some; everything the line leaves out in its
//! details.
//!
//! The Link row is the one exception: the link's own name is its kind
//! (`USB · connected`), as a version number is, so it has no kind word.

use crate::{RichSection, RichWeight, UiStatusKind};

use super::ui_bar_work::UiBarWork;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;

/// One row.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStackBar {
    pub layer: BarLayer,
    /// An icon token, as offers use (`project`, `usb`, `lock`, `firmware`,
    /// `chip`, …): the kind's one icon, the same in every state.
    pub icon: String,
    /// The kind word ("Project", "Access", "Firmware", "Hardware"), drawn as
    /// a fixed muted column after the icon so names line up down the card;
    /// `None` on the Link row ([`BarLayer::kind_word`]).
    pub kind: Option<&'static str>,
    /// The name: the thing itself — "studio", "edit · USB", "v2026.10.16-1",
    /// "XIAO ESP32-C6"; on the Link row the link and how it stands, "USB ·
    /// connected". Here in every state; never a bare "Not known yet".
    pub summary: String,
    /// The state, only when it is not normal: "out of date", "crashing",
    /// "2 boards", "model not set", "editor open".
    pub aside: Option<String>,
    /// An icon alone at the row's end, after the state: the cloud on a
    /// board also reachable through lightplayer.app.
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
    /// Last known rather than current (an offline board's project, access
    /// and firmware): drawn dimmed, its words unchanged (P13).
    pub stale: bool,
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
    /// What the mark means, in words: "Also reachable through
    /// lightplayer.app".
    pub label: String,
    /// Lit when the fact holds right now — drawn filled, in the live tone;
    /// else set up and not holding — drawn struck through.
    pub lit: bool,
}

/// Which bar, in the card's fixed order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BarLayer {
    Project,
    Connection,
    Access,
    Firmware,
    Hardware,
}

impl BarLayer {
    /// The five, in the card's order.
    pub const ALL: [BarLayer; 5] = [
        BarLayer::Project,
        BarLayer::Connection,
        BarLayer::Access,
        BarLayer::Firmware,
        BarLayer::Hardware,
    ];

    /// The layer's name in hooks and logs (`data-bar="firmware"`).
    pub fn as_str(self) -> &'static str {
        match self {
            BarLayer::Project => "project",
            BarLayer::Connection => "connection",
            BarLayer::Access => "access",
            BarLayer::Firmware => "firmware",
            BarLayer::Hardware => "hardware",
        }
    }

    /// The row's kind word: "Project", "Access", "Firmware", "Hardware";
    /// `None` for the Link row, whose link name is its kind.
    pub fn kind_word(self) -> Option<&'static str> {
        match self {
            BarLayer::Project => Some("Project"),
            BarLayer::Connection => None,
            BarLayer::Access => Some("Access"),
            BarLayer::Firmware => Some("Firmware"),
            BarLayer::Hardware => Some("Hardware"),
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

    /// Every row but the Link row carries its kind's word.
    #[test]
    fn every_row_but_the_link_names_its_kind() {
        let kinds: Vec<Option<&str>> = BarLayer::ALL.into_iter().map(BarLayer::kind_word).collect();
        assert_eq!(
            kinds,
            [
                Some("Project"),
                None,
                Some("Access"),
                Some("Firmware"),
                Some("Hardware")
            ]
        );
        let row = UiStackBar::new(BarLayer::Firmware, "firmware", "v2026.10.16-1");
        assert_eq!(row.kind, Some("Firmware"));
        assert_eq!(row.tone, UiStatusKind::Neutral);
        assert!(!row.stale);
    }
}
