//! The status corner: the picture's state, quiet unless it has something to
//! say (`docs/ux/anatomy/board-card.md`, "Corner"). Nothing while the board
//! is live and well, nor on a stale picture (the picture dims; the Link row
//! says since when): P4. "Editing" while the editor holds the picture. A
//! notice's icon only while the connected panel covers the rows that would
//! say it (P2): a row says its own problem once. Its details hold every
//! notice, how the board is running, the picture's facts and the terminal.
//!
//! One source for "what needs you": each row's notice (the first section of
//! its details, Actionable, in the row's tone; a failed row's notice is its
//! failure, [`super::board_card_builder`]). The corner rolls those up with
//! [`RichObjectView::rollup`], so the worst tone wins and ties go to the
//! earlier row. Work that is running never changes the corner (D34): it is
//! not a notice.

use super::board_card_input::BoardCardInput;
use super::board_picture::{board_picture, frame_rate, last_frame, picture_line};
use super::held_board::held_summary;
use super::ui_board_picture::PictureSource;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiStackBar};
use super::ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
use crate::{RichLine, RichObjectView, RichSection, RichWeight, UiStatusKind};

/// The corner's reading while the editor holds the picture.
pub const EDITING: &str = "Editing";

/// A board's status corner, read off its finished rows. `covered`: the
/// connected panel stands in the rows' place, so the corner carries their
/// worst notice's icon.
pub(crate) fn status_corner(
    input: &BoardCardInput<'_>,
    bars: &[UiStackBar],
    covered: bool,
) -> UiStatusCorner {
    let notices = notices(bars);
    let fallback = match input.linked() {
        true => CornerMark::Fine,
        false => CornerMark::Quiet,
    };
    let mark = worst_mark(&notices).filter(|_| covered).unwrap_or(fallback);
    let mut sections = notices;
    sections.push(running_section(input));
    let panels = match input.linked() {
        true => vec![UiDetailPanel::Terminal {
            lines: input.view.terminal.clone(),
            dropped: input.view.terminal_dropped,
        }],
        false => Vec::new(),
    };
    UiStatusCorner {
        mark,
        reading: reading(input),
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

/// The corner's mark for these notices: the worst one's family, when it
/// is a notice at all (blue and green are not).
pub(crate) fn worst_mark(notices: &[RichSection<UiCardAction>]) -> Option<CornerMark> {
    let tone = RichObjectView::new(notices.to_vec()).rollup().tone;
    match tone {
        UiStatusKind::Warning | UiStatusKind::Attention | UiStatusKind::Error => {
            Some(CornerMark::Notice(tone))
        }
        UiStatusKind::Neutral | UiStatusKind::Working | UiStatusKind::Good | UiStatusKind::Live => {
            None
        }
    }
}

/// A bar's name as a notice's title.
pub(crate) fn layer_title(layer: BarLayer) -> &'static str {
    match layer {
        BarLayer::Project => "Project",
        BarLayer::Connection => "Connection",
        BarLayer::Access => "Access",
        BarLayer::Firmware => "Firmware",
        BarLayer::Hardware => "Hardware",
    }
}

/// "Editing" while the editor holds the picture (its last frame, dimmed),
/// else nothing: a live picture says live, a stale one dims (P4).
fn reading(input: &BoardCardInput<'_>) -> Option<String> {
    let picture = board_picture(input);
    (picture.source == PictureSource::Lens && picture.dim).then(|| EDITING.to_string())
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
    use lpa_devices::ActivityKind;
    use lpa_devices::view::OutcomeView;

    use super::super::card_fixtures::{CardFixture, feed, lens_feed};
    use super::super::ui_bar_work::{BarWorkState, UiBarWork};
    use super::super::ui_stack_bar::UiBarDetails;
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;
    use crate::app::devices::device_card_feed_view::FeedLiveness;

    #[test]
    fn the_worst_notice_wins_and_ties_go_to_the_earlier_row() {
        let bars = vec![
            bar_with_notice(BarLayer::Project, UiStatusKind::Attention, "a fault"),
            bar_with_notice(BarLayer::Connection, UiStatusKind::Neutral, "update"),
            bar_with_notice(BarLayer::Firmware, UiStatusKind::Error, "crashing"),
        ];
        let notices = notices(&bars);
        assert_eq!(notices.len(), 3, "every notice is listed");
        assert_eq!(
            worst_mark(&notices),
            Some(CornerMark::Notice(UiStatusKind::Error))
        );
        assert_eq!(
            worst_mark(&notices_of(&[bar_with_notice(
                BarLayer::Firmware,
                UiStatusKind::Neutral,
                "update"
            )])),
            None,
            "an update on offer is not a problem"
        );
    }

    /// P2: a row says its own problem; the corner carries the icon only
    /// while the connected panel covers the rows.
    #[test]
    fn a_notice_shows_in_the_corner_only_while_the_panel_covers_the_rows() {
        let mut ready = CardFixture::ready();
        let bars = vec![bar_with_notice(
            BarLayer::Firmware,
            UiStatusKind::Error,
            "crashing",
        )];
        assert_eq!(
            status_corner(&ready.input(), &bars, false).mark,
            CornerMark::Fine,
            "the row is red; the corner stays quiet"
        );
        assert_eq!(
            status_corner(&ready.input(), &bars, true).mark,
            CornerMark::Notice(UiStatusKind::Error)
        );
        // The notice is in the corner's details either way.
        let corner = status_corner(&ready.input(), &bars, false);
        assert_eq!(
            corner.details.sections[0].sentence.as_deref(),
            Some("crashing")
        );
    }

    #[test]
    fn a_watched_board_is_fine_and_an_offline_one_quiet() {
        let mut ready = CardFixture::ready();
        assert_eq!(
            status_corner(&ready.input(), &[], false).mark,
            CornerMark::Fine
        );
        let mut offline = CardFixture::offline();
        assert_eq!(
            status_corner(&offline.input(), &[], false).mark,
            CornerMark::Quiet
        );
    }

    /// P4: no reading while live, nor on a stale picture; "Editing" while
    /// the editor holds the picture.
    #[test]
    fn the_corner_reads_editing_and_nothing_else() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        fixture.view.engine_fps = Some(58);
        assert_eq!(status_corner(&fixture.input(), &[], false).reading, None);
        fixture.feed = Some(feed(FeedLiveness::Stale, true));
        assert_eq!(status_corner(&fixture.input(), &[], false).reading, None);
        let mut offline = CardFixture::offline();
        let mut saved = feed(FeedLiveness::Offline, true);
        saved.frame_age_secs = Some(5.0 * 3_600.0);
        offline.feed = Some(saved);
        assert_eq!(status_corner(&offline.input(), &[], false).reading, None);
        // The editor holds the board's wire: its last frame, dimmed.
        fixture.feed = Some(feed(FeedLiveness::Lens, true));
        assert_eq!(
            status_corner(&fixture.input(), &[], false)
                .reading
                .as_deref(),
            Some("Editing")
        );
        // The connected card's own session frames are current: no word.
        fixture.feed = Some(lens_feed());
        assert_eq!(status_corner(&fixture.input(), &[], false).reading, None);
    }

    #[test]
    fn the_terminal_is_there_only_while_the_board_is_linked() {
        let mut ready = CardFixture::ready();
        let panels = status_corner(&ready.input(), &[], false).details.panels;
        assert!(matches!(
            panels.as_slice(),
            [UiDetailPanel::Terminal { .. }]
        ));
        let mut offline = CardFixture::offline();
        assert!(
            status_corner(&offline.input(), &[], false)
                .details
                .panels
                .is_empty()
        );
    }

    /// The frame rate and the picture's age are facts in the details, a
    /// label and a value (P17).
    #[test]
    fn running_says_the_state_the_rate_and_the_picture() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        let corner = status_corner(&fixture.input(), &[], false);
        let running = corner.details.sections.last().expect("running");
        assert_eq!(running.title, "Running");
        assert_eq!(
            lines(running),
            [("State", "Ready"), ("Frame rate", "58 fps")],
            "a live picture needs no sentence"
        );
        fixture.feed = Some(feed(FeedLiveness::Stale, true));
        let corner = status_corner(&fixture.input(), &[], false);
        assert_eq!(
            lines(corner.details.sections.last().unwrap()),
            [("State", "Ready"), ("Last frame", "12 s ago")]
        );
    }

    /// While work runs the corner does not change (D34).
    #[test]
    fn running_work_never_changes_the_corner() {
        let mut fixture = CardFixture::ready();
        fixture.view.last_outcome = Some(OutcomeView {
            summary: "ok".to_string(),
            ok: true,
        });
        fixture.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: true,
            at: fixture.now,
        });
        let mut bar = bar_with_notice(BarLayer::Project, UiStatusKind::Neutral, "");
        bar.details = UiBarDetails::default();
        bar.work = Some(UiBarWork {
            state: BarWorkState::Running,
            ..UiBarWork::running("sending · 40%", Some(40))
        });
        assert_eq!(
            status_corner(&fixture.input(), &[bar], true).mark,
            CornerMark::Fine
        );
    }

    fn lines(section: &RichSection<UiCardAction>) -> Vec<(&str, &str)> {
        section
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str()))
            .collect()
    }

    fn notices_of(bars: &[UiStackBar]) -> Vec<RichSection<UiCardAction>> {
        notices(bars)
    }

    fn bar_with_notice(layer: BarLayer, tone: UiStatusKind, sentence: &str) -> UiStackBar {
        UiStackBar {
            tone,
            details: UiBarDetails {
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
