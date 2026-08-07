//! Sections → icon tabs: the card-as-control-panel grouping (M7′).
//!
//! The rich-object model survives the popover's retirement — the same
//! [`RichSection`]s the detail popover rendered become the card's tab
//! content. [`device_card_tabs`] groups a built view's sections onto the
//! ratified tab set (2026-07-24 replan, OUTCOMES (1)):
//!
//! | Tab | sections | present when |
//! |---|---|---|
//! | Play | — (the device's own frames) | the card has a project |
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
//! (`device_rich_object` / `sim_rich_object`) — the titles are identity,
//! not display strings picked per surface. Tab badges derive exactly the
//! way the rollup derives globally: the worst ACTIONABLE tone among the
//! tab's sections, plus advisory chip tones as badge-only signals; only
//! the announcing families (Warning/Attention/Error) show. Danger never
//! badges (Danger weight never shouts).

use crate::UiStatusKind;
use crate::app::rich_object::{RichObjectView, RichSection, RichWeight};

/// The card's icon tabs, in their fixed order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DeviceCardTab {
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
    /// device (health story, board, uid, transport, port, firmware).
    /// Renamed from Settings at G1b ("it really isn't settings"); it
    /// absorbed the Status tab one gate earlier.
    #[default]
    Details,
    Project,
    Performance,
    Console,
    Danger,
}

impl DeviceCardTab {
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
    pub tab: DeviceCardTab,
    pub sections: Vec<RichSection<A>>,
    pub badge: Option<UiStatusKind>,
}

