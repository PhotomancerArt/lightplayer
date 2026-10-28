//! The inspector for the view tree: where you are, and what's true of the
//! selection — the project's facts with nothing selected, a node's ports
//! and patch, a lamp's place, and the properties you can edit.

use crate::domain::node::{NodeId, NodeKind};
use crate::domain::patch_edits::OnWire;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::graph_verb::GraphVerb;
use crate::ux::props::common_props;
use crate::ux::ui::{UiCrumb, UiInspector, UiItem, UiSection};

/// How many of a multiple selection are listed by name.
const LISTED: usize = 8;

impl Editor {
    pub fn view_inspector(&self) -> UiInspector {
        let p = &self.project;
        let selection = &self.selection;
        // One thing: the path down to it. Several: the path to the level
        // they are in, then how many.
        let crumb_end = if selection.len() > 1 {
            self.context()
        } else {
            selection.first().cloned()
        };
        let mut crumbs = vec![UiCrumb {
            target: None,
            label: p.name.clone(),
        }];
        if let Some(t) = crumb_end {
            crumbs.extend(p.chain(&t).into_iter().map(|c| UiCrumb {
                label: p.label(&c),
                target: Some(c),
            }));
        }
        UiInspector {
            crumbs,
            many: (selection.len() > 1).then(|| format!("{} selected", selection.len())),
            sections: self.inspector_sections(),
        }
    }

    fn inspector_sections(&self) -> Vec<UiSection> {
        let p = &self.project;
        let selection = &self.selection;
        let section = |title: &str, items: Vec<UiItem>| UiSection {
            title: Some(title.to_string()),
            items,
        };
        let mut out = Vec::new();
        if selection.is_empty() {
            out.push(section(
                "Project",
                project_facts(p).into_iter().map(UiItem::Fact).collect(),
            ));
            out.push(UiSection {
                title: None,
                items: vec![UiItem::Hint(
                    "Click something on the canvas or in the tree to see it here.".into(),
                )],
            });
            return out;
        }
        if selection.len() > 1 {
            let mut items: Vec<UiItem> = selection
                .iter()
                .take(LISTED)
                .map(|t| UiItem::Fact(p.long_label(t)))
                .collect();
            let more = selection.len().saturating_sub(LISTED);
            if more > 0 {
                items.push(UiItem::Fact(format!("… and {more} more")));
            }
            out.push(section(&format!("{} selected", selection.len()), items));
        }
        if let [t] = selection.as_slice()
            && let Some(mut facts) = node_facts(p, t, self.runs_for_real)
        {
            // A fixture's Patch section leads with the same rows as a
            // component's or an object's (`patch_section.rs`).
            if let Some(rows) = self.patch_section()
                && let Some(patch) = facts
                    .iter_mut()
                    .find(|x| x.title.as_deref() == Some("Patch"))
            {
                patch.items.splice(0..0, rows);
            }
            out.extend(facts);
        }
        // One component or object: the component first, then its object
        // (Yona, 2026-10-25: one tree, components primary; the inspector
        // shows the component's properties, then the object's).
        if let [t @ (Target::Part(_) | Target::Generated(_))] = selection.as_slice() {
            out.extend(self.component_then_object(t));
            if let Some(spans) = span_facts(p, t) {
                out.push(section("Spans", spans));
            }
            return out;
        }
        if let [t @ Target::Span { .. }] = selection.as_slice()
            && let Some(span) = p.span_of(t)
        {
            out.push(section("Span", self.span_items(t, &span)));
        }
        let fixtures_only = selection
            .iter()
            .all(|t| matches!(t, Target::Node(id) if p.fixture(id).is_some()));
        if !fixtures_only && let Some(rows) = self.patch_section() {
            out.push(section("Patch", rows));
        }
        if let [t @ (Target::Part(_) | Target::Generated(_))] = selection.as_slice()
            && let Some(spans) = span_facts(p, t)
        {
            out.push(section("Spans", spans));
        }

        if let [
            t @ Target::Lamp {
                segment: object,
                index,
            },
        ] = selection.as_slice()
            && let Some(info) = lamp_fact(p, t, object, *index)
        {
            out.push(section("Lamp", vec![UiItem::Fact(info)]));
        }
        let props = common_props(p, selection);
        let only_lamps = selection.iter().all(|t| matches!(t, Target::Lamp { .. }));
        if !props.is_empty() {
            let title = if selection.len() > 1 {
                "Shared properties"
            } else {
                "Properties"
            };
            let items = props
                .into_iter()
                .map(|prop| UiItem::Prop {
                    locked: !self.move_draw && prop.desc.reshapes,
                    prop,
                })
                .collect();
            out.push(section(title, items));
        } else if only_lamps {
            out.push(UiSection {
                title: None,
                items: vec![UiItem::Hint(
                    "A lamp's count belongs to its path or ring — Esc selects it.".into(),
                )],
            });
        } else if selection.len() > 1 {
            out.push(UiSection {
                title: None,
                items: vec![UiItem::Hint(
                    "These share no properties. Select things of one kind to edit them together."
                        .into(),
                )],
            });
        }
        out
    }
}

