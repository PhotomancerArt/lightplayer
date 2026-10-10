//! [`CompactPanel`]: the board's panel at card size — what a connected
//! board card draws in its five bars' place (the board card ADR, §4;
//! `docs/style/ui.md` "The board card": connected, the bars become the
//! panel).
//!
//! Core picked what it shows ([`UiBoardPanel`]: the master first, then
//! knobs and toggles, four at most, and how many more the play page has);
//! this piece decides only how it looks:
//!
//! 1. the master, when the first control is a fader: one row across the
//!    card;
//! 2. the rest in one row, each its own widget;
//! 3. **All controls** (and "· N more" when there are more), a plain link to
//!    the board's play page, with the row's action — Edit, or its lock on a
//!    play-only board — flush at its end, outside the link (CD15).
//!
//! Every control is the panel's own [`ModulePanelControl`] at card size:
//! the latch, the gold engaged state, the let-go glyph, the detail popover
//! and the writes are the same as everywhere else in Studio. No panel-wide
//! reset and no auto-save switch here (Q12): the play page has both.
//!
//! **The card never changes height.** The block is exactly the five bars'
//! height (five 28 px rows, their hairlines inside them) and spans their
//! five grid rows; `overflow: hidden` is a guard, never the fit.
//!
//! Walk hooks: `data-board-panel` on the block, `data-all-controls` on the
//! last row; each control carries the panel's own (`data-panel-scope`,
//! `data-panel-channel`, `data-panel-state`: [`ModulePanelControl`]).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBoardPanel, UiCardControl, UiPanelWidget};

use super::{ModulePanelControl, panel_gesture_actions};
use crate::app::board_card::{CardAction, CardActionLook};
use crate::base::{StudioIcon, StudioIconName};

/// The connected card's panel. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CompactPanel(
    /// Core's picks of the board's root panel.
    panel: UiBoardPanel,
    /// The board's play page (All controls); `None` draws the row's words
    /// without a link.
    #[props(default)]
    all_controls: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let on_panel = panel_gesture_actions(on_action);
    let (master, row) = split_master(&panel.controls);
    let more = more_words(panel.more);
    let title = match &more {
        Some(more) => format!("All controls {more}"),
        None => "All controls".to_string(),
    };
    let link_body = rsx! {
        span { class: ROW_ICON_CLASS, aria_hidden: "true",
            StudioIcon { name: StudioIconName::Sliders, size: 12 }
        }
        span { class: ROW_WORDS_CLASS, "All controls" }
        if let Some(more) = more {
            span { class: ROW_MORE_CLASS, "{more}" }
        }
        span { class: ROW_CHEVRON_CLASS, aria_hidden: "true",
            StudioIcon { name: StudioIconName::Collapsed, size: 12 }
        }
    };
    rsx! {
        div { class: PANEL_CLASS, "data-board-panel": "",
            if let Some(master) = master {
                div { class: MASTER_ROW_CLASS,
                    CardControl { control: master, on_panel, on_action }
                }
            }
            if !row.is_empty() {
                div { class: CONTROLS_ROW_CLASS,
                    for control in row {
                        CardControl {
                            key: "{control.scope}/{control.view.channel}",
                            control,
                            on_panel,
                            on_action,
                        }
                    }
                }
            }
            div { class: ALL_CONTROLS_ROW_CLASS, "data-all-controls": "",
                match all_controls {
                    Some(href) => rsx! {
                        a { class: ALL_CONTROLS_LINK_CLASS, href, title, {link_body} }
                    },
                    None => rsx! {
                        span { class: ALL_CONTROLS_LINK_CLASS, title, {link_body} }
                    },
                }
                if let Some(edit) = panel.edit {
                    CardAction { action: edit, look: CardActionLook::BarEnd, on_action }
                }
            }
        }
    }
}

/// One picked control: the panel's own control at card size (its walk
/// hooks are its own).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CardControl(
    control: UiCardControl,
    on_panel: EventHandler<super::PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        ModulePanelControl {
            view: control.view,
            scope: control.scope,
            compact: true,
            on_panel: Some(on_panel),
            on_action: Some(on_action),
        }
    }
}

/// The master (the first control, when it is a fader) and the row after
/// it. Core puts the master first when the panel has one; a panel without
/// one is all row.
fn split_master(controls: &[UiCardControl]) -> (Option<UiCardControl>, Vec<UiCardControl>) {
    match controls.split_first() {
        Some((first, rest)) if matches!(first.view.control.widget, UiPanelWidget::Fader { .. }) => {
            (Some(first.clone()), rest.to_vec())
        }
        _ => (None, controls.to_vec()),
    }
}

/// "· 2 more" when the play page has more than the card shows.
fn more_words(more: usize) -> Option<String> {
    (more > 0).then(|| format!("\u{b7} {more} more"))
}

