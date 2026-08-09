//! The `space` section both visual-side faces grow — the two-sided space
//! model made visible (dimensionality vision D6/D7/D8/D13/D14).
//!
//! **One shape, both sides.** A shader declares where it renders
//! (`ShaderDef.space`: `TwoD { in_1d } | OneD { in_2d }`) and a fixture
//! declares how it consumes (`FixtureDef.consume`: `Auto | Policy {
//! from_1d, force }`). D13 says the two
//! sections are visual mirrors; making them ONE DTO makes that a
//! data-level fact rather than a styling coincidence, so the web renders
//! both through a single component and they cannot drift apart.
//!
//! The section is derived by
//! [`node_face_builder`](crate::app::project::node) from the very config
//! rows it then CLAIMS out of the advanced drawer (the
//! `retire_face_claimed_debug_rows` precedent, one section over): the
//! section IS those rows' surface now, so leaving them in the drawer would
//! be two controls writing one slot.
//!
//! **No parallel write path.** Every cell and flag carries the slot
//! address its gesture targets, and nothing else: a variant choice is the
//! `EnsurePresent <enum>.<Variant>` the generic `EnumVariantField` already
//! dispatches, a flag is the `SetValue` a bool row already dispatches.
//! The tile picker (P4) is a different PRESENTATION of the same op.

use crate::{ProjectSlotAddress, UiCellProjection, UiSlotFieldState, UiVisualSpace};

/// Which side of the two-sided model a section speaks for.
///
/// Not cosmetic: it is what tells the shared component whether the primary
/// cell is a *declaration* ("this shader lives in 1D") or a *policy*
/// ("this fixture accepts 1D sources like so"), and which mirror wording
/// to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSpaceSide {
    /// A visual PRODUCER's declaration — the shader card's `space` slot.
    Producer,
    /// A visual CONSUMER's policy — the fixture card's `consume` slot.
    Consumer,
}

/// What a cell of a space section answers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSpaceCellRole {
    /// The section's leading cell: the shader's `space` enum
    /// (`TwoD`/`OneD`), or — since the P4b rework — the fixture's whole
    /// consume policy as ONE dropdown (`Auto` = "follow the source", a
    /// projection = authored override; the web dispatches the op sequence
    /// per choice).
    Primary,
    /// A 1D producer's answer for 2D consumers (`space.OneD.in_2d`).
    ProducerIn2d,
    /// A 2D producer's answer for 1D consumers (`space.TwoD.in_1d`) —
    /// centre scanline only today, so a single-choice read-only cell.
    ProducerIn1d,
}

/// One selectable answer inside a [`UiSpaceCell`].
///
/// `projection` is the tile's subject: the [`UiCellProjection`] a live tile
/// probe should force to show what THIS choice would look like (P4's
/// forced-policy probes). `None` means the choice defers rather than
/// projecting — the producer-side `Default` ("consumer decides") and the
/// primary cell's own variants.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSpaceChoice {
    /// RAW declared variant ident (`Radial`, not `radial`) — slot paths
    /// address variants verbatim, so this is what the `EnsurePresent`
    /// gesture appends to [`UiSpaceCell::address`].
    pub variant: String,
    /// Display label for the choice.
    pub label: String,
    /// The projection a live tile for this choice should force. `None` for
    /// choices that project nothing.
    pub projection: Option<UiCellProjection>,
    /// Whether this is the cell's active variant.
    pub selected: bool,
}

/// One enum cell of a space section — the thing the tile picker opens on.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSpaceCell {
    /// What this cell answers.
    pub role: UiSpaceCellRole,
    /// Row label, from the backing slot row.
    pub label: String,
    /// Active variant ident (also flagged on its [`UiSpaceChoice`]).
    pub active: String,
    /// Display label of the active variant.
    pub active_label: String,
    /// Every declared variant, in declaration order.
    pub choices: Vec<UiSpaceChoice>,
    /// The ENUM row's address. A choice dispatches `EnsurePresent` at
    /// `address.child_field(&choice.variant)` — the same op the generic
    /// `EnumVariantField` sends. `None` = nothing to dispatch (an
    /// unaddressed or read-only row), which the web renders inert.
    pub address: Option<ProjectSlotAddress>,
    /// The backing row's interaction/validation state.
    pub state: UiSlotFieldState,
    /// The ACTIVE shape's direction row, when the active shape is
    /// directional (extrude/mirror — G1b ruling 4's second section:
    /// "top section is general shape, below that is direction"). Derived
    /// from the flattened payload row (`…in_2d.Extrude.direction` /
    /// `…from_1d.Mirror.direction`), so it carries a REAL address; absent
    /// for radial/angular, for the deferring choices, and while a
    /// consumer is still in `Auto` (no payload rows in the tree yet —
    /// the row appears once a directional shape is picked).
    pub direction: Option<UiSpaceDirection>,
}

