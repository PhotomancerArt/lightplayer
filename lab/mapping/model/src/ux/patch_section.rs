//! The inspector's Patch section (plan P6, R2): the same rows on a
//! fixture, a component and an object, editing that thing's placement
//! (`patch_place.rs`) — lp2014's group panel, as the inspector's own rows.
//!
//! ```text
//! Patch
//!   Wire        Auto onto main                      [Override]   inherited:
//!               from fixture ring display ↗                      the value, a link to
//!                                                                where it is set
//!   Wire        [ D0 of main ▾ ]  [Reset]           its own: Auto onto <output> ·
//!                                                   <output> · <port> · Not on the wire
//!   Start       [ − ]  40  [ + ]                    only when its own wire is a port
//!   Direction   As drawn                [Override]  or [ Reversed ▾ ] [Reset]
//!   Turn        [ − ]  0   [ + ]                    objects only
//! ```
//!
//! An inherited value says what it is and where it is set, never just
//! "follows": the link selects that place. Override makes the value the
//! thing's own without moving anything, and Reset hands it back. A
//! fixture has nothing above it, so its rows are always its own.
//!
//! A row's options are worked out from the project, so a choice names this
//! project's own outputs and ports; that is why these are rows of their
//! own and not property descriptors (whose choices are fixed lists). They
//! follow the same rules: every selected thing is edited together, a
//! value they don't share shows as mixed, and every edit is one undo
//! step. The keys do the same edits (`patching.rs`).

use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::patch_place::{Direction, PatchTarget, Wire};
use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::ui::UiItem;

/// Which row of the Patch section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchField {
    Wire,
    Start,
    Direction,
    Turn,
}

/// What a Patch row asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchInput {
    /// The option at this index of the row's list.
    Choose { field: PatchField, option: usize },
    /// Make an inherited value the thing's own, as it is now.
    Override { field: PatchField },
    /// Drop the thing's own value and follow what is above it again.
    Reset { field: PatchField },
    /// − or + on a number row.
    Nudge { field: PatchField, by: i64 },
    /// A gap's "Patch here": this object (by its index in
    /// `Editor::patch_here_options`) at the gap's start.
    PatchHere { option: usize },
}

/// One option of a choice row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOption {
    Auto(NodeId),
    Port(NodeId, String),
    Off,
    AsDrawn,
    Reversed,
}

impl Editor {
    /// What the selection's placements are on: a fixture node, a component
    /// that makes several objects, or an object (a path is its own object).
    pub fn patch_targets(&self) -> Vec<(NodeId, PatchTarget)> {
        let p = &self.project;
        let mut out = Vec::new();
        for t in &self.selection {
            let target = match t {
                Target::Node(id) if p.fixture(id).is_some() => {
                    out.push((id.clone(), PatchTarget::Fixture));
                    continue;
                }
                Target::Object(o)
                | Target::Span {
                    object: Some(o), ..
                } => PatchTarget::Object(o.clone()),
                // A path is its own object (copy 0 of it, in a repeat).
                Target::Component(c) => match p.component(c) {
                    Some(comp) if comp.is_single_object() => PatchTarget::Object(ObjectId::of(c)),
                    _ => PatchTarget::Component(c.clone()),
                },
                _ => continue,
            };
            if let Some(f) = p.fixture_of_place(&target)
                && !out.iter().any(|(_, t)| t == &target)
            {
                out.push((f, target));
            }
        }
        out
    }

