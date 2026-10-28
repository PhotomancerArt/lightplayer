//! Patching by hand, keyboard first: the editor's half of the lab
//! roadmap's stage 4 (the model's half is `domain/patch_edits.rs`).
//!
//! **Looking is free; a pair takes an arm** (plan D1, the lab's D15 and
//! Studio's walk-up model). Selecting a port, a run of pixels or an object
//! only ever selects. `A` arms a patch from what is selected: one object,
//! or a **free segment** (a run of selected pixels on one port). While
//! armed, ← → walk the **candidates** for the other end — free segments in
//! wire order from an object, objects not yet on the wire from a segment —
//! and the board previews the pair as if it were made, so the piece shows
//! where it would land. Enter (or a click on the other end) makes it; `M`
//! makes it and moves on to the next free segment, still armed, which is
//! old LightPlayer's loop: one key and one look per object. Esc disarms.
//! `S` arms a swap the same way.
//!
//! Unarmed, the wiring keys act on the selected objects: `R` reverses a
//! placement (with Move & draw off; on, it reverses the path itself, D3),
//! `K` `L` turn one lamp, `;` `'` a third, `[` `]` shift a pixel, `U`
//! puts them back to the flow. ⇧← ⇧→ walk the places on the wire (spans and free segments),
//! `.` jumps to the next port's first free segment. On a fixture, ⇧U turns
//! everything off.

use std::ops::Range;

use crate::domain::board::BoardFrame;
use crate::domain::node::NodeId;
use crate::domain::notice::Notice;
use crate::domain::part_ref::TreeRef;
use crate::domain::patch_edits::{OnWire, PatchEdit, PortEdge};
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::editor::Editor;

/// One end of a pair: an object, or a free segment of a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairEnd {
    Object(TreeRef),
    Segment {
        output: NodeId,
        port: String,
        pixels: Range<u32>,
    },
}

/// What an arm will do when it completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmVerb {
    /// Put an object on a free segment.
    Patch,
    /// Two objects trade places.
    Swap,
}

/// A pair half made: one end chosen, the other being looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Armed {
    pub verb: ArmVerb,
    pub from: PairEnd,
    /// Which candidate the arrows are on.
    pub focus: usize,
}

impl Editor {
    // ---- the arm ---------------------------------------------------------

    /// A: arm a patch from the selection.
    pub fn arm_patch(&mut self) {
        let Some(from) = self.selected_end() else {
            self.notice = Some(Notice::refused(
                "Nothing to patch",
                "a patch is made from one segment, or from a run of pixels on one port",
                "select a segment, a port or its pixels, then press A",
            ));
            return;
        };
        self.armed = Some(Armed {
            verb: ArmVerb::Patch,
            from: from.clone(),
            focus: 0,
        });
        if let PairEnd::Segment { .. } = from {
            self.selection = self.end_targets(&from);
        }
        self.focus_first_candidate();
    }

    /// S: arm a swap from one object on the wire.
    pub fn arm_swap(&mut self) {
        let Some(object) = self.selected_object() else {
            self.notice = Some(Notice::refused(
                "Nothing to swap",
                "a swap trades the places of two segments",
                "select one segment on the wire, press S, then pick the other",
            ));
            return;
        };
        if matches!(self.project.on_wire(&object), OnWire::Nowhere) {
            self.notice = Some(Notice::refused(
                format!("{} isn't on the wire", self.segment_words(&object)),
                "a swap trades places, and it has none",
                "A patches it",
            ));
            return;
        }
        self.armed = Some(Armed {
            verb: ArmVerb::Swap,
            from: PairEnd::Object(object),
            focus: 0,
        });
    }

    pub fn disarm(&mut self) {
        self.armed = None;
    }

