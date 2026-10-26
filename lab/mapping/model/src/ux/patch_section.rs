//! The inspector's patching rows: the rules on a fixture or a component,
//! then what one object does on the walk, against the rule its component
//! renders for it (theory "Patching"; the walker is `walker.rs`).
//!
//! ```text
//! circle1 · circle                     the component's rules
//!   Start       [ Follows ▾ ]            Follows · <output> <port> · Next port
//!   Children    [ End to end ▾ ]         End to end · One per port from <port>
//!
//! r3 · ring                            the object, against its rule
//!   Port        D0 of main  · from circle1: end to end ↗   [Override]
//!   Pixel       24          · after ring r2 ↗              [Override]
//!   Port        [ D9 of main ▾ ]  [Reset]                  its own
//!               ring r4 and ring r5 follow it              what moves with it
//!   Direction   As drawn                                   [Override]
//!   Offset      [ − ] 0 [ + ]
//!   On the wire [ Yes ▾ ]
//! ```
//!
//! A value that comes from a rule says what it is and where it came from,
//! never just "follows": the link selects the rule's owner. Override makes
//! the value the object's own without moving anything; Reset hands it back.
//!
//! A row's options are worked out from the project, so a choice names this
//! project's own outputs and ports; that is why these are rows of their
//! own and not property descriptors (whose choices are fixed lists). They
//! follow the same rules: every selected thing is edited together, a
//! value they don't share shows as mixed, and every edit is one undo
//! step. The keys do the same edits (`patching.rs`).

use crate::domain::cursor_op::{Amount, ChannelOp, Cursor, OutputOp, PortOp};
use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::object_override::Direction;
use crate::domain::patch_rule::{Children, ComponentRules, Start};
use crate::domain::patch_target::PatchTarget;
use crate::domain::target::Target;
use crate::domain::walker::{StepSource, Walked};
use crate::ux::editor::Editor;
use crate::ux::ui::UiItem;

/// Which patching row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchField {
    /// A fixture's or a component's Start rule.
    Start,
    /// The pixel its Start is at.
    StartPixel,
    /// A fixture's or a container's Children rule.
    Children,
    /// An object's output and port.
    Port,
    /// An object's start on its port.
    Pixel,
    Direction,
    Offset,
    /// On the wire, or off.
    OnWire,
}

/// What a patching row asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchInput {
    /// The option at this index of the row's list.
    Choose { field: PatchField, option: usize },
    /// Make a value from a rule the object's own, as it is now.
    Override { field: PatchField },
    /// Drop the object's own value and take its rule's again.
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
    Follows,
    At(NodeId, String),
    NextPort,
    EndToEnd,
    /// One per port from this port of this output (the rule keeps only the
    /// port's key; the output names it for the person).
    OnePerPort(NodeId, String),
    Port(NodeId, String),
    AsDrawn,
    Reversed,
    OnWire,
    Off,
}

