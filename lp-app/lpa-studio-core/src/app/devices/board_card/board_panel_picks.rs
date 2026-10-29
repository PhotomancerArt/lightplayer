//! What the connected card draws of the project's root panel (CD6, Q11):
//! all of it, in the panel's order, not a second panel model (the card
//! panel pass, 2026-10-19: the card grows once, on connect, to hold it).
//!
//! In order:
//!
//! 1. **The root's own controls, the master leading**: the master is the
//!    control on the [`MASTER_CHANNEL`] channel, whatever its widget — from
//!    the root's own controls first, else the first nested group (depth
//!    first) that has one — pulled to the front of the card.
//! 2. **Then each nested group's controls** (an effect's, the Pattern
//!    instrument's), depth first, each under its group's name.
//!
//! There is no cap and nothing is left over: no "more", no link to a page
//! holding the rest. The panel-wide reset and the auto-save switch are not
//! controls; the card's ⋯ menu holds them
//! ([`crate::UiDetailPanel::PanelSettings`]).

use crate::{UiPanelControlView, UiPanelGroup};

use super::ui_board_panel::{UiBoardPanel, UiBoardPanelSection, UiCardControl};

/// The channel of the card's master control, the one that leads.
pub const MASTER_CHANNEL: &str = "brightness";

/// The card's panel from `root` (the project's root panel), with the root
/// module's `auto_save` riding along unchanged. Its `edit` is the card
/// builder's to fill, and its `auto_save` the builder's to withhold below
/// the edit tier.
pub fn board_panel_picks(root: &UiPanelGroup, auto_save: Option<bool>) -> UiBoardPanel {
    let master = master_of(root);
    let is_master =
        |view: &UiPanelControlView| master.is_some_and(|(_, it)| std::ptr::eq(it, view));
    let mut lead: Vec<UiCardControl> = master
        .map(|(scope, view)| card_control(scope, view))
        .into_iter()
        .collect();
    lead.extend(
        root.controls
            .iter()
            .filter(|view| !is_master(view))
            .map(|view| card_control(&root.scope, view)),
    );
    let mut sections: Vec<UiBoardPanelSection> = Vec::new();
    if !lead.is_empty() {
        sections.push(UiBoardPanelSection {
            heading: None,
            controls: lead,
        });
    }
    for nested in &root.groups {
        group_sections(nested, &is_master, &mut sections);
    }
    UiBoardPanel {
        target: root.target,
        sections,
        auto_save,
        edit: None,
    }
}

/// A nested group's controls under its name, then its own groups', depth
/// first. A group left with nothing (its only control the master, now
/// leading) has no section.
fn group_sections(
    group: &UiPanelGroup,
    is_master: &impl Fn(&UiPanelControlView) -> bool,
    out: &mut Vec<UiBoardPanelSection>,
) {
    let controls: Vec<UiCardControl> = group
        .controls
        .iter()
        .filter(|view| !is_master(view))
        .map(|view| card_control(&group.scope, view))
        .collect();
    if !controls.is_empty() {
        out.push(UiBoardPanelSection {
            heading: Some(group.label.clone()),
            controls,
        });
    }
    for nested in &group.groups {
        group_sections(nested, is_master, out);
    }
}

/// The master and the scope it lives in: the first control on the
/// [`MASTER_CHANNEL`], the group's own controls before its nested groups',
/// depth first.
fn master_of(group: &UiPanelGroup) -> Option<(&str, &UiPanelControlView)> {
    group
        .controls
        .iter()
        .find(|view| view.channel == MASTER_CHANNEL)
        .map(|view| (group.scope.as_str(), view))
        .or_else(|| group.groups.iter().find_map(master_of))
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

    #[test]
    fn the_master_leads_from_the_root() {
        let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
            control("speed", knob()),
            control(MASTER_CHANNEL, knob()),
            control("sparkle", UiPanelWidget::Toggle),
        ]);
        let panel = board_panel_picks(&root, Some(true));
        assert_eq!(channels(&panel), [MASTER_CHANNEL, "speed", "sparkle"]);
        assert_eq!(scopes(&panel), ["/", "/", "/"]);
        assert_eq!(headings(&panel), [None]);
        assert_eq!(panel.auto_save, Some(true), "rides along");
        assert_eq!(panel.edit, None, "the builder's");
    }

    #[test]
    fn the_master_leads_from_a_nested_group_with_its_scope() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![control("speed", knob())])
            .with_groups(vec![
                UiPanelGroup::new("plasma", "/plasma-1")
                    .with_controls(vec![control("hue", knob())]),
                UiPanelGroup::new("fixture", "/fixture-1")
                    .with_controls(vec![control(MASTER_CHANNEL, knob())]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), [MASTER_CHANNEL, "speed", "hue"]);
        assert_eq!(scopes(&panel), ["/fixture-1", "/", "/plasma-1"]);
        assert_eq!(
            headings(&panel),
            [None, Some("plasma")],
            "the fixture's group held only the master, now leading"
        );
    }

    /// The master is the brightness channel, whatever its widget: a knob on
    /// a panel since the card panel pass, a fader on an older one.
    #[test]
    fn a_brightness_knob_or_fader_is_the_master() {
        for widget in [knob(), fader()] {
            let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
                control("level", fader()),
                control(MASTER_CHANNEL, widget.clone()),
            ]);
            let panel = board_panel_picks(&root, None);
            assert_eq!(channels(&panel), [MASTER_CHANNEL, "level"], "{widget:?}");
        }
    }

    #[test]
    fn with_no_master_the_panel_keeps_its_order() {
        let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
            control("level", fader()),
            control("speed", knob()),
            control("sparkle", UiPanelWidget::Toggle),
        ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), ["level", "speed", "sparkle"]);
    }

    /// No cap and nothing left over: every control on a big panel, the
    /// root's first, then each group's under its name, depth first.
    #[test]
    fn every_control_in_panel_order_under_its_groups_heading() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![
                control(MASTER_CHANNEL, knob()),
                control("a", knob()),
                control("palette", UiPanelWidget::PaletteSwatch),
            ])
            .with_groups(vec![
                UiPanelGroup::new("plasma", "/plasma-1")
                    .with_controls(vec![
                        control("c", UiPanelWidget::Toggle),
                        control("d", knob()),
                    ])
                    .with_groups(vec![
                        UiPanelGroup::new("noise", "/plasma-1/noise")
                            .with_controls(vec![control("e", fader())]),
                    ]),
                UiPanelGroup::new("Pattern", "/playlist").with_controls(vec![control(
                    "pattern",
                    UiPanelWidget::PatternPicker { picker: picker() },
                )]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(
            channels(&panel),
            [MASTER_CHANNEL, "a", "palette", "c", "d", "e", "pattern"]
        );
        assert_eq!(
            headings(&panel),
            [None, Some("plasma"), Some("noise"), Some("Pattern")]
        );
    }

    /// The clock's Speed knob is a control like any other.
    #[test]
    fn the_clocks_speed_knob_is_on_the_card() {
        let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
            control("clock.rate", UiPanelWidget::Speed),
            control(MASTER_CHANNEL, knob()),
        ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), [MASTER_CHANNEL, "clock.rate"]);
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

    fn headings(panel: &UiBoardPanel) -> Vec<Option<&str>> {
        panel
            .sections
            .iter()
            .map(|section| section.heading.as_deref())
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
            entry_target: None,
        }
    }
}