impl Editor {
    /// A part's section — its properties, its start and flow — then its
    /// node's — what it does on the walk. For a ring or a copy the part is
    /// the circle or the repeated thing, and what is edited there changes
    /// every ring or every copy (D11).
    fn component_then_object(&self, t: &Target) -> Vec<UiSection> {
        let p = &self.project;
        let (component, object) = match t {
            Target::Part(c) => (c.clone(), c.node()),
            Target::Generated(o) => (o.part(), o.clone()),
            _ => return Vec::new(),
        };
        if p.part(&component).is_none() {
            return Vec::new();
        }
        let fixture = component.fixture.clone();
        let props = |targets: &[Target]| -> Vec<UiItem> {
            common_props(p, targets)
                .into_iter()
                .map(|prop| UiItem::Prop {
                    locked: !self.move_draw && prop.desc.reshapes,
                    prop,
                })
                .collect()
        };
        let mut comp_items = Vec::new();
        if let Target::Generated(o) = t {
            let what = p.generated_word(o);
            comp_items.push(UiItem::Hint(format!(
                "Changes here change every {what} of {component}."
            )));
        }
        comp_items.extend(props(&[Target::Part(component.clone())]));
        comp_items.extend(self.rule_rows_for(&[(
            fixture.clone(),
            lpc_mapping::patching::PatchTarget::Part(component.part.clone()),
        )]));
        let mut obj_items = Vec::new();
        // A ring's own count, radius and way round live in its circle, but
        // are the ring's alone.
        if matches!(t, Target::Generated(o) if p.ring_of(o).is_some()) {
            obj_items.extend(props(std::slice::from_ref(t)));
        }
        obj_items.extend(
            self.object_rows_for(&[(fixture, object.clone())])
                .unwrap_or_default(),
        );
        vec![
            UiSection {
                title: Some(format!("Part · {component}")),
                items: comp_items,
            },
            UiSection {
                title: Some(format!("{} · {object}", node_title(p, &object))),
                items: obj_items,
            },
        ]
    }
}

/// With nothing selected: what the project holds, and whether every lamp
/// lights.
fn project_facts(p: &Project) -> Vec<String> {
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    let outside = p.lamps_outside().len();
    let unpatched: usize = p
        .effective_patch()
        .unpatched
        .iter()
        .map(|u| u.lamps.len())
        .sum();
    let boxes = p
        .fixtures()
        .iter()
        .filter(|n| n.fixture().is_some_and(|f| f.unsupported.is_some()))
        .count();
    let mut facts = vec![
        plural(p.fixtures().len(), "fixture"),
        plural(p.outputs().len(), "output"),
        format!("{} lamps", p.lamp_count()),
    ];
    // A fixture opened as its box alone: its lamps aren't counted here.
    if boxes > 0 {
        facts.push(format!(
            "{} not shown, its lamps not counted",
            plural(boxes, "fixture")
        ));
    }
    facts.extend([
        if unpatched == 0 {
            "every lamp patched to a port".into()
        } else {
            format!("{unpatched} lamps on no port — they light nothing")
        },
        if outside == 0 {
            "every lamp inside its fixture's box".into()
        } else {
            format!("{outside} lamps outside their fixture's box — never lit")
        },
    ]);
    facts
}

