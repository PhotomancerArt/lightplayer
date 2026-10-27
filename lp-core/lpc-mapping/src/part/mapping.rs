//! One fixture's mapping: layout plus patching (names.html "Mapping").
//!
//! The layout is its parts, a tree; [`crate::part::render`] turns it into
//! the rendered tree, whose leaves are the segments. The patching is the
//! fixture's start and flow, each part's own start and flow, and the
//! overrides by node id; [`crate::patching::patch_mapping`] gives each
//! segment its address.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::map2d_doc::DEFAULT_SAMPLE_DIAMETER;
use crate::part::id_counters::IdCounters;
use crate::part::lamp_ref::LampRef;
use crate::part::part::Part;
use crate::part::part_id::PartId;
use crate::part::part_kind::PartKind;
use crate::part::tree_id::TreeId;
use crate::patching::{Flow, Override, Start};

/// One fixture's mapping.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    /// Each lamp's sample diameter, in the layout's units (a layout fact,
    /// as today's `Map2dDoc` holds it).
    pub sample_diameter: f32,
    /// The authored canvas `[min_x, min_y, width, height]`, when there is
    /// one: what the fit frames instead of the lamps' bounds.
    pub canvas: Option<[f32; 4]>,
    /// The top-level parts, in order: the order their segments come.
    pub parts: Vec<Part>,
    /// The fixture's lamp facts: what every part inherits.
    pub lamp: LampRef,
    /// The next number per kind of part.
    pub next: IdCounters,
    /// Where the fixture starts: its output, port and channel. All Keep is
    /// no start (a problem when anything is to be laid).
    pub start: Start,
    /// How its top-level parts follow each other.
    pub flow: Flow,
    /// What a person set on single nodes, by id, over the flows' starts.
    pub overrides: BTreeMap<TreeId, Override>,
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            sample_diameter: DEFAULT_SAMPLE_DIAMETER,
            canvas: None,
            parts: Vec::new(),
            lamp: LampRef::default(),
            next: IdCounters::default(),
            start: Start::default(),
            flow: Flow::default(),
            overrides: BTreeMap::new(),
        }
    }
}

impl Mapping {
    /// An empty mapping: no parts, today's default sample diameter, no
    /// canvas.
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
