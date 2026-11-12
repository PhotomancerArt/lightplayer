//! The Graph's selection, on the board (plan Q5, D3): the same pulse the
//! Patch view sends, through each output's `highlight` Debug slot, with no
//! firmware change.
//!
//! What the Graph selected becomes [`PatchPulseSubject`]s, in the language
//! the Patch view's matrix gives each kind
//! ([`crate::UiPatchTarget::pulse_language`]):
//!
//! - **an object** (a path, a ring) chases: its lamps in object order, so
//!   the board says which way it runs. Several objects together breathe
//!   (a chase is one object's).
//! - **a fixture**, **a lamp**, **an output**, **a port** or **a pixel**
//!   breathes.
//! - a module says nothing on its own (as in Patch).
//!
//! An object or a lamp counts in its fixture's own lamp numbering (the
//! Graph resolves lamps in the product's order),
//! which core maps through the board's own placements; an output, a port
//! or a pixel counts on the output's wire.
//!
//! **On a board whose fixtures have the `selection` slot** (plan M4b) the
//! fixture side is the Graph's own rules ([`board_selection`]): each fixture
//! is told what to do to its own lamps — every object under the selection
//! chases, a lamp breathes, every other lamp in every fixture dims — and
//! only an output, a port or a pixel goes through `highlight`. A board
//! without the slot keeps [`highlight_of`].
//!
//! **While a path or a circle is drawn** ([`board_draft`]) the board shows
//! the draft as if it were drawn, in the selection's pattern: it isn't in
//! the board's fixtures yet (nothing is saved mid-drawing), so it chases on
//! the output pixels it will land on, through `highlight`, and every
//! fixture dims. Moving the pointer moves the red tail along the LEDs.

use std::collections::BTreeMap;

use lpa_graph::{NodeId, Project, Target};

use crate::{PatchPulseLamps, PatchPulseLanguage, PatchPulseOp, PatchPulseSubject};

/// The pulse for `selection` (`nodes`: the Graph's nodes as Studio's).
pub(crate) fn highlight_of(
    project: &Project,
    nodes: &BTreeMap<NodeId, lpc_model::NodeId>,
    selection: &[Target],
) -> PatchPulseOp {
    let placed = project.placed();
    let mut subjects = Vec::new();
    let mut objects = Vec::new();
    for t in selection {
        match t {
            Target::Node(id) => {
                let Some(&node) = nodes.get(id) else { continue };
                let lamps = if project.fixture(id).is_some() {
                    PatchPulseLamps::Fixture { node, range: None }
                } else if project.output(id).is_some() {
                    PatchPulseLamps::Output { node, range: None }
                } else {
                    continue;
                };
                subjects.push(breath(lamps));
            }
            Target::Port { output, key }
            | Target::Pixel { output, key, .. }
            | Target::Span { output, key, .. } => {
                let (Some(&node), Some(start)) = (
                    nodes.get(output),
                    project.output(output).and_then(|o| o.port_start(key)),
                ) else {
                    continue;
                };
                let range = match t {
                    Target::Pixel { index, .. } => (start + index, 1),
                    Target::Span { .. } => match project.span_of(t) {
                        Some(s) => (start + s.pixels.start, s.pixels.len() as u32),
                        None => continue,
                    },
                    _ => (start, project.port_pixels(output, key)),
                };
                subjects.push(breath(PatchPulseLamps::Output {
                    node,
                    range: Some(range),
                }));
            }
            Target::Lamp {
                segment: object,
                index,
            } => {
                if let Some((node, first, _)) = fixture_lamps(project, nodes, object) {
                    subjects.push(breath(PatchPulseLamps::Fixture {
                        node,
                        range: Some((first + index, 1)),
                    }));
                }
            }
            Target::Part(_) | Target::Generated(_) => {
                // Every object under it, once.
                for (i, (_, o)) in placed.list.iter().enumerate() {
                    if project.chain(&project.tree_target(&o.id)).contains(t)
                        && !objects.contains(&i)
                    {
                        objects.push(i);
                    }
                }
            }
        }
    }
    let language = if objects.len() == 1 && subjects.is_empty() {
        PatchPulseLanguage::Chase
    } else {
        PatchPulseLanguage::Breath
    };
    for i in objects {
        let (_, o) = &placed.list[i];
        if let Some((node, first, lamps)) = fixture_lamps(project, nodes, &o.id) {
            subjects.push(PatchPulseSubject {
                lamps: PatchPulseLamps::Fixture {
                    node,
                    range: Some((first, lamps)),
                },
                language,
            });
        }
    }
    PatchPulseOp { subjects }
}

