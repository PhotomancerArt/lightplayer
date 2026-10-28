//! The inspector's patching rows: the start and flow of a fixture or a
//! part, then what one node does on the walk, against what its part and
//! its parent's flow give it (theory "Patching"; the patcher is
//! `lpc_mapping::patching`, the one the engine runs).
//!
//! A start is stored as ops, one per field (Keep, Set, Add); the Start row
//! offers presets that write ops (C34): Follows (all Keep), a port (output
//! and port Set, channel Set to lamp 0) and Next port (port Add 1, channel
//! Set to lamp 0). A start that is none of those shows no choice.
//!
//! ```text
//! Part · circle1                       the part: its start and flow
//!   Flow        [ End to end ▾ ]         End to end · One per port from <port>
//!   Start       [ Follows ▾ ]            (containers, and parts in a repeat)
//!
//! Segment · circle1/r3                 the node, against what it is given
//!   Output      main                    from circle1's flow: one per port ↗
//!   Port        [ D2 of main ▾ ]        set here · Back to the flow
//!               ring r4 and the centre follow it
//!   Starts at   [ − ] 0 [ + ] lamps     follows ring r2 ↗
//!   Direction   [ Reverse ▾ ]           from circle1 ↗
//!   Offset      [ − ] 0 [ + ] lamps
//!   On the line [ Yes ▾ ]
//! ```
//!
//! Presentation A (the default, these rows) says where each value came
//! from (`start_words.rs`); presentation B (`presentation_b.rs`) shows the
//! flow's start as one line, then only what is set here. Both read the
//! same facts; Debug switches them (lean Q82).
//!
//! A row's options are worked out from the project, so a choice names this
//! project's own outputs and ports (by the name the patch says them by);
//! that is why these are rows of their own and not property descriptors.
//! They follow the same rules: every selected thing is edited together, a
//! value they don't share shows as mixed, and every edit is one undo step.
//! The keys do the same edits (`patching.rs`).

use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputName, OutputOp, PatchTarget, PortOp, Start, Unit,
};

use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::patch_edits::{moves_start, port_key};
use crate::domain::target::{Target, is_single_segment};
use crate::domain::walked::{Cursor, Walked};
use crate::ux::debug_flags::Presentation;
use crate::ux::editor::Editor;
use crate::ux::ui::{UiField, UiFieldValue, UiItem, UiSource};

/// Which patching row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchField {
    /// A fixture's or a part's Start.
    Start,
    /// The lamp its Start is at ("Starts at" under Start).
    StartPixel,
    /// A fixture's or a container's Flow.
    Children,
    /// A node's output (read here; the Port row chooses it).
    Output,
    /// A node's output and port.
    Port,
    /// A node's start on its port ("Starts at").
    Pixel,
    Direction,
    Offset,
    /// On the line, or off.
    OnWire,
}

/// What a patching row asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchInput {
    /// The option at this index of the row's list.
    Choose { field: PatchField, option: usize },
    /// Make a value from the flow the node's own, as it is now.
    Override { field: PatchField },
    /// Back to the flow: drop the node's own value.
    Reset { field: PatchField },
    /// − or + on a number row.
    Nudge { field: PatchField, by: i64 },
    /// Stays here: hold the cursor where it is, whatever the flow says (an
    /// explicit keep, G1 answer 7).
    Stay { field: PatchField },
    /// A gap's "Patch here": this segment (by its index in
    /// `Editor::patch_here_options`) at the gap's start.
    PatchHere { option: usize },
}

/// One option of a choice row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOption {
    Follows,
    At(NodeId, String),
    NextPort,
    EndToEnd,
    /// One per port from this port of this output (the flow keeps only the
    /// port's key; the output names it for the person).
    OnePerPort(NodeId, String),
    Port(NodeId, String),
    /// An explicit keep: the port the cursor is on.
    StaysHere,
    AsDrawn,
    Reversed,
    OnWire,
    Off,
}