    /// What the other end could be, in the order the arrows walk them.
    pub fn candidates(&self) -> Vec<PairEnd> {
        let Some(armed) = &self.armed else {
            return Vec::new();
        };
        let p = &self.project;
        match (&armed.verb, &armed.from) {
            (ArmVerb::Patch, PairEnd::Object(o)) => {
                let n = p.segment(o).map_or(1, |o| o.lamps.len() as u32).max(1);
                p.wire_ports()
                    .into_iter()
                    .flat_map(|(output, port)| {
                        p.free_segments_for(&output, &port, n, std::slice::from_ref(o))
                            .into_iter()
                            .map(move |pixels| PairEnd::Segment {
                                output: output.clone(),
                                port: port.clone(),
                                pixels,
                            })
                    })
                    .collect()
            }
            (ArmVerb::Patch, PairEnd::Segment { .. }) => p
                .placed_segments()
                .iter()
                .filter(|(_, o)| !o.lamps.is_empty())
                .filter(|(_, o)| matches!(p.on_wire(&o.id), OnWire::Nowhere))
                .map(|(_, o)| PairEnd::Object(o.id.clone()))
                .collect(),
            (ArmVerb::Swap, PairEnd::Object(from)) => p
                .placed_segments()
                .iter()
                .filter(|(_, o)| &o.id != from && p.effective_patch().lands(&o.id))
                .map(|(_, o)| PairEnd::Object(o.id.clone()))
                .collect(),
            (ArmVerb::Swap, PairEnd::Segment { .. }) => Vec::new(),
        }
    }

    /// The candidate the arrows are on.
    pub fn focused_candidate(&self) -> Option<PairEnd> {
        let focus = self.armed.as_ref()?.focus;
        self.candidates().into_iter().nth(focus)
    }

    /// ← → while armed.
    pub(crate) fn step_candidate(&mut self, dir: i64) {
        let n = self.candidates().len();
        let Some(armed) = &mut self.armed else {
            return;
        };
        if n == 0 {
            return;
        }
        armed.focus = (armed.focus as i64 + dir).rem_euclid(n as i64) as usize;
    }

    /// Start on the candidate that is most likely the one: from a segment,
    /// the first object not on the wire; from an object, the first free
    /// segment.
    fn focus_first_candidate(&mut self) {
        if let Some(a) = &mut self.armed {
            a.focus = 0;
        }
        if self.candidates().is_empty() {
            let why = match self.armed.as_ref().map(|a| &a.from) {
                Some(PairEnd::Segment { .. }) => "every segment is already on the wire",
                _ => "no port has a free pixel",
            };
            self.notice = Some(Notice::info(format!("Nothing to pair with: {why}")));
        }
    }

    /// Enter while armed: make the pair with the candidate in focus.
    pub(crate) fn complete(&mut self) -> bool {
        match self.focused_candidate() {
            Some(other) => self.complete_with(other),
            None => {
                self.armed = None;
                false
            }
        }
    }

    /// Make the pair with `other`. Returns whether it was made.
    pub(crate) fn complete_with(&mut self, other: PairEnd) -> bool {
        let Some(armed) = self.armed.take() else {
            return false;
        };
        match armed.verb {
            ArmVerb::Patch => {
                let (object, (output, port, pixel)) = match (&armed.from, &other) {
                    (
                        PairEnd::Object(o),
                        PairEnd::Segment {
                            output,
                            port,
                            pixels,
                        },
                    )
                    | (
                        PairEnd::Segment {
                            output,
                            port,
                            pixels,
                        },
                        PairEnd::Object(o),
                    ) => (o.clone(), (output.clone(), port.clone(), pixels.start)),
                    _ => return false,
                };
                self.checkpoint();
                let Some(edit) = self.project.patch_segment(&object, &output, &port, pixel) else {
                    return false;
                };
                self.after_edit();
                self.selection = vec![self.project.tree_target(&object)];
                let port_words = self.project.long_label(&Target::Port {
                    output: output.clone(),
                    key: port,
                });
                self.notice = Some(Notice::info(format!(
                    "{} on {port_words} at {pixel}{}",
                    self.segment_words(&object),
                    pinned_words(edit)
                )));
                self.say_overlaps(&object);
                true
            }
            ArmVerb::Swap => {
                let (PairEnd::Object(a), PairEnd::Object(b)) = (&armed.from, &other) else {
                    return false;
                };
                self.checkpoint();
                let Some(edit) = self.project.swap_placements(a, b) else {
                    return false;
                };
                self.after_edit();
                self.selection = vec![self.project.tree_target(a)];
                self.notice = Some(Notice::info(format!(
                    "{} and {} swapped{}",
                    self.segment_words(a),
                    self.segment_words(b),
                    pinned_words(edit)
                )));
                true
            }
        }
    }

