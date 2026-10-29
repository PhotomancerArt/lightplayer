//! What became of a label: where it went, or why it isn't there.
//!
//! A renderer draws the placed ones; a page can show the dropped ones some
//! other way (a hover note) instead of losing what they said.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelFit {
    /// Lying along the arrow. `moved`: not at the spot it preferred,
    /// because that spot was bent, crowded or too near a head.
    Along { moved: bool },
    /// Level, beside the arrow's midpoint: no stretch of the line was
    /// straight and long enough.
    Beside,
    /// Not drawn.
    Dropped(DropReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The arrow is shorter than the label and its clearances, and there
    /// was no room beside it either.
    TooShort,
    /// Long enough, but bent all the way along, and no room beside it.
    TooCurved,
    /// Every spot that fit overlapped a label placed before it, or another
    /// arrow's head.
    Crowded,
}

impl LabelFit {
    pub fn is_placed(&self) -> bool {
        !matches!(self, LabelFit::Dropped(_))
    }
}
