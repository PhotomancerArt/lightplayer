//! Each node's face for the view tree, in card-local world units: a
//! fixture's drawing, an output's port rows, a shader's stand-in, a plain
//! card's kind. Moved here from the page, with every decision it made
//! (what's lit, what's dimmed, which ports are tinted, which pixels are
//! ringed).

use crate::domain::geom::{Vec2, along_polyline, polyline_length};
use crate::domain::node::{Node, NodeId, NodeKind};
use crate::domain::object::{Object, ObjectId, ObjectShape};
use crate::domain::placement::Frame;
use crate::domain::port_runs::PortRun;
use crate::domain::target::Target;
use crate::ux::card_geometry::{CARD_WIDTH, card_row};
use crate::ux::editor::{Editor, Tool};
use crate::ux::patching::PairEnd;
use crate::ux::pixel_grid::PixelGrid;
use crate::ux::port_grouping::PortGrouping;
use crate::ux::ui::{
    Emphasis, FixtureLook, UiFixtureFace, UiNodeFace, UiObject, UiOutputFace, UiPlainFace,
    UiPortRow, UiRun, UiShape, UiSourceFace, UiWire,
};

/// What every face asks about the selection and the pointer, worked out
/// once for all of them.
pub struct FaceContext {
    hover: Option<Target>,
    context: Option<Target>,
    open: Vec<NodeId>,
    /// Ports the selection lands on.
    tinted: Vec<Target>,
    /// Pixels ringed: each selected lamp's.
    marked: Vec<Target>,
}

impl Editor {
    pub fn face_context(&self) -> FaceContext {
        let p = &self.project;
        let mut tinted: Vec<Target> = Vec::new();
        for t in &self.selection {
            for port in p.ports_of(t) {
                if !tinted.contains(&port) {
                    tinted.push(port);
                }
            }
        }
        FaceContext {
            hover: self.hover_target(),
            context: self.context(),
            open: self.open_fixtures(),
            tinted,
            marked: self.marked_pixels(),
        }
    }

    /// What a node shows on the canvas; a module only its frame.
    pub fn view_face(&self, n: &Node, cx: &FaceContext) -> Option<UiNodeFace> {
        Some(match &n.kind {
            NodeKind::Module { .. } => return None,
            NodeKind::Fixture(_) => UiNodeFace::Fixture(self.fixture_face(&n.id, cx)),
            NodeKind::Output(_) => UiNodeFace::Output(self.output_face(&n.id, cx)),
            NodeKind::Source(s) => UiNodeFace::Source(UiSourceFace {
                source: n.id.clone(),
                kind: s.kind_words(),
                writes: s.writes.to_string(),
                pattern: s.pattern,
            }),
            NodeKind::Other(o) => UiNodeFace::Plain(UiPlainFace {
                kind: o.kind_words(),
            }),
        })
    }

