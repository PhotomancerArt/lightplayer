//! The two-sided patch bay (D34a): the same cells, seen from both ends.
//!
//! A **cell** is one `(fixture span ↔ wire span)` run — what the engine
//! plans as an `OutputFragment` and the wire states as a
//! `WireOutputPlacement`. Both faces render the SAME cells and differ only
//! in what they lay them along and what the label says:
//!
//! - the **output face** lays them along the wire, one row per port, and
//!   labels them with the FIXTURE they came from (`peach_body 22–43 ◀`);
//! - the **fixture face** lays them along the fixture's own channel space
//!   and labels them with where on the WIRE they landed (`→ ch34 ◀`).
//!
//! That difference IS the patch: a body split across two stretches of the
//! strand reads as two cells on the wire and as one unbroken run in its own
//! numbering. Cells that no producer claims are not modelled at all — the
//! row's track shows through, which is how a gap reads.
//!
//! Everything here is READ-ONLY (P6 scope): no cell carries a slot address,
//! because nothing in the bay writes. The patch document is edited as text
//! (`{stem}.patch.json` → the Text asset editor).
//!
//! **Where the numbers come from.** Placements ride the published-frame
//! probe (`OutputFrameEntry::placements`), which is the only description of
//! how a wire was cut — auto-flow ordering is the resolver's, not the
//! project file's, so a client cannot re-derive it. Live pixels come from
//! the same probe's bytes: one frame per output, shared by every cell that
//! reads out of it, so the bay costs one `Rc` clone rather than a copy per
//! cell.

use crate::UiControlProductPreview;

/// One `(fixture span ↔ wire span)` run, as BOTH faces render it.
///
/// A run split across two ports appears as two cells sharing one
/// [`Self::id`] — the twin-hover key. The fixture face never splits: a
/// producer's own channel space is contiguous by construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPatchCell {
    /// Identity of the RUN this cell shows, shared by every face (and every
    /// port-clipped piece) that shows it — the twin-hover key. Opaque:
    /// derived from the producer and the run's own start, never parsed.
    pub id: String,
    /// The producing node's display label (`peach_body`).
    pub producer: String,
    /// The producing node's address path, when the join found the card.
    /// The bay does not navigate today; this is what a later slice's
    /// click-to-counterpart needs, and it is free here.
    pub producer_node: Option<String>,
    /// First lamp of the run in the PRODUCER's own numbering.
    pub source_start: u32,
    /// Lamps in the run.
    pub lamps: u32,
    /// First lamp of the run on the output's wire.
    pub wire_start: u32,
    /// The run is laid onto the wire end-first.
    pub reversed: bool,
    /// Some lamp of this run is claimed by another run too — the engine
    /// darkens those samples and reports the range on the output's status.
    /// Rendered red on both faces.
    pub contested: bool,
    /// The port (output channel key) this cell lands on, when it lands
    /// inside an authored one.
    pub port_key: Option<u32>,
    /// The port's board label (`IO18`, `D10`), when the wire names a pin.
    pub port_label: String,
    /// The output node's display label — what the fixture side names when a
    /// project drives more than one output.
    pub output_label: String,
}

impl UiPatchCell {
    /// Last lamp of the run in the producer's own numbering (inclusive).
    /// `None` for an empty run — a state the engine can plan and the label
    /// must not render as `22–21`.
    #[must_use]
    pub fn source_end(&self) -> Option<u32> {
        self.lamps
            .checked_sub(1)
            .map(|last| self.source_start.saturating_add(last))
    }
}

/// The output card's bay: one row per port, cells laid along the wire.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPatchBay {
    /// One row per authored channel, in slice order.
    pub ports: Vec<UiPatchPort>,
    /// The frame every cell reads its pixels out of — the output's own
    /// published wire frame, verbatim (post-finalize, as the strip sees
    /// it). `None` until one arrives; the cells still draw their spans.
    pub frame: Option<UiControlProductPreview>,
    /// Lamps claimed by more than one run.
    pub contested_lamps: u32,
    /// Lamps below the wire's end that no run claims.
    pub gap_lamps: u32,
}

impl UiPatchBay {
    /// Is there anything to draw? A bay with no port carrying a cell says
    /// nothing the channels section does not already say.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ports.iter().all(|port| port.cells.is_empty())
    }
}

/// One port's row: the stretch of wire it drives, and the cells on it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPatchPort {
    /// The `channels` map key — the port's own name on the card.
    pub key: u32,
    /// The board's label for the wire (`IO18`, `D10`). Empty when the
    /// endpoint names no pin.
    pub pin_label: String,
    /// First wire lamp this port drives.
    pub start: u32,
    /// Lamps it drives.
    pub lamps: u32,
    /// The runs landing on it, clipped to its span, in wire order.
    pub cells: Vec<UiPatchCell>,
}

/// The fixture card's row: the same cells, laid along the fixture's own
/// channel space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiFixturePatch {
    /// Lamps the fixture's own product carries — the row's full width.
    pub lamps: u32,
    /// Its runs, in the fixture's own channel order. An UNPATCHED fixture
    /// has exactly one: auto-flow contributes its whole product as a single
    /// run, and that cell is the honest picture of "wherever it fell".
    pub cells: Vec<UiPatchCell>,
    /// The wire frame the cells read their pixels out of.
    ///
    /// One frame, not one per cell: a fixture feeding two outputs would
    /// show the first output's frame under all of its cells, which is why
    /// [`Self::single_output`] records whether that ever happened. Slice 1
    /// has no such project.
    pub frame: Option<UiControlProductPreview>,
    /// Every cell landed on the output whose frame [`Self::frame`] is.
    /// False means some cell's pixels are read from a different wire than
    /// the one it names, and the face says so rather than lying quietly.
    pub single_output: bool,
}

impl UiFixturePatch {
    /// Does this fixture reach the wire as ONE unbroken forward run?
    ///
    /// Deliberately not called "unpatched": from this side the two are
    /// indistinguishable, and honestly so. Auto-flow puts a fixture
    /// wherever the producers before it ended, and an anchor can name that
    /// exact channel — the run looks identical either way, because it IS
    /// identical. What the fixture face can say is whether its own channel
    /// space arrives in one piece, which is the thing a split reader needs
    /// to see; where it landed is the label's business.
    #[must_use]
    pub fn is_single_run(&self) -> bool {
        match self.cells.as_slice() {
            [cell] => !cell.reversed && cell.source_start == 0 && cell.lamps == self.lamps,
            _ => false,
        }
    }
}