/// An object's fixture (as Studio's node), where its lamps start in the
/// fixture's own numbering, and how many it has.
fn fixture_lamps(
    project: &Project,
    nodes: &BTreeMap<NodeId, lpc_model::NodeId>,
    object: &lpa_graph::TreeRef,
) -> Option<(lpc_model::NodeId, u32, u32)> {
    let placed = project.placed();
    let i = placed.index_of(object)?;
    let (fixture, o) = &placed.list[i];
    let start = placed.fixture_lamps(fixture)?.start;
    let first = (placed.first_lamp(i) - start) as u32;
    Some((*nodes.get(fixture)?, first, o.lamps.len() as u32))
}

fn breath(lamps: PatchPulseLamps) -> PatchPulseSubject {
    PatchPulseSubject {
        lamps,
        language: PatchPulseLanguage::Breath,
    }
}

/// What the board shows for `selection` on firmware whose fixtures have the
/// `selection` slot: each fixture's text (in its own lamp numbering, the
/// engine's `fixture_selection.rs`), and the outputs' pulse for what is
/// selected on the output side.
pub(crate) struct BoardSelection {
    pub fixtures: Vec<(lpc_model::NodeId, String)>,
    pub outputs: PatchPulseOp,
}

pub(crate) fn board_selection(
    project: &Project,
    nodes: &BTreeMap<NodeId, lpc_model::NodeId>,
    selection: &[Target],
) -> BoardSelection {
    let placed = project.placed();
    // Per fixture: the objects that chase and the lamps that breathe.
    let mut chases: BTreeMap<NodeId, Vec<(u32, u32)>> = BTreeMap::new();
    let mut breaths: BTreeMap<NodeId, Vec<u32>> = BTreeMap::new();
    let mut output_side: Vec<Target> = Vec::new();
    let mut fixture_side = false;
    for t in selection {
        match t {
            Target::Node(id) if project.output(id).is_some() => output_side.push(t.clone()),
            Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => {
                output_side.push(t.clone())
            }
            Target::Lamp {
                segment: object,
                index,
            } => {
                fixture_side = true;
                if let Some(i) = placed.index_of(object) {
                    let (fixture, _) = &placed.list[i];
                    if let Some(start) = placed.fixture_lamps(fixture).map(|r| r.start) {
                        let first = (placed.first_lamp(i) - start) as u32;
                        breaths
                            .entry(fixture.clone())
                            .or_default()
                            .push(first + index);
                    }
                }
            }
            Target::Node(_) | Target::Part(_) | Target::Generated(_) => {
                fixture_side = true;
                for (i, (fixture, o)) in placed.list.iter().enumerate() {
                    let under = match t {
                        Target::Node(id) => fixture == id,
                        _ => project.chain(&project.tree_target(&o.id)).contains(t),
                    };
                    if !under {
                        continue;
                    }
                    let Some(start) = placed.fixture_lamps(fixture).map(|r| r.start) else {
                        continue;
                    };
                    let first = (placed.first_lamp(i) - start) as u32;
                    let span = (first, o.lamps.len() as u32);
                    let list = chases.entry(fixture.clone()).or_default();
                    if span.1 > 0 && !list.contains(&span) {
                        list.push(span);
                    }
                }
            }
        }
    }
    let fixtures = if fixture_side {
        project
            .fixtures()
            .iter()
            .filter_map(|n| {
                let node = *nodes.get(&n.id)?;
                let mut parts = Vec::new();
                if let Some(spans) = chases.get(&n.id) {
                    parts.push(format!("chase:{}", spans_text(spans)));
                }
                if let Some(lamps) = breaths.get(&n.id) {
                    let spans: Vec<(u32, u32)> = lamps.iter().map(|l| (*l, 1)).collect();
                    parts.push(format!("breath:{}", spans_text(&spans)));
                }
                if parts.is_empty() {
                    parts.push("dim".to_string());
                }
                Some((node, parts.join(";")))
            })
            .collect()
    } else {
        Vec::new()
    };
    BoardSelection {
        fixtures,
        outputs: highlight_of(project, nodes, &output_side),
    }
}

