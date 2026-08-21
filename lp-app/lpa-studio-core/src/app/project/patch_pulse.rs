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
//! hand-editable string rather than an opaque blob. Since microformat v2 the
//! slot carries TWO light languages, and which one a subject speaks is
//! decided HERE, once: see [`PatchPulseSpace::language`].

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
    /// port table already renders, and a free SEGMENT is the window it was
    /// drawn on.
    Output {
        node: NodeId,
        range: Option<(u32, u32)>,
    },
}

impl PatchPulseSubject {
    /// Which of the two spaces this subject counts in.
    #[must_use]
    pub fn space(&self) -> PatchPulseSpace {
        match self {
            Self::Fixture { .. } => PatchPulseSpace::Fixture,
            Self::Output { .. } => PatchPulseSpace::Wire,
        }
    }

    /// The light language this subject deserves — [`PatchPulseSpace::language`].
    #[must_use]
    pub fn language(&self) -> PatchPulseLanguage {
        self.space().language()
    }
}

/// The space a patch selection counts its lamps in — the classification the
/// language matrix keys on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchPulseSpace {
    /// A producing node's own lamp numbering.
    Fixture,
    /// Lamps on one output's wire.
    Wire,
}

impl PatchPulseSpace {
    /// **THE selection-kind → language matrix (D9)**, and the only place it
    /// is decided: the CLIENT says which space it selected in, core says
    /// which language that space speaks. Nothing downstream may pick.
    ///
    /// | selection kind | space | language |
    /// |---|---|---|
    /// | [`crate::UiPatchTarget::Fixture`] / `Instance` / `Range` / `Cell` (incl. a mapped run) | fixture | CHASE |
    /// | [`crate::UiPatchTarget::Output`] / `Port` / free `Segment` | wire | BREATH |
    /// | [`crate::UiPatchTarget::Module`], nothing | — | no pulse |
    ///
    /// The reason is the question each selection asks. A wire-side one asks
    /// "which strand IS this?", which white breathing answers. A
    /// fixture-side one asks "does this object run the way I think it
    /// does?", which needs DIRECTION — see
    /// `docs/adr/2026-08-10-patch-selection-pulse.md` (Amendment
    /// 2026-08-20). [`crate::UiPatchTarget::pulse_space`] is the target-kind
    /// half of the table.
    #[must_use]
    pub fn language(self) -> PatchPulseLanguage {
        match self {
            Self::Fixture => PatchPulseLanguage::Chase,
            Self::Wire => PatchPulseLanguage::Breath,
        }
    }

    /// The subject for `(node, range)` selected in this space — so a caller
    /// that knows the numbers never has to pick the language by hand.
    #[must_use]
    pub fn subject(self, node: NodeId, range: Option<(u32, u32)>) -> PatchPulseSubject {
        match self {
            Self::Fixture => PatchPulseSubject::Fixture { node, range },
            Self::Wire => PatchPulseSubject::Output { node, range },
        }
    }
}

/// The two light languages the `highlight` slot carries (microformat v2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchPulseLanguage {
    /// v1's bare span list: breathing white over an unordered lamp set.
    Breath,
    /// v2's `chase:` list: ordered, direction-carrying spans in OBJECT
    /// order — blue head, red tail, a white dot sweeping the object's way.
    Chase,
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

/// One piece of a subject on one wire: where it lands, and — for the chase —
/// where it sits in the OBJECT and which way it runs there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PulseSpan {
    /// First lamp of the piece in the SUBJECT's own numbering. Only the
    /// ordering matters; a wire-space subject just repeats `wire_start`.
    source_lamp: u32,
    /// First (lowest) lamp of the piece on the wire.
    wire_start: u32,
    lamps: u32,
    /// The piece's wire lamps DESCEND as the object advances — a strand
    /// plugged in at the far end. Meaningless for the breath, which lights a
    /// set of lamps rather than walking a path.
    reversed: bool,
}

/// The `highlight` value each involved output should carry for `subject`.
///
/// Outputs the subject does not touch are absent — the CALLER clears them.
/// An output the subject touches with an empty intersection is absent too:
/// "no lamps" and "not involved" write the same clear.
///
/// The language is the subject's ([`PatchPulseSpace::language`]), never the
/// caller's: a fixture-side subject writes an ordered `chase:` list, a
/// wire-side one the v1 bare list, byte for byte as before.
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
            PatchPulseSubject::Output { node, range } if *node == wire.node => {
                let (start, lamps) = range.unwrap_or((0, wire.lamps));
                Vec::from([PulseSpan {
                    source_lamp: start,
                    wire_start: start,
                    lamps,
                    reversed: false,
                }])
            }
            PatchPulseSubject::Output { .. } => Vec::new(),
        };
        let text = match subject.language() {
            PatchPulseLanguage::Chase => chase_text(&spans),
            PatchPulseLanguage::Breath => highlight_text(&spans),
        };
        if !text.is_empty() {
            writes.push((wire.node, text));
        }
    }
    writes
}

