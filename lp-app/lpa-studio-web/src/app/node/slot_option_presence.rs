//! Option-presence rendering: option-ness renders as PRESENCE, not as a
//! boolean-looking toggle.
//!
//! [`OptionPresenceStyle::PresenceInRow`] is the LIVE DEFAULT (P5, from P4's
//! recommendation): unset is a muted dashed "not set" chip in the
//! stable-width value cell with a set (plus) affordance in the gesture slot;
//! set is the value editor in the same cell with a clear (trash) affordance.
//! No toggle exists anywhere on option rows — booleans own the toggle
//! vocabulary, so `gamma_correction: Option<bool>` reads as ONE bool control
//! plus one clear gesture. The retired some/none slider is deleted outright;
//! the non-default candidates (child row, checkbox square) stay
//! story-selectable for the P7 review swap.
//!
//! Every style shares [`OptionPresenceCell`], which fixes the set→unset
//! layout jump by construction: both presence states render into one
//! reserved-width box, so flipping presence in either direction never moves
//! the trailing controls' anchor. [`OptionPresenceWidth`] widens the
//! reservation per value-kind class for editors intrinsically wider than the
//! base 8rem box (the P4 caveat) — the reservation is always a MIN, never a
//! max, so an even-wider editor still spans and only the box's leading edge
//! grows (the value area is end-aligned; every trailing control is a fixed
//! square).
//!
//! The gestures are unchanged (M3 decision D1 — the ops are fine, only the
//! affordance changed): set dispatches `EnsurePresent option_path.some`
//! (server default value) and clear dispatches `RemoveValue option_path`,
//! exactly like the retired toggle did.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectSlotAddress, UiAction, UiConfigSlot, UiConfigSlotBody, UiSlotEditorHint,
    UiSlotOptionality, UiSlotValueKind,
};

use crate::app::node::slot_edit_actions::{slot_ensure_present_action, slot_remove_value_action};
use crate::app::node::slot_gesture_fields::{GESTURE_ICON_SIZE, gesture_icon_button_class};
use crate::base::{StudioIcon, StudioIconName};

/// How an option row renders its option-ness. [`Self::PresenceInRow`] is the
/// live default (P5); the other variants stay story-selectable so the user
/// can swap the pick at the P7 review.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OptionPresenceStyle {
    /// The live default — presence as row content: unset renders a muted
    /// "not set" chip in the stable-width value cell with a set (plus)
    /// affordance in the gesture slot; set renders the value editor in the
    /// same cell with a clear (trash) affordance. No toggle anywhere on
    /// option rows.
    #[default]
    PresenceInRow,
    /// Story-only candidate B — the child-row variant: the option row itself
    /// is presence-only (a "set" summary or the "not set" chip, plus the
    /// set/clear affordance); the interior value renders as a CHILD row
    /// (like map entries) when set.
    ChildRow,
    /// Story-only candidate C — minimal delta: a checkbox-square presence
    /// indicator in the trailing gesture slot, visually distinct from both
    /// the retired slider silhouette and the bool segmented control, plus
    /// the stable-width cell.
    CheckboxSquare,
}

/// What fills the stable-width value cell when the value editor does not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionPresenceChip {
    /// Muted, dashed "not set" placeholder (every style's unset state).
    NotSet,
    /// Compact "set" summary (candidate B's set state, where the value
    /// itself lives in the child row).
    SetSummary,
}

/// Width class the presence cell reserves, derived from the value-kind class
/// of the slot body (the P4 width-hint caveat): editors intrinsically wider
/// than the base 8rem box get a wider reservation so the set-state cell
/// never collapses below its editor. Unset bodies are `Empty` (the DTO
/// carries no kind for an absent value), so the chip always sits in the
/// [`Self::Base`] box; for base-tier kinds — every scalar, bool, string,
/// dropdown, dimensions, vec2, and mat2 editor — the set and unset states
/// therefore share one identical box and a presence flip is geometry-stable
/// in BOTH directions. Wider kinds span past the base box from the leading
/// edge only (the reservation is a min, not a max, and the value area is
/// end-aligned), so the trailing gesture anchors never move for any kind.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OptionPresenceWidth {
    /// 8rem — covers the inline scalar footprints: the `w-16` number input
    /// box plus unit suffix (≈6.5rem), the bool segmented control (≈6rem),
    /// the dimensions pair (≈7.5rem), mat2 (≈6.5rem), and the flexible
    /// string/vec2 inputs.
    #[default]
    Base,
    /// 11rem — three-plus flexible columns and mid-size fixed editors:
    /// vec3/vec4 grids (usable column widths), mat3 (≈9.5rem), the affine2d
    /// grid (≈9.5rem), and the XY pad + readout (≈9rem).
    Wide,
    /// 14rem — the widest inline editors: mat4 rows (≈12.5rem) and the
    /// slider track + readout + unit (≈12.5rem).
    ExtraWide,
}