    /// M: patch and move on, lp2014's `m`. Armed, it makes the pair, then
    /// selects the next free segment and arms from it, so the next press of
    /// M patches the next object. Unarmed on an object on the wire (just
    /// patched, and turned until the piece matched), it moves on the same
    /// way. Otherwise it arms (A).
    pub fn patch_next(&mut self) {
        if self.armed.is_none() {
            let placed = self
                .selected_object()
                .filter(|o| self.project.effective_patch().lands(o));
            match placed {
                Some(o) => self.arm_after(&o),
                None => self.arm_patch(),
            }
            return;
        }
        let Some(other) = self.focused_candidate() else {
            self.armed = None;
            return;
        };
        let after = match (&self.armed.as_ref().map(|a| a.from.clone()), &other) {
            (Some(PairEnd::Segment { output, port, .. }), _)
            | (_, PairEnd::Segment { output, port, .. }) => Some((output.clone(), port.clone())),
            _ => None,
        };
        if !self.complete_with(other) {
            return;
        }
        let Some((output, port)) = after else {
            return;
        };
        let notice = self.notice.take();
        self.arm_next_free(&output, &port);
        if self.notice.is_none() {
            self.notice = notice;
        }
    }

    /// Arm from the next free segment after an object on the wire.
    fn arm_after(&mut self, object: &TreeRef) {
        let Some((output, port)) = self
            .project
            .effective_patch()
            .pixel_runs_of(object)
            .next()
            .map(|s| (s.output.clone(), s.port.clone()))
        else {
            return;
        };
        self.arm_next_free(&output, &port);
        if self.armed.is_none() && self.notice.is_none() {
            self.notice = Some(Notice::info("No free pixel after this one"));
        }
    }

    /// Select the next free segment after the selection on `port` (or on a
    /// later port) and arm a patch from it.
    fn arm_next_free(&mut self, output: &NodeId, port: &str) {
        if let Some(next) = self.next_free_segment(output, port, true) {
            self.selection = self.end_targets(&next);
            self.armed = Some(Armed {
                verb: ArmVerb::Patch,
                from: next,
                focus: 0,
            });
            self.focus_first_candidate();
            if self.candidates().is_empty() {
                self.armed = None;
            }
        }
    }

    /// What the board shows while armed: the pair as if it were made, with
    /// the object selected, so its run shows lp2014's pattern where it
    /// would land. `None` when nothing is armed (plan Q4: the board
    /// previews only while armed).
    pub(crate) fn armed_frame(&self, time: f64) -> Option<BoardFrame> {
        let armed = self.armed.as_ref()?;
        let other = self.focused_candidate()?;
        let mut p = self.project.clone();
        let object = match (&armed.verb, &armed.from, &other) {
            (
                ArmVerb::Patch,
                PairEnd::Object(o),
                PairEnd::Segment {
                    output,
                    port,
                    pixels,
                },
            )
            | (
                ArmVerb::Patch,
                PairEnd::Segment {
                    output,
                    port,
                    pixels,
                },
                PairEnd::Object(o),
            ) => {
                p.patch_segment(o, output, port, pixels.start)?;
                o.clone()
            }
            (ArmVerb::Swap, PairEnd::Object(a), PairEnd::Object(b)) => {
                p.swap_placements(a, b)?;
                a.clone()
            }
            _ => return None,
        };
        Some(p.board_frame(&[p.tree_target(&object)], time))
    }

