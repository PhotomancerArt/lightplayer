//! [`CompactPanel`]: the board's whole panel at card size — what a
//! connected board card draws in its eight rows' place (the board card ADR,
//! §4 and its amendments; `docs/ux/anatomy/board-card.md`, connected; the
//! panel layout study, `docs/ux/studies/panel-layout/`).
//!
//! Core laid out what it shows ([`UiBoardPanel`]: every control, in
//! sections, each in shape-then-bus order — the root's own, then each
//! nested group's under its name and kind); this piece decides only how it
//! looks, by each control's [`UiPanelShape`], never by its widget:
//!
//! 1. **the band**: the root's own controls on a tint, with no word;
//! 2. **a group**: a nested group's section under its own line and a
//!    heading of its node's kind icon and name;
//! 3. **the controls**: each strip ([`PanelStrip`], a fader or a palette) a
//!    row of its own, its label and value above a track the card's width;
//!    the block (the Pattern instrument) a row of its own; the cells
//!    ([`PanelCell`], a knob, the Speed knob, a toggle) as many to a row as
//!    the card's width holds — three on a 300 px card;
//! 4. **light lines** between every two controls, edge to edge, a step
//!    lighter than a group's own line, none on the card's outer edge;
//! 5. the foot: a bar with the panel's action — Edit, or its lock on a
//!    play-only board — flush at its end (CD15).
//!
//! The widgets are the panel's own: the latch, the gold engaged state, the
//! let-go, the detail popover and the writes are the same as everywhere
//! else in Studio. The panel reset and the auto-save switch are not here:
//! the card's ⋯ holds them.
//!
//! **The card grows once, to fit.** The block takes the height its controls
//! need, so the card changes height on connect, and back on Done; nothing
//! a board does while connected moves it (no control comes or goes but by
//! an edit to the project).
//!
//! Walk hooks: `data-board-panel` on the block, `data-board-panel-heading`
//! on a group's heading, `data-board-panel-foot` on the foot; each control
//! carries the panel's own (`data-panel-scope`, `data-panel-channel`,
//! `data-panel-state`).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBoardPanel, UiBoardPanelSection, UiCardControl, UiPanelShape};

use super::{ModulePanelControl, PanelCell, PanelGesture, PanelStrip, panel_gesture_actions};
use crate::app::board_card::{CardAction, CardActionLook};
use crate::base::{StudioIcon, node_kind_icon};

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

