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
//! decided in ONE place — the selection-kind matrix on
//! [`crate::UiPatchTarget::pulse_language`] — and CARRIED here on the
//! subject, never re-derived.

use core::any::Any;

use lpc_model::NodeId;
use lpc_wire::WireOutputPlacement;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Samples per RGB lamp — the unit a published frame's extent is stated in.
const SAMPLES_PER_LAMP: u32 = 3;

/// Which lamps a patching surface selected, in the space it naturally
/// knows. The LANGUAGE they are said in rides alongside on
/// [`PatchPulseSubject`] — since the G1 round-3 rework the two are separate
/// axes (a whole fixture and one of its objects count in the same space and
/// speak different tongues).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchPulseLamps {
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

impl PatchPulseLamps {
    /// Which of the two spaces these lamps count in.
    #[must_use]
    pub fn space(&self) -> PatchPulseSpace {
        match self {
            Self::Fixture { .. } => PatchPulseSpace::Fixture,
            Self::Output { .. } => PatchPulseSpace::Wire,
        }
    }
}

/// What a patching surface selected: which lamps, and which light language
/// the D9 matrix gives that selection.
///
/// The language is NOT re-derivable from the lamps — that is the round-3
/// ruling in one sentence. A fixture-space subject can be an OBJECT (chase:
/// "does this run the way I think it does?") or a whole FIXTURE (breath:
/// "these lamps are this fixture"), and only the selection KIND knows which.
/// So the matrix lives on [`crate::UiPatchTarget::pulse_language`] and its
/// answer is CARRIED here, rather than guessed again downstream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchPulseSubject {
    pub lamps: PatchPulseLamps,
    pub language: PatchPulseLanguage,
}

impl PatchPulseSubject {
    /// Which of the two spaces this subject counts in.
    #[must_use]
    pub fn space(&self) -> PatchPulseSpace {
        self.lamps.space()
    }

