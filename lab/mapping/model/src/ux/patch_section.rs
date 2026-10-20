//! The inspector's Patch section (plan P6, R2): the same rows on a
//! fixture, a component and an object, editing that thing's placement
//! (`patch_place.rs`) — lp2014's group panel, as the inspector's own rows.
//!
//! ```text
//! Patch
//!   Wire        [ Follows circle1 ▾ ]   Follows <parent> · Auto onto <output> ·
//!                                       <output> · <port> (every port) · Not on the wire
//!   Start       [ − ]  40  [ + ]        only when its own wire is a port
//!   Direction   [ Follows circle1 ▾ ]   Follows <parent> · As drawn · Reversed
//!   Turn        [ − ]  0   [ + ]        objects only
//! ```
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
    /// − or + on a number row.
    Nudge { field: PatchField, by: i64 },
    /// A gap's "Patch here": this object (by its index in
    /// `Editor::patch_here_options`) at the gap's start.
    PatchHere { option: usize },
}

/// One option of a choice row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOption {
    /// Unset: follow the thing above.
    Follows,
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
                Target::Component(c) => {
                    let mut objects = p.objects_of(c);
                    match (objects.next(), objects.next()) {
                        (Some(o), None) => PatchTarget::Object(o.id.clone()),
                        _ => PatchTarget::Component(c.clone()),
                    }
                }
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
        let wires = self.wire_options(fixture, first);
        let chosen = self.shared(&targets, |f, t| {
            let own = self.own_wire(f, t);
            Some(match own {
                None => PatchOption::Follows,
                Some(Wire::Auto { output }) => PatchOption::Auto(output),
                Some(Wire::At { output, port, .. }) => PatchOption::Port(output, port),
                Some(Wire::Off) => PatchOption::Off,
            })
        });
        items.push(UiItem::PatchChoice {
            label: "Wire",
            options: wires
                .iter()
                .map(|o| self.option_words(fixture, first, o))
                .collect(),
            chosen: chosen.and_then(|c| wires.iter().position(|o| *o == c)),
            field: PatchField::Wire,
        });

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
        let directions = self.direction_options(first);
        let chosen = self.shared(&targets, |f, t| {
            Some(
                match self.project.fixture(f)?.place(t).and_then(|p| p.direction) {
                    None => PatchOption::Follows,
                    Some(Direction::AsDrawn) => PatchOption::AsDrawn,
                    Some(Direction::Reversed) => PatchOption::Reversed,
                },
            )
        });
        items.push(UiItem::PatchChoice {
            label: "Direction",
            options: directions
                .iter()
                .map(|o| self.option_words(fixture, first, o))
                .collect(),
            chosen: chosen.and_then(|c| directions.iter().position(|o| *o == c)),
            field: PatchField::Direction,
        });

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
        let Some((fixture, first)) = targets.first().cloned() else {
            return;
        };
        self.checkpoint();
        match input {
            PatchInput::Choose {
                field: PatchField::Wire,
                option,
            } => {
                let Some(o) = self.wire_options(&fixture, &first).get(option).cloned() else {
                    return;
                };
                for (f, t) in &targets {
                    let wire = match &o {
                        PatchOption::Follows => None,
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
                let Some(o) = self.direction_options(&first).get(option).cloned() else {
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
            PatchInput::Choose { .. } | PatchInput::Nudge { .. } | PatchInput::PatchHere { .. } => {
            }
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

    /// The Wire row's options for a target: follow its parent (not for a
    /// fixture), auto onto each output, every port, not on the wire.
    fn wire_options(&self, _fixture: &NodeId, target: &PatchTarget) -> Vec<PatchOption> {
        let p = &self.project;
        let mut out = Vec::new();
        if *target != PatchTarget::Fixture {
            out.push(PatchOption::Follows);
        }
        for n in p.outputs() {
            out.push(PatchOption::Auto(n.id.clone()));
        }
        for (output, port) in p.wire_ports() {
            out.push(PatchOption::Port(output, port));
        }
        out.push(PatchOption::Off);
        out
    }

    fn direction_options(&self, target: &PatchTarget) -> Vec<PatchOption> {
        let mut out = Vec::new();
        if *target != PatchTarget::Fixture {
            out.push(PatchOption::Follows);
        }
        out.push(PatchOption::AsDrawn);
        out.push(PatchOption::Reversed);
        out
    }

    /// An option in words. "Follows" says what it follows and what that is
    /// now, so an inherited value is never a mystery.
    fn option_words(&self, fixture: &NodeId, target: &PatchTarget, o: &PatchOption) -> String {
        let p = &self.project;
        match o {
            PatchOption::Follows => {
                let chain = p.place_chain(target);
                let parent = chain
                    .get(1)
                    .map(|t| p.target_words(fixture, t))
                    .unwrap_or_default();
                format!("Follows {parent}")
            }
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