/// What the board shows while a draft is drawn: `drafted`, the project
/// with the draft added and the part it became (`Editor::drafted`), chases
/// on the pixels of `now` (the board as it is) its lamps will land on, in
/// the draft's own order, through each output's `highlight`; with
/// `has_slot`, every fixture dims. A draft that lands nowhere only dims.
pub(crate) fn board_draft(
    now: &Project,
    drafted: Option<&(Project, lpa_graph::PartRef)>,
    nodes: &BTreeMap<NodeId, lpc_model::NodeId>,
    has_slot: bool,
) -> BoardSelection {
    let fixtures = if has_slot {
        now.fixtures()
            .iter()
            .filter_map(|n| Some((*nodes.get(&n.id)?, "dim".to_string())))
            .collect()
    } else {
        Vec::new()
    };
    let mut subjects = Vec::new();
    if let Some((p, draft)) = drafted {
        let placed = p.placed();
        let mut runs: Vec<(usize, u32, &lpa_graph::PixelRun)> = p
            .effective_patch()
            .pixel_runs
            .iter()
            .filter(|r| r.segment.part() == *draft)
            .filter_map(|r| Some((placed.index_of(&r.segment)?, r.lamps.start, r)))
            .collect();
        runs.sort_by_key(|(i, first, _)| (*i, *first));
        // Per output, in the order the draft walks them.
        let mut wires: Vec<(NodeId, Vec<(u32, u32, bool)>)> = Vec::new();
        for (_, _, r) in runs {
            let (Some(start), count) = (
                now.output(&r.output).and_then(|o| o.port_start(&r.port)),
                now.port_pixels(&r.output, &r.port),
            ) else {
                continue;
            };
            // Only the pixels the board has now.
            let lit: Vec<u32> = r
                .lamps
                .clone()
                .map(|lamp| r.pixel(lamp))
                .filter(|px| *px < count)
                .collect();
            let (Some(low), lamps) = (lit.iter().min(), lit.len() as u32) else {
                continue;
            };
            let run = (start + low, lamps, r.reversed);
            match wires.iter_mut().find(|(o, _)| *o == r.output) {
                Some((_, list)) => list.push(run),
                None => wires.push((r.output.clone(), vec![run])),
            }
        }
        for (output, runs) in wires {
            if let Some(&node) = nodes.get(&output) {
                subjects.push(PatchPulseSubject {
                    lamps: PatchPulseLamps::OutputRuns { node, runs },
                    language: PatchPulseLanguage::Chase,
                });
            }
        }
    }
    BoardSelection {
        fixtures,
        outputs: PatchPulseOp { subjects },
    }
}

/// `(first, lamps)` spans as the slot writes them: `0-11,24-30`.
fn spans_text(spans: &[(u32, u32)]) -> String {
    spans
        .iter()
        .map(|(first, n)| {
            if *n == 1 {
                first.to_string()
            } else {
                format!("{first}-{}", first + n - 1)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}