impl OptionPresenceWidth {
    /// Reservation tier for a slot body. `Empty`, `Record`, and `Asset`
    /// bodies render chips or compact summaries and stay in the base box.
    pub fn for_body(body: &UiConfigSlotBody) -> Self {
        let UiConfigSlotBody::Value(value) = body else {
            return Self::Base;
        };
        match &value.editor {
            UiSlotEditorHint::Slider { .. } if matches!(value.kind, UiSlotValueKind::F32(_)) => {
                return Self::ExtraWide;
            }
            UiSlotEditorHint::Xy if matches!(value.kind, UiSlotValueKind::Vec2(_)) => {
                return Self::Wide;
            }
            _ => {}
        }
        match &value.kind {
            UiSlotValueKind::Vec3(_)
            | UiSlotValueKind::Vec4(_)
            | UiSlotValueKind::IVec3(_)
            | UiSlotValueKind::IVec4(_)
            | UiSlotValueKind::UVec3(_)
            | UiSlotValueKind::UVec4(_)
            | UiSlotValueKind::BVec3(_)
            | UiSlotValueKind::BVec4(_)
            | UiSlotValueKind::Mat3x3(_) => Self::Wide,
            UiSlotValueKind::Mat4x4(_) => Self::ExtraWide,
            _ => Self::Base,
        }
    }
}

/// Stable-width value cell shared by every presence style.
///
/// The set→unset jump's root cause was the bare "unset" text being narrower
/// than the value editor it replaces, shifting the trailing controls'
/// anchor. This cell reserves one width per [`OptionPresenceWidth`] tier and
/// stretches whichever state renders — the editor or the placeholder chip —
/// to that same box, so presence flips in either direction re-render into an
/// identically sized cell.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptionPresenceCell(
    /// Placeholder chip replacing the value editor; `None` renders
    /// `children` (the editor) stretched to the same reserved box.
    #[props(default = None)]
    chip: Option<OptionPresenceChip>,
    /// Reserved-width tier (see [`OptionPresenceWidth::for_body`]).
    #[props(default)]
    width: OptionPresenceWidth,
    children: Element,
) -> Element {
    rsx! {
        div { class: presence_cell_class(width),
            if let Some(chip) = chip {
                span { class: presence_chip_class(chip), {presence_chip_label(chip)} }
            } else {
                {children}
            }
        }
    }
}

/// Set/clear affordance for the live default and candidate B, in the gesture
/// slot the retired toggle used to occupy: a plus glyph sets (dispatching
/// `EnsurePresent option_path.some`), a trash glyph clears (dispatching
/// `RemoveValue option_path`). Both states are one fixed `h-6 w-6` square in
/// the shared gesture-button family, so the trailing anchor never moves.
/// Non-toggleable rows render the button muted and inert.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptionPresenceActionButton(
    optionality: UiSlotOptionality,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let included = optionality.included;
    let wired = if optionality.can_toggle {
        address.zip(on_action)
    } else {
        None
    };
    let disabled = wired.is_none();
    let (icon, label) = if included {
        (StudioIconName::Remove, "Clear the optional value")
    } else {
        (StudioIconName::Add, "Set the optional value")
    };

    rsx! {
        button {
            class: gesture_icon_button_class(disabled),
            r#type: "button",
            disabled,
            aria_label: label,
            title: label,
            onclick: move |event| {
                event.stop_propagation();
                let Some((address, handler)) = wired.clone() else {
                    return;
                };
                if included {
                    handler.call(slot_remove_value_action(address));
                } else if let Some(some) = address.child_field("some") {
                    handler.call(slot_ensure_present_action(some));
                }
            },
            StudioIcon { name: icon, size: GESTURE_ICON_SIZE }
        }
    }
}

