//! A part: anything authored in a fixture's layout (names.html "Part").
//!
//! Parts form a tree: shapes, and the groups and repeats that hold parts. A
//! repeat's copies are generated parts, never stored.

use alloc::string::String;

use crate::part::lamp_ref::LampRef;
use crate::part::part_id::PartId;
use crate::part::part_kind::PartKind;

/// One authored part.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Flat, unique in its fixture, never reused.
    pub id: PartId,
    /// What a person calls it; free text, never an id.
    pub name: String,
    pub kind: PartKind,
    /// Lamp facts for every segment under it that doesn't say otherwise.
    pub lamp: LampRef,
}

impl Part {
    /// A part with no name and nothing set but its kind.
    #[must_use]
    pub fn new(id: PartId, kind: PartKind) -> Self {
        Self {
            id,
            name: String::new(),
            kind,
            lamp: LampRef::default(),
        }
    }

    /// The parts it holds (a group's, a repeat's one).
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        self.kind.parts()
    }

    /// It, then every part under it, depth first.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Part)) {
        visit(self);
        for part in self.parts() {
            part.walk(visit);
        }
    }
}
