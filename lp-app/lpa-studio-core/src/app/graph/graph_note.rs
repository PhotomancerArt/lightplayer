//! What the page's input changed, for the studio actor to act on.

/// Sent on the actor's queue by [`GraphHandle::input`](super::GraphHandle::input)
/// when an input changed something the rest of Studio must hear about.
/// It says only THAT something changed: the actor reads the editor for
/// what, so a burst of notes acts once, on the latest state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GraphNote {
    /// The project changed: there may be files to write.
    pub edited: bool,
    /// The selection changed: the board's highlight follows it.
    pub selected: bool,
}
