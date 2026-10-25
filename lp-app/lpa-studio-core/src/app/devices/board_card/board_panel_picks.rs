//! What the connected card draws of the project's root panel (CD6, Q11):
//! all of it, in the panel's order, not a second panel model (the card
//! panel pass, 2026-10-19: the card grows once, on connect, to hold it).
//!
//! In order:
//!
//! 1. **The root's own controls**, one section with no heading: the web
//!    draws it as the band.
//! 2. **Then each nested group's controls** (a child module's, the Pattern
//!    instrument's), depth first, each section under its group's name and
//!    kind.
//!
//! Inside every section the controls are in the shape's order, then the
//! bus's, then the walk's ([`crate::sort_panel_controls`]): strips, blocks,
//! then cells, each run by `WELL_KNOWN_CHANNELS`' order, and the panel's
//! own order breaks every tie. Nothing is pulled forward by name, and a
//! group's brightness stays in its group (a dome's is the dome's, not the
//! project's).
//!
//! There is no cap and nothing is left over: no "more", no link to a page
//! holding the rest. The panel-wide reset and the auto-save switch are not
//! controls; the card's ⋯ menu holds them
//! ([`crate::UiDetailPanel::PanelSettings`]).

use crate::{UiPanelControlView, UiPanelGroup, sort_panel_controls};

use super::ui_board_panel::{UiBoardPanel, UiBoardPanelSection, UiCardControl};
use super::ui_board_panel_heading::UiBoardPanelHeading;

/// The card's panel from `root` (the project's root panel), with the root
/// module's `auto_save` riding along unchanged. Its `edit` is the card
/// builder's to fill, and its `auto_save` the builder's to withhold below
/// the edit tier.
pub fn board_panel_picks(root: &UiPanelGroup, auto_save: Option<bool>) -> UiBoardPanel {
    let mut sections: Vec<UiBoardPanelSection> = Vec::new();
    sections.extend(section_of(root, None));
    for nested in &root.groups {
        group_sections(nested, &mut sections);
    }
    UiBoardPanel {
        target: root.target,
        sections,
        auto_save,
        edit: None,
    }
}

/// A nested group's controls under its name and kind, then its own groups',
/// depth first. A group with no controls of its own has no section.
fn group_sections(group: &UiPanelGroup, out: &mut Vec<UiBoardPanelSection>) {
    let heading = UiBoardPanelHeading {
        name: group.label.clone(),
        kind: group.kind.clone(),
    };
    out.extend(section_of(group, Some(heading)));
    for nested in &group.groups {
        group_sections(nested, out);
    }
}

/// One group's own controls as a section, in panel order; `None` for a
/// group that holds none.
fn section_of(
    group: &UiPanelGroup,
    heading: Option<UiBoardPanelHeading>,
) -> Option<UiBoardPanelSection> {
    let mut views: Vec<&UiPanelControlView> = group.controls.iter().collect();
    sort_panel_controls(&mut views);
    let controls: Vec<UiCardControl> = views
        .into_iter()
        .map(|view| card_control(&group.scope, view))
        .collect();
    (!controls.is_empty()).then_some(UiBoardPanelSection { heading, controls })
}

