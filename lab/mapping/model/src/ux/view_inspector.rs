//! The inspector for the view tree: where you are, and what's true of the
//! selection — the project's facts with nothing selected, a node's ports
//! and patch, a lamp's place, and the properties you can edit.

use crate::domain::node::{NodeId, NodeKind};
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::editor::Editor;
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
            && let Some(facts) = node_facts(p, t)
        {
            out.extend(facts);
        }
        if let [t @ Target::Lamp { object, index }] = selection.as_slice()
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
    vec![
        plural(p.fixtures().len(), "fixture"),
        plural(p.outputs().len(), "output"),
        format!("{} lamps", p.lamp_count()),
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
    ]
}

/// One selected lamp: where it is on its path or ring, and the pixel it
/// lands on.
fn lamp_fact(p: &Project, t: &Target, object: &crate::ObjectId, index: u32) -> Option<String> {
    let at = p.lamps_under(t).first().copied()?;
    let n = p.object(object).map(|o| o.lamps.len()).unwrap_or(0);
    let owner = p.long_label(&p.object_target(object));
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
fn node_facts(p: &Project, t: &Target) -> Option<Vec<UiSection>> {
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
                if fx.rules.is_empty() {
                    rules.push(UiItem::Rule("No rule: its lamps go nowhere yet".into()));
                }
                for r in &fx.rules {
                    rules.push(UiItem::Rule(p.rule_label(r)));
                }
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
                let working = fx.rules.iter().any(|r| p.node(r.output()).is_some());
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
                facts.push(fact(format!(
                    "The lab doesn't run GLSL: it shows a stand-in, the {} pattern",
                    src.pattern.words()
                )));
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
                .spans
                .iter()
                .filter(|s| &s.output == output && &s.port == key)
            {
                by.push(fact(format!(
                    "{} lamps {}–{} at {}{}",
                    p.long_label(&p.object_target(&s.object)),
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