/// Candidate C's presence indicator (story-only): a checkbox-square (check
/// glyph when set, empty square when unset) in the trailing gesture slot —
/// square rather than slider so it cannot be mistaken for the bool control
/// family. Clicking dispatches the same set/clear gestures; the indicator is
/// a fixed `h-6 w-6` hit area around a fixed `h-4 w-4` square in both
/// states.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptionPresenceCheckbox(
    optionality: UiSlotOptionality,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let included = optionality.included;
    let wired = if optionality.can_toggle {
        address.zip(on_action)
    } else {
        None
    };
    let disabled = wired.is_none();
    let label = if included {
        "Optional value set (click to clear)"
    } else {
        "Optional value not set (click to set)"
    };

    rsx! {
        button {
            class: presence_checkbox_button_class(disabled),
            r#type: "button",
            disabled,
            aria_label: label,
            title: label,
            onclick: move |event| {
                event.stop_propagation();
                let Some((address, handler)) = wired.clone() else {
                    return;
                };
                if included {
                    handler.call(slot_remove_value_action(address));
                } else if let Some(some) = address.child_field("some") {
                    handler.call(slot_ensure_present_action(some));
                }
            },
            span { class: presence_checkbox_square_class(included),
                if included {
                    StudioIcon { name: StudioIconName::StepComplete, size: 12 }
                }
            }
        }
    }
}

/// Which chip fills the presence cell for a style × inclusion state; `None`
/// renders the value editor in the cell.
pub(crate) fn option_presence_chip(
    style: OptionPresenceStyle,
    included: bool,
) -> Option<OptionPresenceChip> {
    match (style, included) {
        (_, false) => Some(OptionPresenceChip::NotSet),
        (OptionPresenceStyle::ChildRow, true) => Some(OptionPresenceChip::SetSummary),
        (_, true) => None,
    }
}

/// Candidate B's interior value row (story-only): the option row itself is
/// presence-only and the contained value renders as a child row (like map
/// entries) when set. The child carries the interior `some` address so value
/// edits dispatch exactly as they do inline; the parent option row keeps the
/// clear affordance, the presence summary, and the own-edit revert entry
/// (the child carries none, so the inline revert never doubles).
pub(crate) fn option_presence_child_slot(
    slot: &UiConfigSlot,
    body_address: Option<ProjectSlotAddress>,
) -> UiConfigSlot {
    let mut child = slot.clone();
    child.key = format!("{}#some", slot.key);
    child.label = "value".to_string();
    child.address = body_address;
    child.edit_entry_address = None;
    child.optionality = None;
    child.description = None;
    child.detail = None;
    child.issues = Vec::new();
    child
}

/// The reserved-width, stretch-to-fill cell holding either the value editor
/// or a presence chip. `flex-none` keeps the reservation from flexing away;
/// `justify-items-stretch` sizes both states to the full cell width; the
/// per-tier `min-w` is a floor, never a ceiling.
fn presence_cell_class(width: OptionPresenceWidth) -> &'static str {
    match width {
        OptionPresenceWidth::Base => "tw:grid tw:min-w-32 tw:flex-none tw:justify-items-stretch",
        OptionPresenceWidth::Wide => "tw:grid tw:min-w-44 tw:flex-none tw:justify-items-stretch",
        OptionPresenceWidth::ExtraWide => {
            "tw:grid tw:min-w-56 tw:flex-none tw:justify-items-stretch"
        }
    }
}

/// Chip boxes share the editor box metrics (`min-h-7`, `px-2 py-1`,
/// `rounded-xs`, `text-sm`) so a chip and an editor occupy the same-shaped
/// cell; "not set" wears a dashed border on the muted surface (absence),
/// the "set" summary the solid border on the page surface (presence).
fn presence_chip_class(chip: OptionPresenceChip) -> &'static str {
    match chip {
        OptionPresenceChip::NotSet => {
            "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-dashed tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-subtle-foreground"
        }
        OptionPresenceChip::SetSummary => {
            "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
        }
    }
}

fn presence_chip_label(chip: OptionPresenceChip) -> &'static str {
    match chip {
        OptionPresenceChip::NotSet => "not set",
        OptionPresenceChip::SetSummary => "set",
    }
}

