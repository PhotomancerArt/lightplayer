//! [`CompactPanel`]: the board's whole panel at card size — what a
//! connected board card draws in its five bars' place (the board card ADR,
//! §4 and its 2026-10-19 amendment; `docs/ux/anatomy/board-card.md`,
//! connected).
//!
//! Core laid out what it shows ([`UiBoardPanel`]: every control, in
//! sections — the root's own with the master leading, then each nested
//! group's under its name); this piece decides only how it looks:
//!
//! 1. each section's controls in a grid, in panel order: knobs and toggles
//!    as many to a row as the card's width holds, a palette, a fader or the
//!    Pattern instrument across the whole row;
//! 2. a nested group's section under a quiet heading, its name on a
//!    hairline;
//! 3. the foot: a bar with the panel's action — Edit, or its lock on a
//!    play-only board — flush at its end (CD15).
//!
//! Every control is the panel's own [`ModulePanelControl`] at card size:
//! the latch, the gold engaged state, the let-go glyph, the detail popover
//! and the writes are the same as everywhere else in Studio. The panel
//! reset and the auto-save switch are not here: the card's ⋯ holds them.
//!
//! **The card grows once, to fit.** The block takes the height its controls
//! need, so the card changes height on connect, and back on Done; nothing
//! a board does while connected moves it (no control comes or goes but by
//! an edit to the project).
//!
//! Walk hooks: `data-board-panel` on the block, `data-board-panel-heading`
//! on a group's heading, `data-board-panel-foot` on the foot; each control
//! carries the panel's own (`data-panel-scope`, `data-panel-channel`,
//! `data-panel-state`: [`ModulePanelControl`]).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBoardPanel, UiBoardPanelSection, UiCardControl, UiPanelWidget};

use super::{ModulePanelControl, panel_gesture_actions};
use crate::app::board_card::{CardAction, CardActionLook};

/// The connected card's panel. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CompactPanel(
    /// Core's whole panel for the board.
    panel: UiBoardPanel,
    on_action: EventHandler<UiAction>,
) -> Element {
    let on_panel = panel_gesture_actions(on_action);
    let empty = panel.sections.is_empty();
    rsx! {
        div { class: PANEL_CLASS, "data-board-panel": "",
            if empty {
                p { class: EMPTY_CLASS, "This project has no controls." }
            }
            for (index, section) in panel.sections.into_iter().enumerate() {
                PanelSection {
                    key: "{index}",
                    section,
                    on_panel,
                    on_action,
                }
            }
            if let Some(edit) = panel.edit {
                div { class: FOOT_CLASS, "data-board-panel-foot": "",
                    span { class: "tw:min-w-0 tw:flex-1" }
                    CardAction { action: edit, look: CardActionLook::BarEnd, on_action }
                }
            }
        }
    }
}

/// One section: a nested group's quiet heading, then its controls' grid.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelSection(
    section: UiBoardPanelSection,
    on_panel: EventHandler<super::PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: SECTION_CLASS,
            if let Some(heading) = section.heading {
                div { class: HEADING_CLASS, "data-board-panel-heading": "",
                    span { class: HEADING_WORDS_CLASS, title: "{heading}", "{heading}" }
                    span { class: HEADING_RULE_CLASS, aria_hidden: "true" }
                }
            }
            div { class: GRID_CLASS,
                for control in section.controls {
                    CardControl {
                        key: "{control.scope}/{control.view.channel}",
                        control,
                        on_panel,
                        on_action,
                    }
                }
            }
        }
    }
}

/// One control: the panel's own control at card size, in a grid cell, or
/// across the grid's row when it is a strip (its walk hooks are its own).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CardControl(
    control: UiCardControl,
    on_panel: EventHandler<super::PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let cell = match is_strip(&control.view.control.widget) {
        true => STRIP_CELL_CLASS,
        false => CELL_CLASS,
    };
    rsx! {
        div { class: cell,
            ModulePanelControl {
                view: control.view,
                scope: control.scope,
                compact: true,
                on_panel: Some(on_panel),
                on_action: Some(on_action),
            }
        }
    }
}

