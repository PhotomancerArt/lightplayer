//! Sections → icon tabs: the card-as-control-panel grouping (M7′).
//!
//! The rich-object model survives the popover's retirement — the same
//! [`RichSection`]s the detail popover rendered become the card's tab
//! content. [`device_card_tabs`] groups a built view's sections onto the
//! ratified tab set (2026-07-24 replan, OUTCOMES (1)):
//!
//! | Tab | sections | present when |
//! |---|---|---|
//! | Status | Health | always |
//! | Project | Project + Backup | either section exists |
//! | Settings | Technical | the section exists |
//! | Performance | Performance | data flows (data-adaptive: hidden today) |
//! | Console | — (D42; content is the per-session console) | always |
//! | Danger | Danger zone | danger affordances exist |
//!
//! Grouping keys on the FIXED schema titles the roster builders own
//! (`device_rich_object` / `sim_rich_object`) — the titles are identity,
//! not display strings picked per surface. Tab badges derive exactly the
//! way the rollup derives globally: the worst ACTIONABLE tone among the
//! tab's sections, plus advisory chip tones as badge-only signals; only
//! the announcing families (Warning/Attention/Error) show. Danger never
//! badges (Danger weight never shouts).

use crate::UiStatusKind;
use crate::app::rich_object::{RichObjectView, RichSection, RichWeight};

/// The card's icon tabs, in their fixed order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceCardTab {
    Status,
    Project,
    Settings,
    Performance,
    Console,
    Danger,
}

