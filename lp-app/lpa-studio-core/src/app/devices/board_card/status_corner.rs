//! The status corner: the worst notice on the card, then the frame rate or
//! the picture's age; its details hold every notice, how the board is
//! running, the picture's words and the terminal.
//!
//! One source for "what needs you": each bar's notice (the first section of
//! its details, Actionable, in the bar's tone) and each bar's failed work.
//! The corner rolls those up with [`RichObjectView::rollup`], so the worst
//! tone wins and ties go to the earlier bar. Work that is running never
//! changes the corner (D34): it is not a notice.

use super::board_card_input::BoardCardInput;
use super::board_picture::picture_line;
use super::held_board::held_summary;
use super::ui_bar_work::BarWorkState;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiStackBar};
use super::ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
use crate::app::devices::age_words::age_words;
use crate::app::devices::device_card_feed_view::FeedLiveness;
use crate::{RichLine, RichObjectView, RichSection, RichWeight, UiStatusKind};

/// A board's status corner, read off its finished bars.
pub(crate) fn status_corner(input: &BoardCardInput<'_>, bars: &[UiStackBar]) -> UiStatusCorner {
    let notices = notices(bars);
    let fallback = match input.linked() {
        true => CornerMark::Fine,
        false => CornerMark::Quiet,
    };
    let mark = worst_mark(&notices).unwrap_or(fallback);
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

/// Every notice on the card, in bar order: each bar's own notice, and its
/// failed work as an Error notice with the bar's Retry.
pub(crate) fn notices(bars: &[UiStackBar]) -> Vec<RichSection<UiCardAction>> {
    let mut notices = Vec::new();
    for bar in bars {
        if let Some(notice) = bar.details.notice() {
            notices.push(notice.clone());
        }
        if let Some(work) = &bar.work
            && let BarWorkState::Failed { retry } = &work.state
        {
            notices.push(RichSection {
                title: layer_title(bar.layer).to_string(),
                tone: UiStatusKind::Error,
                sentence: Some(work.words.clone()),
                lines: Vec::new(),
                chip: None,
                affordances: retry.iter().cloned().collect(),
                weight: RichWeight::Actionable,
            });
        }
    }
    notices
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

/// "58 fps" while the board is live, else the picture's age, else nothing.
fn reading(input: &BoardCardInput<'_>) -> Option<String> {
    let fps = || {
        input
            .view
            .engine_fps
            .filter(|_| input.linked())
            .map(|fps| format!("{fps} fps"))
    };
    match input.feed.map(|feed| (feed, feed.liveness)) {
        Some((_, FeedLiveness::Live)) => fps(),
        Some((feed, FeedLiveness::Stale | FeedLiveness::Offline | FeedLiveness::Lens)) => {
            feed.frame.as_ref().and(feed.frame_age_secs).map(age_words)
        }
        Some((_, FeedLiveness::Waiting)) | None => fps(),
    }
}

/// "Running": the board's state in its own words, its frame rate, and the
/// picture's line.
fn running_section(input: &BoardCardInput<'_>) -> RichSection<UiCardAction> {
    let view = input.view;
    // A board another tab holds is not "Offline" or "Attached": it is
    // running, in that tab.
    let state = match input.held() {
        Some(held) => held_summary(held).to_string(),
        None => view.state_label.clone(),
    };
    let mut lines = vec![RichLine::new("State", state)];
    if let Some(fps) = view.engine_fps.filter(|_| input.linked()) {
        lines.push(RichLine::new("Frame rate", format!("{fps} fps")));
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

    use super::super::card_fixtures::{CardFixture, feed};
    use super::super::ui_bar_work::UiBarWork;
    use super::super::ui_stack_bar::UiBarDetails;
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;

    #[test]
    fn the_worst_notice_wins_and_ties_go_to_the_earlier_bar() {
        let bars = vec![
            bar_with_notice(BarLayer::Project, UiStatusKind::Attention, "a fault"),
            bar_with_notice(
                BarLayer::Connection,
                UiStatusKind::Warning,
                "not responding",
            ),
            bar_with_notice(BarLayer::Firmware, UiStatusKind::Live, "update"),
        ];
        let notices = notices(&bars);
        assert_eq!(notices.len(), 3, "every notice is listed");
        assert_eq!(
            worst_mark(&notices),
            Some(CornerMark::Notice(UiStatusKind::Attention))
        );
        let mut failed = bars.clone();
        failed[3 - 1].work = Some(UiBarWork {
            words: "The update failed".to_string(),
            percent: None,
            state: BarWorkState::Failed { retry: None },
            cancel: None,
            other_device: false,
        });
        assert_eq!(
            worst_mark(&notices_of(&failed)),
            Some(CornerMark::Notice(UiStatusKind::Error)),
            "a failed activity is an error"
        );
        assert_eq!(
            worst_mark(&notices_of(&[bar_with_notice(
                BarLayer::Firmware,
                UiStatusKind::Live,
                "update"
            )])),
            None,
            "blue is not a problem"
        );
    }

    #[test]
    fn a_watched_board_is_fine_and_an_offline_one_quiet() {
        let mut ready = CardFixture::ready();
        let corner = status_corner(&ready.input(), &[]);
        assert_eq!(corner.mark, CornerMark::Fine);
        let mut offline = CardFixture::offline();
        let corner = status_corner(&offline.input(), &[]);
        assert_eq!(corner.mark, CornerMark::Quiet);
    }

    #[test]
    fn the_reading_is_the_frame_rate_live_else_the_pictures_age() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        fixture.view.engine_fps = Some(58);
        assert_eq!(
            status_corner(&fixture.input(), &[]).reading.as_deref(),
            Some("58 fps")
        );
        fixture.feed = Some(feed(FeedLiveness::Stale, true));
        assert_eq!(
            status_corner(&fixture.input(), &[]).reading.as_deref(),
            Some("12 s ago")
        );
        let mut offline = CardFixture::offline();
        let mut saved = feed(FeedLiveness::Offline, true);
        saved.frame_age_secs = Some(5.0 * 3_600.0);
        offline.feed = Some(saved);
        assert_eq!(
            status_corner(&offline.input(), &[]).reading.as_deref(),
            Some("5 h ago")
        );
        assert_eq!(
            status_corner(&CardFixture::offline().input(), &[]).reading,
            None
        );
    }

    #[test]
    fn the_terminal_is_there_only_while_the_board_is_linked() {
        let mut ready = CardFixture::ready();
        let panels = status_corner(&ready.input(), &[]).details.panels;
        assert!(matches!(
            panels.as_slice(),
            [UiDetailPanel::Terminal { .. }]
        ));
        let mut offline = CardFixture::offline();
        assert!(
            status_corner(&offline.input(), &[])
                .details
                .panels
                .is_empty()
        );
    }

    #[test]
    fn running_says_the_state_the_rate_and_the_picture() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        let corner = status_corner(&fixture.input(), &[]);
        let running = corner.details.sections.last().expect("running");
        assert_eq!(running.title, "Running");
        let lines: Vec<(&str, &str)> = running
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                ("State", "Ready"),
                ("Frame rate", "58 fps"),
                ("Picture", "live · 43 fps")
            ]
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
            words: "Sending the project · 40%".to_string(),
            percent: Some(40),
            state: BarWorkState::Running,
            cancel: None,
            other_device: false,
        });
        assert_eq!(
            status_corner(&fixture.input(), &[bar]).mark,
            CornerMark::Fine
        );
    }

    fn notices_of(bars: &[UiStackBar]) -> Vec<RichSection<UiCardAction>> {
        notices(bars)
    }

    fn bar_with_notice(layer: BarLayer, tone: UiStatusKind, sentence: &str) -> UiStackBar {
        UiStackBar {
            layer,
            icon: "chip".to_string(),
            summary: String::new(),
            aside: None,
            aside_icon: None,
            tone,
            action: None,
            work: None,
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
        }
    }
}