/// A widget that takes the whole row: a palette, a fader, the Pattern
/// instrument. A knob (the Speed knob among them) or a toggle is a cell.
fn is_strip(widget: &UiPanelWidget) -> bool {
    match widget {
        UiPanelWidget::Fader { .. }
        | UiPanelWidget::PaletteSwatch
        | UiPanelWidget::PatternPicker { .. } => true,
        UiPanelWidget::Knob { .. } | UiPanelWidget::Speed | UiPanelWidget::Toggle => false,
    }
}

/// The block: the card's last grid row, as tall as its controls. Its top
/// hairline is the first section's.
const PANEL_CLASS: &str = "ux-board-panel tw:flex tw:min-w-0 tw:flex-col";

/// One section: its top hairline, its heading (when it has one) and its
/// grid.
const SECTION_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-1.5 tw:border-0 tw:border-t tw:border-solid tw:border-border tw:px-2 tw:pt-2 tw:pb-2.5";

/// A nested group's heading: its name, small and quiet, on a hairline that
/// runs to the card's edge.
const HEADING_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:px-0.5";
const HEADING_WORDS_CLASS: &str = "tw:min-w-0 tw:truncate tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.12em] tw:text-dim-foreground";
const HEADING_RULE_CLASS: &str = "tw:h-px tw:min-w-4 tw:flex-1 tw:bg-border-muted";

/// The controls' grid: knobs and toggles at least 50 px a cell, as many to a
/// row as the card holds (three on a phone's narrow card, five on a wide
/// one); a strip spans the row.
const GRID_CLASS: &str =
    "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(50px,1fr))] tw:gap-x-1 tw:gap-y-2";

/// A knob's or a toggle's cell.
const CELL_CLASS: &str = "tw:flex tw:min-w-0 tw:justify-center";

/// A strip's cell: the whole row.
const STRIP_CELL_CLASS: &str = "tw:col-span-full tw:min-w-0 tw:px-0.5";

/// The foot: a bar — 28 px, its hairline, its action flush at its end,
/// rounded with the card at its foot (`ux-board-bar`'s last-child rule
/// rounds the action's corner too).
const FOOT_CLASS: &str = "ux-board-bar tw:relative tw:flex tw:h-7 tw:flex-none tw:min-w-0 tw:items-stretch tw:rounded-b-[7px] tw:border-0 tw:border-t tw:border-solid tw:border-border tw:text-[11.5px] tw:text-muted-foreground";