/// A fixture's patch at a glance: auto, and how many of its objects are
/// on the wire.
fn patch_facts(p: &Project, fixture: &NodeId) -> Vec<UiItem> {
    let objects: Vec<&crate::DrawnSegment> = p
        .segments_in(fixture)
        .into_iter()
        .filter(|o| !o.lamps.is_empty())
        .collect();
    let patch = p.effective_patch();
    let on = objects.iter().filter(|o| patch.lands(&o.id)).count();
    let by_hand = objects
        .iter()
        .filter(|o| matches!(p.on_wire(&o.id), OnWire::Placed { .. }))
        .count();
    let mut out = vec![UiItem::Fact(format!(
        "{on} of {} on the wire · {} waiting",
        objects.len(),
        objects.len() - on
    ))];
    if by_hand > 0 {
        out.push(UiItem::Action {
            text: format!("{by_hand} patched by hand"),
            verb: GraphVerb::UnpatchAll,
        });
    }
    out
}

/// What the second section of a part's inspector is called: the node it is
/// about, in a person's words.
fn node_title(p: &Project, node: &crate::TreeRef) -> &'static str {
    match p.placed().node(node) {
        Some((_, n)) if n.is_leaf() => "Segment",
        _ if node.id.key.is_none() && !node.id.copies.is_empty() => "Copy",
        _ => "On the wire",
    }
}

/// Where a fixture starts, in words: "Starts at D0 of main, pixel 0".
fn start_words(p: &Project, start: &lpc_mapping::patching::Start) -> String {
    use lpc_mapping::patching::{ChannelOp, OutputOp, PortOp, Unit};
    let (OutputOp::Set(name), PortOp::Set { key, .. }) = (&start.output, &start.port) else {
        return "Starts nowhere yet: its lamps go nowhere".into();
    };
    let at = match p.output_named(name.as_str()) {
        Some(output) => p.long_label(&Target::Port {
            output,
            key: key.0.to_string(),
        }),
        None => format!("port {} of {name}, which isn't there", key.0),
    };
    let channel = match start.channel {
        ChannelOp::Set(a) if a.unit == Unit::Lamps => format!("pixel {}", a.n),
        ChannelOp::Set(a) if a.n % 3 == 0 => format!("pixel {}", a.n / 3),
        ChannelOp::Set(a) => format!("channel {}", a.n),
        _ => "where the cursor is".into(),
    };
    format!("Starts at {at}, {channel}")
}

/// Where a drawn thing's lamps land: one line per span, its pixels on its
/// port. `None` for one with no lamps.
fn span_facts(p: &Project, t: &Target) -> Option<Vec<UiItem>> {
    let objects: Vec<crate::TreeRef> = match t {
        Target::Generated(o) => vec![o.clone()],
        Target::Part(id) => p.segments_of(id).iter().map(|o| o.id.clone()).collect(),
        _ => return None,
    };
    if objects
        .iter()
        .all(|o| p.segment(o).is_none_or(|o| o.lamps.is_empty()))
    {
        return None;
    }
    let lines: Vec<UiItem> = p
        .effective_patch()
        .pixel_runs
        .iter()
        .filter(|s| objects.contains(&s.segment))
        .map(|s| {
            // A reversed span runs down the wire from its start pixel.
            let lo = if s.reversed {
                (s.start_pixel + 1).saturating_sub(s.len())
            } else {
                s.start_pixel
            };
            let hi = lo + s.len().max(1) - 1;
            let port = p.long_label(&Target::Port {
                output: s.output.clone(),
                key: s.port.clone(),
            });
            UiItem::Fact(if lo == hi {
                format!("Pixel {lo} of {port}")
            } else {
                format!("Pixels {lo}–{hi} of {port}")
            })
        })
        .collect();
    Some(if lines.is_empty() {
        vec![UiItem::Fact("On no port: its lamps light nothing".into())]
    } else {
        lines
    })
}

/// One selected lamp: where it is on its path or ring, and the pixel it
/// lands on.
fn lamp_fact(p: &Project, t: &Target, object: &crate::TreeRef, index: u32) -> Option<String> {
    let at = p.lamps_under(t).first().copied()?;
    let n = p.segment(object).map(|o| o.lamps.len()).unwrap_or(0);
    let owner = p.long_label(&p.tree_target(object));
    let place = if index == 0 {
        "where the data enters".to_string()
    } else if index as usize + 1 == n {
        "the last lamp".to_string()
    } else {
        format!("{index} from the start")
    };
    let pixel = match p.pixel_of_lamp(object, index) {
        Some(px) => format!(" On {} (ringed).", p.long_label(&px)),
        None => " On no pixel: it lights nothing.".into(),
    };
    Some(format!(
        "Lamp {index} of {n} on {owner} — {place}. At ({:.1}, {:.1}).{pixel}",
        at.x, at.y
    ))
}