    /// The selection's Patch section, when it is all fixtures, components
    /// or objects.
    pub fn patch_section(&self) -> Option<Vec<UiItem>> {
        let targets = self.patch_targets();
        if targets.is_empty() || targets.len() != self.selection.len() {
            return None;
        }
        let (fixture, first) = &targets[0];
        let mut items = Vec::new();

        // Wire.
        if targets.iter().all(|(f, t)| self.wire_inherited(f, t)) {
            let value = self.shared(&targets, |f, t| {
                Some(self.wire_value_words(f, t, self.inherited_wire(f, t).0.as_ref()))
            });
            items.push(UiItem::PatchInherited {
                label: "Wire",
                value: value.unwrap_or_else(|| "mixed".into()),
                from: self.shared_from(&targets, |f, t| self.inherited_wire(f, t).1),
                field: PatchField::Wire,
            });
        } else {
            let wires = self.wire_options();
            let chosen = self.shared(&targets, |f, t| {
                Some(match self.own_wire(f, t) {
                    // A fixture not on the wire is simply unset.
                    None | Some(Wire::Off) => PatchOption::Off,
                    Some(Wire::Auto { output }) => PatchOption::Auto(output),
                    Some(Wire::At { output, port, .. }) => PatchOption::Port(output, port),
                })
            });
            items.push(UiItem::PatchChoice {
                label: "Wire",
                options: wires.iter().map(|o| self.option_words(o)).collect(),
                chosen: chosen.and_then(|c| wires.iter().position(|o| *o == c)),
                reset: self.reset_words(fixture, first),
                field: PatchField::Wire,
            });
        }

        // Start: the pixel of its own wire, when that is a port.
        let starts = self.shared(&targets, |f, t| match self.own_wire(f, t)? {
            Wire::At { pixel, .. } => Some(pixel as i64),
            _ => None,
        });
        if targets
            .iter()
            .all(|(f, t)| matches!(self.own_wire(f, t), Some(Wire::At { .. })))
        {
            items.push(UiItem::PatchNumber {
                label: "Start",
                value: starts,
                note: None,
                field: PatchField::Start,
            });
        }

        // Direction.
        if targets.iter().all(|(f, t)| self.direction_inherited(f, t)) {
            let value = self.shared(&targets, |f, t| {
                Some(direction_option(self.inherited_direction(f, t).0))
            });
            items.push(UiItem::PatchInherited {
                label: "Direction",
                value: value.map_or_else(|| "mixed".into(), |o| self.option_words(&o)),
                from: self.shared_from(&targets, |f, t| self.inherited_direction(f, t).1),
                field: PatchField::Direction,
            });
        } else {
            let directions = [PatchOption::AsDrawn, PatchOption::Reversed];
            let chosen = self.shared(&targets, |f, t| {
                Some(direction_option(
                    self.own_direction(f, t).unwrap_or_default(),
                ))
            });
            items.push(UiItem::PatchChoice {
                label: "Direction",
                options: directions.iter().map(|o| self.option_words(o)).collect(),
                chosen: chosen.and_then(|c| directions.iter().position(|o| *o == c)),
                reset: self.reset_words(fixture, first),
                field: PatchField::Direction,
            });
        }

        // Turn: objects only.
        if targets
            .iter()
            .all(|(_, t)| matches!(t, PatchTarget::Object(_)))
        {
            let turns = self.shared(&targets, |_, t| {
                let PatchTarget::Object(o) = t else {
                    return None;
                };
                Some(
                    self.project
                        .effective_patch()
                        .resolved(o)
                        .map_or(0, |r| r.turn) as i64,
                )
            });
            items.push(UiItem::PatchNumber {
                label: "Turn",
                value: turns,
                note: Some("lamps".into()),
                field: PatchField::Turn,
            });
        }
        Some(items)
    }

    /// What a gap's "Patch here" offers: the objects not on the wire, in
    /// tree order.
    pub fn patch_here_options(&self) -> Vec<ObjectId> {
        let p = &self.project;
        p.placed_objects()
            .iter()
            .filter(|(_, o)| !o.lamps.is_empty() && !p.effective_patch().lands(&o.id))
            .map(|(_, o)| o.id.clone())
            .collect()
    }