impl Editor {
    /// What the selection's rules are on: a fixture node, or a component.
    pub fn rule_targets(&self) -> Vec<(NodeId, PatchTarget)> {
        let p = &self.project;
        let mut out = Vec::new();
        for t in &self.selection {
            let entry = match t {
                Target::Node(id) if p.fixture(id).is_some() => (id.clone(), PatchTarget::Fixture),
                // A ring's or a copy's rules are its component's.
                Target::Component(c) | Target::Object(ObjectId { component: c, .. }) => {
                    match p.fixture_of(c) {
                        Some(n) => (n.id.clone(), PatchTarget::Component(c.clone())),
                        None => continue,
                    }
                }
                _ => continue,
            };
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
        out
    }

    /// The object nodes the selection stands for: a component's own node,
    /// an object, a span's object.
    pub fn patch_objects(&self) -> Vec<(NodeId, ObjectId)> {
        let p = &self.project;
        let mut out = Vec::new();
        for t in &self.selection {
            let o = match t {
                Target::Component(c) => ObjectId::of(c),
                Target::Object(o)
                | Target::Span {
                    object: Some(o), ..
                } => o.clone(),
                _ => continue,
            };
            let Some(f) = p.fixture_of_place(&PatchTarget::Object(o.clone())) else {
                continue;
            };
            if !out.iter().any(|(_, x)| *x == o) {
                out.push((f, o));
            }
        }
        out
    }

    /// The patching rows for the selection: its rules, then its objects'.
    pub fn patch_section(&self) -> Option<Vec<UiItem>> {
        let mut items = self.rule_rows().unwrap_or_default();
        items.extend(self.object_rows().unwrap_or_default());
        (!items.is_empty()).then_some(items)
    }

    /// The rules rows: Start, and on a container Children. `None` unless
    /// every selected thing has rules.
    pub fn rule_rows(&self) -> Option<Vec<UiItem>> {
        let targets = self.rule_targets();
        if targets.is_empty() || targets.len() != self.selection.len() {
            return None;
        }
        Some(self.rule_rows_for(&targets))
    }

    /// The rules rows for these fixtures or components.
    pub fn rule_rows_for(&self, targets: &[(NodeId, PatchTarget)]) -> Vec<UiItem> {
        let targets = targets.to_vec();
        let fixture_only = targets.iter().all(|(_, t)| *t == PatchTarget::Fixture);
        let mut items = Vec::new();

        let starts = self.start_options(fixture_only);
        let chosen = self.shared(&targets, |f, t| Some(start_option(&self.rules_on(f, t).start)));
        items.push(UiItem::PatchChoice {
            label: "Start",
            options: starts.iter().map(|o| self.option_words(o)).collect(),
            chosen: chosen.and_then(|c| starts.iter().position(|o| *o == c)),
            reset: None,
            field: PatchField::Start,
        });
        if targets
            .iter()
            .all(|(f, t)| matches!(self.rules_on(f, t).start, Start::At { .. }))
        {
            let pixel = self.shared(&targets, |f, t| match &self.rules_on(f, t).start {
                Start::At { channel, .. } => {
                    Some(channel.channels(self.per_lamp(f)) as i64 / self.per_lamp(f) as i64)
                }
                _ => None,
            });
            items.push(UiItem::PatchNumber {
                label: "Pixel",
                value: pixel,
                note: None,
                reset: None,
                field: PatchField::StartPixel,
            });
        }
        if targets.iter().all(|(_, t)| self.is_container(t)) {
            let options = self.children_options(&targets);
            let chosen = self.shared(&targets, |f, t| {
                Some(children_option(&self.rules_on(f, t).children))
            });
            items.push(UiItem::PatchChoice {
                label: "Children",
                options: options.iter().map(|o| self.option_words(o)).collect(),
                chosen: chosen.and_then(|c| options.iter().position(|o| c.matches(o))),
                reset: None,
                field: PatchField::Children,
            });
        }
        items
    }

    /// The object rows: where it is on the walk and why, against its rule.
    /// `None` unless every selected thing stands for an object.
    pub fn object_rows(&self) -> Option<Vec<UiItem>> {
        let objects = self.patch_objects();
        if objects.is_empty() || objects.len() != self.selection.len() {
            return None;
        }
        self.object_rows_for(&objects)
    }

    /// The object rows for these objects; `None` when one isn't on the walk.
    pub fn object_rows_for(&self, objects: &[(NodeId, ObjectId)]) -> Option<Vec<UiItem>> {
        let p = &self.project;
        let patch = p.effective_patch();
        let walked: Vec<(&NodeId, &ObjectId, &Walked)> = objects
            .iter()
            .filter_map(|(f, o)| Some((f, o, patch.walked(o)?)))
            .collect();
        if walked.len() != objects.len() {
            return None;
        }
        let own = |o: &ObjectId| p.override_of(o).cloned().unwrap_or_default();
        let mut items = Vec::new();

        // Port: output and port together.
        let port_own = walked.iter().all(|(_, o, _)| {
            let x = own(o);
            x.output.is_some() || x.port.is_some()
        });
        let place = |w: &Walked| -> Option<(NodeId, String)> {
            let at = w.landed.as_ref().unwrap_or(&w.at);
            Some((at.output.clone()?, at.port.clone()?))
        };
        let nowhere = walked.iter().all(|(_, _, w)| place(w).is_none());
        if port_own || nowhere {
            let ports = self.port_options();
            let chosen = self.shared_walked(&walked, |w| {
                place(w).map(|(out, port)| PatchOption::Port(out, port))
            });
            items.push(UiItem::PatchChoice {
                label: "Port",
                options: ports.iter().map(|o| self.option_words(o)).collect(),
                chosen: chosen.and_then(|c| ports.iter().position(|o| *o == c)),
                reset: port_own.then(|| "its rule".into()),
                field: PatchField::Port,
            });
        } else {
            let value = self.shared_walked(&walked, |w| {
                let words = match place(w) {
                    Some((out, port)) => self.port_words(&out, &port),
                    None => "nowhere yet".into(),
                };
                Some(if w.wrapped {
                    format!("{words} · moved on: it didn't fit")
                } else {
                    words
                })
            });
            items.push(UiItem::PatchInherited {
                label: "Port",
                value: value.unwrap_or_else(|| "mixed".into()),
                from: self.shared_source(&walked, 1),
                field: PatchField::Port,
            });
        }

        // Pixel: its start on the port.
        let pixel = self.shared_walked(&walked, |w| {
            let at = w.landed.as_ref().unwrap_or(&w.at);
            Some(at.channel as i64 / self.per_lamp(&w.fixture) as i64)
        });
        let pixel_own = walked.iter().all(|(_, o, _)| own(o).channel.is_some());
        if pixel_own {
            items.push(UiItem::PatchNumber {
                label: "Pixel",
                value: pixel,
                note: None,
                reset: Some("its rule".into()),
                field: PatchField::Pixel,
            });
        } else {
            items.push(UiItem::PatchInherited {
                label: "Pixel",
                value: pixel.map_or_else(|| "mixed".into(), |v| v.to_string()),
                from: self.shared_source(&walked, 2),
                field: PatchField::Pixel,
            });
        }
        if let [(f, o, _)] = walked.as_slice()
            && own(o).moves_start()
            && let Some(words) = self.followers_words(f, o)
        {
            items.push(UiItem::Hint(words));
        }

        // Direction.
        let direction = self.shared_walked(&walked, |w| Some(direction_option(w.direction)));
        if walked.iter().all(|(_, o, _)| own(o).direction.is_some()) {
            let options = [PatchOption::AsDrawn, PatchOption::Reversed];
            items.push(UiItem::PatchChoice {
                label: "Direction",
                options: options.iter().map(|o| self.option_words(o)).collect(),
                chosen: direction.and_then(|c| options.iter().position(|o| *o == c)),
                reset: Some("as drawn".into()),
                field: PatchField::Direction,
            });
        } else {
            items.push(UiItem::PatchInherited {
                label: "Direction",
                value: direction.map_or_else(|| "mixed".into(), |d| self.option_words(&d)),
                from: None,
                field: PatchField::Direction,
            });
        }

        // Offset: leaves only.
        if walked
            .iter()
            .all(|(_, o, _)| p.placed().node(o).is_some_and(|(_, n)| n.is_leaf()))
        {
            let offset = self.shared_walked(&walked, |w| Some(w.offset as i64));
            items.push(UiItem::PatchNumber {
                label: "Offset",
                value: offset,
                note: Some("lamps".into()),
                reset: walked
                    .iter()
                    .all(|(_, o, _)| own(o).offset.is_some())
                    .then(|| "none".into()),
                field: PatchField::Offset,
            });
        }

        // On the wire.
        let options = [PatchOption::OnWire, PatchOption::Off];
        let chosen = self.shared_walked(&walked, |w| {
            Some(if w.off {
                PatchOption::Off
            } else {
                PatchOption::OnWire
            })
        });
        items.push(UiItem::PatchChoice {
            label: "On the wire",
            options: options.iter().map(|o| self.option_words(o)).collect(),
            chosen: chosen.and_then(|c| options.iter().position(|o| *o == c)),
            reset: None,
            field: PatchField::OnWire,
        });
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

    /// Apply a patching row's edit to every selected thing, as one undo
    /// step.
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
                        PatchOption::Follows => Start::Follows,
                        PatchOption::NextPort => Start::NextPort,
                        PatchOption::At(out, port) => Start::At {
                            output: out.clone(),
                            port: port.clone(),
                            channel: Amount::ZERO,
                        },
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
                    if let Start::At {
                        output,
                        port,
                        channel,
                    } = self.rules_on(f, t).start
                    {
                        let per = self.per_lamp(f) as i64;
                        let px = (channel.channels(per as u32) as i64 / per + by).max(0) as u32;
                        self.project.set_start(
                            f,
                            t,
                            Start::At {
                                output,
                                port,
                                channel: Amount::px(px),
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
                    let children = match &o {
                        PatchOption::EndToEnd => Children::EndToEnd,
                        PatchOption::OnePerPort(_, from) => {
                            Children::OnePerPort { from: from.clone() }
                        }
                        _ => continue,
                    };
                    self.project.set_children(f, t, children);
                }
            }
            PatchInput::Choose {
                field: PatchField::Port,
                option,
            } => {
                let Some(PatchOption::Port(out, port)) = self.port_options().get(option).cloned()
                else {
                    return;
                };
                // A port of its own starts it at that port's first pixel:
                // "ring 3 is on D9, ch 0".
                for (_, o) in &objects {
                    self.project.edit_override(o, |x| {
                        x.output = Some(OutputOp::Set(out.clone()));
                        x.port = Some(PortOp::set(port.clone()));
                        x.channel = Some(ChannelOp::Set(Amount::ZERO));
                        x.off = false;
                    });
                }
            }
            PatchInput::Override {
                field: PatchField::Port,
            } => {
                for (_, o) in &objects {
                    if let Some(at) = self.landed(o) {
                        self.project.edit_override(o, |x| {
                            x.output = at.output.clone().map(OutputOp::Set);
                            x.port = at.port.clone().map(PortOp::set);
                            x.channel = Some(ChannelOp::Set(Amount::ch(at.channel)));
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
            PatchInput::Override {
                field: PatchField::Pixel,
            } => {
                for (_, o) in &objects {
                    if let Some(at) = self.landed(o) {
                        self.project.edit_override(o, |x| {
                            x.channel = Some(ChannelOp::Set(Amount::ch(at.channel)))
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
                            x.channel = Some(ChannelOp::Set(Amount::px(px)))
                        });
                    }
                }
            }
            PatchInput::Choose {
                field: PatchField::Direction,
                option,
            } => {
                let d = if option == 1 {
                    Direction::Reversed
                } else {
                    Direction::AsDrawn
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

    fn shared_walked<V: PartialEq>(
        &self,
        walked: &[(&NodeId, &ObjectId, &Walked)],
        value: impl Fn(&Walked) -> Option<V>,
    ) -> Option<V> {
        let mut it = walked.iter().map(|(_, _, w)| value(w));
        let first = it.next()??;
        it.all(|v| v.as_ref() == Some(&first)).then_some(first)
    }

    /// Where one step of every selected object came from, when they share
    /// it: its words, and what selects it.
    fn shared_source(
        &self,
        walked: &[(&NodeId, &ObjectId, &Walked)],
        field: usize,
    ) -> Option<(String, Target)> {
        self.shared_walked(walked, |w| self.source_words(w, field))
    }

    /// Where a walked object's step came from, in words, with the thing to
    /// select to see it. A step nothing set says what it carried on after.
    pub fn source_words(&self, w: &Walked, field: usize) -> Option<(String, Target)> {
        let p = &self.project;
        match &w.sources[field] {
            StepSource::Override => None,
            StepSource::Follows => match self.previous_leaf(&w.fixture, &w.object) {
                Some(prev) => Some((
                    format!("after {}", p.label(&p.object_target(&prev))),
                    p.object_target(&prev),
                )),
                // The first thing laid: where the fixture starts.
                None => p.fixture_start(&w.fixture).map(|_| {
                    (
                        format!("from fixture {}'s start", p.node_name(&w.fixture)),
                        Target::Node(w.fixture.clone()),
                    )
                }),
            },
            StepSource::Start(t) => Some((
                format!("from {}'s start", self.owner_words(&w.fixture, t)),
                self.owner_target(&w.fixture, t),
            )),
            StepSource::Children(t) => {
                let rules = match t {
                    PatchTarget::Object(o) => self.rules_on(&w.fixture, &PatchTarget::Component(o.component.clone())),
                    other => self.rules_on(&w.fixture, other),
                };
                Some((
                    format!(
                        "from {}: {}",
                        self.owner_words(&w.fixture, t),
                        children_option(&rules.children).words()
                    ),
                    self.owner_target(&w.fixture, t),
                ))
            }
        }
    }

    /// The leaf the walk laid before this object, in its fixture.
    fn previous_leaf(&self, fixture: &NodeId, object: &ObjectId) -> Option<ObjectId> {
        let p = &self.project;
        let leaves: Vec<&ObjectId> = p
            .placed_objects()
            .iter()
            .filter(|(f, _)| f == fixture)
            .map(|(_, o)| &o.id)
            .collect();
        let first_mine = match p.placed().node(object) {
            Some((_, n)) => n.leaves().first().map(|l| l.id.clone())?,
            None => return None,
        };
        let i = leaves.iter().position(|o| **o == first_mine)?;
        i.checked_sub(1).map(|j| leaves[j].clone())
    }

    /// The objects that move with one whose start is its own: those after
    /// it that only carry on. "ring r4 and ring r5 follow it".
    fn followers_words(&self, fixture: &NodeId, object: &ObjectId) -> Option<String> {
        let p = &self.project;
        let patch = p.effective_patch();
        let leaves: Vec<&ObjectId> = p
            .placed_objects()
            .iter()
            .filter(|(f, _)| f == fixture)
            .map(|(_, o)| &o.id)
            .collect();
        let last_mine = p.placed().node(object)?.1.leaves().last()?.id.clone();
        let i = leaves.iter().position(|o| **o == last_mine)?;
        let mut names = Vec::new();
        for o in &leaves[i + 1..] {
            let w = patch.walked(o)?;
            if w.off {
                continue;
            }
            if !w.sources.iter().all(|s| *s == StepSource::Follows) {
                break;
            }
            names.push(p.label(&p.object_target(o)));
        }
        match names.len() {
            0 => None,
            1 => Some(format!("{} follows it", names[0])),
            n if n <= 4 => {
                let (last, rest) = names.split_last()?;
                Some(format!("{} and {last} follow it", rest.join(", ")))
            }
            n => Some(format!("{}, {} and {} more follow it", names[0], names[1], n - 2)),
        }
    }

    /// The rules on a fixture or a component.
    fn rules_on(&self, fixture: &NodeId, t: &PatchTarget) -> ComponentRules {
        let Some(f) = self.project.fixture(fixture) else {
            return ComponentRules::default();
        };
        match t {
            PatchTarget::Fixture => ComponentRules {
                start: f.start.clone(),
                children: f.children.clone(),
            },
            PatchTarget::Component(c) => f.rules_of(c),
            PatchTarget::Object(o) => f.rules_of(&o.component),
        }
    }

    fn per_lamp(&self, fixture: &NodeId) -> u32 {
        self.project
            .fixture(fixture)
            .map_or(3, |f| f.channels_per_lamp.max(1))
    }

    /// Whether a rules target has children: the fixture, a group, a circle,
    /// a repeat.
    fn is_container(&self, t: &PatchTarget) -> bool {
        match t {
            PatchTarget::Fixture => true,
            PatchTarget::Component(c) => self
                .project
                .component(c)
                .is_some_and(|c| !c.is_single_object()),
            PatchTarget::Object(_) => false,
        }
    }

    /// Where an object's span starts now (after any wrap).
    fn landed(&self, object: &ObjectId) -> Option<Cursor> {
        let w = self.project.effective_patch().walked(object)?;
        Some(w.landed.clone().unwrap_or_else(|| w.at.clone()))
    }

    /// The owner of a rule in words: "fixture ring display", "circle1",
    /// "copy 2 of repeat1".
    fn owner_words(&self, fixture: &NodeId, t: &PatchTarget) -> String {
        let p = &self.project;
        match t {
            PatchTarget::Fixture => format!("fixture {}", p.node_name(fixture)),
            PatchTarget::Component(c) => c.to_string(),
            PatchTarget::Object(o) => p.long_label(&p.object_target(o)),
        }
    }

    fn owner_target(&self, fixture: &NodeId, t: &PatchTarget) -> Target {
        match t {
            PatchTarget::Fixture => Target::Node(fixture.clone()),
            PatchTarget::Component(c) => Target::Component(c.clone()),
            PatchTarget::Object(o) => self.project.object_target(o),
        }
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

    /// Children's options: end to end, or one per port from each port of
    /// the output the targets are on (none when that isn't one output).
    fn children_options(&self, targets: &[(NodeId, PatchTarget)]) -> Vec<PatchOption> {
        let p = &self.project;
        let output = self.shared(targets, |f, t| {
            let walked = match t {
                PatchTarget::Fixture => p
                    .effective_patch()
                    .walked
                    .iter()
                    .find(|w| &w.fixture == f),
                PatchTarget::Component(c) => p.effective_patch().walked(&ObjectId::of(c)),
                PatchTarget::Object(o) => p.effective_patch().walked(o),
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

    /// The Port row's options: every port of every output.
    fn port_options(&self) -> Vec<PatchOption> {
        self.project
            .wire_ports()
            .into_iter()
            .map(|(o, p)| PatchOption::Port(o, p))
            .collect()
    }

    fn port_words(&self, output: &NodeId, port: &str) -> String {
        self.project.long_label(&Target::Port {
            output: output.clone(),
            key: port.to_string(),
        })
    }

    /// An option in words.
    fn option_words(&self, o: &PatchOption) -> String {
        let p = &self.project;
        match o {
            PatchOption::Follows => "Follows".into(),
            PatchOption::At(out, port) | PatchOption::Port(out, port) => self.port_words(out, port),
            PatchOption::NextPort => "Next port".into(),
            PatchOption::EndToEnd => "End to end".into(),
            PatchOption::OnePerPort(out, port) => {
                let name = p
                    .output(out)
                    .and_then(|o| o.port(port))
                    .map_or_else(|| port.clone(), |x| x.name.clone());
                format!("One per port from {name}")
            }
            PatchOption::AsDrawn => "As drawn".into(),
            PatchOption::Reversed => "Reversed".into(),
            PatchOption::OnWire => "Yes".into(),
            PatchOption::Off => "Off".into(),
        }
    }
}

fn start_option(s: &Start) -> PatchOption {
    match s {
        Start::Follows => PatchOption::Follows,
        Start::At { output, port, .. } => PatchOption::At(output.clone(), port.clone()),
        Start::NextPort => PatchOption::NextPort,
    }
}

/// A Children rule as the option it is among `options` (One per port is
/// matched by its port's key).
fn children_option(c: &Children) -> ChildrenKey {
    match c {
        Children::EndToEnd => ChildrenKey::EndToEnd,
        Children::OnePerPort { from } => ChildrenKey::OnePerPort(from.clone()),
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

    fn words(&self) -> String {
        match self {
            ChildrenKey::EndToEnd => "end to end".into(),
            ChildrenKey::OnePerPort(_) => "one per port".into(),
        }
    }
}

fn direction_option(d: Direction) -> PatchOption {
    match d {
        Direction::AsDrawn => PatchOption::AsDrawn,
        Direction::Reversed => PatchOption::Reversed,
    }
}
