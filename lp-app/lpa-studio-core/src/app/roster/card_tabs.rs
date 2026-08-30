//! Sections → icon tabs: the card-as-control-panel grouping (M7′).
//!
//! The rich-object model survives the popover's retirement — the same
//! [`RichSection`]s the detail popover rendered become the card's tab
//! content. [`card_tabs`] groups a built view's sections onto the
//! ratified tab set (2026-07-24 replan, OUTCOMES (1)):
//!
//! | Tab | sections | present when |
//! |---|---|---|
//! | Play | — (the runtime's own frames) | the card has a project |
//! | Details | Health + Technical | always |
//! | Project | Project + Backup | either section exists |
//! | Performance | Performance | data flows (data-adaptive: hidden today) |
//! | Console | — (D42; content is the per-session console) | always |
//! | Danger | Danger zone | danger affordances exist |
//!
//! There is no Status tab (G1 ruling, honest-device-preview plan,
//! 2026-08-05): health evidence rides Details (né Settings — renamed at
//! G1b), which inherited Status's front-door role — the stable default a
//! fresh card with no picture opens on. This amends the M7′ "Status is
//! stable core" grammar.
//!
//! Grouping keys on the FIXED schema titles the roster builders own
//! (`sim_rich_object` today; the rebuilt device model's projection joins
//! it) — the titles are identity, not display strings picked per surface.
//! Tab badges derive exactly the
//! way the rollup derives globally: the worst ACTIONABLE tone among the
//! tab's sections, plus advisory chip tones as badge-only signals; only
//! the announcing families (Warning/Attention/Error) show. Danger never
//! badges (Danger weight never shouts).

use crate::UiStatusKind;
use crate::app::rich_object::{RichObjectView, RichSection, RichWeight};

/// The card's icon tabs, in their fixed order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CardTab {
    /// ▶ — the running control product, rendered from frames read OFF the
    /// device (honest-device-preview plan, converged spike section 1). It
    /// leads the row: on a connected card it is what you came to look at,
    /// and the controller opens a fresh connected card on it
    /// (`effective_card_tab`'s default rule).
    ///
    /// Presence is the caller's `has_play` — the card has a project whose
    /// running shape there is something honest to draw. A board with
    /// nothing on it gets no ▶: an empty frame promising a picture is the
    /// dishonesty the G2 hero-strip ruling threw out.
    ///
    /// It stays OUT of [`Default`] on purpose: the default is what a card
    /// with no live evidence at all opens on, and that is the front door.
    Play,
    /// The card's front door — the stable default a fresh card with no
    /// picture opens on: everything known and remembered about the
    /// runtime (health story, board, and — for a device — identity,
    /// transport, port, firmware).
    /// Renamed from Settings at G1b ("it really isn't settings"); it
    /// absorbed the Status tab one gate earlier.
    #[default]
    Details,
    Project,
    Performance,
    Console,
    Danger,
}

impl CardTab {
    /// Human label (tooltips; the pane mode may show it next to the icon).
    pub fn label(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Details => "Details",
            Self::Project => "Project",
            Self::Performance => "Performance",
            Self::Console => "Console",
            Self::Danger => "Danger",
        }
    }
}

/// One tab of the card's control panel: the sections it renders and the
/// badge tone it wears (already filtered to the announcing families).
#[derive(Clone, Debug, PartialEq)]
pub struct CardTabView<A> {
    pub tab: CardTab,
    pub sections: Vec<RichSection<A>>,
    pub badge: Option<UiStatusKind>,
}

/// Group a rich-object view's sections onto the card's tabs. Tab presence
/// is data-adaptive (a tab with nothing honest to show is absent), except
/// Details and Console — the stable core every card carries.
///
/// `has_play` decides the ▶ tab, which no section feeds: its content is the
/// runtime's own published frames, not rich-object evidence. It is an
/// argument rather than something derived here so the ONE rule — a card
/// with a project has a picture to show — is stated once by the caller that
/// knows the card, and so the tab set stays core-owned and core-tested
/// instead of being spliced in renderer-side.
pub fn card_tabs<A>(view: RichObjectView<A>, has_play: bool) -> Vec<CardTabView<A>> {
    let mut project = Vec::new();
    let mut settings = Vec::new();
    let mut technical = Vec::new();
    let mut performance = Vec::new();
    let mut danger = Vec::new();
    for section in view.sections {
        match section.title.as_str() {
            "Project" | "Backup" => project.push(section),
            "Technical" => technical.push(section),
            "Performance" => performance.push(section),
            "Danger zone" => danger.push(section),
            // Health — and, defensively, any future section the mapping
            // hasn't learned: Details is the card's front door.
            _ => settings.push(section),
        }
    }
    // Health evidence leads the folded tab; Technical follows it.
    settings.extend(technical);
    // ▶ leads (converged spike, section 1): the picture is the reason you
    // look at a connected card, so it sits leftmost and the front door is
    // one tab away rather than the other way round.
    let mut tabs = Vec::new();
    if has_play {
        tabs.push(tab_view(CardTab::Play, Vec::new()));
    }
    tabs.push(tab_view(CardTab::Details, settings));
    if !project.is_empty() {
        tabs.push(tab_view(CardTab::Project, project));
    }
    if !performance.is_empty() {
        tabs.push(tab_view(CardTab::Performance, performance));
    }
    tabs.push(tab_view(CardTab::Console, Vec::new()));
    if !danger.is_empty() {
        tabs.push(tab_view(CardTab::Danger, danger));
    }
    tabs
}

