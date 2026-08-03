//! Board callouts: "press THIS button", drawn on the board.
//!
//! Silkscreen is tiny. A user told to "hold BOOT" reads `P?` `R?` on a
//! thumb-sized board and gives up — so the instruction points at the thing
//! instead of naming it. The anatomy story proved the machinery (computed
//! anchors over the deterministic layout); this is that, as an API.
//!
//! Two decisions from the M2b gate (2026-08-02) shape it:
//!
//! - **One callout at a time.** A recovery ritual is a SEQUENCE of actions
//!   ("hold BOOT", then "tap RST"), so the drawing shows the action you take
//!   now. The prop is a `Vec` because a doc figure may label several parts
//!   at once, but instructional callers should pass one.
//! - **Anatomy gold, never attention-orange.** A callout explains; it does
//!   not alarm. Orange belongs to the card's own state, and a diagram that
//!   shouts competes with the status it sits beside.
//!
//! Anchors resolve against [`BoardLayout`], so a callout lands exactly where
//! the feature is drawn — including features whose geometry only exists at
//! layout time (a button's `y` may be expressed from the bottom edge).

use crate::geometry::BoardLayout;

/// What a callout points at, addressed the way a human names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalloutTarget {
    /// A button by silkscreen label ("BOOT", "RST"). Matched
    /// case-insensitively — sidecars say `BOOT`, prose says `boot`.
    Button(String),
    /// A rail pin or screw terminal by GPIO number.
    Gpio(u8),
    /// A rail pin or terminal by silkscreen label ("D10", "IO18", "5V").
    PinLabel(String),
    /// The onboard RGB pixel.
    Rgb,
    /// A USB connector by label ("USB", "UART").
    Usb(String),
}

/// One instruction drawn on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardCallout {
    pub target: CalloutTarget,
    /// The instruction. Carries any disambiguation the drawing cannot show
    /// — a stacked pair renders as two adjacent buttons, so the text says
    /// "the TOP button of the pair".
    pub text: String,
    /// Step number in a ritual, rendered as a bold "Step 1." lead-in.
    /// Physical instructions are ORDERED and the order is the part people
    /// get wrong ("hold BOOT *while* you plug in"), so the sequence is
    /// first-class rather than something each caller re-words.
    pub step: Option<u32>,
}

impl BoardCallout {
    pub fn new(target: CalloutTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
            step: None,
        }
    }

    /// One numbered step of a ritual: "Step 1. Hold BOOT".
    pub fn step(number: u32, target: CalloutTarget, text: impl Into<String>) -> Self {
        Self {
            step: Some(number),
            ..Self::new(target, text)
        }
    }

    /// Point at a button by label.
    pub fn button(label: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(CalloutTarget::Button(label.into()), text)
    }

    /// Point at a pin by GPIO.
    pub fn gpio(gpio: u8, text: impl Into<String>) -> Self {
        Self::new(CalloutTarget::Gpio(gpio), text)
    }

    /// Point at a USB connector by label.
    pub fn usb(label: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(CalloutTarget::Usb(label.into()), text)
    }
}

/// A callout resolved to drawing coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct CalloutPlacement {
    pub text: String,
    /// Step number, when this callout is part of a ritual.
    pub step: Option<u32>,
    /// The point the leader touches — the feature itself.
    pub anchor: (f32, f32),
    /// Where the label's text baseline sits.
    pub label: (f32, f32),
    /// `true` = `text-anchor: start` (label extends right of its x).
    pub start_anchored: bool,
}

/// Horizontal distance from the feature to the label, in drawing units.
const LEAD: f32 = 26.0;

impl BoardLayout {
    /// Resolve `callouts` against this layout, dropping any whose target
    /// this board does not have.
    ///
    /// Dropping is deliberate: a caller that says "press BOOT" against a
    /// board with no BOOT button has a data problem, and a diagram is the
    /// wrong place to raise it — the lint in the board editor is. Better a
    /// drawing that omits an instruction than one with an arrow pointing at
    /// nothing.
    pub fn place_callouts(&self, callouts: &[BoardCallout]) -> Vec<CalloutPlacement> {
        callouts
            .iter()
            .filter_map(|callout| self.place_callout(callout))
            .collect()
    }

    fn place_callout(&self, callout: &BoardCallout) -> Option<CalloutPlacement> {
        let anchor = self.anchor_for(&callout.target)?;
        // Lead away from the nearer vertical edge, so the label leaves the
        // board rather than crossing it.
        let outward_right = anchor.0 >= self.board_w / 2.0;
        let label_x = if outward_right {
            anchor.0 + LEAD
        } else {
            anchor.0 - LEAD
        };
        Some(CalloutPlacement {
            text: callout.text.clone(),
            step: callout.step,
            anchor,
            label: (label_x, anchor.1 + 2.5),
            start_anchored: outward_right,
        })
    }

