//! Reading a format-6 document into a [`Mapping`].
//!
//! The format is read first and on its own, as `Map2dDoc::from_json` does,
//! so a newer document is refused as newer ("this build reads up to 6")
//! rather than as malformed. Then the whole document, then its rules, each
//! refusal naming what it is about:
//!
//! - a part id that breaks the id rule, holds `@` or `/`, or is used twice;
//! - a `next` word that is no kind of part, or a counter that would mint an
//!   id a part already has (an absent counter is 1);
//! - a ring key that breaks the key rule or is used twice in its circle, or
//!   a `next_ring` that would mint a key a ring already has;
//! - an override keyed by text that is not a node id (a well-formed id
//!   naming no node is read and kept: the patcher reports it, D12);
//! - a repeat of more than [`crate::MAX_REPEAT_COUNT`] copies;
//! - a path's `d` that isn't path data, or uses a command not built yet.
//!
//! Unknown fields are ignored, as in every map2d format (`map2d_doc.rs`,
//! rule 1): a field an older build may skip needs no format change. One it
//! would misread does.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::map2d_doc::MAX_REPEAT_COUNT;
use crate::map2d6::doc_read::{CircleText, DocText, KindText, PartText, read_doc, read_format};
use crate::map2d6::json_text::{JsonText, Read};
use crate::map2d6::map2d6_error::Map2d6Error;
use crate::map2d6::numbered_id::{minted_part_number, minted_ring_number};
use crate::map2d6::path_data::{PathDataError, parse_path_data};
use crate::part::{
    Circle, IdCounters, Mapping, PART_KIND_WORDS, Part, PartId, PartKind, Path, Repeat, Ring,
    SegmentKey, Shape, TreeId,
};

/// The format this reader reads and the writer writes.
pub const MAP2D6_FORMAT: u32 = 6;

/// A map2d document's `format`, read on its own.
pub fn peek_format(json: &str) -> Result<u32, Map2d6Error> {
    whole(json, read_format)
}

/// Read all of `json` with `read`.
fn whole<T>(json: &str, read: fn(&mut JsonText) -> Read<T>) -> Result<T, Map2d6Error> {
    let mut text = JsonText::new(json);
    match read(&mut text).and_then(|value| text.end().map(|()| value)) {
        Ok(value) => Ok(value),
        Err(_) => Err(text.error()),
    }
}

/// Read a format-6 document (rules in the module doc).
pub fn read(json: &str) -> Result<Mapping, Map2d6Error> {
    match peek_format(json)? {
        MAP2D6_FORMAT => {}
        found @ 1..MAP2D6_FORMAT => return Err(Map2d6Error::OlderFormat { found }),
        found => return Err(Map2d6Error::UnsupportedFormat { found }),
    }
    let doc: DocText = whole(json, read_doc)?;

    let mut ids = BTreeSet::new();
    let parts = doc
        .parts
        .into_iter()
        .map(|part| read_part(part, &mut ids))
        .collect::<Result<Vec<_>, _>>()?;

    let mut next = IdCounters::new();
    for (word, n) in &doc.next {
        if !PART_KIND_WORDS.contains(&word.as_str()) {
            return Err(Map2d6Error::UnknownCounter { word: word.clone() });
        }
        next.raise(word, *n);
    }
    for id in &ids {
        if let Some((word, n)) = minted_part_number(id) {
            if next.next(word) <= n {
                return Err(Map2d6Error::CounterBelowId {
                    word: word.to_string(),
                    next: next.next(word),
                    id: id.clone(),
                });
            }
        }
    }

    let mut overrides = BTreeMap::new();
    for (text, entry) in doc.overrides {
        let Some(id) = TreeId::parse(&text) else {
            return Err(Map2d6Error::MalformedOverrideId { id: text });
        };
        overrides.insert(id, entry);
    }

    Ok(Mapping {
        sample_diameter: doc.sample_diameter,
        canvas: doc.canvas,
        parts,
        lamp: Default::default(),
        next,
        start: doc.start,
        flow: doc.flow,
        overrides,
    })
}

