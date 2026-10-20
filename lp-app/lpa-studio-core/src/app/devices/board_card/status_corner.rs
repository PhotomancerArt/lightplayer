//! The status corner: a quiet ⋯ cut out of the picture's corner, in every
//! state (`docs/ux/anatomy/board-card.md`, "Corner"). It says nothing
//! itself: what needs you is the surface's (P2: once, and loud there), the
//! editor holding the board too. It opens the card's details: every notice,
//! how the board is running, the picture's facts and the terminal.
//!
//! The notices are each row's own (the first section of its details,
//! Actionable, in the row's tone; a failed row's notice is its failure,
//! [`super::board_card_builder`]), in row order. Work that is running is
//! not a notice (D34).

use super::board_card_input::BoardCardInput;
use super::board_picture::{frame_rate, last_frame, picture_line};
use super::held_board::held_summary;
use super::ui_board_panel::UiBoardPanel;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiStackBar};
use super::ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
use crate::{RichLine, RichSection, RichWeight, UiStatusKind};

/// A board's status corner, read off its finished rows and, while it is
/// connected, its panel.
pub(crate) fn status_corner(
    input: &BoardCardInput<'_>,
    bars: &[UiStackBar],
    panel: Option<&UiBoardPanel>,
) -> UiStatusCorner {
    let mut sections = notices(bars);
    sections.push(running_section(input));
    let mut panels: Vec<UiDetailPanel> = panel.and_then(panel_settings).into_iter().collect();
    if input.linked() {
        panels.push(UiDetailPanel::Terminal {
            lines: input.view.terminal.clone(),
            dropped: input.view.terminal_dropped,
        });
    }
    UiStatusCorner {
        mark: match input.linked() {
            true => CornerMark::Fine,
            false => CornerMark::Quiet,
        },
        details: UiCornerDetails { sections, panels },
    }
}

/// Every notice on the card, in row order: each row's own notice (a failed
/// row's is its failure, with the row's Retry).
pub(crate) fn notices(bars: &[UiStackBar]) -> Vec<RichSection<UiCardAction>> {
    bars.iter()
        .filter_map(|bar| bar.details.notice().cloned())
        .collect()
}

/// The connected panel's settings, which the card's ⋯ holds (the panel
/// draws only controls): the reset over the root's scope and, at the edit
/// tier, the auto-save switch. `None` when neither can be offered.
fn panel_settings(panel: &UiBoardPanel) -> Option<UiDetailPanel> {
    (panel.target.is_some() || panel.auto_save.is_some()).then(|| UiDetailPanel::PanelSettings {
        target: panel.target,
        held: panel.held(),
        auto_save: panel.auto_save,
    })
}

/// A bar's name as a notice's title.
pub(crate) fn layer_title(layer: BarLayer) -> &'static str {
    match layer {
        BarLayer::Project => "Project",
        BarLayer::Usb => "USB",
        BarLayer::Wifi => "Wi\u{2011}Fi",
        BarLayer::Bluetooth => "Bluetooth",
        BarLayer::Cloud => "lightplayer.app",
        BarLayer::Access => "Access",
        BarLayer::Firmware => "Firmware",
        BarLayer::Hardware => "Hardware",
    }
}

/// "Running": the board's state in its own words, and the picture's facts
/// — its frame rate, how old it is, and the sentence that says where it
/// comes from or why there is none.
fn running_section(input: &BoardCardInput<'_>) -> RichSection<UiCardAction> {
    let view = input.view;
    // A board another tab holds is not "Offline" or "Attached": it is
    // running, in that tab.
    let state = match input.held() {
        Some(held) => held_summary(held).to_string(),
        None => view.state_label.clone(),
    };
    let mut lines = vec![RichLine::new("State", state)];
    if let Some(fps) = frame_rate(input) {
        lines.push(RichLine::new("Frame rate", fps));
    }
    if let Some(age) = last_frame(input) {
        lines.push(RichLine::new("Last frame", age));
    }
    if let Some(picture) = picture_line(input) {
        lines.push(RichLine::new("Picture", picture));
    }
    RichSection {
        title: "Running".to_string(),
        tone: UiStatusKind::Neutral,
        sentence: None,
        lines,
        chip: None,
        affordances: Vec::new(),
        weight: RichWeight::Advisory,
    }
}

#[cfg(test)]
mod tests {
    use super::super::card_fixtures::{CardFixture, feed};
    use super::super::ui_stack_bar::UiBarDetails;
    use super::*;
    use crate::app::devices::device_card_feed_view::FeedLiveness;

