//! Canvas color vocabulary: the object fill cycle and the selection accent.

/// Object fill palette (wiring-order cycling; matches the UX spike).
pub(crate) const OBJECT_COLORS: &[&str] = &[
    "#5aa9e6", "#3fd68e", "#e4c065", "#c792ea", "#f0913b", "#64d8cb",
];

/// Selection accent (provisional spike blue; violet is reserved for
/// bound-state semantics elsewhere in Studio).
pub(crate) const SELECTION_COLOR: &str = "#4c9ffe";

#[must_use]
pub fn object_color(object_index: usize) -> &'static str {
    OBJECT_COLORS[object_index % OBJECT_COLORS.len()]
}