/// The sentence for a project with no controls.
const EMPTY_CLASS: &str = "tw:m-0 tw:border-0 tw:border-t tw:border-solid tw:border-border tw:px-2.5 tw:py-3 tw:text-[11.5px] tw:text-subtle-foreground";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit, UiSlotFieldState,
        UiSlotValue,
    };

    use super::*;

    /// Every control in every section is drawn, in order; a nested group's
    /// section wears its heading, the root's does not.
    #[test]
    fn every_section_and_control_is_drawn_under_its_heading() {
        let html = render(panel(vec![
            section(None, vec![knob("brightness"), knob("speed")]),
            section(Some("Plasma 1"), vec![knob("hue")]),
        ]));
        let channels = attribute_values(&html, "data-panel-channel");
        assert_eq!(channels, ["brightness", "speed", "hue"]);
        assert_eq!(
            html.matches("data-board-panel-heading").count(),
            1,
            "{html}"
        );
        let heading_at = html.find("Plasma 1").expect("the heading");
        let hue_at = html.find(r#"data-panel-channel="hue""#).expect("hue");
        let speed_at = html.find(r#"data-panel-channel="speed""#).expect("speed");
        assert!(speed_at < heading_at && heading_at < hue_at, "{html}");
    }

    /// A palette, a fader or the Pattern instrument takes the whole row; a
    /// knob, the Speed knob and a toggle are cells.
    #[test]
    fn strips_span_the_row_and_knobs_are_cells() {
        assert!(is_strip(&UiPanelWidget::PaletteSwatch));
        assert!(is_strip(&fader("level").control.widget));
        assert!(!is_strip(&UiPanelWidget::Speed));
        assert!(!is_strip(&UiPanelWidget::Toggle));
        assert!(!is_strip(&knob("hue").control.widget));
        let html = render(panel(vec![section(
            None,
            vec![knob("speed"), fader("level")],
        )]));
        let level = html
            .find(r#"data-panel-channel="level""#)
            .expect("the fader");
        let cell = &html[html[..level].rfind(STRIP_CELL_CLASS).expect("its cell")..level];
        assert!(!cell.contains(CELL_CLASS), "the fader is in a strip cell");
    }

    /// The card grows to fit: no fixed height, nothing clipped.
    #[test]
    fn the_block_takes_the_height_it_needs() {
        assert!(!PANEL_CLASS.contains("tw:h-["), "{PANEL_CLASS}");
        assert!(!PANEL_CLASS.contains("overflow"), "{PANEL_CLASS}");
        assert!(FOOT_CLASS.contains("tw:h-7"), "the foot is a 28 px bar");
    }

    /// No Edit: no foot. No controls: one sentence, and nothing to press.
    #[test]
    fn edit_none_draws_no_foot() {
        let html = render(panel(vec![section(None, vec![knob("speed")])]));
        assert!(!html.contains("data-board-panel-foot"), "{html}");
        let html = render(panel(Vec::new()));
        assert!(html.contains("This project has no controls."), "{html}");
        assert!(!html.contains("<button"), "{html}");
    }

    /// Nothing on the card leads elsewhere: no link, no "more".
    #[test]
    fn the_panel_has_no_link_and_no_more() {
        let html = render(panel(vec![section(None, vec![knob("speed")])]));
        assert!(!html.contains("href="), "{html}");
        assert!(!html.contains("more"), "{html}");
        assert!(!html.contains("All controls"), "{html}");
    }

    /// The walk hooks: the block, each control's channel and state.
    #[test]
    fn the_walk_hooks_are_present() {
        let held = knob("hue").with_state(UiPanelControlState::Engaged, Some("authored 0.4"));
        let following =
            knob("speed").with_state(UiPanelControlState::ReadFollowing, Some("lfo · speed"));
        let html = render(panel(vec![section(
            None,
            vec![knob("brightness"), following, held],
        )]));
        assert!(html.contains("data-board-panel"), "{html}");
        for (channel, state) in [
            ("brightness", "read-default"),
            ("speed", "read-following"),
            ("hue", "engaged"),
        ] {
            let hook = format!(r#"data-panel-channel="{channel}" data-panel-state="{state}""#);
            assert!(html.contains(&hook), "{hook} missing from {html}");
        }
    }

    fn panel(sections: Vec<UiBoardPanelSection>) -> UiBoardPanel {
        UiBoardPanel {
            target: None,
            sections,
            auto_save: None,
            edit: None,
        }
    }

    fn section(heading: Option<&str>, views: Vec<UiPanelControlView>) -> UiBoardPanelSection {
        UiBoardPanelSection {
            heading: heading.map(str::to_string),
            controls: views
                .into_iter()
                .map(|view| UiCardControl {
                    scope: "/".to_string(),
                    view,
                })
                .collect(),
        }
    }

    fn render(panel: UiBoardPanel) -> String {
        crate::app::board_card::card_test_fixtures::render(Harness, HarnessProps { panel })
    }

    /// The values of every `attribute="…"` in `html`, in order.
    fn attribute_values(html: &str, attribute: &str) -> Vec<String> {
        crate::app::board_card::card_test_fixtures::attribute_values(html, attribute)
    }

    /// The panel on its own, as a card mounts it.
    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn Harness(panel: UiBoardPanel) -> Element {
        rsx! {
            CompactPanel { panel, on_action: |_| {} }
        }
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
