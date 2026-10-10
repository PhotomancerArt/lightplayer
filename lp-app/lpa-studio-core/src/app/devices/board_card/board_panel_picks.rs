//! What the connected card draws of the project's root panel (CD6, Q11):
//! a pick, not a second panel model.
//!
//! In order:
//!
//! 1. **The master**: a control on the [`MASTER_CHANNEL`] channel drawn as a
//!    fader ([`UiPanelWidget::Fader`]) — from the root's own controls first,
//!    else the first nested group (depth first) that has one.
//! 2. **Then the controls with a card size**, knobs and toggles, in the
//!    panel's order: the root's own controls, then each group's, depth
//!    first.
//!
//! [`BOARD_PANEL_CONTROLS`] at most. Everything else — faders other than the
//! master, palettes, the clock's transport, the pattern picker, and what did
//! not fit — is the play page's (All controls), and `more` counts it. The
//! card never draws the panel-wide reset or the auto-save switch (Q12).

use crate::{UiPanelControlView, UiPanelGroup, UiPanelWidget};

use super::ui_board_panel::{UiBoardPanel, UiCardControl};

/// The channel the card's master fader drives.
pub const MASTER_CHANNEL: &str = "brightness";

/// How many controls the card draws at most, the master included.
pub const BOARD_PANEL_CONTROLS: usize = 4;

/// The card's panel picked from `root` (the project's root panel), with
/// the root module's `auto_save` riding along unchanged. Its `edit` is the
/// card builder's to fill, and its `auto_save` the builder's to withhold
/// below the edit tier.
pub fn board_panel_picks(root: &UiPanelGroup, auto_save: Option<bool>) -> UiBoardPanel {
    let all = panel_order(root);
    let master = all.iter().position(|(_, view)| is_master(view));
    let mut picked: Vec<usize> = master.into_iter().collect();
    for (at, (_, view)) in all.iter().enumerate() {
        if picked.len() >= BOARD_PANEL_CONTROLS {
            break;
        }
        if Some(at) != master && has_card_size(view) {
            picked.push(at);
        }
    }
    let controls: Vec<UiCardControl> = picked
        .iter()
        .map(|at| {
            let (scope, view) = all[*at];
            UiCardControl {
                scope: scope.to_string(),
                view: view.clone(),
            }
        })
        .collect();
    UiBoardPanel {
        target: root.target,
        more: all.len() - controls.len(),
        controls,
        auto_save,
        edit: None,
    }
}

/// Every control in the panel with the scope it lives in, in the panel's
/// order: the group's own controls, then each nested group's, depth first.
fn panel_order(group: &UiPanelGroup) -> Vec<(&str, &UiPanelControlView)> {
    let mut all: Vec<(&str, &UiPanelControlView)> = group
        .controls
        .iter()
        .map(|view| (group.scope.as_str(), view))
        .collect();
    for nested in &group.groups {
        all.extend(panel_order(nested));
    }
    all
}

/// The master: the brightness channel, drawn as a fader.
fn is_master(view: &UiPanelControlView) -> bool {
    view.channel == MASTER_CHANNEL && matches!(view.control.widget, UiPanelWidget::Fader { .. })
}

/// A widget the card draws at its own size: a knob or a toggle.
fn has_card_size(view: &UiPanelControlView) -> bool {
    matches!(
        view.control.widget,
        UiPanelWidget::Knob { .. } | UiPanelWidget::Toggle
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        OfferPath, PlayState, UiClockTransport, UiPanelControl, UiPanelEmit, UiPatternPicker,
        UiSlotFieldState, UiSlotValue,
    };

    #[test]
    fn the_master_leads_from_the_root() {
        let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
            control("speed", knob()),
            control(MASTER_CHANNEL, fader()),
            control("sparkle", UiPanelWidget::Toggle),
        ]);
        let panel = board_panel_picks(&root, Some(true));
        assert_eq!(channels(&panel), [MASTER_CHANNEL, "speed", "sparkle"]);
        assert_eq!(scopes(&panel), ["/", "/", "/"]);
        assert_eq!(panel.more, 0);
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
                    .with_controls(vec![control(MASTER_CHANNEL, fader())]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), [MASTER_CHANNEL, "speed", "hue"]);
        assert_eq!(scopes(&panel), ["/fixture-1", "/", "/plasma-1"]);
    }

    #[test]
    fn a_brightness_knob_is_a_knob_not_the_master() {
        let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
            control(MASTER_CHANNEL, knob()),
            control("level", fader()),
        ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), [MASTER_CHANNEL]);
        assert_eq!(panel.more, 1, "a fader that is not the master");
    }

    #[test]
    fn with_no_master_knobs_and_toggles_lead() {
        let root = UiPanelGroup::new("Porch", "/").with_controls(vec![
            control("level", fader()),
            control("speed", knob()),
            control("sparkle", UiPanelWidget::Toggle),
        ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), ["speed", "sparkle"]);
        assert_eq!(panel.more, 1);
    }

    #[test]
    fn four_at_most_and_more_counts_the_rest() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![
                control(MASTER_CHANNEL, fader()),
                control("a", knob()),
                control("b", knob()),
            ])
            .with_groups(vec![
                UiPanelGroup::new("plasma", "/plasma-1").with_controls(vec![
                    control("c", UiPanelWidget::Toggle),
                    control("d", knob()),
                    control("e", knob()),
                ]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), [MASTER_CHANNEL, "a", "b", "c"]);
        assert_eq!(panel.controls.len(), BOARD_PANEL_CONTROLS);
        assert_eq!(panel.more, 2, "d and e");
    }

    #[test]
    fn a_palette_a_transport_and_a_pattern_picker_are_never_picked() {
        let root = UiPanelGroup::new("Porch", "/")
            .with_controls(vec![
                control("palette", UiPanelWidget::PaletteSwatch),
                control("speed", knob()),
            ])
            .with_groups(vec![
                UiPanelGroup::new("clock", "/clock").with_controls(vec![control(
                    "transport",
                    UiPanelWidget::Transport {
                        transport: transport(),
                    },
                )]),
                UiPanelGroup::new("Pattern", "/playlist").with_controls(vec![control(
                    "pattern",
                    UiPanelWidget::PatternPicker { picker: picker() },
                )]),
            ]);
        let panel = board_panel_picks(&root, None);
        assert_eq!(channels(&panel), ["speed"]);
        assert_eq!(panel.more, 3, "the palette, the transport, the picker");
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
        assert!(panel.controls.is_empty());
        assert_eq!(panel.more, 0);
        assert_eq!(panel.target, Some(target), "the root's scope target");
    }

    fn channels(panel: &UiBoardPanel) -> Vec<&str> {
        panel
            .controls
            .iter()
            .map(|control| control.view.channel.as_str())
            .collect()
    }

    fn scopes(panel: &UiBoardPanel) -> Vec<&str> {
        panel
            .controls
            .iter()
            .map(|control| control.scope.as_str())
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
                wires: Vec::new(),
            },
        )
    }

    fn transport() -> UiClockTransport {
        UiClockTransport {
            seconds: 0.0,
            play_state: PlayState::Playing,
            rate: 1.0,
            scrub_offset_seconds: 0.0,
            play_state_address: None,
            rate_address: None,
            scrub_address: None,
            play_state_override: None,
            rate_override: None,
            scrub_override: None,
        }
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
