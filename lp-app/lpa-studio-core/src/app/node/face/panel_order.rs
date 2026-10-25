//! The order a panel's controls are drawn in: strips, then blocks, then
//! cells; each run by the bus's well-known registry, then by the walk.
//!
//! The board card's picks apply it to every section
//! ([`crate::board_panel_picks`]). The play page and the editor's Panel
//! section keep the walk's order until their own plans adopt this one; the
//! function sits here, beside the shape, so they can.
//!
//! Nothing here names a channel. A channel ranks by its place in
//! [`lpc_model::WELL_KNOWN_CHANNELS`] (the picker's display order), and any
//! channel the registry does not know ranks after every one it does. The
//! sort is stable, so the walk's order (nodes and slots as the tree lists
//! them, names sorted) breaks every tie.

use std::borrow::Borrow;

use lpc_model::WELL_KNOWN_CHANNELS;

use crate::{UiPanelControlView, UiPanelShape};

/// The sort key of a control: its shape's run (strips 0, blocks 1, cells
/// 2), then its channel's rank in the bus's registry.
#[must_use]
pub fn panel_order_key(view: &UiPanelControlView) -> (u8, usize) {
    let run = match view.control.widget.shape() {
        UiPanelShape::Strip => 0,
        UiPanelShape::Block => 1,
        UiPanelShape::Cell => 2,
    };
    (run, channel_rank(&view.channel))
}

/// Put a group's controls in panel order. Stable: controls with the same
/// key keep the order they came in.
pub fn sort_panel_controls<T: Borrow<UiPanelControlView>>(controls: &mut [T]) {
    controls.sort_by_key(|control| panel_order_key(control.borrow()));
}

/// A channel's place in the registry; past its end for a channel not in it.
fn channel_rank(channel: &str) -> usize {
    WELL_KNOWN_CHANNELS
        .iter()
        .position(|known| known.name == channel)
        .unwrap_or(WELL_KNOWN_CHANNELS.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        OfferPath, UiPanelControl, UiPanelEmit, UiPanelWidget, UiPatternPicker, UiSlotFieldState,
        UiSlotValue,
    };

    /// The panel reads the bus registry's order. If this fails, someone
    /// reordered `WELL_KNOWN_CHANNELS` (`lpc-model/src/bus/well_known.rs`),
    /// and that moves every panel's order with it.
    #[test]
    fn the_registrys_order_is_the_panels_order() {
        let rank = channel_rank;
        assert!(
            rank("brightness") < rank("palette") && rank("palette") < rank("clock.rate"),
            "a panel's order reads WELL_KNOWN_CHANNELS' order (brightness, \
             palette, clock.rate): reordering the registry moves every panel"
        );
    }

    #[test]
    fn a_channel_the_registry_does_not_know_ranks_after_every_one_it_does() {
        let last_known = WELL_KNOWN_CHANNELS.len() - 1;
        assert!(channel_rank("control.out") <= last_known);
        assert_eq!(channel_rank("scale"), WELL_KNOWN_CHANNELS.len());
    }

    #[test]
    fn strips_come_before_blocks_before_cells_whatever_the_walk_said() {
        let mut controls = vec![
            control("clock.rate", UiPanelWidget::Speed),
            control("playlist.cycle", block()),
            control("brightness", fader()),
            control("sparkle", UiPanelWidget::Toggle),
            control("palette", UiPanelWidget::PaletteSwatch),
            control("level", knob()),
        ];
        sort_panel_controls(&mut controls);
        assert_eq!(
            channels(&controls),
            [
                "brightness",
                "palette",
                "playlist.cycle",
                "clock.rate",
                "sparkle",
                "level"
            ]
        );
    }

    #[test]
    fn inside_a_run_a_registered_channel_leads_and_the_walk_breaks_ties() {
        let mut controls = vec![
            control("zeta", knob()),
            control("alpha", knob()),
            control("clock.rate", UiPanelWidget::Speed),
            control("mid", knob()),
        ];
        sort_panel_controls(&mut controls);
        assert_eq!(
            channels(&controls),
            ["clock.rate", "zeta", "alpha", "mid"],
            "two unregistered channels keep the walk's order, not the alphabet's"
        );
    }

    /// The order on the real PLAYFUL choker (the walk lists the clock's
    /// Speed, the fixture's brightness, the shader's palette, then its
    /// scale), the order that left Scale alone under the palette.
    #[test]
    fn playfuls_walk_order_comes_out_brightness_palette_speed_scale() {
        let mut controls = vec![
            control("clock.rate", UiPanelWidget::Speed),
            control("brightness", fader()),
            control("palette", UiPanelWidget::PaletteSwatch),
            control("scale", knob()),
        ];
        sort_panel_controls(&mut controls);
        assert_eq!(
            channels(&controls),
            ["brightness", "palette", "clock.rate", "scale"]
        );
    }

    /// The rule has no exceptions: a stepped knob that happens to sit on
    /// the palette channel is a cell, ranked among the cells by the registry.
    #[test]
    fn a_stepped_knob_on_the_palette_channel_is_a_cell() {
        let stepped = UiPanelWidget::Knob {
            min: 0.0,
            max: 8.0,
            step: Some(1.0),
        };
        let mut controls = vec![
            control("scale", knob()),
            control("palette", stepped),
            control("brightness", fader()),
            control("clock.rate", UiPanelWidget::Speed),
        ];
        sort_panel_controls(&mut controls);
        assert_eq!(
            channels(&controls),
            ["brightness", "palette", "clock.rate", "scale"],
            "the brightness strip leads; the stepped knob leads the cells"
        );
        assert_eq!(panel_order_key(&controls[1]).0, 2, "a cell's run");
    }

    fn channels(controls: &[UiPanelControlView]) -> Vec<&str> {
        controls
            .iter()
            .map(|control| control.channel.as_str())
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

    fn block() -> UiPanelWidget {
        UiPanelWidget::PatternPicker {
            picker: UiPatternPicker {
                verbs: OfferPath::root("project"),
                entries: Vec::new(),
                active: None,
                cycle: lpc_model::PlaylistCycle::Hold,
                cycle_target: None,
                skip_target: None,
            },
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
}