fn read_part(doc: PartText, ids: &mut BTreeSet<String>) -> Result<Part, Map2d6Error> {
    let id = read_part_id(&doc.id)?;
    if !ids.insert(doc.id.clone()) {
        return Err(Map2d6Error::DuplicatePartId { id: doc.id });
    }
    let kind = match doc.kind {
        KindText::Path(path) => {
            let commands = parse_path_data(&path.d).map_err(|error| match error {
                PathDataError::NotBuilt(command) => Map2d6Error::PathCommandNotBuilt {
                    part: doc.id.clone(),
                    command,
                },
                PathDataError::Bad(reason) => Map2d6Error::PathData {
                    part: doc.id.clone(),
                    reason,
                },
            })?;
            PartKind::Shape(Shape::Path(Path {
                commands,
                count: path.count,
                reversed: path.reversed,
                align: path.align,
                split: path.split,
            }))
        }
        KindText::Shape(shape) => PartKind::Shape(shape),
        KindText::Circle(circle) => PartKind::Shape(Shape::Circle(read_circle(&doc.id, circle)?)),
        KindText::Group(parts) => PartKind::Group {
            parts: parts
                .into_iter()
                .map(|part| read_part(part, ids))
                .collect::<Result<Vec<_>, _>>()?,
        },
        KindText::Repeat {
            center,
            count,
            part,
        } => {
            if count > MAX_REPEAT_COUNT {
                return Err(Map2d6Error::TooManyCopies { part: doc.id, count });
            }
            PartKind::Repeat(Repeat {
                part: Box::new(read_part(*part, ids)?),
                center,
                count,
            })
        }
    };
    let mut part = Part::new(id, kind);
    part.name = doc.name;
    part.start = doc.start;
    part.flow = doc.flow;
    Ok(part)
}

/// A part id, by today's rule; `@` and `/` named, since a node id uses
/// them to add copies and keys.
fn read_part_id(id: &str) -> Result<PartId, Map2d6Error> {
    let marked = [
        ('@', "'@', which marks a repeat copy in a node id"),
        ('/', "'/', which marks a generated key in a node id"),
    ];
    if let Some((_, why)) = marked.iter().find(|(mark, _)| id.contains(*mark)) {
        return Err(Map2d6Error::InvalidPartId {
            id: id.to_string(),
            reason: alloc::format!("a part id can't hold {why}"),
        });
    }
    PartId::new(id).map_err(|reason| Map2d6Error::InvalidPartId {
        id: id.to_string(),
        reason,
    })
}

fn read_circle(part: &str, doc: CircleText) -> Result<Circle, Map2d6Error> {
    let mut keys = BTreeSet::new();
    let mut rings = Vec::with_capacity(doc.rings.len());
    for ring in &doc.rings {
        let refuse = |reason: &str| Map2d6Error::InvalidRingKey {
            part: part.to_string(),
            key: ring.key.clone(),
            reason: reason.to_string(),
        };
        let key = SegmentKey::new(&ring.key).map_err(|reason| refuse(&reason))?;
        if !keys.insert(ring.key.as_str()) {
            return Err(refuse("two rings have this key"));
        }
        if let Some(n) = minted_ring_number(&ring.key) {
            if doc.next_ring <= n {
                return Err(Map2d6Error::RingCounterBelowKey {
                    part: part.to_string(),
                    next: doc.next_ring,
                    key: ring.key.clone(),
                });
            }
        }
        rings.push(Ring {
            key,
            radius: ring.radius,
            count: ring.count,
        });
    }
    Ok(Circle {
        center: doc.center,
        start_angle_deg: doc.start_angle_deg,
        dir: doc.dir,
        drawn_from: doc.drawn_from,
        split: doc.split,
        rings,
        next_ring: doc.next_ring,
    })
}
