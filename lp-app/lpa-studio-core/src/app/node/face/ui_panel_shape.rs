//! [`UiPanelShape`]: how a panel control takes its room.

/// How a panel control takes its room (the panel layout spike, 2026-10-19).
/// A surface lays controls out by shape, never by widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPanelShape {
    /// One unit, many to a line: a knob, the Speed knob, a stepped knob, a toggle.
    Cell,
    /// The line's whole width, its label and value above: a fader, a palette.
    Strip,
    /// An instrument of its own, the whole width: the Pattern instrument.
    Block,
}
