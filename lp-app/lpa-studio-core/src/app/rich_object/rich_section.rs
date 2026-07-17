//! One section of a rich object's detail.

use crate::UiStatusKind;

/// One titled section of a rich object's detail surface: a tone, small
/// label→value facts, an optional advisory chip, and the section's
/// affordance identities.
///
/// Generic over the consumer's affordance identity `A` (device sections
/// carry `DeviceDetailAffordance`; other rich objects bring their own
/// vocabulary) — the model stays renderer- and wiring-independent, exactly
/// like [`RosterAffordance`](crate::app::roster::RosterAffordance).
///
/// Affordance cardinality: an [`Advisory`](RichWeight::Advisory) or
/// [`Actionable`](RichWeight::Actionable) section carries **at most one**
/// affordance (the rollup consumes the first); a
/// [`Danger`](RichWeight::Danger) section may list several destructive
/// rows (flash · erase) — they never participate in rollup, so the ≤1
/// rule has nothing to protect there.
#[derive(Clone, Debug, PartialEq)]
pub struct RichSection<A> {
    /// Fixed schema title ("Health", "Project", …). Sections render in
    /// schema order — users learn where things are (Q4) — so the title is
    /// identity, never a sort key.
    pub title: String,
    /// The section's health family. Only [`RichWeight::Actionable`]
    /// sections may color the object's rollup.
    pub tone: UiStatusKind,
    /// Small label→value facts.
    pub lines: Vec<RichLine>,
    /// A standing advisory chip (e.g. "Firmware update available"): tones
    /// a chip, never the object indicator.
    pub chip: Option<RichChip>,
    /// The section's affordance identities (see the cardinality note
    /// above). Wiring to concrete actions is the renderer's job.
    pub affordances: Vec<A>,
    /// How the section participates in rollup.
    pub weight: RichWeight,
}

/// How a section participates in the object's rollup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RichWeight {
    /// Facts only: may tone a chip, never the object indicator.
    Advisory,
    /// Competes for the object's indicator tone and primary affordance.
    Actionable,
    /// Always present, never shouts: renders visually separated (the
    /// inline red-tinted danger zone, Q5), never sorts up, never colors
    /// rollup.
    Danger,
}

/// One label→value fact row inside a section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichLine {
    pub label: String,
    pub value: String,
}

impl RichLine {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// A standing advisory chip inside a section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichChip {
    /// Chip tone — advisory by construction: it never reaches the rollup.
    pub tone: UiStatusKind,
    pub text: String,
}