    /// Apply a Patch row's edit to every selected thing, as one undo step.
    pub fn patch_input(&mut self, input: PatchInput) {
        if let PatchInput::PatchHere { option } = input {
            let gap = match self.selection.as_slice() {
                [t @ Target::Span { object: None, .. }] => self.project.span_of(t),
                _ => None,
            };
            let object = self.patch_here_options().get(option).cloned();
            if let (Some(gap), Some(object)) = (gap, object) {
                self.checkpoint();
                self.project
                    .patch_object(&object, &gap.output, &gap.port, gap.pixels.start);
                self.after_edit();
                self.selection = vec![self.project.object_target(&object)];
            }
            return;
        }
        let targets = self.patch_targets();
        if targets.is_empty() {
            return;
        }
        self.checkpoint();
        match input {
            PatchInput::Choose {
                field: PatchField::Wire,
                option,
            } => {
                let Some(o) = self.wire_options().get(option).cloned() else {
                    return;
                };
                for (f, t) in &targets {
                    let wire = match &o {
                        PatchOption::Auto(out) => Some(Wire::Auto {
                            output: out.clone(),
                        }),
                        PatchOption::Port(out, port) => Some(Wire::At {
                            output: out.clone(),
                            port: port.clone(),
                            pixel: self.first_fit(f, t, out, port),
                        }),
                        // A fixture not on the wire is simply unset.
                        PatchOption::Off if *t == PatchTarget::Fixture => None,
                        PatchOption::Off => Some(Wire::Off),
                        _ => continue,
                    };
                    self.project.set_wire(f, t, wire);
                }
            }
            PatchInput::Choose {
                field: PatchField::Direction,
                option,
            } => {
                let Some(o) = [PatchOption::AsDrawn, PatchOption::Reversed]
                    .get(option)
                    .cloned()
                else {
                    return;
                };
                let direction = match o {
                    PatchOption::AsDrawn => Some(Direction::AsDrawn),
                    PatchOption::Reversed => Some(Direction::Reversed),
                    _ => None,
                };
                for (f, t) in &targets {
                    self.project.set_direction(f, t, direction);
                }
            }
            PatchInput::Override {
                field: PatchField::Wire,
            } => {
                for (f, t) in &targets {
                    if !self.wire_inherited(f, t) {
                        continue;
                    }
                    let wire = match self.inherited_wire(f, t).0 {
                        // On a port: where it lands now, so nothing moves.
                        Some(Wire::At {
                            output,
                            port,
                            pixel,
                        }) => Some(Wire::At {
                            pixel: self.lands_at(f, t, &output, &port).unwrap_or(pixel),
                            output,
                            port,
                        }),
                        Some(w) => Some(w),
                        None => Some(Wire::Off),
                    };
                    self.project.set_wire(f, t, wire);
                }
            }
            PatchInput::Override {
                field: PatchField::Direction,
            } => {
                for (f, t) in &targets {
                    if self.direction_inherited(f, t) {
                        let d = self.inherited_direction(f, t).0;
                        self.project.set_direction(f, t, Some(d));
                    }
                }
            }
            PatchInput::Reset {
                field: PatchField::Wire,
            } => {
                for (f, t) in &targets {
                    if *t != PatchTarget::Fixture {
                        self.project.set_wire(f, t, None);
                    }
                }
            }
            PatchInput::Reset {
                field: PatchField::Direction,
            } => {
                for (f, t) in &targets {
                    if *t != PatchTarget::Fixture {
                        self.project.set_direction(f, t, None);
                    }
                }
            }
            PatchInput::Nudge {
                field: PatchField::Start,
                by,
            } => {
                for (f, t) in &targets {
                    if let Some(Wire::At {
                        output,
                        port,
                        pixel,
                    }) = self.own_wire(f, t)
                    {
                        let to = (pixel as i64 + by).max(0) as u32;
                        let to = match self.project.output(&output).and_then(|o| o.port(&port)) {
                            Some(p) => p.pixels.map_or(to, |n| to.min(n.saturating_sub(1))),
                            None => to,
                        };
                        self.project.set_wire(
                            f,
                            t,
                            Some(Wire::At {
                                output,
                                port,
                                pixel: to,
                            }),
                        );
                    }
                }
            }
            PatchInput::Nudge {
                field: PatchField::Turn,
                by,
            } => {
                for (_, t) in &targets {
                    if let PatchTarget::Object(o) = t {
                        self.project.turn_placement(o, by);
                    }
                }
            }
            PatchInput::Choose { .. }
            | PatchInput::Override { .. }
            | PatchInput::Reset { .. }
            | PatchInput::Nudge { .. }
            | PatchInput::PatchHere { .. } => {}
        }
        self.after_edit();
    }