/// A fixture-space lamp range, mapped onto one wire through its runs, **in
/// object order**.
///
/// Each run carries `source_lamp..source_lamp+lamps` of the producer to
/// `wire_lamp..`, forward or reversed. The subject range intersects each
/// run in SOURCE space; a reversed run lays the producer's high end at the
/// run's low wire end, so the surviving piece re-bases from the tail — the
/// same arithmetic as the bay derivation's `clip`, in the other direction.
///
/// The pieces come out ordered by the SOURCE lamp they start at, because
/// that IS the object's order — the chase's first span must hold the
/// object's lamp 0 however the strand was plugged in, and the placements
/// arrive in the wire's planning order, which is a different story. The
/// sort is stable, so an overlap keeps the planner's word on which run came
/// first.
fn fixture_wire_spans(
    node: NodeId,
    range: Option<(u32, u32)>,
    placements: &[WireOutputPlacement],
) -> Vec<PulseSpan> {
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
        spans.push(PulseSpan {
            source_lamp: lo,
            wire_start,
            lamps: hi - lo,
            reversed: run.reversed,
        });
    }
    spans.sort_by_key(|span| span.source_lamp);
    spans
}

/// Render spans as the v1 `highlight` microformat: sorted, merged,
/// inclusive — the breath lights a SET of lamps, so order and direction are
/// dropped on purpose.
fn highlight_text(spans: &[PulseSpan]) -> String {
    let mut spans: Vec<(u32, u32)> = spans
        .iter()
        .filter(|span| span.lamps > 0)
        .map(|span| (span.wire_start, span.lamps))
        .collect();
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

/// Render spans as the v2 `chase:` microformat: the object's own order,
/// each run's direction written into its range.
///
/// Nothing is sorted or merged here — the spans arrive in object order from
/// [`fixture_wire_spans`] and every join between them is a fact about the
/// wiring the chase exists to show. A REVERSED run serializes as a
/// DESCENDING range (`55-34`), which is how the grammar says "this strand is
/// walked backward"; a one-lamp run is a bare number either way.
///
/// The grammar is the engine's, not ours: `parse_highlight` in
/// `lp-core/lpc-engine/src/nodes/output/output_node.rs`, specified in
/// `docs/adr/2026-08-10-patch-selection-pulse.md` (Amendment 2026-08-20 —
/// microformat v2). The exact bytes are asserted in this module's tests;
/// the parse side is covered by the engine's.
fn chase_text(spans: &[PulseSpan]) -> String {
    let mut text = String::new();
    for span in spans.iter().filter(|span| span.lamps > 0) {
        if !text.is_empty() {
            text.push(',');
        }
        let low = span.wire_start;
        let high = span.wire_start.saturating_add(span.lamps - 1);
        if low == high {
            text.push_str(&format!("{low}"));
        } else if span.reversed {
            text.push_str(&format!("{high}-{low}"));
        } else {
            text.push_str(&format!("{low}-{high}"));
        }
    }
    if text.is_empty() {
        // Empty means CLEAR to the caller and byte-identity to the engine —
        // a bare `chase:` would be neither.
        return text;
    }
    format!("chase:{text}")
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

    /// A forward wire-space piece — what the breath's renderer sees, where
    /// object position and direction carry no meaning.
    fn span(wire_start: u32, lamps: u32) -> PulseSpan {
        PulseSpan {
            source_lamp: wire_start,
            wire_start,
            lamps,
            reversed: false,
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
            Vec::from([(NodeId::new(1), String::from("chase:0-21,55-34"))]),
            "both body stretches in object order, the re-plugged one descending"
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
            Vec::from([(NodeId::new(1), String::from("chase:20-21,55-54"))]),
            "the reversed run re-bases from its tail, and runs backward"
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

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("chase:26-29"))])
        );
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
            highlight_text(&[span(34, 22), span(0, 22), span(22, 12)]),
            "0-55",
            "the peach's full cover reads as one range"
        );
        assert_eq!(highlight_text(&[span(7, 1), span(0, 0)]), "7");
        assert_eq!(highlight_text(&[]), "");
    }

    /// D9, per selection kind — the matrix core owns so no surface can
    /// pick a language by hand.
    #[test]
    fn the_language_matrix_follows_the_selections_space() {
        use crate::{PatchPulseLanguage, PatchPulseSpace, UiPatchTarget};

        let node = NodeId::new(2);
        let chase = [
            UiPatchTarget::Fixture { node },
            UiPatchTarget::Instance {
                node,
                path: String::from("/sector/2"),
            },
            UiPatchTarget::Range {
                node,
                start: 0,
                count: Some(4),
            },
            UiPatchTarget::Cell {
                id: String::from("2:0:0:0"),
            },
        ];
        for target in &chase {
            assert_eq!(
                target.pulse_space(),
                Some(PatchPulseSpace::Fixture),
                "{target:?} counts in the object's own numbering"
            );
            assert_eq!(target.pulse_language(), Some(PatchPulseLanguage::Chase));
        }

        let breath = [
            UiPatchTarget::Output { node },
            UiPatchTarget::Port { node, port: 1 },
            UiPatchTarget::Segment {
                node,
                port: 1,
                start: 12,
                lamps: 24,
            },
        ];
        for target in &breath {
            assert_eq!(
                target.pulse_space(),
                Some(PatchPulseSpace::Wire),
                "{target:?} counts in wire lamps"
            );
            assert_eq!(target.pulse_language(), Some(PatchPulseLanguage::Breath));
        }

        let module = UiPatchTarget::Module { node };
        assert_eq!(module.pulse_space(), None, "a module names no lamps");
        assert_eq!(module.pulse_language(), None);

        // …and the subject built for a space speaks that space's language,
        // whichever surface resolved the numbers.
        assert_eq!(
            PatchPulseSpace::Fixture.subject(node, None).language(),
            PatchPulseLanguage::Chase
        );
        assert_eq!(
            PatchPulseSpace::Wire.subject(node, Some((0, 4))).language(),
            PatchPulseLanguage::Breath
        );
    }

    /// The chase's order is the OBJECT's, not the wire's: a fixture whose
    /// second half is plugged in first must still list the half holding
    /// lamp 0 first, or the engine walks the dot backward through it.
    #[test]
    fn chase_spans_come_out_in_object_order_not_wire_order() {
        // Object lamps 60–119 sit at wire 0, lamps 0–59 at wire 60 — the
        // planner hands them over in wire order.
        let placements = Vec::from([run(2, 60, 120, 0, 60, false), run(2, 0, 120, 60, 60, false)]);
        let writes = pulse_highlights(
            &PatchPulseSubject::Fixture {
                node: NodeId::new(2),
                range: None,
            },
            &Vec::from([PulseWire {
                node: NodeId::new(1),
                placements: &placements,
                lamps: 120,
            }]),
        );

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("chase:60-119,0-59"))]),
            "the object's lamp 0 leads, wherever the wire put it"
        );
    }

    /// Adjacent spans are NOT merged in the chase: each join is a fact
    /// about the wiring, and merging would hide a direction flip.
    #[test]
    fn chase_text_keeps_touching_spans_apart_and_writes_direction() {
        assert_eq!(
            chase_text(&[
                PulseSpan {
                    source_lamp: 0,
                    wire_start: 0,
                    lamps: 30,
                    reversed: false,
                },
                PulseSpan {
                    source_lamp: 30,
                    wire_start: 30,
                    lamps: 30,
                    reversed: true,
                },
            ]),
            "chase:0-29,59-30"
        );
        // A one-lamp run is a bare number either way round.
        assert_eq!(
            chase_text(&[PulseSpan {
                source_lamp: 3,
                wire_start: 7,
                lamps: 1,
                reversed: true,
            }]),
            "chase:7"
        );
        // Nothing named is a CLEAR, not a bare prefix: an empty string is
        // what the caller reads as "this output is not involved".
        assert_eq!(chase_text(&[]), "");
        assert_eq!(
            chase_text(&[PulseSpan {
                source_lamp: 0,
                wire_start: 0,
                lamps: 0,
                reversed: false,
            }]),
            ""
        );
    }

    /// A free SEGMENT arrives as a wire window (the surface clips it to its
    /// port) and breathes like the port it sits on — byte-identical to what
    /// a port selection of the same lamps writes.
    #[test]
    fn a_segment_window_breathes_like_any_other_wire_span() {
        let placements = peach();
        let writes = pulse_highlights(
            &PatchPulseSubject::Output {
                node: NodeId::new(1),
                range: Some((12, 8)),
            },
            &wires(&placements),
        );

        assert_eq!(writes, Vec::from([(NodeId::new(1), String::from("12-19"))]));
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