fn card_control(scope: &str, view: &UiPanelControlView) -> UiCardControl {
    UiCardControl {
        scope: scope.to_string(),
        view: view.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        OfferPath, UiPanelControl, UiPanelControlState, UiPanelEmit, UiPanelWidget,
        UiPatternPicker, UiSlotFieldState, UiSlotValue,
    };

    /// Nothing is hoisted: a nested group's brightness stays in its group,
    /// first there. The root's band keeps what the root holds.
    #[test]
    fn nothing_is_hoisted_a_groups_brightness_stays_in_its_group() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![control("clock.rate", UiPanelWidget::Speed)])
            .with_groups(vec![
                UiPanelGroup::new("Dome", "/dome")
                    .with_kind("Module")
                    .with_controls(vec![
                        control("bands", knob()),
                        control("brightness", fader()),
                    ]),
            ]);
        let panel = board_panel_picks(&root, Some(true));
        assert_eq!(channels(&panel), ["clock.rate", "brightness", "bands"]);
        assert_eq!(scopes(&panel), ["/", "/dome", "/dome"]);
        assert_eq!(
            sections(&panel),
            [vec!["clock.rate"], vec!["brightness", "bands"]]
        );
        assert_eq!(panel.auto_save, Some(true), "rides along");
        assert_eq!(panel.edit, None, "the builder's");
    }

    /// The root's section is the band, with no heading; each nested one is
    /// its group's name and kind.
    #[test]
    fn the_root_has_no_heading_and_each_nested_group_its_name_and_kind() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_kind("Module")
            .with_controls(vec![control("a", knob())])
            .with_groups(vec![
                UiPanelGroup::new("plasma", "/plasma-1")
                    .with_kind("Module")
                    .with_controls(vec![control("b", knob())])
                    .with_groups(vec![
                        UiPanelGroup::new("noise", "/plasma-1/noise")
                            .with_kind("Shader")
                            .with_controls(vec![control("c", knob())]),
                    ]),
                UiPanelGroup::new("Pattern", "/playlist")
                    .with_kind("Playlist")
                    .with_controls(vec![control("d", knob())]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(
            headings(&panel),
            [
                None,
                Some(("plasma", "Module")),
                Some(("noise", "Shader")),
                Some(("Pattern", "Playlist"))
            ]
        );
    }

    /// No cap and nothing left over: every control on a big panel, the
    /// root's first, then each group's under its heading, depth first, and
    /// every section in shape-then-bus order.
    #[test]
    fn every_section_is_in_shape_then_bus_order() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![
                control("clock.rate", UiPanelWidget::Speed),
                control("a", knob()),
                control("palette", UiPanelWidget::PaletteSwatch),
                control("brightness", fader()),
            ])
            .with_groups(vec![
                UiPanelGroup::new("plasma", "/plasma-1")
                    .with_controls(vec![
                        control("c", UiPanelWidget::Toggle),
                        control("d", knob()),
                        control("e", fader()),
                    ])
                    .with_groups(vec![
                        UiPanelGroup::new("noise", "/plasma-1/noise")
                            .with_controls(vec![control("f", knob()), control("e", fader())]),
                    ]),
                UiPanelGroup::new("Pattern", "/playlist").with_controls(vec![
                    control("pattern", UiPanelWidget::PatternPicker { picker: picker() }),
                    control("level", knob()),
                    control("palette", UiPanelWidget::PaletteSwatch),
                ]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(
            sections(&panel),
            [
                vec!["brightness", "palette", "clock.rate", "a"],
                vec!["e", "c", "d"],
                vec!["e", "f"],
                vec!["palette", "pattern", "level"],
            ]
        );
    }

    /// The order that left Scale alone under the palette: the real
    /// PLAYFUL choker's walk order, laid out as the band.
    #[test]
    fn playfuls_band_is_brightness_palette_speed_scale() {
        let root = UiPanelGroup::new("PLAYFUL", "/").with_controls(vec![
            control("clock.rate", UiPanelWidget::Speed),
            control("brightness", fader()),
            control("palette", UiPanelWidget::PaletteSwatch),
            control("scale", knob()),
        ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(
            channels(&panel),
            ["brightness", "palette", "clock.rate", "scale"]
        );
        assert_eq!(headings(&panel), [None]);
    }

    /// What the ⋯ menu's reset lets go of: every held control.
    #[test]
    fn held_counts_the_engaged_controls() {
        let held = |channel: &str| {
            control(channel, knob()).with_state(UiPanelControlState::Engaged, None::<&str>)
        };
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![held("a"), control("b", knob())])
            .with_groups(vec![
                UiPanelGroup::new("plasma", "/plasma-1").with_controls(vec![held("c")]),
            ]);
        assert_eq!(board_panel_picks(&root, None).held(), 2);
    }

    #[test]
    fn an_empty_panel_picks_nothing() {
        let target = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        let root = UiPanelGroup::new("Porch", "/")
            .with_target(target)
            .with_groups(vec![UiPanelGroup::new("plasma", "/plasma-1")]);
        let panel = board_panel_picks(&root, Some(false));
        assert!(panel.sections.is_empty(), "{:?}", panel.sections);
        assert_eq!(panel.target, Some(target), "the root's scope target");
    }

    fn channels(panel: &UiBoardPanel) -> Vec<&str> {
        panel
            .controls()
            .map(|control| control.view.channel.as_str())
            .collect()
    }

    fn scopes(panel: &UiBoardPanel) -> Vec<&str> {
        panel
            .controls()
            .map(|control| control.scope.as_str())
            .collect()
    }

    fn headings(panel: &UiBoardPanel) -> Vec<Option<(&str, &str)>> {
        panel
            .sections
            .iter()
            .map(|section| {
                section
                    .heading
                    .as_ref()
                    .map(|heading| (heading.name.as_str(), heading.kind.as_str()))
            })
            .collect()
    }

    fn sections(panel: &UiBoardPanel) -> Vec<Vec<&str>> {
        panel
            .sections
            .iter()
            .map(|section| {
                section
                    .controls
                    .iter()
                    .map(|control| control.view.channel.as_str())
                    .collect()
            })
            .collect()
    }

    fn knob() -> UiPanelWidget {
        UiPanelWidget::Knob {
            min: 0.0,
            max: 1.0,
            step: None,
        }
    }

    fn fader() -> UiPanelWidget {
        UiPanelWidget::Fader {
            min: 0.0,
            max: 1.0,
            step: None,
        }
    }

    fn control(channel: &str, widget: UiPanelWidget) -> UiPanelControlView {
        UiPanelControlView::new(
            channel,
            UiPanelControl {
                label: channel.to_string(),
                address: None,
                widget,
                value: UiSlotValue::f32(0.5),
                emit: UiPanelEmit::Value,
                live_value: None,
                live_gradient: None,
                panel_target: None,
                unit: None,
                state: UiSlotFieldState::editable(),
                aspects: Vec::new(),
            },
        )
    }

    fn picker() -> UiPatternPicker {
        UiPatternPicker {
            verbs: OfferPath::root("project"),
            entries: Vec::new(),
            active: None,
            cycle: lpc_model::PlaylistCycle::Hold,
            cycle_target: None,
            skip_target: None,
        }
    }
}
