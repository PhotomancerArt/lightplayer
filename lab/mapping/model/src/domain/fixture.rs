//! A fixture: one physical thing (a tail, a ring display, a dome), with its
//! own bounding box and mapping (its parts and its patching). Its coordinates are its own:
//! `(0, 0)` is the top-left of its box, wherever the layout puts it.
//!
//! A fixture is one kind of node (`node.rs`); its id and name live on the
//! node. What it carries of its own is its drawing, its placements and
//! the visual channel it reads. Each frame it samples that channel's
//! render product at its lamps and publishes their colours as its control
//! product (`board.rs`), as the product's fixture node does.
//!
//! The control channel it writes follows its placements, read the
//! product's way: the fixture writes the channel the outputs they name
//! read (fit-check §6, `bindings.rs`). So a fixture names no channel of its
//! own for that.

use lpc_mapping::part::{Mapping, Part, PartId, PartKind};

use crate::domain::channel::Channel;
use crate::domain::color_order::ColorOrder;
use crate::domain::geom::{Rect, Vec2};

#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    /// The fixture's extent, from `(0, 0)`. Lamps outside it are never lit
    /// (D4).
    pub size: Vec2,
    /// Its mapping: its parts and its patching (`lpc_mapping::part`), as
    /// its map2d format 6 file holds them. What the patcher makes of it is
    /// derived (`patch.rs`), never stored.
    pub mapping: Mapping,
    /// Which byte of each of its pixels is red, green and blue.
    pub color_order: ColorOrder,
    /// The visual channel whose render product it samples (the product's
    /// `bindings.input.source`).
    pub reads: Channel,
    /// What about it the lab can't hold yet, when there is something.
    /// Such a fixture opens as its box alone: nothing is drawn in it,
    /// nothing can be drawn into it or resized, and its files are never
    /// written. The rest of the project opens round it (plan M4).
    pub unsupported: Option<Unsupported>,
    /// The product's `render_size`, when it has one: the shape of the
    /// picture it samples, which its box is fitted into (`picture_rect`).
    pub render_size: Option<(u32, u32)>,
}

/// Why a fixture opens as its box alone, and where it can be edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// What the lab can't hold, as a noun phrase for a sentence: "its
    /// point-list mapping", "a patch row that places part of an object".
    pub what: String,
    /// The view that edits it, by the name the user sees it under.
    pub edit_in: String,
}

impl Fixture {
    /// A new fixture's side: it starts square, this big each way, until
    /// you resize it.
    pub const DEFAULT_SIDE: f64 = 100.0;
    /// The smallest a fixture gets, either way.
    pub const MIN_SIDE: f64 = 4.0;

    /// A new, empty, square fixture.
    pub fn square() -> Self {
        Self::new(Vec2::new(Self::DEFAULT_SIDE, Self::DEFAULT_SIDE))
    }

    pub fn new(size: Vec2) -> Self {
        Self {
            size,
            mapping: Mapping::new(),
            color_order: ColorOrder::default(),
            reads: Channel::visual_out(),
            unsupported: None,
            render_size: None,
        }
    }

    /// The box in the fixture's own coordinates.
    pub fn bounds(&self) -> Rect {
        Rect::new(Vec2::default(), self.size)
    }

    /// Where a point of the fixture (its own coordinates) falls in its box,
    /// as the unit square a render product is sampled in: `(0, 0)` the
    /// top-left, `(1, 1)` the bottom-right.
    pub fn uv(&self, at: Vec2) -> Vec2 {
        Vec2::new(at.x / self.size.x.max(1e-9), at.y / self.size.y.max(1e-9))
    }

    /// How many channels one of its lamps takes (its lamps' colors; RGB is
    /// 3, and the lab has only RGB lamps so far).
    pub fn channels_per_lamp(&self) -> u32 {
        self.mapping.lamp.channels_per_lamp().max(1)
    }

    /// Its top-level parts, in wiring order.
    pub fn parts(&self) -> &[Part] {
        &self.mapping.parts
    }

    pub fn part(&self, id: &PartId) -> Option<&Part> {
        self.mapping.part(id)
    }

    pub fn part_mut(&mut self, id: &PartId) -> Option<&mut Part> {
        find_mut(&mut self.mapping.parts, id)
    }

    /// The part holding `id` (a group, or a repeat), or `None` when `id`
    /// is top-level (or absent).
    pub fn parent_part(&self, id: &PartId) -> Option<&PartId> {
        parent_of(&self.mapping.parts, id, None)
    }

    /// Drop overrides that set nothing any more.
    pub fn tidy_patching(&mut self) {
        self.mapping.overrides.retain(|_, o| !o.is_empty());
    }

    /// Take a part (and everything under it) out of a group's list or the
    /// fixture's, wherever it lives. A repeat's one part can't be taken
    /// out alone.
    pub fn remove(&mut self, id: &PartId) -> Option<Part> {
        remove_from(&mut self.mapping.parts, id)
    }
}

fn find_mut<'a>(list: &'a mut [Part], id: &PartId) -> Option<&'a mut Part> {
    for p in list {
        if &p.id == id {
            return Some(p);
        }
        if let Some(found) = find_mut(p.kind.parts_mut(), id) {
            return Some(found);
        }
    }
    None
}

fn parent_of<'a>(list: &'a [Part], id: &PartId, parent: Option<&'a PartId>) -> Option<&'a PartId> {
    for p in list {
        if &p.id == id {
            return parent;
        }
        if let Some(found) = parent_of(p.parts(), id, Some(&p.id)) {
            return Some(found);
        }
    }
    None
}

fn remove_from(list: &mut Vec<Part>, id: &PartId) -> Option<Part> {
    if let Some(i) = list.iter().position(|p| &p.id == id) {
        return Some(list.remove(i));
    }
    for p in list.iter_mut() {
        if let PartKind::Group { parts } = &mut p.kind
            && let Some(found) = remove_from(parts, id)
        {
            return Some(found);
        }
    }
    None
}
