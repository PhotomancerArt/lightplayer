//! Writing a [`Mapping`] as a format-6 document: one canonical form.
//!
//! Pretty JSON (two-space indent) ending in a newline; fields in a fixed
//! order, each left out when it holds its default; overrides sorted by
//! their id's text. A document read and written back is the same bytes.
//!
//! **Counters are written honest.** A kind's `next` is written above every
//! part id that reads as minted from it (`path3` → at least 4), and a
//! circle's `next_ring` above every `r<n>` key, even when the mapping's own
//! counter is lower (a part built with a hand-made id): a low counter on
//! disk would let a deleted id be minted again, and an override kept for
//! the old part would quietly point at the new one (D12).
//!
//! **Refused, by name:** lamp facts (format 6 saves none; G1, DD12: the
//! lamp-model plan adds them with its own bump), a port step only a flow
//! renders (`plus` above 0), and numbers that aren't finite.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::map2d_doc::RingOrder;
use crate::map2d6::map2d6_doc::Map2d6Doc;
use crate::map2d6::map2d6_error::Map2d6Error;
use crate::map2d6::numbered_id::{minted_part_number, minted_ring_number};
use crate::map2d6::op_doc::{AmountDoc, ChannelOpDoc, FlowDoc, OutputOpDoc, PortOpDoc, StartDoc};
use crate::map2d6::override_doc::OverrideDoc;
use crate::map2d6::part_doc::{
    CircleDoc, DrawnFromDoc, GroupDoc, KindDoc, PartDoc, PathDoc, PointsDoc, RepeatDoc, RingDoc,
};
use crate::map2d6::path_data::write_path_data;
use crate::map2d6::read::MAP2D6_FORMAT;
use crate::part::{Circle, LampRef, Mapping, Part, PartKind, Shape};
use crate::patching::{Amount, ChannelOp, Flow, OutputOp, Override, PortOp, Start, Unit};

/// Write a mapping as a format-6 document (rules in the module doc).
pub fn write(mapping: &Mapping) -> Result<String, Map2d6Error> {
    if mapping.lamp != LampRef::default() {
        return Err(no_lamp_facts("the fixture"));
    }
    finite("sample_diameter", &[mapping.sample_diameter])?;
    if let Some(canvas) = &mapping.canvas {
        finite("canvas", canvas)?;
    }

    let mut next: BTreeMap<String, u32> = mapping
        .next
        .iter()
        .map(|(word, n)| (word.to_string(), n))
        .collect();
    for part in mapping.all_parts() {
        if let Some((word, n)) = minted_part_number(part.id.as_str()) {
            let held = next.entry(word.to_string()).or_insert(1);
            *held = (*held).max(n.saturating_add(1));
        }
    }

    let mut overrides = BTreeMap::new();
    for (id, entry) in &mapping.overrides {
        overrides.insert(id.to_string(), write_override(entry)?);
    }

    let doc = Map2d6Doc {
        format: MAP2D6_FORMAT,
        sample_diameter: mapping.sample_diameter,
        canvas: mapping.canvas,
        next,
        start: write_start(&mapping.start)?,
        flow: write_flow(&mapping.flow),
        parts: mapping
            .parts
            .iter()
            .map(write_part)
            .collect::<Result<Vec<_>, _>>()?,
        overrides,
    };
    let mut json = serde_json::to_string_pretty(&doc)
        .map_err(|e| Map2d6Error::NotWritable {
            reason: e.to_string(),
        })?;
    json.push('\n');
    Ok(json)
}

