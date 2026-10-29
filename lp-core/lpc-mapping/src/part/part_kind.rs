//! The three kinds of part: shapes, groups and repeats.

use alloc::vec::Vec;

use crate::part::part::Part;
use crate::part::repeat::Repeat;
use crate::part::shape::Shape;

/// What a part is.
#[derive(Debug, Clone, PartialEq)]
pub enum PartKind {
    /// A drawing: it renders segments.
    Shape(Shape),
    /// Parts held together, for arranging and moving them. Renders none of
    /// its own, and is never part of an id.
    Group { parts: Vec<Part> },
    /// Copies of one part.
    Repeat(Repeat),
}

impl PartKind {
    /// The word its ids are minted from (`path`, `group`, `repeat`…).
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            PartKind::Shape(shape) => shape.word(),
            PartKind::Group { .. } => "group",
            PartKind::Repeat(_) => "repeat",
        }
    }

    /// The parts it holds: a group's, or a repeat's one.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        match self {
            PartKind::Group { parts } => parts,
            PartKind::Repeat(repeat) => core::slice::from_ref(&*repeat.part),
            PartKind::Shape(_) => &[],
        }
    }

    /// The parts it holds, to edit in place.
    pub fn parts_mut(&mut self) -> &mut [Part] {
        match self {
            PartKind::Group { parts } => parts,
            PartKind::Repeat(repeat) => core::slice::from_mut(&mut *repeat.part),
            PartKind::Shape(_) => &mut [],
        }
    }
}