/// What the inspector says about one node, port or pixel, beyond its
/// properties.
/// `runs_for_real`: the project runs on a board or Studio's preview host,
/// so a shader shows its own picture, not a stand-in.
fn node_facts(p: &Project, t: &Target, runs_for_real: bool) -> Option<Vec<UiSection>> {
    let patch = p.effective_patch();
    let mut ports: Vec<UiItem> = Vec::new();
    let mut rules: Vec<UiItem> = Vec::new();
    let mut facts: Vec<UiItem> = Vec::new();
    let fact = |s: String| UiItem::Fact(s);
    match t {
        Target::Node(id) => match p.node(id).map(|n| &n.kind) {
            Some(NodeKind::Output(o)) => {
                for port in &o.ports {
                    let used = patch.used(id, &port.key);
                    let pixels = p.port_pixels(id, &port.key);
                    let rest = if port.pixels.is_none() {
                        " (the rest)"
                    } else {
                        ""
                    };
                    ports.push(UiItem::Port {
                        name: port.name.clone(),
                        fill: format!("{used}/{pixels} px{rest}{}", orders(p, id, &port.key)),
                        ratio: (used as f64 / pixels.max(1) as f64).min(1.0),
                    });
                }
                if let Some(label) = &o.label {
                    facts.push(fact(format!("Patches name it “{label}”")));
                }
                let feeds: Vec<String> = p
                    .fixtures()
                    .iter()
                    .filter(|n| p.outputs_of(&n.id).contains(id))
                    .map(|n| n.name.clone())
                    .collect();
                if !feeds.is_empty() {
                    facts.push(fact(format!("Fed by {}", feeds.join(", "))));
                }
                facts.push(fact(format!(
                    "Reads {}: each fixture on it arrives as its own control product",
                    o.reads
                )));
            }
            Some(NodeKind::Fixture(fx)) => {
                facts.push(fact(format!("Colour order {}", fx.color_order.word())));
                facts.push(fact(match p.visual_source(id) {
                    Some(src) => {
                        format!("Samples {} on {} at its lamps", p.node_name(&src), fx.reads)
                    }
                    None => format!("Nothing writes {}: its lamps have no colour", fx.reads),
                }));
                // Auto and how much is on the wire first, with what changes
                // them (patching by hand), then the rules themselves.
                rules.extend(patch_facts(p, id));
                rules.push(UiItem::Rule(start_words(p, &fx.mapping.start)));
                let lands: Vec<String> = p.ports_of(t).iter().map(|x| p.long_label(x)).collect();
                if !lands.is_empty() {
                    rules.push(UiItem::Rule(format!("Lands on {}", lands.join(", "))));
                }
                let left: usize = patch
                    .unpatched
                    .iter()
                    .filter(|u| &u.fixture == id)
                    .map(|u| u.lamps.len())
                    .sum();
                let working = !p.control_outputs(id).is_empty();
                if left > 0 && working {
                    rules.push(UiItem::Rule(format!(
                        "{left} lamps past the end of its ports"
                    )));
                }
                rules.extend(
                    patch
                        .problems
                        .iter()
                        .filter(|x| &x.fixture == id)
                        .map(|x| UiItem::Problem(x.notice(p))),
                );
            }
            Some(NodeKind::Module { children }) => {
                facts.push(fact(format!(
                    "A module: {} nodes inside, laid out by themselves",
                    children.len()
                )));
            }
            Some(NodeKind::Source(src)) => {
                let feeds: Vec<String> = p
                    .bindings()
                    .iter()
                    .filter(|b| &b.from == id)
                    .map(|b| p.node_name(&b.to))
                    .collect();
                facts.push(fact(format!(
                    "A {} writing its picture to {}{}",
                    src.kind_words(),
                    src.writes,
                    if feeds.is_empty() {
                        String::new()
                    } else {
                        format!(", read by {}", feeds.join(", "))
                    }
                )));
                if !runs_for_real {
                    facts.push(fact(format!(
                        "The lab doesn't run GLSL: it shows a stand-in, the {} pattern",
                        src.pattern.words()
                    )));
                }
                if let Some(code) = &src.glsl {
                    facts.push(UiItem::Code(code.clone()));
                }
            }
            Some(NodeKind::Other(o)) => {
                facts.push(fact(format!(
                    "A {} node: on the canvas, carried along, not edited here",
                    o.kind_words()
                )));
            }
            None => return None,
        },
        Target::Port { output, key } => {
            let port = p.output(output)?.port(key)?;
            let (used, pixels) = p.port_fill(output, key);
            let rest = if port.pixels.is_none() {
                " · takes the rest"
            } else {
                ""
            };
            facts.push(fact(format!(
                "{used} of {pixels} px used{rest}{} · on {}",
                orders(p, output, key),
                p.node_name(output)
            )));
            let mut by: Vec<UiItem> = Vec::new();
            for s in patch
                .pixel_runs
                .iter()
                .filter(|s| &s.output == output && &s.port == key)
            {
                by.push(fact(format!(
                    "{} lamps {}–{} at {}{}",
                    p.long_label(&p.tree_target(&s.segment)),
                    s.lamps.start,
                    s.lamps.end - 1,
                    s.start_pixel,
                    if s.reversed { " (running down)" } else { "" }
                )));
            }
            if by.is_empty() {
                facts.push(fact("Nothing is patched here yet".into()));
            }
            facts.extend(by);
        }
        Target::Pixel { output, key, index } => {
            p.output(output)?.port(key)?;
            facts.push(fact(format!(
                "Pixel {index} of {} on {}{}",
                p.port_pixels(output, key),
                p.long_label(&Target::Port {
                    output: output.clone(),
                    key: key.clone(),
                }),
                orders(p, output, key)
            )));
            facts.push(fact(match p.lamp_on_pixel(output, key, *index) {
                Some(lamp) => format!(
                    "{} is patched here — ringed on the canvas",
                    p.long_label(&lamp)
                ),
                None => {
                    "Nothing is patched to this pixel; it lights anyway, so you can find it".into()
                }
            }));
        }
        _ => return None,
    }
    let mut out = Vec::new();
    if !ports.is_empty() {
        out.push(UiSection {
            title: Some("Ports".into()),
            items: ports,
        });
    }
    if !rules.is_empty() {
        out.push(UiSection {
            title: Some("Patch".into()),
            items: rules,
        });
    }
    if !facts.is_empty() {
        out.push(UiSection {
            title: None,
            items: facts,
        });
    }
    Some(out)
}