fn write_part(part: &Part) -> Result<PartDoc, Map2d6Error> {
    let id = part.id.as_str();
    if part.lamp != LampRef::default() {
        return Err(no_lamp_facts(&format!("part {id:?}")));
    }
    let kind = match &part.kind {
        PartKind::Shape(Shape::Path(path)) => KindDoc::Path(PathDoc {
            d: write_path_data(&path.commands)
                .map_err(|reason| Map2d6Error::NotWritable { reason })?,
            count: path.count,
            reversed: path.reversed,
            align: path.align,
            split: path.split,
        }),
        PartKind::Shape(Shape::Polygon(polygon)) => {
            finite(id, polygon.points.as_flattened())?;
            KindDoc::Polygon(polygon.clone())
        }
        PartKind::Shape(Shape::Circle(circle)) => KindDoc::Circle(write_circle(id, circle)?),
        PartKind::Shape(Shape::Grid(grid)) => {
            finite(id, &[grid.origin[0], grid.origin[1], grid.pitch])?;
            KindDoc::Grid(grid.clone())
        }
        PartKind::Shape(Shape::FilledPolygon(filled)) => {
            finite(id, filled.points.as_flattened())?;
            finite(
                id,
                &[filled.pitch, filled.angle_deg, filled.origin[0], filled.origin[1]],
            )?;
            KindDoc::FilledPolygon(filled.clone())
        }
        PartKind::Shape(Shape::Points(points)) => {
            finite(id, points.at.as_flattened())?;
            KindDoc::Points(PointsDoc {
                at: points.at.clone(),
            })
        }
        PartKind::Group { parts } => KindDoc::Group(GroupDoc {
            parts: parts
                .iter()
                .map(write_part)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        PartKind::Repeat(repeat) => {
            finite(id, &repeat.center)?;
            KindDoc::Repeat(RepeatDoc {
                center: repeat.center,
                count: repeat.count,
                part: alloc::boxed::Box::new(write_part(&repeat.part)?),
            })
        }
    };
    Ok(PartDoc {
        id: id.to_string(),
        name: part.name.clone(),
        start: write_start(&part.start)?,
        flow: write_flow(&part.flow),
        kind,
    })
}

fn write_circle(id: &str, circle: &Circle) -> Result<CircleDoc, Map2d6Error> {
    finite(id, &[circle.center[0], circle.center[1], circle.start_angle_deg])?;
    let mut next_ring = circle.next_ring.max(1);
    let mut rings = Vec::with_capacity(circle.rings.len());
    for ring in &circle.rings {
        finite(id, &[ring.radius])?;
        if let Some(n) = minted_ring_number(ring.key.as_str()) {
            next_ring = next_ring.max(n.saturating_add(1));
        }
        rings.push(RingDoc {
            key: ring.key.to_string(),
            radius: ring.radius,
            count: ring.count,
        });
    }
    Ok(CircleDoc {
        center: circle.center,
        start_angle_deg: circle.start_angle_deg,
        dir: circle.dir,
        drawn_from: match circle.drawn_from {
            RingOrder::OuterFirst => DrawnFromDoc::Outside,
            RingOrder::InnerFirst => DrawnFromDoc::Inside,
        },
        split: circle.split,
        next_ring,
        rings,
    })
}

/// A start: Keep is left out.
fn write_start(start: &Start) -> Result<StartDoc, Map2d6Error> {
    Ok(StartDoc {
        output: match &start.output {
            OutputOp::Keep => None,
            op => Some(write_output_op(op)),
        },
        port: match &start.port {
            PortOp::Keep => None,
            op => Some(write_port_op(op)?),
        },
        channel: match start.channel {
            ChannelOp::Keep => None,
            op => Some(write_channel_op(op)),
        },
    })
}

/// An override: Keep is written (`"keep"`), absent is left out.
fn write_override(entry: &Override) -> Result<OverrideDoc, Map2d6Error> {
    Ok(OverrideDoc {
        output: entry.output.as_ref().map(write_output_op),
        port: entry.port.as_ref().map(write_port_op).transpose()?,
        channel: entry.channel.map(write_channel_op),
        off: entry.off,
        direction: entry.direction,
        offset: entry.offset,
    })
}

fn write_output_op(op: &OutputOp) -> OutputOpDoc {
    match op {
        OutputOp::Keep => OutputOpDoc::Keep,
        OutputOp::Set(name) => OutputOpDoc::Set(name.as_str().to_string()),
    }
}

fn write_port_op(op: &PortOp) -> Result<PortOpDoc, Map2d6Error> {
    Ok(match *op {
        PortOp::Keep => PortOpDoc::Keep,
        PortOp::Set { key, plus: 0 } => PortOpDoc::Set(key.0),
        PortOp::Set { key, plus } => {
            return Err(Map2d6Error::NotWritable {
                reason: format!(
                    "port {key} plus {plus} is a step only a flow renders; a saved start sets one port"
                ),
            });
        }
        PortOp::Add(n) => PortOpDoc::Add(n),
    })
}

fn write_channel_op(op: ChannelOp) -> ChannelOpDoc {
    match op {
        ChannelOp::Keep => ChannelOpDoc::Keep,
        ChannelOp::Set(amount) => ChannelOpDoc::Set(write_amount(amount)),
        ChannelOp::Add(amount) => ChannelOpDoc::Add(write_amount(amount)),
    }
}

fn write_amount(amount: Amount) -> AmountDoc {
    match amount.unit {
        Unit::Lamps => AmountDoc::Lamps(amount.n),
        Unit::Channels => AmountDoc::Channels(amount.n),
    }
}

fn write_flow(flow: &Flow) -> FlowDoc {
    match flow {
        Flow::EndToEnd => FlowDoc::EndToEnd,
        Flow::OnePerPort { from } => FlowDoc::OnePerPort { from: from.0 },
    }
}

fn no_lamp_facts(whose: &str) -> Map2d6Error {
    Map2d6Error::NotWritable {
        reason: format!(
            "{whose} sets lamp facts (colors, order, a lamp spec); format 6 saves none"
        ),
    }
}

/// Refuse a number no JSON text reads back to.
fn finite(whose: &str, numbers: &[f32]) -> Result<(), Map2d6Error> {
    match numbers.iter().find(|n| !n.is_finite()) {
        Some(n) => Err(Map2d6Error::NotWritable {
            reason: format!("{whose}: {n} is not a finite number"),
        }),
        None => Ok(()),
    }
}