    fn anchor_for(&self, target: &CalloutTarget) -> Option<(f32, f32)> {
        match target {
            CalloutTarget::Button(label) => self
                .buttons
                .iter()
                .find(|button| button.label.eq_ignore_ascii_case(label))
                .map(|button| button.center),
            CalloutTarget::Gpio(gpio) => self
                .rail_rows()
                .find(|row| row.gpio == Some(*gpio))
                .map(|row| row.pad.center())
                .or_else(|| {
                    self.band
                        .iter()
                        .find(|row| row.gpio == Some(*gpio))
                        .map(|row| (row.pad_x, row.leader[0].1))
                }),
            CalloutTarget::PinLabel(label) => self
                .rail_rows()
                .find(|row| {
                    row.label
                        .as_ref()
                        .is_some_and(|row_label| row_label.text.eq_ignore_ascii_case(label))
                })
                .map(|row| row.pad.center()),
            // The renderer draws the pixel from the sidecar's own point, and
            // there is at most one, so the layout carries no separate entry.
            CalloutTarget::Rgb => None,
            CalloutTarget::Usb(label) => self
                .usb
                .iter()
                .find(|usb| usb.label.eq_ignore_ascii_case(label))
                .map(|usb| usb.center),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::board_by_id;
    use crate::geometry::DiagramOptions;

    fn layout(board_id: &str) -> BoardLayout {
        BoardLayout::compute(
            board_by_id(board_id).expect("board in catalog"),
            &DiagramOptions::default(),
        )
    }

    /// The point of the whole milestone: the arrow lands ON the button the
    /// renderer drew, not near it.
    #[test]
    fn a_button_callout_anchors_at_the_drawn_cap() {
        let layout = layout("espressif/esp32-c6-devkitc-1");
        let button = layout.buttons.first().expect("the C6 devkit has buttons");
        let placed =
            layout.place_callouts(&[BoardCallout::button(&button.label, "press and hold this")]);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].anchor, button.center);
    }

    /// The DOM-Z-102's stacked pair: both buttons are addressable, and they
    /// resolve to DIFFERENT points — the case that motivated the milestone.
    #[test]
    fn stacked_buttons_resolve_separately() {
        let layout = layout("domraem/dom-z-102");
        let placed = layout.place_callouts(&[
            BoardCallout::button("BOOT", "hold BOOT — the TOP button of the pair"),
            BoardCallout::button("RST", "then tap RST — the BOTTOM button"),
        ]);
        assert_eq!(placed.len(), 2);
        assert_ne!(placed[0].anchor, placed[1].anchor);
        assert!(
            placed[0].anchor.1 < placed[1].anchor.1,
            "BOOT is drawn above RST"
        );
    }

    #[test]
    fn labels_lead_away_from_the_nearer_edge() {
        let layout = layout("domraem/dom-z-102");
        for placed in layout.place_callouts(&[BoardCallout::button("BOOT", "x")]) {
            if placed.anchor.0 < layout.board_w / 2.0 {
                assert!(placed.label.0 < placed.anchor.0, "left features lead left");
                assert!(!placed.start_anchored);
            } else {
                assert!(
                    placed.label.0 > placed.anchor.0,
                    "right features lead right"
                );
                assert!(placed.start_anchored);
            }
        }
    }

    #[test]
    fn a_gpio_callout_finds_a_rail_pin() {
        let layout = layout("domraem/dom-z-102");
        // IO18 is one of the four silkscreened data channels.
        let placed = layout.place_callouts(&[BoardCallout::gpio(18, "wire your strip here")]);
        assert_eq!(placed.len(), 1);
    }

    /// A target this board lacks is dropped, not drawn pointing at nothing.
    #[test]
    fn unknown_targets_are_dropped() {
        let layout = layout("seeed/xiao-esp32-c6");
        let placed = layout.place_callouts(&[
            BoardCallout::button("NOPE", "this button does not exist"),
            BoardCallout::gpio(250, "nor this gpio"),
        ]);
        assert!(placed.is_empty());
    }

    #[test]
    fn button_labels_match_case_insensitively() {
        let layout = layout("domraem/dom-z-102");
        assert_eq!(
            layout
                .place_callouts(&[BoardCallout::button("boot", "x")])
                .len(),
            1
        );
    }
}