    /// Every row's notice is in the corner's details, in row order, then
    /// "Running".
    #[test]
    fn every_notice_is_in_the_details_in_row_order() {
        let mut ready = CardFixture::ready();
        let bars = vec![
            bar_with_notice(BarLayer::Project, UiStatusKind::Attention, "a fault"),
            bar_with_notice(BarLayer::Firmware, UiStatusKind::Error, "crashing"),
        ];
        let corner = status_corner(&ready.input(), &bars, None);
        let sentences: Vec<Option<&str>> = corner
            .details
            .sections
            .iter()
            .map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [Some("a fault"), Some("crashing"), None]);
        assert_eq!(corner.details.sections[2].title, "Running");
    }

    /// The corner is a quiet ⋯ in every state: a problem is the surface's
    /// to say (P2), and nothing marks it here.
    #[test]
    fn a_watched_board_is_fine_and_an_offline_one_quiet() {
        let mut ready = CardFixture::ready();
        let crashing = [bar_with_notice(
            BarLayer::Firmware,
            UiStatusKind::Error,
            "crashing",
        )];
        assert_eq!(
            status_corner(&ready.input(), &crashing, None).mark,
            CornerMark::Fine
        );
        let mut offline = CardFixture::offline();
        assert_eq!(
            status_corner(&offline.input(), &[], None).mark,
            CornerMark::Quiet
        );
    }

    #[test]
    fn the_terminal_is_there_only_while_the_board_is_linked() {
        let mut ready = CardFixture::ready();
        let panels = status_corner(&ready.input(), &[], None).details.panels;
        assert!(matches!(
            panels.as_slice(),
            [UiDetailPanel::Terminal { .. }]
        ));
        let mut offline = CardFixture::offline();
        assert!(
            status_corner(&offline.input(), &[], None)
                .details
                .panels
                .is_empty()
        );
    }

    /// Connected, the ⋯ holds the panel's settings ahead of the terminal:
    /// the reset over the root's scope, how many controls it lets go of,
    /// and auto-save. With neither a scope nor auto-save, nothing.
    #[test]
    fn the_connected_panels_settings_lead_the_panels() {
        let mut ready = CardFixture::ready();
        let target = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        let panel = UiBoardPanel {
            target: Some(target),
            sections: Vec::new(),
            auto_save: Some(false),
            edit: None,
        };
        let panels = status_corner(&ready.input(), &[], Some(&panel))
            .details
            .panels;
        assert!(
            matches!(
                panels.as_slice(),
                [
                    UiDetailPanel::PanelSettings {
                        target: Some(_),
                        held: 0,
                        auto_save: Some(false),
                    },
                    UiDetailPanel::Terminal { .. },
                ]
            ),
            "{panels:?}"
        );
        let bare = UiBoardPanel {
            target: None,
            auto_save: None,
            ..panel
        };
        let panels = status_corner(&ready.input(), &[], Some(&bare))
            .details
            .panels;
        assert!(
            matches!(panels.as_slice(), [UiDetailPanel::Terminal { .. }]),
            "{panels:?}"
        );
    }

    /// The frame rate and the picture's age are facts in the details, a
    /// label and a value (P17).
    #[test]
    fn running_says_the_state_the_rate_and_the_picture() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        let corner = status_corner(&fixture.input(), &[], None);
        let running = corner.details.sections.last().expect("running");
        assert_eq!(running.title, "Running");
        assert_eq!(
            lines(running),
            [("State", "Ready"), ("Frame rate", "58 fps")],
            "a live picture needs no sentence"
        );
        fixture.feed = Some(feed(FeedLiveness::Stale, true));
        let corner = status_corner(&fixture.input(), &[], None);
        assert_eq!(
            lines(corner.details.sections.last().unwrap()),
            [("State", "Ready"), ("Last frame", "12 s ago")]
        );
    }

    fn lines(section: &RichSection<UiCardAction>) -> Vec<(&str, &str)> {
        section
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str()))
            .collect()
    }

    fn bar_with_notice(layer: BarLayer, tone: UiStatusKind, sentence: &str) -> UiStackBar {
        UiStackBar {
            tone,
            details: UiBarDetails {
                switch: None,
                sections: vec![RichSection {
                    title: layer_title(layer).to_string(),
                    tone,
                    sentence: Some(sentence.to_string()),
                    lines: Vec::new(),
                    chip: None,
                    affordances: Vec::new(),
                    weight: RichWeight::Actionable,
                }],
                panels: Vec::new(),
                raised: false,
            },
            ..UiStackBar::new(layer, "chip", "")
        }
    }
}
