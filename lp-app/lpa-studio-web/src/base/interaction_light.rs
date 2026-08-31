//! The interaction-light vocabulary: the class fragments that put the
//! spectrum on hover, press, focus and drag-in-flight.
//!
//! The CSS lives in `style.css` (the "Interaction light" section) because
//! every one of these effects needs a pseudo-element, a `@property`, or a
//! keyframe — none of which Tailwind utilities express. What lives *here*
//! is the naming: one place that says which surfaces get the full ring,
//! which get the lighter row edge, and what "focusable" costs at the call
//! site, so the answer is a function call rather than a copied string.
//!
//! House rules the fragments encode:
//!
//! - **Selection is not interaction light.** Selected/focused rows keep the
//!   neutral white outline; only hover, press and drag wear the spectrum.
//! - **Status hues beat decoration.** A status-toned control answers the
//!   pointer in its own color — see [`InlineButtonTone`][crate::base::InlineButtonTone].
//! - **Never a footprint change.** The ring is an absolutely-positioned
//!   pseudo-element and the focus ring is an `outline`; neither is in flow.
//!
//! # The whole vocabulary
//!
//! Class names appear as literals in a few call sites that must stay
//! `&'static str` (the gallery-card and action-button class tables, which
//! are pure lookup and never allocate). This list is the index:
//!
//! | class | where |
//! |---|---|
//! | `ux-ir-ring` | the ring host — see [`ir_ring_class`] |
//! | `ux-ir-ring-inset` | modifier: pull the ring to `inset: 0` for hosts that clip |
//! | `ux-ir-ring-on` | modifier: pin the ring on with no pointer over it (drag in flight) |
//! | `ux-row-edge` | dense rows — see [`row_edge_class`] |
//! | `ux-card-lift` | gallery-card 1px hover lift |
//! | `ux-drag-chip` | the lifted shadow a dragged card wears |
//! | `ux-press-flare` | the `:active` bloom on action buttons |
//! | `ux-focus-ring` | the focus convention — see [`focus_ring_class`] |
//! | `ux-spectrum-cta` | the standing spectrum-outline Primary (self-contained ring — never compose with `ux-ir-ring`; devices-treatments gate 2026-08-31 succeeded the gradient fill) |
//! | `ux-conic-spinner` | see [`conic_spinner_class`] |
//! | `ux-iri-fill` / `-static` | see [`iridescent_fill_class`] |

/// The full iridescent ring plus its bloom, for controls whose box is not
/// clipped: buttons, chips, anything without `overflow: hidden`. The ring
/// sits at `inset: -1px`, just outside the border. Hosts that DO clip add
/// `ux-ir-ring-inset` alongside it, or the ring is clipped away entirely.
pub fn ir_ring_class() -> &'static str {
    "ux-ir-ring"
}

/// The lighter variant for dense rows — a spectrum left edge and the bloom,
/// no ring. Full rings on every row of a tree read as noise.
pub fn row_edge_class() -> &'static str {
    "ux-row-edge"
}

/// The app-wide keyboard-focus convention: token-colored border plus a soft
/// offset outline. Add it to any control this refresh touches; text inputs
/// get it from the bare-element rule in `style.css` already.
pub fn focus_ring_class() -> &'static str {
    "ux-focus-ring"
}

/// The conic working spinner (22px).
pub fn conic_spinner_class() -> &'static str {
    "ux-conic-spinner"
}

/// The iridescent progress fill, sweeping under its own animation.
pub fn iridescent_fill_class() -> &'static str {
    "ux-iri-fill"
}

/// The iridescent fill's paint WITHOUT the sweep, for bars that already
/// carry their own animation (the timeout countdown, the indeterminate
/// shuttle). Two `animation` declarations on one element is one too many,
/// and the unlayered CSS rule would be the one that won.
pub fn iridescent_fill_static_class() -> &'static str {
    "ux-iri-fill-static"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_ring_carries_no_modifier() {
        // `ux-ir-ring` is the host and nothing more: the inset (clipping
        // hosts) and pinned-on (drag in flight) behaviours are opt-in
        // modifiers, so a control that merely wants a hover ring cannot
        // accidentally inherit either.
        assert_eq!(ir_ring_class(), "ux-ir-ring");
    }

    #[test]
    fn dense_rows_take_the_edge_not_the_ring() {
        assert!(!row_edge_class().contains("ux-ir-ring"));
    }

    #[test]
    fn the_two_progress_fills_differ_only_in_who_animates() {
        // A bar that already animates takes the `-static` paint: two
        // `animation` declarations on one element is one too many, and the
        // unlayered CSS rule is the one that would win.
        assert_ne!(iridescent_fill_class(), iridescent_fill_static_class());
    }
}