impl Editor {
    /// What the selection's starts and flows are on: a fixture node, or a
    /// part.
    pub fn rule_targets(&self) -> Vec<(NodeId, PatchTarget)> {
        let p = &self.project;
        let mut out = Vec::new();
        for t in &self.selection {
            let entry = match t {
                Target::Node(id) if p.fixture(id).is_some() => (id.clone(), PatchTarget::Fixture),
                // A ring's or a copy's start and flow are its part's.
                Target::Part(_) | Target::Generated(_) => {
                    let Some(c) = t.owner() else { continue };
                    if p.part(&c).is_none() {
                        continue;
                    }
                    (c.fixture, PatchTarget::Part(c.part))
                }
                _ => continue,
            };
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
        out
    }

    /// The nodes the selection stands for: a part's own node, a generated
    /// one, a span's segment.
    pub fn patch_objects(&self) -> Vec<(NodeId, TreeRef)> {
        let p = &self.project;
        let mut out = Vec::new();
        for t in &self.selection {
            let o = match t {
                Target::Part(c) => c.node(),
                Target::Generated(o)
                | Target::Span {
                    segment: Some(o), ..
                } => o.clone(),
                _ => continue,
            };
            let Some(f) = p.fixture(&o.fixture).map(|_| o.fixture.clone()) else {
                continue;
            };
            if !out.iter().any(|(_, x)| *x == o) {
                out.push((f, o));
            }
        }
        out
    }

    /// The patching rows for the selection: its starts and flows, then its
    /// nodes'.
    pub fn patch_section(&self) -> Option<Vec<UiItem>> {
        let mut items = self.rule_rows().unwrap_or_default();
        items.extend(self.object_rows().unwrap_or_default());
        (!items.is_empty()).then_some(items)
    }

    /// The Start and Flow rows. `None` unless every selected thing has
    /// them.
    pub fn rule_rows(&self) -> Option<Vec<UiItem>> {
        let targets = self.rule_targets();
        if targets.is_empty() || targets.len() != self.selection.len() {
            return None;
        }
        Some(self.rule_rows_for(&targets))
    }

    /// The Start and Flow rows for these fixtures or parts: Flow on a
    /// container, then Start (lean Q83: a part's Start shows only on a
    /// container and on a part inside a repeat; a fixture always has one).
    pub fn rule_rows_for(&self, targets: &[(NodeId, PatchTarget)]) -> Vec<UiItem> {
        let targets = targets.to_vec();
        let fixture_only = targets.iter().all(|(_, t)| *t == PatchTarget::Fixture);
        let mut items = Vec::new();

        if targets.iter().all(|(_, t)| self.is_container(t)) {
            let options = self.children_options(&targets);
            let chosen = self.shared(&targets, |f, t| {
                Some(children_option(&self.rules_on(f, t).1))
            });
            items.push(UiItem::Field(UiField {
                label: "Flow",
                field: PatchField::Children,
                value: UiFieldValue::Choice {
                    options: options.iter().map(|o| self.option_words(o)).collect(),
                    chosen: chosen.and_then(|c| options.iter().position(|o| c.matches(o))),
                },
                source: None,
                note: None,
            }));
        }

        let shows_start = targets.iter().all(|(f, t)| match t {
            PatchTarget::Fixture => true,
            PatchTarget::Part(part) => {
                let r = PartRef::new(f, part);
                self.is_container(t) || self.project.in_repeat(&r)
            }
            PatchTarget::Node(_) => false,
        });
        if !shows_start {
            return items;
        }
        let starts = self.start_options(fixture_only);
        let chosen = self.shared(&targets, |f, t| self.start_option(&self.rules_on(f, t).0));
        let every_copy = targets.iter().all(|(f, t)| match t {
            PatchTarget::Part(part) => self.project.in_repeat(&PartRef::new(f, part)),
            _ => false,
        });
        items.push(UiItem::Field(UiField {
            label: "Start",
            field: PatchField::Start,
            value: UiFieldValue::Choice {
                options: starts.iter().map(|o| self.option_words(o)).collect(),
                chosen: chosen.and_then(|c| starts.iter().position(|o| *o == c)),
            },
            source: None,
            note: every_copy.then(|| "every copy starts this way".to_string()),
        }));
        if targets.iter().all(|(f, t)| {
            matches!(
                self.start_option(&self.rules_on(f, t).0),
                Some(PatchOption::At(..))
            )
        }) {
            let lamp = self.shared(&targets, |f, t| match &self.rules_on(f, t).0.channel {
                ChannelOp::Set(amount) => {
                    Some(channels(*amount, self.per_lamp(f)) as i64 / self.per_lamp(f) as i64)
                }
                _ => None,
            });
            items.push(UiItem::Field(UiField {
                label: "Starts at",
                field: PatchField::StartPixel,
                value: UiFieldValue::Number {
                    value: lamp,
                    unit: "lamps",
                    stays: false,
                },
                source: None,
                note: None,
            }));
        }
        items
    }

    /// The node rows: where it is on the walk and why. `None` unless every
    /// selected thing stands for a node.
    pub fn object_rows(&self) -> Option<Vec<UiItem>> {
        let objects = self.patch_objects();
        if objects.is_empty() || objects.len() != self.selection.len() {
            return None;
        }
        self.object_rows_for(&objects)
    }

    /// The node rows for these nodes, in the presentation Debug chose;
    /// `None` when one isn't on the walk.
    pub fn object_rows_for(&self, objects: &[(NodeId, TreeRef)]) -> Option<Vec<UiItem>> {
        let fields = self.node_fields(objects)?;
        Some(match self.debug.presentation {
            Presentation::A => fields.into_iter().map(UiItem::Field).collect(),
            Presentation::B => self.presentation_b(objects, fields),
        })
    }

    /// Every row a node has, each with its value and where it came from
    /// (presentation A; B keeps only those set here).
    pub fn node_fields(&self, objects: &[(NodeId, TreeRef)]) -> Option<Vec<UiField>> {
        let p = &self.project;
        let patch = p.effective_patch();
        let walked: Vec<(&NodeId, &TreeRef, &Walked)> = objects
            .iter()
            .filter_map(|(f, o)| Some((f, o, patch.walked(o)?)))
            .collect();
        if walked.len() != objects.len() {
            return None;
        }
        let own = |o: &TreeRef| p.override_of(o).cloned().unwrap_or_default();
        let segments = walked
            .iter()
            .all(|(_, o, _)| p.placed().node(o).is_some_and(|(_, n)| n.is_leaf()));
        let mut fields = Vec::new();

        if segments {
            let place = |w: &Walked| -> Option<(NodeId, String)> {
                let at = w.landed.as_ref().unwrap_or(&w.at);
                Some((at.output.clone()?, at.port.clone()?))
            };
            // Output: read here; the Port row chooses it.
            let output = self.shared_walked(&walked, |w| {
                let at = w.landed.as_ref().unwrap_or(&w.at);
                Some(match (&at.output, &at.output_name) {
                    (Some(o), _) => p.output_words(o),
                    (None, Some(name)) => format!("{name}, which isn't there"),
                    (None, None) => "nowhere yet".into(),
                })
            });
            fields.push(UiField {
                label: "Output",
                field: PatchField::Output,
                value: UiFieldValue::Text(output.unwrap_or_else(|| "mixed".into())),
                source: self.shared_source(&walked, 0),
                note: None,
            });

            // Port: output and port together, with the presets.
            let ports = self.node_port_options();
            let chosen = self.shared_walked(&walked, |w| {
                place(w).map(|(out, port)| PatchOption::Port(out, port))
            });
            let mut notes = Vec::new();
            if let [(_, o, w)] = walked.as_slice() {
                if w.wrapped {
                    notes.push("moved on to the next port: it didn't fit".to_string());
                }
                if moves_start(&own(o))
                    && !w.off
                    && let Some(words) = p.follows_it_words(o)
                {
                    notes.push(words);
                }
            }
            fields.push(UiField {
                label: "Port",
                field: PatchField::Port,
                value: UiFieldValue::Choice {
                    options: ports.iter().map(|o| self.option_words(o)).collect(),
                    chosen: chosen.and_then(|c| ports.iter().position(|o| *o == c)),
                },
                source: self.shared_source(&walked, 1),
                note: (!notes.is_empty()).then(|| notes.join(" · ")),
            });

            // Starts at: in lamps, in channels only off a lamp boundary.
            let channel = |w: &Walked| w.landed.as_ref().unwrap_or(&w.at).channel;
            let off_boundary = walked
                .iter()
                .any(|(f, _, w)| channel(w) % self.per_lamp(f) != 0);
            let value = self.shared_walked(&walked, |w| {
                Some(if off_boundary {
                    channel(w) as i64
                } else {
                    channel(w) as i64 / self.per_lamp(&w.fixture) as i64
                })
            });
            fields.push(UiField {
                label: "Starts at",
                field: PatchField::Pixel,
                value: UiFieldValue::Number {
                    value,
                    unit: if off_boundary { "channels" } else { "lamps" },
                    stays: true,
                },
                source: self.shared_source(&walked, 2),
                note: None,
            });
        }

        // Direction.
        let options = [PatchOption::AsDrawn, PatchOption::Reversed];
        let direction = self.shared_walked(&walked, |w| Some(direction_option(w.direction)));
        let ring_note = match walked.as_slice() {
            [(_, o, _)] if p.ring_of(o).is_some() => Some(
                "this ring only, on the line: its lamps stay where they are (the circle's \
                 Counter-clockwise turns every ring)"
                    .to_string(),
            ),
            _ => None,
        };
        fields.push(UiField {
            label: "Direction",
            field: PatchField::Direction,
            value: UiFieldValue::Choice {
                options: options.iter().map(|o| self.option_words(o)).collect(),
                chosen: direction.and_then(|c| options.iter().position(|o| *o == c)),
            },
            source: self.shared_override_source(&walked, |x| x.direction.is_some()),
            note: ring_note,
        });

        // Offset: segments only.
        if segments {
            fields.push(UiField {
                label: "Offset",
                field: PatchField::Offset,
                value: UiFieldValue::Number {
                    value: self.shared_walked(&walked, |w| Some(w.offset as i64)),
                    unit: "lamps",
                    stays: false,
                },
                source: self.shared_override_source(&walked, |x| x.offset.is_some()),
                note: None,
            });
        }

        // On the line.
        let options = [PatchOption::OnWire, PatchOption::Off];
        let chosen = self.shared_walked(&walked, |w| {
            Some(if w.off {
                PatchOption::Off
            } else {
                PatchOption::OnWire
            })
        });
        let note = match walked.as_slice() {
            [(_, o, w)] if w.off => p.moves_back_words(o),
            _ => None,
        };
        fields.push(UiField {
            label: "On the line",
            field: PatchField::OnWire,
            value: UiFieldValue::Choice {
                options: options.iter().map(|o| self.option_words(o)).collect(),
                chosen: chosen.and_then(|c| options.iter().position(|o| *o == c)),
            },
            source: self.shared_override_source(&walked, |x| x.off),
            note,
        });
        Some(fields)
    }

    /// What a gap's "Patch here" offers: the segments not on the line, in
    /// tree order.
    pub fn patch_here_options(&self) -> Vec<TreeRef> {
        let p = &self.project;
        p.placed_segments()
            .iter()
            .filter(|(_, o)| !o.lamps.is_empty() && !p.effective_patch().lands(&o.id))
            .map(|(_, o)| o.id.clone())
            .collect()
    }

    /// Apply a patching row's edit to every selected thing, as one undo
    /// step.
    pub fn patch_input(&mut self, input: PatchInput) {
        if let PatchInput::PatchHere { option } = input {
            let gap = match self.selection.as_slice() {
                [t @ Target::Span { segment: None, .. }] => self.project.span_of(t),
                _ => None,
            };
            let object = self.patch_here_options().get(option).cloned();
            if let (Some(gap), Some(object)) = (gap, object) {
                self.checkpoint();
                self.project
                    .patch_segment(&object, &gap.output, &gap.port, gap.pixels.start);
                self.after_edit();
                self.selection = vec![self.project.tree_target(&object)];
            }
            return;
        }
        let rules = self.rule_targets();
        let objects = self.patch_objects();
        self.checkpoint();
        match input {
            PatchInput::Choose {
                field: PatchField::Start,
                option,
            } => {
                let fixture_only = rules.iter().all(|(_, t)| *t == PatchTarget::Fixture);
                let Some(o) = self.start_options(fixture_only).get(option).cloned() else {
                    return;
                };
                for (f, t) in &rules {
                    let start = match &o {
                        PatchOption::Follows => Start::KEEP,
                        PatchOption::NextPort => Start::next_port(),
                        PatchOption::At(out, port) => {
                            match self.project.start_ops_at(out, port, 0) {
                                Some(start) => start,
                                None => continue,
                            }
                        }
                        _ => continue,
                    };
                    self.project.set_start(f, t, start);
                }
            }
            PatchInput::Nudge {
                field: PatchField::StartPixel,
                by,
            } => {
                for (f, t) in &rules {
                    let start = self.rules_on(f, t).0;
                    if let ChannelOp::Set(channel) = start.channel {
                        let per = self.per_lamp(f) as i64;
                        let px = (channels(channel, per as u32) as i64 / per + by).max(0) as u32;
                        self.project.set_start(
                            f,
                            t,
                            Start {
                                channel: ChannelOp::Set(Amount::lamps(px)),
                                ..start
                            },
                        );
                    }
                }
            }
            PatchInput::Choose {
                field: PatchField::Children,
                option,
            } => {
                let Some(o) = self.children_options(&rules).get(option).cloned() else {
                    return;
                };
                for (f, t) in &rules {
                    let flow = match &o {
                        PatchOption::EndToEnd => Flow::EndToEnd,
                        PatchOption::OnePerPort(_, from) => match port_key(from) {
                            Some(from) => Flow::OnePerPort { from },
                            None => continue,
                        },
                        _ => continue,
                    };
                    self.project.set_flow(f, t, flow);
                }
            }
            PatchInput::Choose {
                field: PatchField::Port,
                option,
            } => match self.node_port_options().get(option).cloned() {
                // A port of its own starts it at that port's first lamp:
                // "ring 3 is on D9, from its first lamp".
                Some(PatchOption::Port(out, port)) => {
                    for (_, o) in &objects {
                        self.project.patch_segment(o, &out, &port, 0);
                    }
                }
                Some(PatchOption::NextPort) => {
                    for (_, o) in &objects {
                        self.project.edit_override(o, |x| {
                            x.output = None;
                            x.port = Some(PortOp::Add(1));
                            x.channel = Some(ChannelOp::Set(Amount::lamps(0)));
                            x.off = false;
                        });
                    }
                }
                Some(PatchOption::StaysHere) => {
                    for (_, o) in &objects {
                        self.project.edit_override(o, |x| {
                            x.output = Some(OutputOp::Keep);
                            x.port = Some(PortOp::Keep);
                        });
                    }
                }
                _ => return,
            },
            PatchInput::Stay {
                field: PatchField::Port,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| {
                        x.output = Some(OutputOp::Keep);
                        x.port = Some(PortOp::Keep);
                    });
                }
            }
            PatchInput::Stay {
                field: PatchField::Pixel,
            } => {
                for (_, o) in &objects {
                    self.project
                        .edit_override(o, |x| x.channel = Some(ChannelOp::Keep));
                }
            }
            PatchInput::Override {
                field: PatchField::Port | PatchField::Output,
            } => {
                for (_, o) in &objects {
                    if let Some(at) = self.landed(o) {
                        self.project.edit_override(o, |x| {
                            x.output = at
                                .output_name
                                .as_deref()
                                .map(|n| OutputOp::Set(OutputName::new(n)));
                            x.port = at
                                .port
                                .as_deref()
                                .and_then(port_key)
                                .map(|k| PortOp::set(k.0));
                            x.channel = Some(ChannelOp::Set(Amount::channels(at.channel)));
                        });
                    }
                }
            }
            PatchInput::Reset {
                field: PatchField::Port,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| {
                        x.output = None;
                        x.port = None;
                        x.channel = None;
                    });
                }
            }
            PatchInput::Reset {
                field: PatchField::Output,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.output = None);
                }
            }
            PatchInput::Override {
                field: PatchField::Pixel,
            } => {
                for (_, o) in &objects {
                    if let Some(at) = self.landed(o) {
                        self.project.edit_override(o, |x| {
                            x.channel = Some(ChannelOp::Set(Amount::channels(at.channel)))
                        });
                    }
                }
            }
            PatchInput::Reset {
                field: PatchField::Pixel,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.channel = None);
                }
            }
            PatchInput::Nudge {
                field: PatchField::Pixel,
                by,
            } => {
                for (f, o) in &objects {
                    let per = self.per_lamp(f);
                    if let Some(at) = self.landed(o) {
                        let px = (at.channel as i64 / per as i64 + by).max(0) as u32;
                        self.project.edit_override(o, |x| {
                            x.channel = Some(ChannelOp::Set(Amount::lamps(px)))
                        });
                    }
                }
            }
            PatchInput::Choose {
                field: PatchField::Direction,
                option,
            } => {
                let d = if option == 1 {
                    Direction::Reverse
                } else {
                    Direction::Forward
                };
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.direction = Some(d));
                }
            }
            PatchInput::Override {
                field: PatchField::Direction,
            } => {
                for (_, o) in &objects {
                    let d = self
                        .project
                        .effective_patch()
                        .walked(o)
                        .map(|w| w.direction)
                        .unwrap_or_default();
                    self.project.edit_override(o, |x| x.direction = Some(d));
                }
            }
            PatchInput::Reset {
                field: PatchField::Direction,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.direction = None);
                }
            }
            PatchInput::Nudge {
                field: PatchField::Offset,
                by,
            } => {
                for (_, o) in &objects {
                    self.project.turn_placement(o, by);
                }
            }
            PatchInput::Override {
                field: PatchField::Offset,
            } => {
                for (_, o) in &objects {
                    let offset = self
                        .project
                        .effective_patch()
                        .walked(o)
                        .map_or(0, |w| w.offset);
                    self.project.edit_override(o, |x| x.offset = Some(offset));
                }
            }
            PatchInput::Reset {
                field: PatchField::Offset,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.offset = None);
                }
            }
            PatchInput::Choose {
                field: PatchField::OnWire,
                option,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.off = option == 1);
                }
            }
            // Off is the only value On the line can be set to here.
            PatchInput::Override {
                field: PatchField::OnWire,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.off = true);
                }
            }
            PatchInput::Reset {
                field: PatchField::OnWire,
            } => {
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| x.off = false);
                }
            }
            PatchInput::Choose { .. }
            | PatchInput::Override { .. }
            | PatchInput::Reset { .. }
            | PatchInput::Nudge { .. }
            | PatchInput::Stay { .. }
            | PatchInput::PatchHere { .. } => {}
        }
        self.after_edit();
    }

    /// A problem's Remove: take away an override whose node isn't there
    /// any more, as one undo step. Nothing for a node that is.
    pub fn remove_gone_override(&mut self, node: &TreeRef) {
        if self.project.placed().node(node).is_some() || self.project.override_of(node).is_none()
        {
            return;
        }
        self.checkpoint();
        if let Some(f) = self.project.fixture_mut(&node.fixture) {
            f.mapping.overrides.remove(&node.id);
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

    fn shared_walked<V: PartialEq>(
        &self,
        walked: &[(&NodeId, &TreeRef, &Walked)],
        value: impl Fn(&Walked) -> Option<V>,
    ) -> Option<V> {
        let mut it = walked.iter().map(|(_, _, w)| value(w));
        let first = it.next()??;
        it.all(|v| v.as_ref() == Some(&first)).then_some(first)
    }

    /// Where one step of every selected node came from, when they agree.
    fn shared_source(
        &self,
        walked: &[(&NodeId, &TreeRef, &Walked)],
        field: usize,
    ) -> Option<UiSource> {
        agreed(
            walked
                .iter()
                .map(|(_, _, w)| self.project.step_source(w, field)),
        )
    }

    /// Where a direction, offset or Off of every selected node came from,
    /// when they agree.
    fn shared_override_source(
        &self,
        walked: &[(&NodeId, &TreeRef, &Walked)],
        has: impl Fn(&lpc_mapping::patching::Override) -> bool + Copy,
    ) -> Option<UiSource> {
        agreed(
            walked
                .iter()
                .map(|(_, o, _)| self.project.override_source(o, has)),
        )
    }

    /// The start and flow of a fixture or a part (a node's are its part's).
    fn rules_on(&self, fixture: &NodeId, t: &PatchTarget) -> (Start, Flow) {
        let Some(f) = self.project.fixture(fixture) else {
            return (Start::KEEP, Flow::EndToEnd);
        };
        let part = match t {
            PatchTarget::Fixture => return (f.mapping.start.clone(), f.mapping.flow.clone()),
            PatchTarget::Part(c) => f.part(c),
            PatchTarget::Node(o) => f.part(&o.part),
        };
        part.map_or((Start::KEEP, Flow::EndToEnd), |p| {
            (p.start.clone(), p.flow.clone())
        })
    }

    /// A start as the Start row's preset it is, when it is one.
    fn start_option(&self, s: &Start) -> Option<PatchOption> {
        if s.is_keep() {
            return Some(PatchOption::Follows);
        }
        if *s == Start::next_port() {
            return Some(PatchOption::NextPort);
        }
        match (&s.output, &s.port, &s.channel) {
            (OutputOp::Set(name), PortOp::Set { key, plus: 0 }, ChannelOp::Set(_)) => Some(
                PatchOption::At(self.project.output_named(name.as_str())?, key.0.to_string()),
            ),
            _ => None,
        }
    }

    pub(crate) fn per_lamp(&self, fixture: &NodeId) -> u32 {
        self.project
            .fixture(fixture)
            .map_or(3, |f| f.channels_per_lamp())
    }

    /// Whether a start-and-flow target has children: the fixture, a group,
    /// a circle, a repeat.
    fn is_container(&self, t: &PatchTarget) -> bool {
        match t {
            PatchTarget::Fixture => true,
            PatchTarget::Part(_) => self
                .rule_part(t)
                .is_some_and(|c| !is_single_segment(&c.kind)),
            PatchTarget::Node(_) => false,
        }
    }

    /// The part a start-and-flow target names, in the selection's fixture.
    fn rule_part(&self, t: &PatchTarget) -> Option<&lpc_mapping::part::Part> {
        let PatchTarget::Part(c) = t else { return None };
        self.rule_targets()
            .iter()
            .find(|(_, x)| x == t)
            .and_then(|(f, _)| self.project.part(&PartRef::new(f, c)))
    }

    /// Where a node's span starts now (after any wrap).
    fn landed(&self, object: &TreeRef) -> Option<Cursor> {
        let w = self.project.effective_patch().walked(object)?;
        Some(w.landed.clone().unwrap_or_else(|| w.at.clone()))
    }

    /// Start's options: follows (not for a fixture, which must say), each
    /// port, the next port (not for a fixture).
    fn start_options(&self, fixture: bool) -> Vec<PatchOption> {
        let mut out = Vec::new();
        if !fixture {
            out.push(PatchOption::Follows);
        }
        for (output, port) in self.project.wire_ports() {
            out.push(PatchOption::At(output, port));
        }
        if !fixture {
            out.push(PatchOption::NextPort);
        }
        out
    }

    /// Flow's options: end to end, or one per port from each port of the
    /// output the targets are on (none when that isn't one output).
    fn children_options(&self, targets: &[(NodeId, PatchTarget)]) -> Vec<PatchOption> {
        let p = &self.project;
        let output = self.shared(targets, |f, t| {
            let walked = match t {
                PatchTarget::Fixture => p.effective_patch().walked.iter().find(|w| &w.fixture == f),
                PatchTarget::Part(c) => p.effective_patch().walked(&PartRef::new(f, c).node()),
                PatchTarget::Node(o) => p.effective_patch().walked(&TreeRef::new(f, o.clone())),
            };
            walked?.at.output.clone()
        });
        let mut out = vec![PatchOption::EndToEnd];
        for (o, port) in p.wire_ports() {
            if output.as_ref() == Some(&o) {
                out.push(PatchOption::OnePerPort(o, port));
            }
        }
        out
    }

    /// The Port row's options: every port of every output (At <port>),
    /// then Next port and Stays here (presets that write ops, Q26).
    fn node_port_options(&self) -> Vec<PatchOption> {
        let mut out: Vec<PatchOption> = self
            .project
            .wire_ports()
            .into_iter()
            .map(|(o, p)| PatchOption::Port(o, p))
            .collect();
        out.push(PatchOption::NextPort);
        out.push(PatchOption::StaysHere);
        out
    }

    /// An option in words.
    pub(crate) fn option_words(&self, o: &PatchOption) -> String {
        let p = &self.project;
        match o {
            PatchOption::Follows => "Follows".into(),
            PatchOption::At(out, port) | PatchOption::Port(out, port) => p.port_words(out, port),
            PatchOption::NextPort => "Next port".into(),
            PatchOption::StaysHere => "Stays here".into(),
            PatchOption::EndToEnd => "End to end".into(),
            PatchOption::OnePerPort(out, port) => {
                format!("One per port from {}", p.pin_words(out, port))
            }
            PatchOption::AsDrawn => "Forward".into(),
            PatchOption::Reversed => "Reverse".into(),
            PatchOption::OnWire => "Yes".into(),
            PatchOption::Off => "Off".into(),
        }
    }
}

/// One value if every one agrees, `Mixed` when they don't, `None` when
/// none says anything.
fn agreed(mut sources: impl Iterator<Item = Option<UiSource>>) -> Option<UiSource> {
    let first = sources.next()?;
    if sources.all(|s| s == first) {
        first
    } else {
        Some(UiSource::Mixed)
    }
}

/// An amount in channels, for lamps of `per_lamp` channels.
fn channels(amount: Amount, per_lamp: u32) -> u32 {
    match amount.unit {
        Unit::Lamps => amount.n.saturating_mul(per_lamp),
        Unit::Channels => amount.n,
    }
}

/// A flow as the option it is among `options` (One per port is matched by
/// its port's key).
fn children_option(c: &Flow) -> ChildrenKey {
    match c {
        Flow::EndToEnd => ChildrenKey::EndToEnd,
        Flow::OnePerPort { from } => ChildrenKey::OnePerPort(from.0.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChildrenKey {
    EndToEnd,
    OnePerPort(String),
}

impl ChildrenKey {
    fn matches(&self, o: &PatchOption) -> bool {
        match (self, o) {
            (ChildrenKey::EndToEnd, PatchOption::EndToEnd) => true,
            (ChildrenKey::OnePerPort(a), PatchOption::OnePerPort(_, b)) => a == b,
            _ => false,
        }
    }
}

fn direction_option(d: Direction) -> PatchOption {
    match d {
        Direction::Forward => PatchOption::AsDrawn,
        Direction::Reverse => PatchOption::Reversed,
    }
}