    /// The light language this subject was named in.
    #[must_use]
    pub fn language(&self) -> PatchPulseLanguage {
        self.language
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
    /// The lamps for `(node, range)` counted in this space. The LANGUAGE is
    /// the target kind's ([`crate::UiPatchTarget::pulse_language`]), never
    /// the space's — see [`PatchPulseSubject`].
    #[must_use]
    pub fn lamps(self, node: NodeId, range: Option<(u32, u32)>) -> PatchPulseLamps {
        match self {
            Self::Fixture => PatchPulseLamps::Fixture { node, range },
            Self::Wire => PatchPulseLamps::Output { node, range },
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

/// Pulse the lamps behind the selected patch subjects on the live
/// sim/hardware — or, with none, stop pulsing.
///
/// Dispatched to `ProjectController::NODE_ID`; the controller maps each
/// subject through the published placements and writes each affected
/// output's `highlight` Debug slot (clearing outputs that stop being
/// involved). A Debug slot, so nothing dirties, nothing saves, and a
/// forgotten pulse dies with the project unload.
///
/// More than one subject is the multi-selection's UNION (unified-selection
/// P2): several fixtures breathing at once. The sibling invariant makes a
/// multi-subject list breath-only — chase is a single-object language —
/// so same-output texts merge as plain span lists.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchPulseOp {
    /// The subjects to pulse; empty clears the pulse everywhere.
    pub subjects: Vec<PatchPulseSubject>,
}

impl PatchPulseOp {
    /// The zero-or-one form every single-selection surface dispatches.
    #[must_use]
    pub fn from_option(subject: Option<PatchPulseSubject>) -> Self {
        Self {
            subjects: subject.into_iter().collect(),
        }
    }
}

/// Merge two same-output highlight texts (the multi-selection union).
/// Breath texts are bare span lists and concatenate; a `chase:` text never
/// legitimately meets another text (chase = single object, enforced by the
/// selection's sibling invariant), so on that impossible collision the
/// FIRST text stands rather than corrupting the microformat.
#[must_use]
pub(crate) fn merge_highlight_texts(first: &str, second: &str) -> String {
    if first.starts_with("chase:") || second.starts_with("chase:") {
        return first.to_string();
    }
    if first.is_empty() {
        return second.to_string();
    }
    if second.is_empty() {
        return first.to_string();
    }
    format!("{first},{second}")
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
/// The language is the SUBJECT's — the answer the D9 matrix already gave
/// upstream ([`crate::UiPatchTarget::pulse_language`]), never re-decided
/// here: an object writes an ordered `chase:` list, a fixture or a wire span
/// the v1 bare list, byte for byte as before.
pub(crate) fn pulse_highlights(
    subject: &PatchPulseSubject,
    wires: &[PulseWire<'_>],
) -> Vec<(NodeId, String)> {
    let mut writes = Vec::new();
    for wire in wires {
        let spans = match &subject.lamps {
            PatchPulseLamps::Fixture { node, range } => {
                fixture_wire_spans(*node, *range, wire.placements)
            }
            PatchPulseLamps::Output { node, range } if *node == wire.node => {
                let (start, lamps) = range.unwrap_or((0, wire.lamps));
                Vec::from([PulseSpan {
                    source_lamp: start,
                    wire_start: start,
                    lamps,
                    reversed: false,
                }])
            }
            PatchPulseLamps::Output { .. } => Vec::new(),
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

    /// A fixture-space subject in the OBJECT language — what an instance /
    /// range / cell selection resolves to (D9).
    fn chased(node: NodeId, range: Option<(u32, u32)>) -> PatchPulseSubject {
        PatchPulseSubject {
            lamps: PatchPulseLamps::Fixture { node, range },
            language: PatchPulseLanguage::Chase,
        }
    }

    /// A wire-space subject — always the breath.
    fn breathed(node: NodeId, range: Option<(u32, u32)>) -> PatchPulseSubject {
        PatchPulseSubject {
            lamps: PatchPulseLamps::Output { node, range },
            language: PatchPulseLanguage::Breath,
        }
    }

    fn wires(placements: &[WireOutputPlacement]) -> Vec<PulseWire<'_>> {
        Vec::from([PulseWire {
            node: NodeId::new(1),
            placements,
            lamps: 56,
        }])
    }

    /// An object spanning its whole fixture (the range grain an id-less
    /// strand selects at) chases every run it has, in object order.
    #[test]
    fn an_object_covering_a_fixture_chases_every_run_it_has() {
        let placements = peach();
        let writes = pulse_highlights(&chased(NodeId::new(2), None), &wires(&placements));

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
        let writes = pulse_highlights(&chased(NodeId::new(2), Some((20, 4))), &wires(&placements));

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("chase:20-21,55-54"))]),
            "the reversed run re-bases from its tail, and runs backward"
        );
    }

    #[test]
    fn an_instance_range_inside_one_run_maps_straight_through() {
        let placements = peach();
        let writes = pulse_highlights(&chased(NodeId::new(3), Some((4, 4))), &wires(&placements));

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("chase:26-29"))])
        );
    }

    #[test]
    fn a_port_selection_is_already_a_wire_span() {
        let placements = peach();
        let writes = pulse_highlights(
            &breathed(NodeId::new(1), Some((40, 16))),
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
        let writes = pulse_highlights(&breathed(NodeId::new(1), None), &all);

        assert_eq!(writes, Vec::from([(NodeId::new(1), String::from("0-55"))]));
    }

    #[test]
    fn a_subject_off_every_wire_writes_nothing() {
        let placements = peach();
        let writes = pulse_highlights(&chased(NodeId::new(9), None), &wires(&placements));

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

    /// D9 as reworked at G1 round 3: CHASE is the OBJECT language, and
    /// nothing else speaks it. Space and language are separate axes now —
    /// a whole fixture counts in fixture numbering and still breathes.
    #[test]
    fn the_chase_is_the_object_language_and_everything_else_breathes() {
        use crate::{PatchPulseLanguage, PatchPulseSpace, UiPatchTarget};

        let node = NodeId::new(2);
        let chase = [
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

        // The round-3 change: a FIXTURE is a bag of objects, not one of
        // them. It still counts in fixture numbering — and it breathes,
        // because a multi-run fixture has no single direction to claim.
        let fixture = UiPatchTarget::Fixture { node };
        assert_eq!(fixture.pulse_space(), Some(PatchPulseSpace::Fixture));
        assert_eq!(
            fixture.pulse_language(),
            Some(PatchPulseLanguage::Breath),
            "\"these lamps are this fixture\" — no direction claim"
        );

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
        assert_eq!(module.pulse_subject(node, None), None);
    }

    /// The subject a target builds carries BOTH answers, so nothing
    /// downstream re-derives a language from the lamps it was handed.
    #[test]
    fn a_targets_subject_carries_its_lamps_and_its_language() {
        use crate::UiPatchTarget;

        let node = NodeId::new(2);
        assert_eq!(
            UiPatchTarget::Range {
                node,
                start: 0,
                count: Some(4),
            }
            .pulse_subject(node, Some((0, 4))),
            Some(chased(node, Some((0, 4)))),
        );
        // The same fixture-space lamps, named as the whole fixture: same
        // `PatchPulseLamps`, different tongue.
        assert_eq!(
            UiPatchTarget::Fixture { node }.pulse_subject(node, None),
            Some(PatchPulseSubject {
                lamps: PatchPulseLamps::Fixture { node, range: None },
                language: PatchPulseLanguage::Breath,
            }),
        );
        assert_eq!(
            UiPatchTarget::Port { node, port: 1 }.pulse_subject(node, Some((0, 4))),
            Some(breathed(node, Some((0, 4)))),
        );
    }

    /// And the bytes follow the language, not the space: a whole fixture
    /// writes the BARE list its runs cover — sorted and merged, no head, no
    /// tail — where an object of the same fixture writes the ordered chase.
    #[test]
    fn a_whole_fixture_breathes_the_lamps_its_runs_cover() {
        let placements = peach();
        let writes = pulse_highlights(
            &PatchPulseSubject {
                lamps: PatchPulseLamps::Fixture {
                    node: NodeId::new(2),
                    range: None,
                },
                language: PatchPulseLanguage::Breath,
            },
            &wires(&placements),
        );

        assert_eq!(
            writes,
            Vec::from([(NodeId::new(1), String::from("0-21,34-55"))]),
            "one lamp set, no direction — the two body stretches as they lie \
             on the wire"
        );
        // The very same lamps, named as an object, keep object order and the
        // reversed run's descending range.
        assert_eq!(
            pulse_highlights(&chased(NodeId::new(2), None), &wires(&placements)),
            Vec::from([(NodeId::new(1), String::from("chase:0-21,55-34"))]),
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
            &chased(NodeId::new(2), None),
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
            &breathed(NodeId::new(1), Some((12, 8))),
            &wires(&placements),
        );

        assert_eq!(writes, Vec::from([(NodeId::new(1), String::from("12-19"))]));
    }

    #[test]
    fn patch_pulse_is_editor_foreground_class() {
        let op = PatchPulseOp {
            subjects: Vec::new(),
        };
        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
    }
}