/// The block: the five bars' rows of the card's grid, at their height
/// (5 × 28 px; each bar's hairline is inside its 28 px). A column: the
/// master's strip, the row taking what is left, All controls at the foot.
/// `overflow-hidden` is the guard: core picked at most four, which fit.
const PANEL_CLASS: &str = "ux-board-panel tw:row-span-5 tw:flex tw:h-[140px] tw:min-h-0 tw:min-w-0 tw:flex-col tw:overflow-hidden";

/// The master's strip: one row across the card, its top hairline the name
/// bar's foot.
const MASTER_ROW_CLASS: &str = "tw:flex tw:flex-none tw:min-w-0 tw:items-center tw:border-0 tw:border-t tw:border-solid tw:border-border tw:px-2.5";

/// The row of knobs and toggles: what the master and All controls leave,
/// the controls spread across it.
const CONTROLS_ROW_CLASS: &str = "tw:flex tw:min-h-0 tw:min-w-0 tw:flex-1 tw:items-center tw:justify-around tw:gap-1 tw:border-0 tw:border-t tw:border-solid tw:border-border tw:px-1.5";

/// All controls: a bar — 28 px, its hairline, its action flush at its end,
/// rounded with the card at its foot (`ux-board-bar`'s last-child rule
/// rounds the action's corner too).
const ALL_CONTROLS_ROW_CLASS: &str = "ux-board-bar tw:relative tw:flex tw:h-7 tw:flex-none tw:min-w-0 tw:items-stretch tw:rounded-b-[7px] tw:border-0 tw:border-t tw:border-solid tw:border-border tw:text-[11.5px] tw:text-muted-foreground";

/// The row's link: the whole row but its action, a bar's line.
const ALL_CONTROLS_LINK_CLASS: &str = "tw:flex tw:h-full tw:min-w-0 tw:flex-1 tw:items-center tw:gap-1.5 tw:rounded-bl-[7px] tw:px-2.5 tw:text-inherit tw:no-underline tw:transition-colors tw:hover:bg-white/5 ux-focus-ring";