    /// A click while armed: the other end under the pointer, if there is
    /// one there.
    pub(crate) fn end_at(&self, at: crate::domain::geom::Vec2) -> Option<PairEnd> {
        let armed = self.armed.as_ref()?;
        let here = self.project.things_at(&self.camera, at);
        let wants_object = !matches!(
            (&armed.verb, &armed.from),
            (ArmVerb::Patch, PairEnd::Object(_))
        );
        if wants_object {
            return here
                .iter()
                .find_map(|t| self.object_of(t))
                .map(PairEnd::Object);
        }
        let PairEnd::Object(o) = &armed.from else {
            return None;
        };
        let n = self.project.segment(o).map_or(1, |o| o.lamps.len() as u32);
        here.iter().find_map(|t| match t {
            Target::Pixel { output, key, index } => Some(PairEnd::Segment {
                output: output.clone(),
                port: key.clone(),
                pixels: *index..index + n,
            }),
            _ => None,
        })
    }

    // ---- the wiring keys ---------------------------------------------------

    /// U: take the selected objects off the wire.
    pub fn unpatch_selection(&mut self) {
        let objects = self.selected_objects();
        if objects.is_empty() {
            self.notice = Some(Notice::refused(
                "Nothing to put back to the flow",
                "U takes away a segment's own start, so it follows its flow again",
                "select the segments to put back",
            ));
            return;
        }
        if let Some(o) = objects
            .iter()
            .find(|o| matches!(self.project.on_wire(o), OnWire::Flowing))
        {
            self.notice = Some(Notice::refused(
                format!("{} already follows its flow", self.segment_words(o)),
                "U puts a segment back to the flow, and it is already",
                "to take it off the line, choose Off for On the line in its Segment section",
            ));
            return;
        }
        self.checkpoint();
        let n = objects
            .iter()
            .filter(|o| self.project.unpatch_segment(o))
            .count();
        self.after_edit();
        self.notice = Some(Notice::info(match n {
            0 => "Nothing was set here".to_string(),
            1 => format!("{}: back to the flow", self.segment_words(&objects[0])),
            n => format!("{n} segments back to the flow"),
        }));
    }

    /// R with Move & draw off: reverse the selected objects' placements.
    pub(crate) fn reverse_placements(&mut self) {
        self.wiring_edit("reversed", |p, o| p.reverse_placement(o).map(Ok));
    }

    /// K L (one lamp) and ; ' (a third): turn the selected placements.
    pub fn turn_selection(&mut self, steps: i64, coarse: bool) {
        self.wiring_edit("turned", |p, o| {
            let by = if coarse {
                steps * p.coarse_turn(o) as i64
            } else {
                steps
            };
            p.turn_placement(o, by).map(Ok)
        });
    }

    /// [ ]: shift the selected placements a pixel along their ports.
    pub fn shift_selection(&mut self, pixels: i64) {
        self.wiring_edit("shifted", |p, o| Some(p.shift_placement(o, pixels)));
    }

    /// One wiring edit on every selected object on the wire, as one undo
    /// step. Objects not on the wire are refused, and an edge of a port
    /// stops a shift.
    fn wiring_edit(
        &mut self,
        did: &str,
        edit: impl Fn(&mut Project, &TreeRef) -> Option<Result<PatchEdit, PortEdge>>,
    ) {
        let objects = self.selected_objects();
        if objects.is_empty() {
            self.notice = Some(Notice::refused(
                format!("Nothing to be {did}"),
                "wiring keys act on segments on the wire",
                "select a segment; A patches one that isn't on the wire",
            ));
            return;
        }
        if let Some(o) = objects
            .iter()
            .find(|o| matches!(self.project.on_wire(o), OnWire::Nowhere))
        {
            self.notice = Some(Notice::refused(
                format!("{} isn't on the wire", self.segment_words(o)),
                "it has no place to change",
                "A patches it",
            ));
            return;
        }
        let base = self.project.clone();
        self.checkpoint();
        let mut pinned = 0;
        for o in &objects {
            match edit(&mut self.project, o) {
                Some(Ok(e)) => pinned += e.pinned,
                Some(Err(edge)) => {
                    // Put everything back: one key, one undo step, all or
                    // nothing.
                    self.project = base;
                    self.undo_pop();
                    let end = match edge {
                        PortEdge::Start => "start",
                        PortEdge::End => "end",
                    };
                    self.notice = Some(Notice::refused(
                        format!("{} is at the {end} of its port", self.segment_words(o)),
                        "a placement never leaves its port",
                        "patch it onto another port instead (A)",
                    ));
                    return;
                }
                None => {}
            }
        }
        self.after_edit();
        let what = match objects.as_slice() {
            [o] => self.segment_words(o),
            many => format!("{} segments", many.len()),
        };
        self.notice = Some(Notice::info(format!(
            "{what}: {did}{}",
            pinned_words(PatchEdit { pinned })
        )));
        for o in &objects {
            self.say_overlaps(o);
        }
    }