fn tab_view<A>(tab: CardTab, sections: Vec<RichSection<A>>) -> CardTabView<A> {
    let badge = if tab == CardTab::Danger {
        None
    } else {
        tab_badge(&sections)
    };
    CardTabView {
        tab,
        sections,
        badge,
    }
}

/// The tab's badge: worst actionable section tone plus advisory chip
/// tones, kept only when it announces (Warning/Attention/Error) — the
/// per-tab analogue of the global rollup.
fn tab_badge<A>(sections: &[RichSection<A>]) -> Option<UiStatusKind> {
    sections
        .iter()
        .flat_map(|section| {
            let actionable = (section.weight == RichWeight::Actionable).then_some(section.tone);
            let chip = section.chip.as_ref().map(|chip| chip.tone);
            actionable.into_iter().chain(chip)
        })
        .max_by_key(|tone| tone_severity(*tone))
        .filter(|tone| tone_severity(*tone) >= tone_severity(UiStatusKind::Warning))
}

/// Worst-first rank, mirroring the rollup's severity order.
fn tone_severity(tone: UiStatusKind) -> u8 {
    match tone {
        UiStatusKind::Neutral => 0,
        UiStatusKind::Good => 1,
        UiStatusKind::Working => 2,
        UiStatusKind::Warning | UiStatusKind::Attention => 3,
        UiStatusKind::Error => 4,
    }
}

#[cfg(test)]
mod tests {
    use crate::app::roster::sim_card_state::SimCardState;
    use crate::app::roster::sim_rich_object::{SimDetailAffordance, SimRichInput, sim_rich_object};

    use super::*;

    #[test]
    fn loaded_sim_gains_the_project_tab_and_play_leads_the_row() {
        let tabs = card_tabs(
            sim_rich_object(&SimRichInput {
                state: SimCardState::Running,
                project_name: Some("porch-sign"),
                board_id: None,
            }),
            true,
        );
        assert_eq!(
            tab_ids(&tabs),
            vec![
                CardTab::Play,
                CardTab::Details,
                CardTab::Project,
                CardTab::Console,
                CardTab::Danger,
            ]
        );
        // ▶ carries no sections — its content is the runtime's own frames —
        // so it must never badge (a badge would be a health claim the tab
        // cannot back up).
        let play = tab(&tabs, CardTab::Play);
        assert!(play.sections.is_empty());
        assert_eq!(play.badge, None);
        assert_eq!(
            tab(&tabs, CardTab::Danger).sections[0].affordances,
            vec![SimDetailAffordance::StopSimulator]
        );
        // Danger never badges (Danger weight never shouts).
        assert_eq!(tab(&tabs, CardTab::Danger).badge, None);
    }

    #[test]
    fn sim_tabs_are_the_honestly_applicable_set() {
        let tabs = card_tabs(
            sim_rich_object(&SimRichInput {
                state: SimCardState::Empty,
                project_name: None,
                board_id: None,
            }),
            false,
        );
        // empty → no Project tab; the folded Details front door and the
        // stop-sim danger zone are always there
        assert_eq!(
            tab_ids(&tabs),
            vec![CardTab::Details, CardTab::Console, CardTab::Danger]
        );
        // A healthy card announces nothing.
        assert!(tabs.iter().all(|tab| tab.badge.is_none()));
    }

    fn tab_ids<A>(tabs: &[CardTabView<A>]) -> Vec<CardTab> {
        tabs.iter().map(|tab| tab.tab).collect()
    }

    fn tab<'a, A>(tabs: &'a [CardTabView<A>], id: CardTab) -> &'a CardTabView<A> {
        tabs.iter().find(|tab| tab.tab == id).expect("tab present")
    }
}