    fn fixture_face(&self, id: &NodeId, cx: &FaceContext) -> UiFixtureFace {
        let p = &self.project;
        let frame = p.frame(id);
        let local = |v: Vec2| frame.to_local(v);
        let context = &cx.context;
        let objects = p
            .objects_in(id)
            .into_iter()
            .map(|o| {
                let chain = p.chain(&p.object_target(&o.id));
                let emphasis = if chain.iter().any(|t| self.selection.contains(t)) {
                    Emphasis::Lit
                } else if context.as_ref().is_some_and(|c| !chain.contains(c)) {
                    Emphasis::Dimmed
                } else {
                    Emphasis::Full
                };
                let mut ui = ui_object(o, emphasis, frame);
                ui.jumpers = p
                    .path_jumpers(&o.id.component)
                    .iter()
                    .map(|g| *g as usize)
                    .collect();
                ui
            })
            .collect();
        let not_shown = p.fixture(id).and_then(|f| f.unsupported.as_ref()).map(|u| {
            format!(
                "The Graph can't show {} yet. Edit it in {}.",
                u.what, u.edit_in
            )
        });
        let working = cx.open.contains(id)
            || self.selection.contains(&Target::Node(id.clone()))
            || self.tool != Tool::Select;
        if !working {
            return UiFixtureFace {
                fixture: id.clone(),
                look: FixtureLook::Live,
                objects,
                wires: Vec::new(),
                samples: None,
                render_size: None,
                not_shown,
            };
        }
        let dp = p.data_path(id);
        let mut wires: Vec<UiWire> = dp
            .links
            .iter()
            .map(|(a, b)| UiWire::Link {
                from: local(*a),
                to: local(*b),
            })
            .collect();
        wires.extend(dp.entries.iter().map(|e| UiWire::Entry {
            at: local(e.at),
            along: e.along,
        }));
        UiFixtureFace {
            fixture: id.clone(),
            look: FixtureLook::Editing,
            objects,
            wires,
            samples: p.visual_source(id),
            render_size: p.fixture(id).and_then(|f| f.render_size),
            not_shown,
        }
    }

    /// A port's runs as the card groups them (`port_grouping.rs`).
    fn grouped_runs(&self, id: &NodeId, key: &str, size: u32) -> Vec<PortRun> {
        let runs = self.project.port_runs(id, key);
        match self.grouping {
            PortGrouping::Objects => runs,
            PortGrouping::Pixels => runs.into_iter().filter(|r| r.object.is_some()).collect(),
            PortGrouping::Groups => runs
                .into_iter()
                .flat_map(|r| match r.object {
                    Some(_) => vec![r],
                    None => {
                        let mut out = Vec::new();
                        let mut at = r.pixels.start;
                        while at < r.pixels.end {
                            let end = (at + size.max(1)).min(r.pixels.end);
                            out.push(PortRun {
                                pixels: at..end,
                                object: None,
                            });
                            at = end;
                        }
                        out
                    }
                })
                .collect(),
        }
    }

    fn output_face(&self, id: &NodeId, cx: &FaceContext) -> UiOutputFace {
        let p = &self.project;
        let patch = p.effective_patch();
        let Some(output) = p.output(id) else {
            return UiOutputFace {
                output: id.clone(),
                ports: Vec::new(),
            };
        };
        let (tinted, hover) = (&cx.tinted, &cx.hover);
        // A run is lit while its object, or anything round it, is hovered
        // or selected.
        let lit_by = |object: &ObjectId| {
            let t = p.object_target(object);
            hover
                .iter()
                .chain(self.selection.iter())
                .any(|s| p.is_ancestor_or_self(s, &t) || p.is_ancestor_or_self(&t, s))
        };
        let marked = &cx.marked;
        // The rows span the card as wide as it was placed.
        let width = p.node_rect(id).map_or(CARD_WIDTH, |r| r.width());
        // While a patch is armed, the candidate in focus is lit (an object)
        // or ringed (a free segment), as a hover would be.
        let candidate = self.focused_candidate();
        let size = self.segment_size();
        let ports = output
            .ports
            .iter()
            .enumerate()
            .filter_map(|(i, port)| {
                let row = card_row(i, width);
                let port_runs = self.grouped_runs(id, &port.key, size);
                let pixels = p.port_pixels(id, &port.key);
                let starts = port_runs.iter().skip(1).map(|r| r.pixels.start).collect();
                let grid = PixelGrid::in_row(row, pixels, starts);
                let runs = port_runs
                    .into_iter()
                    .map(|run| UiRun {
                        rects: grid.run_outline(run.pixels.clone()),
                        emphasis: match &run.object {
                            Some(o) if lit_by(o) => Emphasis::Lit,
                            Some(o) if candidate == Some(PairEnd::Object(o.clone())) => {
                                Emphasis::Lit
                            }
                            Some(_) => Emphasis::Full,
                            None => Emphasis::Quiet,
                        },
                    })
                    .collect();
                let target = Target::Port {
                    output: id.clone(),
                    key: port.key.clone(),
                };
                let mut overlaps = Vec::new();
                let mut shared = 0;
                for x in patch
                    .overlaps
                    .iter()
                    .filter(|x| &x.output == id && x.port == port.key)
                {
                    overlaps.extend(grid.run_outline(x.pixels.clone()));
                    shared += x.pixels.len();
                }
                let mut marked: Vec<u32> = marked
                    .iter()
                    .filter_map(|t| match t {
                        Target::Pixel { output, key, index }
                            if output == id && *key == port.key =>
                        {
                            Some(*index)
                        }
                        _ => None,
                    })
                    .collect();
                if let Some(PairEnd::Segment {
                    output,
                    port: key,
                    pixels,
                }) = &candidate
                    && output == id
                    && *key == port.key
                {
                    marked.extend(pixels.clone().filter(|&px| px < grid.count));
                }
                Some(UiPortRow {
                    key: port.key.clone(),
                    name: port.name.clone(),
                    fill: format!("{}/{pixels} px", patch.used(id, &port.key)),
                    row,
                    tinted: tinted.contains(&target),
                    grid,
                    runs,
                    marked,
                    overlaps,
                    overlap: (shared > 0).then(|| format!("overlap {shared}")),
                })
            })
            .collect();
        UiOutputFace {
            output: id.clone(),
            ports,
        }
    }
}