    /// ⇧U: take every object of the selection's fixture off the wire.
    pub fn unpatch_all_selected(&mut self) {
        let Some(fixture) = self.selected_fixture() else {
            self.notice = Some(Notice::refused(
                "No fixture to turn off",
                "⇧U takes every segment of one fixture off the line",
                "select a fixture, or anything in one",
            ));
            return;
        };
        self.checkpoint();
        let n = self.project.unpatch_all(&fixture);
        self.after_edit();
        let name = self.project.node_name(&fixture);
        self.notice = Some(Notice::info(format!(
            "{name}: everything off, {n} were lit; patch them by hand"
        )));
    }

    // ---- walking the wire ----------------------------------------------------

    /// ⇧← ⇧→: the previous or next span on the wire, every port in order:
    /// the objects on it and the gaps between them (`span.rs`).
    pub fn step_wire(&mut self, dir: i64) {
        let places = self.wire_spans();
        if places.is_empty() {
            self.notice = Some(Notice::info("No port has anything on it yet"));
            return;
        }
        let here = match self.selection.as_slice() {
            [t @ Target::Span { .. }] => places.iter().position(|s| s == t),
            [Target::Port { output, key }] => {
                // From a port: its first span, or the last before it.
                let first = places
                    .iter()
                    .position(|s| matches!(s, Target::Span { output: o, key: k, .. } if o == output && k == key))
                    .unwrap_or(places.len());
                let i = if dir > 0 {
                    first as i64
                } else {
                    first as i64 - 1
                };
                return self.go_to_place(&places, i, dir);
            }
            _ => match self.selected_end() {
                Some(PairEnd::Object(o)) => places
                    .iter()
                    .position(|s| matches!(s, Target::Span { segment: Some(x), .. } if *x == o)),
                Some(PairEnd::Segment {
                    output,
                    port,
                    pixels,
                }) => {
                    // Loose pixels: first, the span they are in.
                    if let Some(i) = places.iter().position(|s| {
                        self.project.span_of(s).is_some_and(|x| {
                            x.output == output && x.port == port && x.pixels.contains(&pixels.start)
                        })
                    }) {
                        return self.go_to_place(&places, i as i64, dir);
                    }
                    None
                }
                None => None,
            },
        };
        let i = match here {
            Some(i) => i as i64 + dir,
            None if dir > 0 => 0,
            None => places.len() as i64 - 1,
        };
        self.go_to_place(&places, i, dir);
    }

    fn go_to_place(&mut self, places: &[Target], i: i64, dir: i64) {
        if i < 0 || i >= places.len() as i64 {
            let end = if dir < 0 { "first" } else { "last" };
            self.notice = Some(Notice::info(format!("This is the {end} place on the wire")));
            return;
        }
        self.selection = vec![places[i as usize].clone()];
    }

    /// Every span on the wire, port after port, in pixel order.
    fn wire_spans(&self) -> Vec<Target> {
        let p = &self.project;
        p.wire_ports()
            .iter()
            .flat_map(|(o, k)| p.spans(o, k))
            .map(|s| s.target())
            .collect()
    }

    /// .: the next port's first free segment.
    pub fn next_port(&mut self) {
        match self.next_port_segment() {
            Some(seg) => self.selection = self.end_targets(&seg),
            None => self.notice = Some(Notice::info("No port after this one has a free pixel")),
        }
    }

    /// Where . goes: the first free segment on a port after the
    /// selection's.
    pub fn next_port_segment(&self) -> Option<PairEnd> {
        let ports = self.project.wire_ports();
        let start = self
            .selection_port()
            .and_then(|h| ports.iter().position(|p| *p == h))
            .map_or(0, |i| i + 1);
        let (output, port) = ports.get(start)?;
        self.next_free_segment(output, port, false)
    }

