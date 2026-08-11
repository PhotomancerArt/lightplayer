//! Patch-subject pulse (Q27/D43): selection → wire lamp spans → the
//! `highlight` Debug slot.
//!
//! When a patching surface selects a subject — an object instance, a lamp
//! range, a port — the physical rig should say which lamps that is: the
//! engine pulses them over the live frame (sim and hardware alike, because
//! the slot rides the ordinary overlay to wherever the engine runs). This
//! module is the CONTROLLER seam: it owns the subject vocabulary and the
//! subject→wire-span arithmetic, and deliberately knows nothing about which
//! page drove the selection — the patching UI is being re-housed inside the
//! mapping editor, and the seam must outlive the move.
//!
//! Subjects come in the two spaces a patch speaks (D32v: the surface is a
//! mapper between them):
//!
//! - **fixture space** — a producing node's own lamp numbering. Object
//!   instances and range selections both reduce to this (an instance IS a
//!   derived lamp range, per the instance table). Only core can map it to
//!   the wire: the placements are the resolver's, not re-derivable
//!   client-side.
//! - **wire space** — a lamp range on one output. Ports live here already:
//!   ports are UI grain (Q21), so a port selection arrives as the span its
//!   port table renders, and "the whole output" is simply no range.
//!
//! The written value is the `highlight` slot's microformat — inclusive lamp
//! ranges, `"0-29,45"` — chosen so the Debug section shows a legible,
//! hand-editable string rather than an opaque blob.

use core::any::Any;

use lpc_model::NodeId;
use lpc_wire::WireOutputPlacement;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Samples per RGB lamp — the unit a published frame's extent is stated in.
const SAMPLES_PER_LAMP: u32 = 3;

/// What a patching surface selected, in the space it naturally knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchPulseSubject {
    /// A producing fixture, or a `(start, lamps)` range of one, in its OWN
    /// numbering. `None` = the whole fixture. Instances and range-grain
    /// selections both arrive here.
    Fixture {
        node: NodeId,
        range: Option<(u32, u32)>,
    },
    /// A `(start, lamps)` wire range on one output. `None` = the whole
    /// wire. Ports are UI grain (Q21): a port selection is the span its
    /// port table already renders.
    Output {
        node: NodeId,
        range: Option<(u32, u32)>,
    },
}

/// Pulse the lamps behind one patch subject on the live sim/hardware —
/// or, with no subject, stop pulsing.
///
/// Dispatched to `ProjectController::NODE_ID`; the controller maps the
/// subject through the published placements and writes each affected
/// output's `highlight` Debug slot (clearing outputs that stop being
/// involved). A Debug slot, so nothing dirties, nothing saves, and a
/// forgotten pulse dies with the project unload.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchPulseOp {
    /// The subject to pulse, or `None` to clear the pulse everywhere.
    pub subject: Option<PatchPulseSubject>,
}

impl ControllerOp for PatchPulseOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Pulse selection",
            "Pulse the selected lamps on the live output.",
            ActionPriority::Secondary,
        )
    }

    fn action_class(&self) -> ActionClass {
        // Editor foreground like the slot-level edit ops: a stale pulse
        // pointing at the previous selection is actively misleading.
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// One output's wire, as the pulse arithmetic reads it.
pub(crate) struct PulseWire<'a> {
    pub node: NodeId,
    /// The runs the wire was cut into, in planning order.
    pub placements: &'a [WireOutputPlacement],
    /// The wire's extent in lamps — the published frame's, or as far as the
    /// runs reach (same rule as the bay derivation's `wire_lamps`).
    pub lamps: u32,
}

/// The `highlight` value each involved output should carry for `subject`.
///
/// Outputs the subject does not touch are absent — the CALLER clears them.
/// An output the subject touches with an empty intersection is absent too:
/// "no lamps" and "not involved" write the same clear.
pub(crate) fn pulse_highlights(
    subject: &PatchPulseSubject,
    wires: &[PulseWire<'_>],
) -> Vec<(NodeId, String)> {
    let mut writes = Vec::new();
    for wire in wires {
        let spans = match subject {
            PatchPulseSubject::Fixture { node, range } => {
                fixture_wire_spans(*node, *range, wire.placements)
            }
            PatchPulseSubject::Output { node, range } if *node == wire.node => match range {
                Some((start, lamps)) => Vec::from([(*start, *lamps)]),
                None => Vec::from([(0, wire.lamps)]),
            },
            PatchPulseSubject::Output { .. } => Vec::new(),
        };
        let text = highlight_text(spans);
        if !text.is_empty() {
            writes.push((wire.node, text));
        }
    }
    writes
}

/// A fixture-space lamp range, mapped onto one wire through its runs.
///
/// Each run carries `source_lamp..source_lamp+lamps` of the producer to
/// `wire_lamp..`, forward or reversed. The subject range intersects each
/// run in SOURCE space; a reversed run lays the producer's high end at the
/// run's low wire end, so the surviving piece re-bases from the tail — the
/// same arithmetic as the bay derivation's `clip`, in the other direction.
fn fixture_wire_spans(
    node: NodeId,
    range: Option<(u32, u32)>,
    placements: &[WireOutputPlacement],
) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    for run in placements {
        if run.node != node {
            continue;
        }
        let run_end = run.source_lamp.saturating_add(run.lamps);
        let (lo, hi) = match range {
            Some((start, lamps)) => (
                start.max(run.source_lamp),
                start.saturating_add(lamps).min(run_end),
            ),
            None => (run.source_lamp, run_end),
        };
        if hi <= lo {
            continue;
        }
        let head = lo - run.source_lamp;
        let tail = run_end - hi;
        let wire_start = if run.reversed {
            run.wire_lamp.saturating_add(tail)
        } else {
            run.wire_lamp.saturating_add(head)
        };
        spans.push((wire_start, hi - lo));
    }
    spans
}

