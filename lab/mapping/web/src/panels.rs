//! The tree, the inspector and the hint bar. All three are drawn from
//! model data: tree rows, property descriptors, hints.

use dioxus::prelude::*;
use lab_mapping_model::{
    Editor, Hint, HintKind, NodeKind, Notice, NoticeKind, Project, Prop, PropEdit, PropKind,
    PropValue, Target, common_props,
};

#[component]
pub fn TreePane(ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    let e = ed.read();
    let rows = e.project.tree_rows();
    let selection = e.selection.clone();
    // A selected lamp lights its line or ring's row.
    let lit: Vec<Target> = selection
        .iter()
        .map(|t| match t {
            Target::Lamp { object, .. } => e.project.object_target(object),
            other => other.clone(),
        })
        .collect();
    let context = e.context();
    let name = e.project.name.clone();
    let kinds: Vec<&'static str> = rows
        .iter()
        .map(|r| match &r.target {
            Target::Node(_) => e.project.kind_word(&r.target),
            Target::Port { .. } => "port",
            _ => "",
        })
        .collect();
    drop(e);

    rsx! {
        div { class: "pane-title", "{name}" }
        if rows.is_empty() {
            div { class: "empty", "Nothing here yet. Press L to draw a line." }
        }
        for (row, kind) in rows.into_iter().zip(kinds) {
            {
                let t = row.target.clone();
                let mut class = String::from("row");
                if selection.contains(&t) {
                    class.push_str(" row-selected");
                } else if lit.contains(&t) {
                    class.push_str(" row-lit");
                }
                if context.as_ref() == Some(&t) {
                    class.push_str(" row-context");
                }
                if row.derived {
                    class.push_str(" row-derived");
                }
                if !kind.is_empty() {
                    class.push_str(" row-");
                    class.push_str(kind);
                }
                let pad = 10 + row.depth * 16;
                rsx! {
                    div {
                        key: "{row.label}-{row.depth}-{pad}",
                        class: "{class}",
                        style: "padding-left: {pad}px",
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |evt| {
                            if evt.modifiers().shift() {
                                ed.write().toggle_select(t.clone());
                            } else {
                                ed.write().select(vec![t.clone()]);
                            }
                        },
                        title: "{row.detail}",
                        span { class: "row-label", "{row.label}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Inspector(ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    let e = ed.read();
    let f = &e.project;
    let selection = e.selection.clone();
    // One thing: the path down to it. Several: the path to the level they
    // are in, then how many.
    let crumb_end = if selection.len() > 1 {
        e.context()
    } else {
        selection.first().cloned()
    };
    let crumbs: Vec<(Target, String)> = crumb_end
        .map(|t| {
            f.chain(&t)
                .into_iter()
                .map(|c| (c.clone(), f.label(&c)))
                .collect()
        })
        .unwrap_or_default();
    let many = (selection.len() > 1).then(|| format!("{} selected", selection.len()));
    let props = common_props(f, &selection);
    let project_name = f.name.clone();
    let summary: Vec<String> = selection.iter().take(8).map(|t| f.long_label(t)).collect();
    let more = selection.len().saturating_sub(8);
    let lamp_info: Option<String> = match selection.as_slice() {
        [t @ Target::Lamp { object, index }] => {
            let pos = f.lamps_under(t).first().copied();
            let n = f.object(object).map(|o| o.lamps.len()).unwrap_or(0);
            let owner = f.long_label(&f.object_target(object));
            pos.map(|p| {
                let place = if *index == 0 {
                    "where the data enters".to_string()
                } else if *index as usize + 1 == n {
                    "the last lamp".to_string()
                } else {
                    format!("{index} from the start")
                };
                format!(
                    "Lamp {index} of {n} on {owner} — {place}. At ({:.1}, {:.1}).",
                    p.x, p.y
                )
            })
        }
        _ => None,
    };
    let fixture_facts = if selection.is_empty() {
        let outside = f.lamps_outside().len();
        let n = f.fixtures().len();
        let outputs = f.outputs().len();
        let patch = f.effective_patch();
        let unpatched: usize = patch.unpatched.iter().map(|u| u.lamps.len()).sum();
        Some(vec![
            format!("{n} fixture{}", if n == 1 { "" } else { "s" }),
            format!("{outputs} output{}", if outputs == 1 { "" } else { "s" }),
            format!("{} lamps", f.lamp_count()),
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
        ])
    } else {
        None
    };
    let only_lamps =
        !selection.is_empty() && selection.iter().all(|t| matches!(t, Target::Lamp { .. }));
    let node = match selection.as_slice() {
        [t] => node_facts(f, t),
        _ => None,
    };
    drop(e);

    rsx! {
        div { class: "crumbs",
            button {
                class: "crumb",
                tabindex: "-1",
                onmousedown: move |evt| evt.prevent_default(),
                onclick: move |_| ed.write().select(vec![]),
                "{project_name}"
            }
            for (i, (t, label)) in crumbs.into_iter().enumerate() {
                span { key: "c{i}", class: "crumb-step",
                    span { class: "crumb-sep", "›" }
                    button {
                        class: "crumb",
                        tabindex: "-1",
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| ed.write().select(vec![t.clone()]),
                        "{label}"
                    }
                }
            }
            if let Some(many) = many {
                span { class: "crumb-step",
                    span { class: "crumb-sep", "›" }
                    span { class: "crumb crumb-many", "{many}" }
                }
            }
        }
        if let Some(facts) = fixture_facts {
            div { class: "section-title", "Project" }
            for (i, fact) in facts.into_iter().enumerate() {
                div { key: "{i}", class: "fact", "{fact}" }
            }
            div { class: "hint-inline", "Click something on the canvas or in the tree to see it here." }
        } else {
            if selection.len() > 1 {
                div { class: "section-title", "{selection.len()} selected" }
                for (i, s) in summary.into_iter().enumerate() {
                    div { key: "{i}", class: "fact", "{s}" }
                }
                if more > 0 {
                    div { class: "fact", "… and {more} more" }
                }
            }
            if let Some(n) = node {
                if !n.ports.is_empty() {
                    div { class: "section-title", "Ports" }
                    for (i, (name, fill, ratio)) in n.ports.into_iter().enumerate() {
                        div { key: "p{i}", class: "port-fact",
                            span { "{name}" }
                            span { class: "port-fact-fill", "{fill}" }
                            div { class: "port-fact-bar", div { class: "port-fact-bar-in", style: "width: {ratio * 100.0}%" } }
                        }
                    }
                }
                if !n.patch.is_empty() || !n.problems.is_empty() {
                    div { class: "section-title", "Patch" }
                    for (i, fact) in n.patch.into_iter().enumerate() {
                        div { key: "r{i}", class: "fact patch-rule", "{fact}" }
                    }
                    for (i, pr) in n.problems.into_iter().enumerate() {
                        div { key: "x{i}", class: "notice refused problem",
                            div { class: "notice-text", span { class: "notice-tag", "problem: " } "{pr.text}" }
                            if let Some(why) = pr.why {
                                div { class: "notice-line", span { class: "notice-eq", "= why: " } "{why}" }
                            }
                            if let Some(help) = pr.help {
                                div { class: "notice-line", span { class: "notice-eq", "= help: " } "{help}" }
                            }
                        }
                    }
                }
                for (i, fact) in n.facts.into_iter().enumerate() {
                    div { key: "n{i}", class: "fact", "{fact}" }
                }
            }
            if let Some(info) = lamp_info {
                div { class: "section-title", "Lamp" }
                div { class: "fact", "{info}" }
            }
            if !props.is_empty() {
                div { class: "section-title",
                    if selection.len() > 1 { "Shared properties" } else { "Properties" }
                }
                for p in props {
                    PropRow { ed, prop: p }
                }
            } else if only_lamps {
                div { class: "hint-inline", "A lamp's count belongs to its line or ring — Esc selects it." }
            } else if selection.len() > 1 {
                div { class: "hint-inline", "These share no properties. Select things of one kind to edit them together." }
            }
        }
    }
}

/// What the inspector says about one node or port, beyond its properties.
#[derive(Default)]
struct NodeFacts {
    /// An output's ports: name, `50/60 px`, how full.
    ports: Vec<(String, String, f64)>,
    /// A fixture's rules, then where its lamps land.
    patch: Vec<String>,
    problems: Vec<Notice>,
    facts: Vec<String>,
}

fn node_facts(f: &Project, t: &Target) -> Option<NodeFacts> {
    let mut out = NodeFacts::default();
    let patch = f.effective_patch();
    match t {
        Target::Node(id) => match f.node(id).map(|n| &n.kind) {
            Some(NodeKind::Output(o)) => {
                for p in &o.ports {
                    let used = patch.used(id, &p.key);
                    out.ports.push((
                        p.name.clone(),
                        format!("{used}/{} px · {}", p.pixels, p.format.word()),
                        (used as f64 / p.pixels.max(1) as f64).min(1.0),
                    ));
                }
                let feeds: Vec<String> = f
                    .fixtures()
                    .iter()
                    .filter(|n| f.outputs_of(&n.id).contains(id))
                    .map(|n| n.name.clone())
                    .collect();
                if !feeds.is_empty() {
                    out.facts.push(format!("Fed by {}", feeds.join(", ")));
                }
            }
            Some(NodeKind::Fixture(fx)) => {
                if fx.rules.is_empty() {
                    out.patch.push("No rule: its lamps go nowhere yet".into());
                }
                for r in &fx.rules {
                    out.patch.push(f.rule_label(r));
                }
                let ports: Vec<String> = f.ports_of(t).iter().map(|p| f.long_label(p)).collect();
                if !ports.is_empty() {
                    out.patch.push(format!("Lands on {}", ports.join(", ")));
                }
                let left: usize = patch
                    .unpatched
                    .iter()
                    .filter(|u| &u.fixture == id)
                    .map(|u| u.lamps.len())
                    .sum();
                let working = fx.rules.iter().any(|r| f.node(r.output()).is_some());
                if left > 0 && working {
                    out.patch
                        .push(format!("{left} lamps past the end of its ports"));
                }
                out.problems = patch
                    .problems
                    .iter()
                    .filter(|p| &p.fixture == id)
                    .map(|p| p.notice(f))
                    .collect();
            }
            Some(NodeKind::Module { children }) => {
                out.facts.push(format!(
                    "A module: {} nodes inside, laid out by themselves",
                    children.len()
                ));
            }
            None => return None,
        },
        Target::Port { output, key } => {
            let port = f.output(output)?.port(key)?;
            let (used, pixels) = f.port_fill(output, key);
            out.facts.push(format!(
                "{used} of {pixels} px used · {} · on {}",
                port.format.word(),
                f.node_name(output)
            ));
            let mut by: Vec<String> = Vec::new();
            for s in patch
                .spans
                .iter()
                .filter(|s| &s.output == output && &s.port == key)
            {
                by.push(format!(
                    "{} lamps {}–{} at {}",
                    f.long_label(&f.object_target(&s.object)),
                    s.lamps.start,
                    s.lamps.end - 1,
                    s.start_pixel
                ));
            }
            if by.is_empty() {
                out.facts.push("Nothing is patched here yet".into());
            }
            out.facts.extend(by);
        }
        _ => return None,
    }
    Some(out)
}

/// One property, laid out from its descriptor alone.
#[component]
fn PropRow(ed: Signal<Editor>, prop: Prop) -> Element {
    let mut ed = ed;
    let id = prop.desc.id;
    let value = prop.value.describe();
    let mixed = prop.value.is_mixed();
    let control = match prop.desc.kind {
        PropKind::Count { .. } | PropKind::Number { .. } => {
            let shown = if mixed { String::new() } else { value.clone() };
            let step = match prop.desc.kind {
                PropKind::Number { step, .. } => step,
                _ => 1.0,
            };
            let is_count = matches!(prop.desc.kind, PropKind::Count { .. });
            rsx! {
                div { class: "num",
                    button {
                        tabindex: "-1",
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| ed.write().edit_selection_prop(id, PropEdit::Nudge(-1)),
                        "−"
                    }
                    input {
                        r#type: "number",
                        step: "{step}",
                        value: "{shown}",
                        placeholder: "{value}",
                        // Typing here must not drive the canvas.
                        onkeydown: move |evt| evt.stop_propagation(),
                        onchange: move |evt| {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                let edit = if is_count { PropEdit::SetCount(v.round() as i64) } else { PropEdit::SetNumber(v) };
                                ed.write().edit_selection_prop(id, edit);
                            }
                        },
                    }
                    button {
                        tabindex: "-1",
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| ed.write().edit_selection_prop(id, PropEdit::Nudge(1)),
                        "+"
                    }
                    if prop.desc.nudge {
                        kbd { class: "key-hint", "− =" }
                    }
                }
            }
        }
        PropKind::Flag => {
            let on = matches!(prop.value, PropValue::Flag { off: 0, .. });
            let label = if mixed {
                "mixed".to_string()
            } else if on {
                "yes".into()
            } else {
                "no".into()
            };
            rsx! {
                button {
                    class: if on { "flag on" } else { "flag" },
                    tabindex: "-1",
                    onmousedown: move |evt| evt.prevent_default(),
                    onclick: move |_| ed.write().edit_selection_prop(id, PropEdit::SetFlag(!on)),
                    "{label}"
                }
            }
        }
    };
    rsx! {
        div { class: "prop",
            span { class: "prop-label", "{prop.desc.label}" }
            {control}
            if mixed {
                span { class: "prop-mixed", "{value}" }
            }
        }
    }
}

#[component]
pub fn HintBar(ed: Signal<Editor>) -> Element {
    let e = ed.read();
    let bar = e.hint_bar();
    let notice = e.notice.clone();
    drop(e);
    let (mouse, keys): (Vec<Hint>, Vec<Hint>) = bar
        .hints
        .iter()
        .cloned()
        .partition(|h| h.kind == HintKind::Mouse);
    rsx! {
        if let Some(n) = notice {
            div { class: if n.kind == NoticeKind::Refused { "notice refused" } else { "notice" },
                div { class: "notice-text",
                    if n.kind == NoticeKind::Refused { span { class: "notice-tag", "can't: " } }
                    "{n.text}"
                }
                if let Some(why) = n.why {
                    div { class: "notice-line", span { class: "notice-eq", "= why: " } "{why}" }
                }
                if let Some(help) = n.help {
                    div { class: "notice-line", span { class: "notice-eq", "= help: " } "{help}" }
                }
            }
        }
        div { class: "bar",
            if !bar.lead.is_empty() {
                div { class: "lead", "{bar.lead}" }
            }
            HintRow { label: "mouse", hints: mouse, quiet: false }
            HintRow { label: "keys", hints: keys, quiet: false }
            HintRow { label: "view", hints: bar.view, quiet: true }
        }
    }
}

/// One labelled row of the hint bar: what the mouse does, what the keys do,
/// or how to move around the canvas.
#[component]
fn HintRow(label: &'static str, hints: Vec<Hint>, quiet: bool) -> Element {
    if hints.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: if quiet { "hint-row quiet" } else { "hint-row" },
            span { class: "hint-row-label", "{label}" }
            div { class: "hint-row-items",
                for (i, hint) in hints.into_iter().enumerate() {
                    span { key: "{i}", class: "hint", kbd { "{hint.keys}" } span { class: "hint-does", "{hint.does}" } }
                }
            }
        }
    }
}