/// Fixed hit area around candidate C's checkbox-square.
fn presence_checkbox_button_class(disabled: bool) -> &'static str {
    if disabled {
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:appearance-none tw:items-center tw:justify-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-subtle-foreground"
    } else {
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-muted-foreground"
    }
}

/// The square itself: fixed `h-4 w-4` in both states so presence flips never
/// resize the indicator; the set state fills with the accent family plus the
/// check glyph.
fn presence_checkbox_square_class(included: bool) -> &'static str {
    if included {
        "tw:inline-flex tw:h-4 tw:w-4 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-accent-bg tw:text-accent"
    } else {
        "tw:inline-flex tw:h-4 tw:w-4 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-page"
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiSlotEditorHint, UiSlotValue};

    use super::*;

    #[test]
    fn presence_in_row_is_the_live_default() {
        // P5: the P4 recommendation is the default every production
        // `ConfigSlotRow` gets without opting in; the retired toggle no
        // longer exists as a style at all.
        assert_eq!(
            OptionPresenceStyle::default(),
            OptionPresenceStyle::PresenceInRow
        );
    }

    #[test]
    fn presence_flip_is_geometry_stable_in_both_directions() {
        // The jump-regression assertion (set→unset AND unset→set): the cell
        // class is a pure function of the body's width tier, and for every
        // base-tier kind the set body and the unset (`Empty`) body resolve
        // to the IDENTICAL reserved cell — flipping presence in either
        // direction re-renders into the same box by construction.
        let unset = presence_cell_class(OptionPresenceWidth::for_body(&UiConfigSlotBody::Empty));
        for value in [
            UiSlotValue::u32(255),
            UiSlotValue::i32(-3),
            UiSlotValue::f32(0.5),
            UiSlotValue::bool(true),
            UiSlotValue::string("warm"),
        ] {
            let body = UiConfigSlotBody::Value(value);
            let set = presence_cell_class(OptionPresenceWidth::for_body(&body));
            assert_eq!(set, unset, "set and unset share one reserved box");
        }
        // And the gesture slot: set (trash) and unset (plus) render the same
        // fixed-square button class, so the trailing anchor never moves.
        assert!(gesture_icon_button_class(false).contains("tw:h-6"));
        assert!(gesture_icon_button_class(false).contains("tw:w-6"));
    }

    #[test]
    fn presence_cell_reserves_one_width_and_stretches_both_states() {
        // Stability by construction: whichever state renders (editor or
        // chip), it fills the same reserved-width cell, so a presence flip
        // in either direction cannot change the cell's footprint.
        for width in [
            OptionPresenceWidth::Base,
            OptionPresenceWidth::Wide,
            OptionPresenceWidth::ExtraWide,
        ] {
            let cell = presence_cell_class(width);
            assert!(cell.contains("tw:min-w-"), "reserved width: {cell}");
            assert!(cell.contains("tw:flex-none"), "no flex resize: {cell}");
            assert!(
                cell.contains("tw:justify-items-stretch"),
                "both states stretch to the reservation: {cell}"
            );
        }
    }

    #[test]
    fn width_tiers_match_the_editor_footprint_classes() {
        // The P4 caveat handled: kinds whose inline editors are
        // intrinsically wider than the base 8rem box reserve a wider cell
        // (a MIN — even wider editors still span), so the set-state cell
        // never collapses below its editor.
        use OptionPresenceWidth::{Base, ExtraWide, Wide};

        let tier =
            |value: UiSlotValue| OptionPresenceWidth::for_body(&UiConfigSlotBody::Value(value));

        // Base: the scalar/bool/string footprints the 8rem box was sized to.
        assert_eq!(tier(UiSlotValue::u32(1)), Base);
        assert_eq!(tier(UiSlotValue::bool(true)), Base);
        assert_eq!(tier(UiSlotValue::string("x")), Base);
        assert_eq!(tier(UiSlotValue::vec2([0.0, 1.0])), Base);
        // Non-value bodies (unset chips, record summaries) stay base.
        assert_eq!(
            OptionPresenceWidth::for_body(&UiConfigSlotBody::Empty),
            Base
        );

        // Wide: 3+ column grids and the XY pad.
        assert_eq!(tier(UiSlotValue::vec3([0.0, 1.0, 2.0])), Wide);
        assert_eq!(tier(UiSlotValue::vec4([0.0, 1.0, 2.0, 3.0])), Wide);
        assert_eq!(tier(UiSlotValue::mat3x3([[0.0; 3]; 3])), Wide);
        let xy = UiSlotValue::vec2([0.25, 0.75]).with_editor(UiSlotEditorHint::Xy);
        assert_eq!(tier(xy), Wide);

        // ExtraWide: mat4 rows and the slider track + readout.
        assert_eq!(tier(UiSlotValue::mat4x4([[0.0; 4]; 4])), ExtraWide);
        let slider = UiSlotValue::f32(0.5).with_editor(UiSlotEditorHint::Slider {
            min: 0.0,
            max: 1.0,
            step: None,
        });
        assert_eq!(tier(slider), ExtraWide);
    }

    #[test]
    fn presence_chips_share_the_editor_box_metrics() {
        // The chip must occupy the same-shaped box as the editor it
        // replaces (min height and padding), not render as bare text — the
        // bare "unset" text was the jump's root cause.
        for chip in [OptionPresenceChip::NotSet, OptionPresenceChip::SetSummary] {
            let class = presence_chip_class(chip);
            assert!(class.contains("tw:min-h-7"), "editor min height: {class}");
            assert!(class.contains("tw:px-2"), "editor padding: {class}");
            assert!(class.contains("tw:rounded-xs"), "editor radius: {class}");
        }
    }

    #[test]
    fn gesture_affordances_hold_a_fixed_footprint_in_every_state() {
        // The trailing gesture slot stays anchored because the button is
        // the same fixed square whether wired or inert, set or unset.
        for disabled in [false, true] {
            let action = gesture_icon_button_class(disabled);
            assert!(action.contains("tw:h-6"), "{action}");
            assert!(action.contains("tw:w-6"), "{action}");
            assert!(action.contains("tw:flex-none"), "{action}");
            let hit = presence_checkbox_button_class(disabled);
            assert!(hit.contains("tw:h-6"), "{hit}");
            assert!(hit.contains("tw:w-6"), "{hit}");
        }
        for included in [false, true] {
            let square = presence_checkbox_square_class(included);
            assert!(square.contains("tw:h-4"), "{square}");
            assert!(square.contains("tw:w-4"), "{square}");
            assert!(square.contains("tw:flex-none"), "{square}");
        }
    }

    #[test]
    fn chip_selection_matches_the_style_semantics() {
        use OptionPresenceChip::{NotSet, SetSummary};
        use OptionPresenceStyle::{CheckboxSquare, ChildRow, PresenceInRow};

        // Unset always shows the "not set" placeholder.
        assert_eq!(option_presence_chip(PresenceInRow, false), Some(NotSet));
        assert_eq!(option_presence_chip(ChildRow, false), Some(NotSet));
        assert_eq!(option_presence_chip(CheckboxSquare, false), Some(NotSet));
        // Set shows the editor inline except for the child-row candidate,
        // whose value lives in the child row.
        assert_eq!(option_presence_chip(PresenceInRow, true), None);
        assert_eq!(option_presence_chip(CheckboxSquare, true), None);
        assert_eq!(option_presence_chip(ChildRow, true), Some(SetSummary));
    }

    #[test]
    fn child_slot_moves_the_value_and_keeps_edit_chrome_on_the_parent() {
        use lpa_studio_core::{ProjectNodeAddress, ProjectSlotRoot, SlotPath, UiSlotValue};

        let address = ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap(),
            ProjectSlotRoot::def(),
            SlotPath::parse("brightness").unwrap(),
        );
        let some_address = address.child_field("some");
        let slot = UiConfigSlot::value("brightness", "Brightness", UiSlotValue::u32(255))
            .with_address(address.clone())
            .with_edit_entry_address(address)
            .with_optionality(UiSlotOptionality::included(true));

        let child = option_presence_child_slot(&slot, some_address.clone());
        assert_eq!(child.address, some_address, "value edits target `.some`");
        assert_eq!(child.edit_entry_address, None, "revert stays on the parent");
        assert_eq!(child.optionality, None, "the child is not itself an option");
        assert_eq!(child.label, "value");
        assert_eq!(child.body, slot.body, "the interior value moves whole");
    }
}
