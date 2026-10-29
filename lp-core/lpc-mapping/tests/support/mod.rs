//! Test-only helpers shared by the statement files: today's document as a
//! mapping, and positions as bits.
//!
//! Not the upgrade: M3's v11→v12 step carries its own frozen copy. This is
//! the smallest honest conversion, for parity tests and example documents.

use lpc_mapping::part::{
    Circle, Mapping, Part, PartId, PartKind, Path, PathCommand, Repeat, Shape,
};
use lpc_mapping::{Map2dDoc, Map2dShape};

/// Today's document as a mapping: one part per object, in order. A part's
/// id is the object's id, else its name lower-cased when that is a valid id
/// no part has yet (`F-lower` → `f-lower`), else a minted one. A repeat's
/// shape becomes a part with a minted id of its own; a ring set becomes a
/// circle split one; a path's gaps become moves.
pub fn today_as_mapping(doc: &Map2dDoc) -> Mapping {
    let mut m = Mapping::new();
    m.sample_diameter = doc.sample_diameter;
    m.canvas = doc.canvas;
    for object in &doc.objects {
        let kind = kind_of(&object.shape, &mut m);
        let named = object
            .id
            .as_ref()
            .map(|id| id.as_str().to_string())
            .or_else(|| Some(object.name.to_lowercase()))
            .and_then(|id| PartId::new(&id).ok())
            .filter(|id| m.part(id).is_none());
        let mut part = match named {
            Some(id) => Part::new(id, kind),
            None => m.new_part(kind),
        };
        part.name = object.name.clone();
        m.parts.push(part);
    }
    m
}

/// Today's shape as a part kind.
fn kind_of(shape: &Map2dShape, m: &mut Mapping) -> PartKind {
    match shape {
        Map2dShape::Grid(grid) => PartKind::Shape(Shape::Grid(grid.clone())),
        Map2dShape::Ring(ring) => {
            PartKind::Shape(Shape::Circle(Circle::from_ring_set(ring).unwrap()))
        }
        Map2dShape::Polygon(polygon) => PartKind::Shape(Shape::Polygon(polygon.clone())),
        Map2dShape::FilledPolygon(filled) => PartKind::Shape(Shape::FilledPolygon(filled.clone())),
        Map2dShape::Path(path) => {
            let commands = path
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i == 0 || path.gaps.contains(&(i as u32 - 1)) {
                        PathCommand::Move(*p)
                    } else {
                        PathCommand::Line(*p)
                    }
                })
                .collect();
            let mut new = Path::of(commands, path.count);
            new.reversed = path.reversed;
            new.align = path.align;
            PartKind::Shape(Shape::Path(new))
        }
        Map2dShape::Repeat(repeat) => {
            let inner = kind_of(&repeat.shape, m);
            let part = m.new_part(inner);
            PartKind::Repeat(Repeat::new(part, repeat.center, repeat.count))
        }
    }
}

/// Positions as their bits, so a comparison is bit for bit.
pub fn bits(positions: &[[f32; 2]]) -> Vec<[u32; 2]> {
    positions
        .iter()
        .map(|p| [p[0].to_bits(), p[1].to_bits()])
        .collect()
}