/// An object as its fixture's face draws it, moved from world units to the
/// fixture's own by `origin`.
pub(crate) fn ui_object(o: &Object, emphasis: Emphasis, frame: Frame) -> UiObject {
    let local = |v: Vec2| frame.to_local(v);
    let shape = ui_shape(&o.shape, local, |r| r / frame.scale);
    // Only the first two lamps matter here: where the data enters, and
    // which way a ring runs.
    let lamps: Vec<Vec2> = o.lamps.iter().take(2).map(|l| local(l.pos)).collect();
    UiObject {
        id: o.id.clone(),
        arrow: direction_arrow(&shape, &lamps),
        shape,
        emphasis,
        first_lamp: lamps.first().copied(),
        jumpers: Vec::new(),
    }
}

/// A drawn shape as the page draws it: `point` moves a point, `length` a
/// radius, into the face's (or the screen's) units.
pub(crate) fn ui_shape(
    shape: &ObjectShape,
    point: impl Fn(Vec2) -> Vec2,
    length: impl Fn(f64) -> f64,
) -> UiShape {
    match shape {
        ObjectShape::Polyline { points } => {
            UiShape::Polyline(points.iter().map(|p| point(*p)).collect())
        }
        ObjectShape::Ring { center, radius } => UiShape::Ring {
            center: point(*center),
            radius: length(*radius),
        },
        ObjectShape::Rings { center, radii } => UiShape::Rings {
            center: point(*center),
            radii: radii.iter().map(|r| length(*r)).collect(),
        },
        ObjectShape::Filled { outline } => {
            UiShape::Outline(outline.iter().map(|p| point(*p)).collect())
        }
    }
}

/// Where a small arrow shows which way the data runs: halfway along a
/// path, just after the top on a ring (between its first two lamps). A
/// point and a unit direction.
pub(crate) fn direction_arrow(shape: &UiShape, lamps: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let (at, dir) = match shape {
        UiShape::Polyline(points) => along_polyline(points, polyline_length(points) / 2.0)?,
        // A ring's, a ring set's and a lattice's: between the first two
        // lamps.
        UiShape::Ring { .. } | UiShape::Rings { .. } | UiShape::Outline(_) => match lamps {
            [first, second, ..] => (first.lerp(*second, 0.5), *second - *first),
            _ => return None,
        },
    };
    let len = dir.length();
    (len > 1e-9).then(|| (at, dir * (1.0 / len)))
}
