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
//! lab resolves lamps in the product's order, `lab-mapping-fit-tests`),
//! which core maps through the board's own placements; an output, a port
//! or a pixel counts on the output's wire.

use std::collections::BTreeMap;

use lab_mapping_model::{NodeId, Project, Target};

use crate::{PatchPulseLamps, PatchPulseLanguage, PatchPulseOp, PatchPulseSubject};

/// The pulse for `selection` (`nodes`: the lab's nodes as Studio's).
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
            Target::Port { output, key } | Target::Pixel { output, key, .. } => {
                let (Some(&node), Some(start)) = (
                    nodes.get(output),
                    project.output(output).and_then(|o| o.port_start(key)),
                ) else {
                    continue;
                };
                let range = match t {
                    Target::Pixel { index, .. } => (start + index, 1),
                    _ => (start, project.port_pixels(output, key)),
                };
                subjects.push(breath(PatchPulseLamps::Output {
                    node,
                    range: Some(range),
                }));
            }
            Target::Lamp { object, index } => {
                if let Some((node, first, _)) = fixture_lamps(project, nodes, object) {
                    subjects.push(breath(PatchPulseLamps::Fixture {
                        node,
                        range: Some((first + index, 1)),
                    }));
                }
            }
            Target::Component(_) | Target::Object(_) => {
                // Every object under it, once.
                for (i, (_, o)) in placed.list.iter().enumerate() {
                    if project.chain(&project.object_target(&o.id)).contains(t)
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
    object: &lab_mapping_model::ObjectId,
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