/// Group a rich-object view's sections onto the card's tabs. Tab presence
/// is data-adaptive (a tab with nothing honest to show is absent), except
/// Details and Console — the stable core every card carries.
///
/// `has_play` decides the ▶ tab, which no section feeds: its content is the
/// device's own published frames, not rich-object evidence. It is an
/// argument rather than something derived here so the ONE rule — a card
/// with a project has a picture to show — is stated once by the caller that
/// knows the card, and so the tab set stays core-owned and core-tested
/// instead of being spliced in renderer-side.
pub fn device_card_tabs<A>(view: RichObjectView<A>, has_play: bool) -> Vec<CardTabView<A>> {
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
        tabs.push(tab_view(DeviceCardTab::Play, Vec::new()));
    }
    tabs.push(tab_view(DeviceCardTab::Details, settings));
    if !project.is_empty() {
        tabs.push(tab_view(DeviceCardTab::Project, project));
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
    use lpc_model::LpFeature;
    use lpc_wire::{BuildFacts, HardwareFacts};

    use crate::app::roster::device_rich_object::{DeviceRichInput, device_rich_object};
    use crate::app::roster::roster_card_state::RosterCardState;
    use crate::app::roster::sim_rich_object::{SimDetailAffordance, SimRichInput, sim_rich_object};
    use crate::app::roster::{BundledFirmware, DeviceDetailAffordance, RosterAffordance};

    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn running_behind_device_groups_onto_the_ratified_tabs() {
        let state = RosterCardState::RunningBehind {
            observed_version: Some(3),
            head_version: Some(5),
        };
        let tabs = device_card_tabs(device_rich_object(&input(&state)), false);
        assert_eq!(
            tab_ids(&tabs),
            vec![
                DeviceCardTab::Details,
                DeviceCardTab::Project,
                DeviceCardTab::Console,
                DeviceCardTab::Danger,
            ]
        );
        // Health announces on the folded Settings front door; Project's
        // drift announces too.
        assert_eq!(tabs[0].badge, Some(UiStatusKind::Attention));
        assert_eq!(tabs[1].badge, Some(UiStatusKind::Attention));
        // Health leads the folded section list; Technical follows.
        let titles: Vec<&str> = tabs[0]
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(titles.last(), Some(&"Technical"));
        // Danger never badges, and carries the destructive rows.
        assert_eq!(tabs[3].badge, None);
        assert_eq!(
            tabs[3].sections[0].affordances,
            vec![
                DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot),
                DeviceDetailAffordance::BackUpFilesystem,
                DeviceDetailAffordance::Roster(RosterAffordance::WipeProject),
                DeviceDetailAffordance::FlashFirmware,
                DeviceDetailAffordance::EraseDevice,
                DeviceDetailAffordance::DisconnectDevice,
            ]
        );
    }

    /// ▶ leads the row and carries no sections — its content is the
    /// device's frames, so no rich-object evidence ever lands on it and it
    /// must never badge (a badge would be a health claim the tab cannot
    /// back up).
    #[test]
    fn play_leads_the_row_and_carries_nothing_but_the_picture() {
        let state = RosterCardState::RunningUpToDate;
        let with_play = device_card_tabs(device_rich_object(&input(&state)), true);
        assert_eq!(
            tab_ids(&with_play),
            vec![
                DeviceCardTab::Play,
                DeviceCardTab::Details,
                DeviceCardTab::Project,
                DeviceCardTab::Console,
                DeviceCardTab::Danger,
            ]
        );
        let play = tab(&with_play, DeviceCardTab::Play);
        assert!(play.sections.is_empty());
        assert_eq!(play.badge, None);
        // and the same card with nothing to show simply loses the tab —
        // every other tab keeps its place.
        let without = device_card_tabs(device_rich_object(&input(&state)), false);
        assert_eq!(tab_ids(&without), tab_ids(&with_play)[1..]);
    }

    #[test]
    fn backup_rides_the_project_tab_on_the_diverged_card() {
        let tabs = device_card_tabs(
            device_rich_object(&input(&RosterCardState::EditedOnDevice {
                local_saved_at: None,
                pushed_at: None,
            })),
            false,
        );
        let project = tab(&tabs, DeviceCardTab::Project);
        let titles: Vec<&str> = project
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Project", "Backup"]);
    }

    #[test]
    fn advisory_fw_chip_badges_the_folded_settings_tab() {
        let bundled = BundledFirmware {
            commit: "def987654321".to_string(),
            dirty: false,
        };
        // WITHOUT the advisory chip, a Good health never badges the folded
        // tab — the badge announces, it doesn't decorate.
        let quiet = device_card_tabs(
            device_rich_object(&input(&RosterCardState::RunningUpToDate)),
            false,
        );
        assert_eq!(tab(&quiet, DeviceCardTab::Details).badge, None);
        let mut input = input(&RosterCardState::RunningUpToDate);
        input.bundled_fw = Some(&bundled);
        let tabs = device_card_tabs(device_rich_object(&input), false);
        assert_eq!(
            tab(&tabs, DeviceCardTab::Details).badge,
            Some(UiStatusKind::Attention)
        );
    }

    #[test]
    fn working_states_drop_the_danger_tab() {
        let state = RosterCardState::OperationInFlight {
            label: "Installing firmware".to_string(),
            percent: Some(62),
        };
        let tabs = device_card_tabs(device_rich_object(&input(&state)), false);
        assert!(!tab_ids(&tabs).contains(&DeviceCardTab::Danger));
    }

    #[test]
    fn offline_device_moves_reconnect_to_the_play_box_and_keeps_forget_in_danger() {
        let state = RosterCardState::Offline {
            last_seen_at: Some(NOW - 2.0 * 86_400.0),
        };
        let mut input = input(&state);
        input.fw = None;
        let tabs = device_card_tabs(device_rich_object(&input), true);
        assert_eq!(
            tab_ids(&tabs),
            vec![
                DeviceCardTab::Play,
                DeviceCardTab::Details,
                DeviceCardTab::Project,
                DeviceCardTab::Console,
                DeviceCardTab::Danger,
            ]
        );
        // G1b ruling 8: a card WITH a ▶ tab carries Reconnect inside the
        // picture box (renderer-side), so the front door stays button-free.
        assert_eq!(
            tab(&tabs, DeviceCardTab::Details).sections[0].affordances,
            Vec::new()
        );
        // …but a card with no project — no ▶ — keeps the front-door
        // Reconnect: there is no other surface.
        let mut projectless = input.clone();
        projectless.project_name = None;
        let bare = device_card_tabs(device_rich_object(&projectless), false);
        assert_eq!(
            tab(&bare, DeviceCardTab::Details).sections[0].affordances,
            vec![DeviceDetailAffordance::Roster(RosterAffordance::Reconnect)]
        );
        // registered + offline → Troubleshoot (always offered, 2026-07-31)
        // then Forget. Troubleshoot earns its place even here: an offline
        // card is exactly a device that stopped answering, and the sheet's
        // Reconnect and recovery steps are what you want.
        assert_eq!(
            tab(&tabs, DeviceCardTab::Danger).sections[0].affordances,
            vec![
                DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot),
                DeviceDetailAffordance::ForgetDevice
            ]
        );
        // a Neutral remembered card announces nothing
        assert!(tabs.iter().all(|tab| tab.badge.is_none()));
    }

    #[test]
    fn loaded_sim_gains_the_project_tab() {
        let tabs = device_card_tabs(
            sim_rich_object(&SimRichInput {
                state: &RosterCardState::RunningUpToDate,
                project_name: Some("porch-sign"),
                board_id: None,
                now_secs: NOW,
            }),
            true,
        );
        assert_eq!(
            tab_ids(&tabs),
            vec![
                DeviceCardTab::Play,
                DeviceCardTab::Details,
                DeviceCardTab::Project,
                DeviceCardTab::Console,
                DeviceCardTab::Danger,
            ]
        );
        assert_eq!(
            tab(&tabs, DeviceCardTab::Danger).sections[0].affordances,
            vec![SimDetailAffordance::StopSimulator]
        );
    }

    #[test]
    fn sim_tabs_are_the_honestly_applicable_set() {
        let tabs = device_card_tabs(
            sim_rich_object(&SimRichInput {
                state: &RosterCardState::ConnectedEmpty,
                project_name: None,
                board_id: None,
                now_secs: NOW,
            }),
            false,
        );
        // empty → no Project tab; the folded Settings front door and the
        // stop-sim danger zone are always there
        assert_eq!(
            tab_ids(&tabs),
            vec![
                DeviceCardTab::Details,
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
            hardware: Some(&DEVICE_HW),
            bundled_fw: None,
            detected_chip: None,
            port_label: None,
            board_id: None,
            now_secs: NOW,
        }
    }

    static DEVICE_FW: std::sync::LazyLock<BuildFacts> = std::sync::LazyLock::new(|| BuildFacts {
        features: vec![
            LpFeature::NodeButton,
            LpFeature::NodeClock,
            LpFeature::NodeFluid,
            LpFeature::NodeFixture,
            LpFeature::NodePlaylist,
            LpFeature::NodeRadio,
            LpFeature::NodeShader,
            LpFeature::NodeTexture,
            LpFeature::SvcButton,
            LpFeature::SvcRadioEspnow,
            LpFeature::GfxLpvm,
        ],
        package: "fw-esp32c6".to_string(),
        commit: "abc123456789".to_string(),
        dirty: false,
        profile: "release-esp32".to_string(),
    });

    /// An all-capable unit: the gaps-only Technical lines add nothing here,
    /// which is the point.
    static DEVICE_HW: std::sync::LazyLock<HardwareFacts> =
        std::sync::LazyLock::new(|| HardwareFacts {
            radio: true,
            button: true,
            board_id: None,
            ..Default::default()
        });
}