/// One section: the band (the root's, no heading) or a group (its heading),
/// then its controls with the light lines between them.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelSection(
    section: UiBoardPanelSection,
    on_panel: EventHandler<PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let class = match section.heading {
        None => BAND_CLASS,
        Some(_) => GROUP_CLASS,
    };
    rsx! {
        div { class,
            if let Some(heading) = section.heading {
                div { class: HEADING_CLASS, "data-board-panel-heading": "",
                    span { class: HEADING_ICON_CLASS, aria_hidden: "true",
                        StudioIcon { name: node_kind_icon(&heading.kind), size: HEADING_ICON_SIZE }
                    }
                    span { class: HEADING_WORDS_CLASS, title: "{heading.name}", "{heading.name}" }
                }
            }
            div { class: "ux-panel-lines",
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
}

/// One control, drawn by its shape: a strip, the block, or a cell.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CardControl(
    control: UiCardControl,
    on_panel: EventHandler<PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    match control.view.control.widget.shape() {
        UiPanelShape::Strip => rsx! {
            PanelStrip { control, on_panel, on_action }
        },
        // The block keeps the panel's own drawing, across the row: the
        // Pattern instrument carries its own controls and resets.
        UiPanelShape::Block => rsx! {
            div { class: BLOCK_CLASS,
                ModulePanelControl {
                    view: control.view,
                    scope: control.scope,
                    on_panel: Some(on_panel),
                    on_action: Some(on_action),
                }
            }
        },
        UiPanelShape::Cell => rsx! {
            PanelCell { control, on_panel, on_action }
        },
    }
}

/// The block: the card's last grid row, as tall as its controls. Its top
/// hairline is the first section's.
const PANEL_CLASS: &str = "ux-board-panel tw:flex tw:min-w-0 tw:flex-col";

/// The root's section: the band, under the card's hairline. No heading.
const BAND_CLASS: &str = "ux-panel-band tw:min-w-0 tw:border-0 tw:border-t tw:border-solid tw:border-border";

/// A nested group's section: its own line, a step stronger than the light
/// lines between controls, then its heading.
const GROUP_CLASS: &str =
    "tw:min-w-0 tw:border-0 tw:border-t tw:border-solid tw:border-border";

/// A nested group's heading: its node's kind icon and its name, small and
/// quiet; no box, no hairline after the name.
const HEADING_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:px-2.5 tw:pt-2 tw:pb-0.5 tw:text-dim-foreground";
const HEADING_ICON_CLASS: &str = "tw:inline-flex tw:flex-none";
const HEADING_ICON_SIZE: u32 = 11;
const HEADING_WORDS_CLASS: &str = "tw:min-w-0 tw:truncate tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.12em]";

/// The controls' grid: cells as many to a row as the card holds — at least
/// 80 px, room for a label like BRIGHTNESS (three to a row on a 300 px card,
/// four on a wide one), and 50 px on a phone's narrow card (`@container
/// board-card`, three to a row, a long label cut); a strip or the block
/// spans the row. No gaps: the light lines are the gaps (`ux-panel-grid`).
const GRID_CLASS: &str = "ux-panel-grid tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(80px,1fr))] tw:@max-[239px]:grid-cols-[repeat(auto-fill,minmax(50px,1fr))]";

/// The block's row: the whole width, as tall as the instrument.
const BLOCK_CLASS: &str = "tw:col-span-full tw:min-w-0 tw:px-2 tw:py-2";

/// The foot: a bar — 28 px, its hairline, its action flush at its end,
/// rounded with the card at its foot (`ux-board-bar`'s last-child rule
/// rounds the action's corner too).
const FOOT_CLASS: &str = "ux-board-bar tw:relative tw:flex tw:h-7 tw:flex-none tw:min-w-0 tw:items-stretch tw:rounded-b-[7px] tw:border-0 tw:border-t tw:border-solid tw:border-border tw:text-[11.5px] tw:text-muted-foreground";

/// The sentence for a project with no controls.
const EMPTY_CLASS: &str = "tw:m-0 tw:border-0 tw:border-t tw:border-solid tw:border-border tw:px-2.5 tw:py-3 tw:text-[11.5px] tw:text-subtle-foreground";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiBoardPanelHeading, UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit,
        UiPanelWidget, UiSlotFieldState, UiSlotValue,
    };

    use crate::base::StudioIconName;

    use super::*;

    /// Every control in every section is drawn, in order; a nested group's
    /// section wears its heading, the root's does not.
    #[test]
    fn every_section_and_control_is_drawn_under_its_heading() {
        let html = render(panel(vec![
            section(None, vec![knob("brightness"), knob("speed")]),
            section(Some(("Plasma 1", "Module")), vec![knob("hue")]),
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

    /// A strip (a fader, a palette) takes the whole row; a cell (a knob, the
    /// Speed knob, a toggle) takes one unit of it. The shape decides, never
    /// the widget.
    #[test]
    fn strips_span_the_row_and_cells_do_not() {
        let html = render(panel(vec![section(
            None,
            vec![
                control("speed", UiPanelWidget::Speed),
                control("sparkle", UiPanelWidget::Toggle),
                fader("level"),
                control("palette", UiPanelWidget::PaletteSwatch),
            ],
        )]));
        let spans_the_row = |channel: &str| opening_tag(&html, channel).contains("tw:col-span-full");
        assert!(!spans_the_row("speed"), "the Speed knob is a cell");
        assert!(!spans_the_row("sparkle"), "a toggle is a cell");
        assert!(spans_the_row("level"), "a fader is a strip");
        assert!(spans_the_row("palette"), "a palette is a strip");
    }

    /// The root's section is the band: a tint, and no heading, no word.
    #[test]
    fn the_roots_section_is_the_band_with_no_heading() {
        let html = render(panel(vec![section(None, vec![knob("speed")])]));
        assert!(html.contains("ux-panel-band"), "{html}");
        assert!(!html.contains("data-board-panel-heading"), "{html}");
        assert!(!html.to_lowercase().contains("global"), "{html}");
    }

    /// A nested group's heading is its kind's icon and its name, and its
    /// section is no band.
    #[test]
    fn a_group_heading_draws_its_kinds_icon_and_its_name() {
        let html = render(panel(vec![section(
            Some(("Dome", "Module")),
            vec![knob("bands")],
        )]));
        let heading_at = html.find("data-board-panel-heading").expect("a heading");
        let heading = &html[heading_at..html.find(r#"data-panel-channel="bands""#).expect("bands")];
        assert!(heading.contains("Dome"), "{heading}");
        let module_icon = icon(node_kind_icon("Module"));
        let playlist_icon = icon(node_kind_icon("Playlist"));
        assert_ne!(module_icon, playlist_icon);
        assert!(heading.contains(&module_icon), "the Module icon in {heading}");
        assert!(!heading.contains(&playlist_icon), "{heading}");
        assert!(!html.contains("ux-panel-band"), "{html}");
    }

    /// The light lines are the grid's: no gaps between controls, every
    /// section's controls inside the lines' clip.
    #[test]
    fn controls_sit_in_the_lined_grid_with_no_gaps() {
        assert!(GRID_CLASS.contains("ux-panel-grid"), "{GRID_CLASS}");
        assert!(!GRID_CLASS.contains(":gap-"), "the lines are the gaps: {GRID_CLASS}");
        let html = render(panel(vec![section(None, vec![knob("speed")])]));
        let lines = html.find("ux-panel-lines").expect("the lines' clip");
        let speed = html.find(r#"data-panel-channel="speed""#).expect("speed");
        assert!(lines < speed, "{html}");
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

    fn section(
        heading: Option<(&str, &str)>,
        views: Vec<UiPanelControlView>,
    ) -> UiBoardPanelSection {
        UiBoardPanelSection {
            heading: heading.map(|(name, kind)| UiBoardPanelHeading {
                name: name.to_string(),
                kind: kind.to_string(),
            }),
            controls: views
                .into_iter()
                .map(|view| UiCardControl {
                    scope: "/".to_string(),
                    view,
                })
                .collect(),
        }
    }

    /// The opening tag of the element that carries `channel`'s walk hook.
    fn opening_tag<'a>(html: &'a str, channel: &str) -> &'a str {
        let at = html
            .find(&format!(r#"data-panel-channel="{channel}""#))
            .unwrap_or_else(|| panic!("{channel} missing from {html}"));
        let start = html[..at].rfind('<').expect("the tag's start");
        let end = at + html[at..].find('>').expect("the tag's end");
        &html[start..end]
    }

    /// An icon on its own, as the heading draws it.
    fn icon(name: StudioIconName) -> String {
        crate::app::board_card::card_test_fixtures::render(IconAlone, IconAloneProps { name })
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn IconAlone(name: StudioIconName) -> Element {
        rsx! {
            StudioIcon { name, size: HEADING_ICON_SIZE }
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