    // ---- helpers -------------------------------------------------------------

    /// A value every target shares, or `None` when they differ (or any has
    /// none).
    fn shared<V: PartialEq>(
        &self,
        targets: &[(NodeId, PatchTarget)],
        value: impl Fn(&NodeId, &PatchTarget) -> Option<V>,
    ) -> Option<V> {
        let mut it = targets.iter().map(|(f, t)| value(f, t));
        let first = it.next()??;
        it.all(|v| v.as_ref() == Some(&first)).then_some(first)
    }

    /// A target's own wire, unset or not.
    fn own_wire(&self, fixture: &NodeId, target: &PatchTarget) -> Option<Wire> {
        self.project.fixture(fixture)?.place(target)?.wire.clone()
    }

    /// A thing's own direction, unset or not.
    fn own_direction(&self, fixture: &NodeId, target: &PatchTarget) -> Option<Direction> {
        self.project.fixture(fixture)?.place(target)?.direction
    }

    /// Whether a thing's wire comes from above: it sets none, and it has an
    /// above (a fixture's is always its own).
    fn wire_inherited(&self, fixture: &NodeId, target: &PatchTarget) -> bool {
        *target != PatchTarget::Fixture && self.own_wire(fixture, target).is_none()
    }

    fn direction_inherited(&self, fixture: &NodeId, target: &PatchTarget) -> bool {
        *target != PatchTarget::Fixture && self.own_direction(fixture, target).is_none()
    }

    /// The wire a thing would follow, and the placement that sets it: the
    /// first one above it that has one.
    fn inherited_wire(
        &self,
        fixture: &NodeId,
        target: &PatchTarget,
    ) -> (Option<Wire>, Option<PatchTarget>) {
        let p = &self.project;
        let Some(f) = p.fixture(fixture) else {
            return (None, None);
        };
        for t in p.place_chain(target).into_iter().skip(1) {
            if let Some(w) = f.place(&t).and_then(|pl| pl.wire.clone()) {
                return (Some(w), Some(t));
            }
        }
        (None, None)
    }

    /// The direction a thing would follow, and where it is set (`None`: as
    /// drawn, by default).
    fn inherited_direction(
        &self,
        fixture: &NodeId,
        target: &PatchTarget,
    ) -> (Direction, Option<PatchTarget>) {
        let p = &self.project;
        let Some(f) = p.fixture(fixture) else {
            return (Direction::AsDrawn, None);
        };
        for t in p.place_chain(target).into_iter().skip(1) {
            if let Some(d) = f.place(&t).and_then(|pl| pl.direction) {
                return (d, Some(t));
            }
        }
        (Direction::AsDrawn, None)
    }

    /// Where the targets' values are set, when they share one place: its
    /// words and the target that selects it.
    fn shared_from(
        &self,
        targets: &[(NodeId, PatchTarget)],
        from: impl Fn(&NodeId, &PatchTarget) -> Option<PatchTarget>,
    ) -> Option<(String, Target)> {
        let (fixture, place) = self.shared(targets, |f, t| Some((f.clone(), from(f, t)?)))?;
        Some((
            self.place_words(&fixture, &place),
            self.place_selection(&fixture, &place),
        ))
    }

    /// What Reset would follow again, in words; `None` for a fixture.
    fn reset_words(&self, fixture: &NodeId, target: &PatchTarget) -> Option<String> {
        let parent = self.project.place_chain(target).into_iter().nth(1)?;
        Some(self.place_words(fixture, &parent))
    }

    /// A placement's place in words, a fixture named as one: "fixture ring
    /// display", "circle1", "ring r1 of circle1".
    fn place_words(&self, fixture: &NodeId, place: &PatchTarget) -> String {
        let p = &self.project;
        match place {
            PatchTarget::Fixture => format!("fixture {}", p.node_name(fixture)),
            PatchTarget::Component(_) => p.target_words(fixture, place),
            PatchTarget::Object(o) => p.long_label(&p.object_target(o)),
        }
    }