/// Render spans as the `highlight` microformat: sorted, merged, inclusive.
fn highlight_text(mut spans: Vec<(u32, u32)>) -> String {
    spans.retain(|(_, lamps)| *lamps > 0);
    spans.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(spans.len());
    for (start, lamps) in spans {
        let end = start.saturating_add(lamps);
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let mut text = String::new();
    for (index, (start, end)) in merged.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        let last = end - 1;
        if *start == last {
            text.push_str(&format!("{start}"));
        } else {
            text.push_str(&format!("{start}-{last}"));
        }
    }
    text
}

/// A wire's extent in lamps: the published frame's when one has arrived,
/// else as far as the runs reach — the bay derivation's `wire_lamps` rule.
pub(crate) fn wire_extent_lamps(
    frame: Option<&crate::UiControlProductPreview>,
    placements: &[WireOutputPlacement],
) -> u32 {
    if let Some(frame) = frame {
        let lamps = frame.extent.sample_count() / SAMPLES_PER_LAMP;
        if lamps > 0 {
            return lamps;
        }
    }
    placements
        .iter()
        .map(|run| run.wire_lamp.saturating_add(run.lamps))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The peach's cut (the bay derivation's fixture): body 0–21 forward at
    /// 0, leaf at 22, body 22–43 laid back down the strand from 34.
    fn peach() -> Vec<WireOutputPlacement> {
        Vec::from([
            run(2, 0, 44, 0, 22, false),
            run(3, 0, 12, 22, 12, false),
            run(2, 22, 44, 34, 22, true),
        ])
    }

    fn run(
        node: u32,
        source_lamp: u32,
        source_lamps: u32,
        wire_lamp: u32,
        lamps: u32,
        reversed: bool,
    ) -> WireOutputPlacement {
        WireOutputPlacement {
            node: NodeId::new(node),
            output: 0,
            source_lamp,
            source_lamps,
            wire_lamp,
            lamps,
            reversed,
        }
    }

    fn wires(placements: &[WireOutputPlacement]) -> Vec<PulseWire<'_>> {
        Vec::from([PulseWire {
            node: NodeId::new(1),
            placements,
            lamps: 56,
        }])
    }

    #[test]
    fn a_whole_fixture_pulses_every_run_it_has_on_the_wire() {
        let placements = peach();
        let writes = pulse_highlights(
            &PatchPulseSubject::Fixture {
                node: NodeId::new(2),
                range: None,
            },
            &wires(&placements),
        );

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("0-21,34-55"))]),
            "both body stretches, merged and inclusive"
        );
    }

    #[test]
    fn a_fixture_range_intersects_each_run_in_source_space() {
        let placements = peach();
        // Lamps 20..24 of the body: 20–21 sit at the end of the forward
        // run, 22–23 at the START of the reversed one — which lays the
        // producer's high end low, so they land at the run's far END.
        let writes = pulse_highlights(
            &PatchPulseSubject::Fixture {
                node: NodeId::new(2),
                range: Some((20, 4)),
            },
            &wires(&placements),
        );

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("20-21,54-55"))]),
            "the reversed run re-bases from its tail"
        );
    }

    #[test]
    fn an_instance_range_inside_one_run_maps_straight_through() {
        let placements = peach();
        let writes = pulse_highlights(
            &PatchPulseSubject::Fixture {
                node: NodeId::new(3),
                range: Some((4, 4)),
            },
            &wires(&placements),
        );

        assert_eq!(writes, Vec::from([(NodeId::new(1), String::from("26-29"))]));
    }

    #[test]
    fn a_port_selection_is_already_a_wire_span() {
        let placements = peach();
        let writes = pulse_highlights(
            &PatchPulseSubject::Output {
                node: NodeId::new(1),
                range: Some((40, 16)),
            },
            &wires(&placements),
        );

        assert_eq!(writes, Vec::from([(NodeId::new(1), String::from("40-55"))]));
    }

    #[test]
    fn a_whole_output_pulses_its_extent_and_only_its_own_wire() {
        let placements = peach();
        let mut all = wires(&placements);
        all.push(PulseWire {
            node: NodeId::new(9),
            placements: &[],
            lamps: 10,
        });
        let writes = pulse_highlights(
            &PatchPulseSubject::Output {
                node: NodeId::new(1),
                range: None,
            },
            &all,
        );

        assert_eq!(writes, Vec::from([(NodeId::new(1), String::from("0-55"))]));
    }

    #[test]
    fn a_subject_off_every_wire_writes_nothing() {
        let placements = peach();
        let writes = pulse_highlights(
            &PatchPulseSubject::Fixture {
                node: NodeId::new(9),
                range: None,
            },
            &wires(&placements),
        );

        assert!(writes.is_empty(), "the caller clears what nothing names");
    }

    #[test]
    fn highlight_text_merges_touching_spans() {
        assert_eq!(
            highlight_text(Vec::from([(34, 22), (0, 22), (22, 12)])),
            "0-55",
            "the peach's full cover reads as one range"
        );
        assert_eq!(highlight_text(Vec::from([(7, 1), (0, 0)])), "7");
        assert_eq!(highlight_text(Vec::new()), "");
    }

    #[test]
    fn patch_pulse_is_editor_foreground_class() {
        let op = PatchPulseOp { subject: None };
        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
    }
}