impl DeviceCardTab {
    /// Human label (tooltips; the pane mode may show it next to the icon).
    pub fn label(self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Project => "Project",
            Self::Settings => "Settings",
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
    pub tab: DeviceCardTab,
    pub sections: Vec<RichSection<A>>,
    pub badge: Option<UiStatusKind>,
}

/// Group a rich-object view's sections onto the card's tabs. Tab presence
/// is data-adaptive (a tab with nothing honest to show is absent), except
/// Status and Console — the stable core every card carries.
pub fn device_card_tabs<A>(view: RichObjectView<A>) -> Vec<CardTabView<A>> {
    let mut status = Vec::new();
    let mut project = Vec::new();
    let mut settings = Vec::new();
    let mut performance = Vec::new();
    let mut danger = Vec::new();
    for section in view.sections {
        match section.title.as_str() {
            "Project" | "Backup" => project.push(section),
            "Technical" => settings.push(section),
            "Performance" => performance.push(section),
            "Danger zone" => danger.push(section),
            // Health — and, defensively, any future section the mapping
            // hasn't learned: Status is the card's front door.
            _ => status.push(section),
        }
    }
    let mut tabs = vec![tab_view(DeviceCardTab::Status, status)];
    if !project.is_empty() {
        tabs.push(tab_view(DeviceCardTab::Project, project));
    }
    if !settings.is_empty() {
        tabs.push(tab_view(DeviceCardTab::Settings, settings));
    }
    if !performance.is_empty() {
        tabs.push(tab_view(DeviceCardTab::Performance, performance));
    }
    tabs.push(tab_view(DeviceCardTab::Console, Vec::new()));
    if !danger.is_empty() {
        tabs.push(tab_view(DeviceCardTab::Danger, danger));
    }
    tabs
}

fn tab_view<A>(tab: DeviceCardTab, sections: Vec<RichSection<A>>) -> CardTabView<A> {
    let badge = if tab == DeviceCardTab::Danger {
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
    use lpc_wire::FwProvenance;

    use crate::app::roster::device_rich_object::{DeviceRichInput, device_rich_object};
    use crate::app::roster::roster_card_state::RosterCardState;
    use crate::app::roster::sim_rich_object::{SimRichInput, sim_rich_object};
    use crate::app::roster::{BundledFirmware, DeviceDetailAffordance};

    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn running_behind_device_groups_onto_the_ratified_tabs() {
        let state = RosterCardState::RunningBehind {
            observed_version: Some(3),
            head_version: Some(5),
        };
        let tabs = device_card_tabs(device_rich_object(&input(&state)));
        assert_eq!(
            tab_ids(&tabs),
            vec![
                DeviceCardTab::Status,
                DeviceCardTab::Project,
                DeviceCardTab::Settings,
                DeviceCardTab::Console,
                DeviceCardTab::Danger,
            ]
        );
        // Health announces on Status; Project's drift announces too.
        assert_eq!(tabs[0].badge, Some(UiStatusKind::Attention));
        assert_eq!(tabs[1].badge, Some(UiStatusKind::Attention));
        // Technical is advisory with no chip here: quiet.
        assert_eq!(tabs[2].badge, None);
        // Danger never badges, and carries the destructive rows.
        assert_eq!(tabs[4].badge, None);
        assert_eq!(
            tabs[4].sections[0].affordances,
            vec![
                DeviceDetailAffordance::FlashFirmware,
                DeviceDetailAffordance::EraseDevice,
            ]
        );
    }

    #[test]
    fn backup_rides_the_project_tab_on_the_diverged_card() {
        let tabs = device_card_tabs(device_rich_object(&input(&RosterCardState::EditedOnDevice)));
        let project = tab(&tabs, DeviceCardTab::Project);
        let titles: Vec<&str> = project
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Project", "Backup"]);
    }

    #[test]
    fn advisory_fw_chip_badges_settings_without_touching_status() {
        let bundled = BundledFirmware {
            commit: "def987654321".to_string(),
            dirty: false,
        };
        let mut input = input(&RosterCardState::RunningUpToDate);
        input.bundled_fw = Some(&bundled);
        let tabs = device_card_tabs(device_rich_object(&input));
        assert_eq!(
            tab(&tabs, DeviceCardTab::Settings).badge,
            Some(UiStatusKind::Attention)
        );
        // a Good health never badges — the badge announces, it doesn't decorate
        assert_eq!(tab(&tabs, DeviceCardTab::Status).badge, None);
    }

    #[test]
    fn working_states_drop_the_danger_tab() {
        let state = RosterCardState::OperationInFlight {
            label: "Installing firmware".to_string(),
            percent: Some(62),
        };
        let tabs = device_card_tabs(device_rich_object(&input(&state)));
        assert!(!tab_ids(&tabs).contains(&DeviceCardTab::Danger));
    }

    #[test]
    fn sim_tabs_are_the_honestly_applicable_set() {
        let tabs = device_card_tabs(sim_rich_object(&SimRichInput {
            state: &RosterCardState::ConnectedEmpty,
            project_name: None,
            now_secs: NOW,
        }));
        // no Technical evidence → no Settings tab; empty → no Project tab;
        // the stop-sim danger zone is always there
        assert_eq!(
            tab_ids(&tabs),
            vec![
                DeviceCardTab::Status,
                DeviceCardTab::Console,
                DeviceCardTab::Danger,
            ]
        );
    }

    fn tab_ids<A>(tabs: &[CardTabView<A>]) -> Vec<DeviceCardTab> {
        tabs.iter().map(|tab| tab.tab).collect()
    }

    fn tab<'a, A>(tabs: &'a [CardTabView<A>], id: DeviceCardTab) -> &'a CardTabView<A> {
        tabs.iter().find(|tab| tab.tab == id).expect("tab present")
    }

    fn input<'a>(state: &'a RosterCardState) -> DeviceRichInput<'a> {
        DeviceRichInput {
            state,
            uid: Some("dev_7pQr5St89uVwXy2C"),
            transport: "USB",
            project_name: Some("porch-sign"),
            fw: Some(&DEVICE_FW),
            bundled_fw: None,
            now_secs: NOW,
        }
    }

    static DEVICE_FW: std::sync::LazyLock<FwProvenance> =
        std::sync::LazyLock::new(|| FwProvenance {
            package: "fw-esp32".to_string(),
            commit: "abc123456789".to_string(),
            dirty: false,
            profile: "release-esp32".to_string(),
        });
}
