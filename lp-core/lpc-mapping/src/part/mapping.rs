//! One fixture's mapping: layout plus patching (names.html "Mapping").
//!
//! The layout is its parts, a tree; [`crate::part::render`] turns it into
//! the rendered tree, whose leaves are the segments.

use alloc::vec::Vec;

use crate::part::id_counters::IdCounters;
use crate::part::lamp_ref::LampRef;
use crate::part::part::Part;
use crate::part::part_id::PartId;
use crate::part::part_kind::PartKind;

/// One fixture's mapping.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mapping {
    /// The top-level parts, in order: the order their segments come.
    pub parts: Vec<Part>,
    /// The fixture's lamp facts: what every part inherits.
    pub lamp: LampRef,
    /// The next number per kind of part.
    pub next: IdCounters,
}

impl Mapping {
    /// An empty mapping.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint an id for a new part of this kind: the next number for its
    /// word, never one the mapping already has, never one it had before.
    pub fn mint(&mut self, kind: &PartKind) -> PartId {
        let parts = &self.parts;
        self.next.mint(kind.word(), |id| find(parts, id).is_some())
    }

    /// Mint an id and wrap a part of this kind in it.
    pub fn new_part(&mut self, kind: PartKind) -> Part {
        let id = self.mint(&kind);
        Part::new(id, kind)
    }

    /// The part with this id, anywhere in the tree.
    #[must_use]
    pub fn part(&self, id: &PartId) -> Option<&Part> {
        find(&self.parts, id)
    }

    /// Every part, depth first.
    #[must_use]
    pub fn all_parts(&self) -> Vec<&Part> {
        let mut out = Vec::new();
        for part in &self.parts {
            part.walk(&mut |p| out.push(p));
        }
        out
    }
}

fn find<'a>(parts: &'a [Part], id: &PartId) -> Option<&'a Part> {
    parts.iter().find_map(|p| {
        if &p.id == id {
            Some(p)
        } else {
            find(p.parts(), id)
        }
    })
}