    /// The selection that shows a placement's place.
    fn place_selection(&self, fixture: &NodeId, place: &PatchTarget) -> Target {
        match place {
            PatchTarget::Fixture => Target::Node(fixture.clone()),
            PatchTarget::Component(c) => Target::Component(c.clone()),
            PatchTarget::Object(o) => self.project.object_target(o),
        }
    }

    /// An inherited wire in words, with where the thing lands when it is on
    /// a port: "D0 of main · pixels 24–63".
    fn wire_value_words(
        &self,
        fixture: &NodeId,
        target: &PatchTarget,
        wire: Option<&Wire>,
    ) -> String {
        match wire {
            None | Some(Wire::Off) => self.option_words(&PatchOption::Off),
            Some(Wire::Auto { output }) => self.option_words(&PatchOption::Auto(output.clone())),
            Some(Wire::At { output, port, .. }) => {
                let at = self.option_words(&PatchOption::Port(output.clone(), port.clone()));
                match self.landed_pixels(fixture, target, output, port) {
                    Some(px) if px.len() == 1 => format!("{at} · pixel {}", px.start),
                    Some(px) => format!("{at} · pixels {}–{}", px.start, px.end - 1),
                    None => at,
                }
            }
        }
    }

    /// The pixels a placement's objects cover on one port, first to last.
    fn landed_pixels(
        &self,
        fixture: &NodeId,
        target: &PatchTarget,
        output: &NodeId,
        port: &str,
    ) -> Option<std::ops::Range<u32>> {
        let p = &self.project;
        let objects = p.objects_of_place(fixture, target);
        p.effective_patch()
            .pixel_runs
            .iter()
            .filter(|r| &r.output == output && r.port == port && objects.contains(&r.object))
            .map(|r| r.pixels())
            .reduce(|a, b| a.start.min(b.start)..a.end.max(b.end))
    }

    /// Where a placement's objects start on a port now.
    fn lands_at(
        &self,
        fixture: &NodeId,
        target: &PatchTarget,
        output: &NodeId,
        port: &str,
    ) -> Option<u32> {
        self.landed_pixels(fixture, target, output, port)
            .map(|px| px.start)
    }

    /// The Wire row's options: auto onto each output, every port, not on
    /// the wire.
    fn wire_options(&self) -> Vec<PatchOption> {
        let p = &self.project;
        let mut out = Vec::new();
        for n in p.outputs() {
            out.push(PatchOption::Auto(n.id.clone()));
        }
        for (output, port) in p.wire_ports() {
            out.push(PatchOption::Port(output, port));
        }
        out.push(PatchOption::Off);
        out
    }

    /// An option in words.
    fn option_words(&self, o: &PatchOption) -> String {
        let p = &self.project;
        match o {
            PatchOption::Auto(out) => format!("Auto onto {}", p.node_name(out)),
            PatchOption::Port(out, port) => p.long_label(&Target::Port {
                output: out.clone(),
                key: port.clone(),
            }),
            PatchOption::Off => "Not on the wire".into(),
            PatchOption::AsDrawn => "As drawn".into(),
            PatchOption::Reversed => "Reversed".into(),
        }
    }

    /// Where a thing chosen onto a port starts: the first free pixel there
    /// it fits in (its own pixels count as free), else the first free one,
    /// else 0.
    fn first_fit(
        &self,
        fixture: &NodeId,
        target: &PatchTarget,
        output: &NodeId,
        port: &str,
    ) -> u32 {
        let p = &self.project;
        let objects: Vec<ObjectId> = p.objects_of_place(fixture, target);
        let n: u32 = objects
            .iter()
            .filter_map(|o| p.object(o))
            .map(|o| o.lamps.len() as u32)
            .sum::<u32>()
            .max(1);
        let free = p.free_segments_for(output, port, n, &objects);
        free.iter()
            .find(|r| r.len() as u32 >= n)
            .or(free.first())
            .map_or(0, |r| r.start)
    }
}

fn direction_option(d: Direction) -> PatchOption {
    match d {
        Direction::AsDrawn => PatchOption::AsDrawn,
        Direction::Reversed => PatchOption::Reversed,
    }
}