/// The row's pieces. "All controls" never truncates; on a phone's narrow
/// card (the card is its own container, `ux-board-card`) the chevron goes
/// and "· N more" truncates first — the link's title says it whole.
const ROW_ICON_CLASS: &str = "tw:inline-flex tw:flex-none tw:text-subtle-foreground";
const ROW_WORDS_CLASS: &str = "tw:flex-none tw:whitespace-nowrap tw:text-foreground";
const ROW_MORE_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:truncate tw:text-subtle-foreground";
const ROW_CHEVRON_CLASS: &str =
    "tw:ml-auto tw:inline-flex tw:flex-none tw:text-dim-foreground tw:@max-[239px]:hidden";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit, UiSlotFieldState,
        UiSlotValue,
    };

    use super::*;

    /// A fader first draws as the full-width strip; the rest share the row.
    #[test]
    fn a_fader_first_is_the_master_strip() {
        let html = render(panel(
            vec![fader("brightness"), knob("speed"), knob("hue")],
            0,
        ));
        let strip = section(&html, MASTER_ROW_CLASS);
        assert!(
            strip.contains(r#"data-panel-channel="brightness""#),
            "{strip}"
        );
        let row = section(&html, CONTROLS_ROW_CLASS);
        for channel in ["speed", "hue"] {
            assert!(
                row.contains(&format!(r#"data-panel-channel="{channel}""#)),
                "{row}"
            );
        }
        assert!(!row.contains(r#"data-panel-channel="brightness""#));
        assert!(
            html.contains(COMPACT_FADER_MARK),
            "the master is a card-size fader"
        );
    }

    /// With no fader first, every control is in the one row.
    #[test]
    fn no_fader_first_puts_every_control_in_the_row() {
        let html = render(panel(vec![knob("speed"), knob("hue"), fader("level")], 0));
        assert!(!html.contains(MASTER_ROW_CLASS), "no master strip");
        let row = section(&html, CONTROLS_ROW_CLASS);
        for channel in ["speed", "hue", "level"] {
            assert!(
                row.contains(&format!(r#"data-panel-channel="{channel}""#)),
                "{row}"
            );
        }
    }

    /// "N more" only when the play page has more.
    #[test]
    fn n_more_shows_only_when_there_are_more() {
        assert_eq!(more_words(0), None);
        assert_eq!(more_words(2).as_deref(), Some("\u{b7} 2 more"));
        let html = render(panel(vec![knob("speed")], 0));
        assert!(html.contains("All controls"));
        assert!(!html.contains("more"), "{html}");
        let html = render(panel(vec![knob("speed")], 11));
        assert!(html.contains("\u{b7} 11 more"), "{html}");
    }

    /// All controls is a plain link to the play page; without one, the
    /// row still reads, unlinked.
    #[test]
    fn all_controls_is_a_plain_link() {
        let html = render_with(panel(vec![knob("speed")], 0), Some("/p/porch-prjx/play"));
        let row = section(&html, ALL_CONTROLS_ROW_CLASS);
        assert!(row.contains(r#"href="/p/porch-prjx/play""#), "{row}");
        let html = render(panel(vec![knob("speed")], 0));
        assert!(!html.contains("href="), "{html}");
    }

    /// No Edit on the panel draws no button at the row's end.
    #[test]
    fn edit_none_draws_no_button() {
        let html = render(panel(vec![knob("speed")], 0));
        let row = section(&html, ALL_CONTROLS_ROW_CLASS);
        assert!(!row.contains("<button"), "{row}");
        assert!(!row.contains("data-offer-path"), "{row}");
    }

    /// The block's height is the five bars': 5 × 28 px, over their five
    /// grid rows; overflow is a guard.
    #[test]
    fn the_block_is_the_five_bars_height() {
        assert!(PANEL_CLASS.contains("tw:h-[140px]"), "{PANEL_CLASS}");
        assert!(PANEL_CLASS.contains("tw:row-span-5"), "{PANEL_CLASS}");
        assert!(PANEL_CLASS.contains("tw:overflow-hidden"), "{PANEL_CLASS}");
        assert_eq!(BARS * BAR_PX, 140, "five 28 px bars");
        assert!(
            ALL_CONTROLS_ROW_CLASS.contains("tw:h-7"),
            "the foot is a 28 px bar"
        );
    }

    /// The walk hooks: the block, each control's channel and state, the
    /// All controls row.
    #[test]
    fn the_walk_hooks_are_present() {
        let mut held = knob("hue");
        held = held.with_state(UiPanelControlState::Engaged, Some("authored 0.4"));
        let following =
            knob("speed").with_state(UiPanelControlState::ReadFollowing, Some("lfo · speed"));
        let html = render(panel(vec![fader("brightness"), following, held], 3));
        assert!(html.contains("data-board-panel"), "{html}");
        assert!(html.contains("data-all-controls"), "{html}");
        for (channel, state) in [
            ("brightness", "read-default"),
            ("speed", "read-following"),
            ("hue", "engaged"),
        ] {
            let hook = format!(r#"data-panel-channel="{channel}" data-panel-state="{state}""#);
            assert!(html.contains(&hook), "{hook} missing from {html}");
        }
    }

    /// Five bars, 28 px each (`board_card.rs`'s grid).
    const BARS: u32 = 5;
    const BAR_PX: u32 = 28;

    /// A card-size fader's column class, as it renders.
    const COMPACT_FADER_MARK: &str = "tw:grid-cols-[fit-content(38%)_minmax(0,1fr)_auto]";

    fn panel(views: Vec<UiPanelControlView>, more: usize) -> UiBoardPanel {
        UiBoardPanel {
            target: None,
            controls: views
                .into_iter()
                .map(|view| UiCardControl {
                    scope: "/".to_string(),
                    view,
                })
                .collect(),
            more,
            auto_save: None,
            edit: None,
        }
    }

    fn render(panel: UiBoardPanel) -> String {
        render_with(panel, None)
    }

    fn render_with(panel: UiBoardPanel, all_controls: Option<&str>) -> String {
        crate::app::board_card::card_test_fixtures::render(
            Harness,
            HarnessProps {
                panel,
                all_controls: all_controls.map(str::to_string),
            },
        )
    }

    /// The panel on its own, as a card mounts it.
    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn Harness(panel: UiBoardPanel, all_controls: Option<String>) -> Element {
        rsx! {
            CompactPanel { panel, all_controls, on_action: |_| {} }
        }
    }

    /// The markup from the element wearing `class` to its end: the
    /// element's opening tag through the next sibling row (rows do not
    /// nest, so the slice holds exactly that row's content).
    fn section(html: &str, class: &str) -> String {
        let at = html
            .find(&format!(r#"class="{class}""#))
            .unwrap_or_else(|| panic!("no element with {class} in {html}"));
        let rest = &html[at..];
        let end = [MASTER_ROW_CLASS, CONTROLS_ROW_CLASS, ALL_CONTROLS_ROW_CLASS]
            .iter()
            .filter(|other| **other != class)
            .filter_map(|other| rest.find(&format!(r#"class="{other}""#)))
            .min()
            .unwrap_or(rest.len());
        rest[..end].to_string()
    }

    fn control(channel: &str, widget: UiPanelWidget) -> UiPanelControlView {
        UiPanelControlView::new(
            channel,
            UiPanelControl {
                emit: UiPanelEmit::Value,
                label: channel.to_string(),
                address: None,
                widget,
                value: UiSlotValue::f32(0.5),
                live_value: None,
                live_gradient: None,
                panel_target: None,
                unit: None,
                state: UiSlotFieldState::editable(),
                aspects: Vec::new(),
                wires: Vec::new(),
            },
        )
    }

    fn fader(channel: &str) -> UiPanelControlView {
        control(
            channel,
            UiPanelWidget::Fader {
                min: 0.0,
                max: 1.0,
                step: None,
            },
        )
    }

    fn knob(channel: &str) -> UiPanelControlView {
        control(
            channel,
            UiPanelWidget::Knob {
                min: 0.0,
                max: 1.0,
                step: None,
            },
        )
    }
}