    /// Whether ⇧← ⇧→ have anywhere to go.
    pub fn wire_has_places(&self) -> bool {
        !self.wire_spans().is_empty()
    }

    /// The first free segment on `port` (after `after_selection`'s place
    /// there, when asked), else on the ports after it, sized to the next
    /// object not on the wire.
    fn next_free_segment(
        &self,
        output: &NodeId,
        port: &str,
        after_selection: bool,
    ) -> Option<PairEnd> {
        let size = self.segment_size();
        let from = if after_selection {
            self.selected_objects()
                .first()
                .and_then(|o| {
                    self.project
                        .effective_patch()
                        .pixel_runs_of(o)
                        .filter(|s| &s.output == output && s.port == port)
                        .map(|s| s.pixels().end)
                        .max()
                })
                .unwrap_or(0)
        } else {
            0
        };
        let ports = self.project.wire_ports();
        let start = ports
            .iter()
            .position(|(o, k)| o == output && k == port)
            .unwrap_or(0);
        for (i, (o, k)) in ports.iter().enumerate().skip(start) {
            let min = if i == start { from } else { 0 };
            if let Some(pixels) = self
                .project
                .free_segments(o, k, size)
                .into_iter()
                .find(|r| r.start >= min)
            {
                return Some(PairEnd::Segment {
                    output: o.clone(),
                    port: k.clone(),
                    pixels,
                });
            }
        }
        None
    }

    /// How long a free segment is: the next object to patch, in the
    /// fixtures' order (plan Q3).
    pub fn segment_size(&self) -> u32 {
        let p = &self.project;
        p.fixtures()
            .iter()
            .find_map(|n| p.next_unplaced(&n.id))
            .and_then(|o| p.segment(&o))
            .map_or(1, |o| o.lamps.len() as u32)
            .max(1)
    }

    // ---- what is selected ----------------------------------------------------

    /// The selection as one end of a pair: one object, a run of pixels on
    /// one port, or a port (its first free segment).
    pub fn selected_end(&self) -> Option<PairEnd> {
        if let Some(o) = self.selected_object() {
            return Some(PairEnd::Object(o));
        }
        if let [t @ Target::Span { segment: None, .. }] = self.selection.as_slice() {
            let gap = self.project.span_of(t)?;
            return Some(PairEnd::Segment {
                output: gap.output,
                port: gap.port,
                pixels: gap.pixels,
            });
        }
        if let [Target::Port { output, key }] = self.selection.as_slice() {
            return self.next_free_segment(output, key, false).filter(|s| {
                matches!(s, PairEnd::Segment { output: o, port, .. } if o == output && port == key)
            });
        }
        let mut pixels: Vec<(&NodeId, &String, u32)> = self
            .selection
            .iter()
            .filter_map(|t| match t {
                Target::Pixel { output, key, index } => Some((output, key, *index)),
                _ => None,
            })
            .collect();
        if pixels.is_empty() || pixels.len() != self.selection.len() {
            return None;
        }
        let (output, port) = (pixels[0].0, pixels[0].1);
        if pixels.iter().any(|(o, k, _)| *o != output || *k != port) {
            return None;
        }
        pixels.sort_by_key(|(_, _, i)| *i);
        let (lo, hi) = (pixels[0].2, pixels[pixels.len() - 1].2);
        if hi - lo + 1 != pixels.len() as u32 {
            return None;
        }
        Some(PairEnd::Segment {
            output: output.clone(),
            port: port.clone(),
            pixels: lo..hi + 1,
        })
    }

    /// The one object selected (a path is its own object).
    pub fn selected_object(&self) -> Option<TreeRef> {
        match self.selection.as_slice() {
            [t] => self.object_of(t),
            _ => None,
        }
    }

    /// Every object the selection names, one per target.
    pub fn selected_objects(&self) -> Vec<TreeRef> {
        let mut out = Vec::new();
        for t in &self.selection {
            if let Some(o) = self.object_of(t)
                && !out.contains(&o)
            {
                out.push(o);
            }
        }
        out
    }