/// The direction row under a directional projection cell (G1b ruling 4
/// + the mirror-direction ruling): a segmented choice over whatever
/// direction vocabulary the ACTIVE shape declares — extrude's
/// `ProjectionDirection` (`Right`…`Up`), mirror's `MirrorDirection`
/// fold (`InwardX`…`OutwardY`).
///
/// `variants` is read from the flattened enum row itself (declaration
/// order), never hardcoded, so the web renders whichever vocabulary the
/// model declares per shape. A pick dispatches `EnsurePresent` at
/// `address.child_field(<variant>)`, the generic enum-row gesture.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSpaceDirection {
    /// Active direction variant ident.
    pub active: String,
    /// Every declared variant ident, in declaration order.
    pub variants: Vec<String>,
    /// The flattened direction enum row's address
    /// (`<cell>.<Shape>.direction`). `None` renders inert.
    pub address: Option<ProjectSlotAddress>,
    /// The backing row's interaction/validation state.
    pub state: UiSlotFieldState,
}

impl UiSpaceCell {
    /// Whether this cell offers a real choice. A single-variant cell (the
    /// 2D→1D answer, `Default` only today) is a statement, not a picker.
    pub fn is_choosable(&self) -> bool {
        self.address.is_some() && self.state.editable && self.choices.len() > 1
    }
}

/// The D1 failure made visible: the shader DECLARES one space and its GLSL
/// defines the other space's entry.
///
/// The compiler refuses that outright (`lp_shader::ShaderEntrySpace` — the
/// declaration is the entry contract, never sniffed from source), so the
/// state reaching the card is a compile error on the node's status. There
/// is no structured error class to match on today, so this is recovered
/// from the status detail text; see
/// [`crate::app::project::node`]'s `space_mismatch` for the debt note.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSpaceMismatch {
    /// The space the def declares.
    pub declared: UiVisualSpace,
    /// The space the GLSL's entry point actually implements.
    pub entry: UiVisualSpace,
    /// The compiler's own message, verbatim — the card shows the
    /// structured pair and keeps the raw text for the detail surface.
    pub message: String,
}

/// The `space` section of a shader or fixture card.
///
/// Derived only when the backing slot rows are present; a node whose
/// projection predates the slots (or a hand-built test fixture) keeps
/// `None` and renders no section, exactly like the other optional face
/// blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSpaceSection {
    /// Which side this section speaks for.
    pub side: UiSpaceSide,
    /// The leading enum cell (`space` / `consume`).
    pub primary: UiSpaceCell,
    /// The space this side DECLARES it lives in, when it declares one.
    /// `Some` on the producer side (`TwoD`/`OneD`); always `None` on the
    /// consumer side — a fixture states a policy, not a space (its own
    /// dimensionality comes from its mapping).
    pub declared_space: Option<UiVisualSpace>,
    /// The answer cells under the primary, in render order.
    pub cells: Vec<UiSpaceCell>,
    /// The declared-vs-entry mismatch, when the node is in it (D1).
    pub mismatch: Option<UiSpaceMismatch>,
}

impl UiSpaceSection {
    /// Every cell in render order: the primary first, then its answers.
    pub fn all_cells(&self) -> impl Iterator<Item = &UiSpaceCell> {
        std::iter::once(&self.primary).chain(self.cells.iter())
    }

    /// The cell in this section filling `role`.
    pub fn cell(&self, role: UiSpaceCellRole) -> Option<&UiSpaceCell> {
        self.all_cells().find(|cell| cell.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(role: UiSpaceCellRole, choices: usize) -> UiSpaceCell {
        UiSpaceCell {
            role,
            label: "Space".to_string(),
            active: "TwoD".to_string(),
            active_label: "2D".to_string(),
            choices: (0..choices)
                .map(|index| UiSpaceChoice {
                    variant: format!("V{index}"),
                    label: format!("v{index}"),
                    projection: None,
                    selected: index == 0,
                })
                .collect(),
            address: None,
            state: UiSlotFieldState::editable(),
            direction: None,
        }
    }

    #[test]
    fn a_single_variant_cell_is_a_statement_not_a_picker() {
        // The 2D→1D answer has exactly one declared variant today
        // (centre scanline). Offering a dropdown over one choice would
        // invite a gesture with nothing to change.
        let mut single = cell(UiSpaceCellRole::ProducerIn1d, 1);
        assert!(!single.is_choosable(), "no address, one choice");
        single.choices.push(UiSpaceChoice {
            variant: "Extrude".to_string(),
            label: "extrude".to_string(),
            projection: Some(UiCellProjection::Extrude(
                crate::UiProjectionDirection::Right,
            )),
            selected: false,
        });
        assert!(
            !single.is_choosable(),
            "an unaddressed cell has nothing to dispatch"
        );
    }

    #[test]
    fn cells_are_addressable_by_role() {
        let section = UiSpaceSection {
            side: UiSpaceSide::Producer,
            primary: cell(UiSpaceCellRole::Primary, 2),
            declared_space: Some(UiVisualSpace::TwoD),
            cells: vec![cell(UiSpaceCellRole::ProducerIn1d, 1)],
            mismatch: None,
        };
        assert_eq!(section.all_cells().count(), 2);
        assert!(section.cell(UiSpaceCellRole::Primary).is_some());
        assert!(section.cell(UiSpaceCellRole::ProducerIn2d).is_none());
    }
}