/// The colour orders landing on a port, for its line: ` · GRB`, or
/// ` · GRB + RGB (mixed)` when two fixtures disagree.
fn orders(p: &Project, output: &NodeId, key: &str) -> String {
    let words: Vec<&str> = p
        .color_orders_on(output, key)
        .into_iter()
        .map(|c| c.word())
        .collect();
    match words.len() {
        0 => String::new(),
        1 => format!(" · {}", words[0]),
        _ => format!(" · {} (mixed)", words.join(" + ")),
    }
}

impl Editor {
    /// A span in the inspector: what's on it, a way to its object, or for a
    /// gap, what can be patched here (plan D13).
    fn span_items(&self, t: &Target, span: &crate::domain::span::Span) -> Vec<UiItem> {
        let p = &self.project;
        let port = t
            .port_of_pixel()
            .map(|port| p.long_label(&port))
            .unwrap_or_default();
        let pixels = if span.pixels.len() == 1 {
            format!("Pixel {}", span.pixels.start)
        } else {
            format!(
                "Pixels {}–{}",
                span.pixels.start,
                span.pixels.end.saturating_sub(1)
            )
        };
        match &span.segment {
            Some(o) => {
                let target = p.tree_target(o);
                vec![
                    UiItem::Fact(format!("{pixels} of {port}")),
                    UiItem::Select {
                        text: format!("Select {}", p.long_label(&target)),
                        target,
                    },
                ]
            }
            None => {
                let options: Vec<String> = self
                    .patch_here_options()
                    .iter()
                    .map(|o| p.long_label(&p.tree_target(o)))
                    .collect();
                let mut items = vec![UiItem::Fact(format!("Free · {pixels} of {port}"))];
                if options.is_empty() {
                    items.push(UiItem::Hint("Every object is on the wire.".into()));
                } else {
                    items.push(UiItem::PatchHere { options });
                }
                items
            }
        }
    }
}
