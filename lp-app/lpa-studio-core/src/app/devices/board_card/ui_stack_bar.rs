//! [`UiStackBar`]: one of the card's five bars — project, connection,
//! access, firmware, hardware, always in that order. One line each: an
//! icon, a summary, an aside, at most one action flush at its end; its work
//! when it is doing some; everything the line leaves out in its details.

use crate::{RichSection, RichWeight, UiStatusKind};

use super::ui_bar_work::UiBarWork;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;

/// One bar.
#[derive(Clone, Debug, PartialEq)]
pub struct UiStackBar {
    pub layer: BarLayer,
    /// An icon token, as offers use (`project`, `usb`, `lock`, `firmware`,
    /// `chip`, …).
    pub icon: String,
    /// What the bar says: "USB · live", "Nothing on it yet", the version.
    pub summary: String,
    /// The quieter words at the end: "3 boards", "also cloud", "last seen".
    pub aside: Option<String>,
    /// The icon leading the aside ("also cloud" carries `cloud`).
    pub aside_icon: Option<String>,
    /// The bar's tint: a notice's family, or Neutral. Blue
    /// ([`UiStatusKind::Live`]) only for Update.
    pub tone: UiStatusKind,
    /// The one offer flush at the bar's end.
    pub action: Option<UiCardAction>,
    /// Work this bar is doing, or has just done; while it runs the bar
    /// reads neutral with the work's words.
    pub work: Option<UiBarWork>,
    pub details: UiBarDetails,
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