    /// The object a target is: an object, or a component that makes
    /// exactly one.
    fn object_of(&self, t: &Target) -> Option<TreeRef> {
        match t {
            Target::Generated(o)
            | Target::Span {
                segment: Some(o), ..
            } => Some(o.clone()),
            Target::Part(c) => self
                .project
                .part(c)
                .is_some_and(|comp| crate::domain::target::is_single_segment(&comp.kind))
                .then(|| c.node()),
            _ => None,
        }
    }

    /// The fixture the selection is in.
    fn selected_fixture(&self) -> Option<NodeId> {
        self.selection
            .first()
            .and_then(|t| self.project.fixture_of_target(t))
    }

    /// The port the selection is on: a port, a pixel, or an object's first
    /// span.
    fn selection_port(&self) -> Option<(NodeId, String)> {
        match self.selection.first()? {
            Target::Port { output, key }
            | Target::Pixel { output, key, .. }
            | Target::Span { output, key, .. } => Some((output.clone(), key.clone())),
            t => {
                let o = self.object_of(t)?;
                let s = self.project.effective_patch().pixel_runs_of(&o).next()?;
                Some((s.output.clone(), s.port.clone()))
            }
        }
    }

    /// The targets that select an end: an object's, or a segment's — the
    /// gap span it starts, else its pixels.
    pub(crate) fn end_targets(&self, end: &PairEnd) -> Vec<Target> {
        match end {
            PairEnd::Object(o) => vec![self.project.tree_target(o)],
            PairEnd::Segment {
                output,
                port,
                pixels,
            } if let Some(gap) = self
                .project
                .spans(output, port)
                .into_iter()
                .find(|s| s.segment.is_none() && s.pixels.start == pixels.start) =>
            {
                vec![gap.target()]
            }
            PairEnd::Segment {
                output,
                port,
                pixels,
            } => pixels
                .clone()
                .map(|index| Target::Pixel {
                    output: output.clone(),
                    key: port.clone(),
                    index,
                })
                .collect(),
        }
    }

    /// An end in words: `panel 7`, `Box 1 port 2 · pixels 0–118`.
    pub fn end_words(&self, end: &PairEnd) -> String {
        match end {
            PairEnd::Object(o) => self.segment_words(o),
            PairEnd::Segment {
                output,
                port,
                pixels,
            } => {
                let port = self.project.long_label(&Target::Port {
                    output: output.clone(),
                    key: port.clone(),
                });
                format!("{port} · {} pixels from {}", pixels.len(), pixels.start)
            }
        }
    }

    fn segment_words(&self, o: &TreeRef) -> String {
        self.project.long_label(&self.project.tree_target(o))
    }

    /// Tell when an object now shares pixels with another (plan D7).
    fn say_overlaps(&mut self, object: &TreeRef) {
        let found = self
            .project
            .effective_patch()
            .overlaps
            .iter()
            .find(|x| x.segments.contains(object))
            .cloned();
        if let Some(x) = found {
            let other = if &x.segments[0] == object {
                &x.segments[1]
            } else {
                &x.segments[0]
            };
            let shows = self.segment_words(&x.segments[1]);
            self.notice = Some(Notice::refused(
                format!(
                    "{} overlaps {} on {} pixels",
                    self.segment_words(object),
                    self.segment_words(other),
                    x.pixels.len()
                ),
                format!("two segments share those pixels; the board shows {shows}"),
                "shift one ([ ]) or patch it elsewhere (A)",
            ));
        }
    }
}

impl Project {
    /// Every port, in wire order: outputs as the project lists them, each
    /// one's ports in key order.
    pub fn wire_ports(&self) -> Vec<(NodeId, String)> {
        self.outputs()
            .iter()
            .flat_map(|n| {
                n.output()
                    .map(|o| {
                        o.ports
                            .iter()
                            .map(|p| (n.id.clone(), p.key.clone()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }
}

fn pinned_words(edit: PatchEdit) -> String {
    match edit.pinned {
        0 => String::new(),
        1 => " · pinned 1 other segment where it was".into(),
        n => format!(" · pinned {n} other segments where they were"),
    }
}
