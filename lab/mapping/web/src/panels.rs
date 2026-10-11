//! The tree, the inspector and the hint bar. All three are drawn from
//! model data: tree rows, property descriptors, hints.

use dioxus::prelude::*;
use lab_mapping_model::{
    Editor, NoticeKind, Prop, PropEdit, PropKind, PropValue, Target, common_props,
};

#[component]
pub fn TreePane(ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    let e = ed.read();
    let rows = e.fixture.tree_rows();
    let selection = e.selection.clone();
    // A selected lamp lights its line or ring's row.
    let lit: Vec<Target> = selection
        .iter()
        .map(|t| match t {
            Target::Lamp { object, .. } => e.fixture.object_target(object),
            other => other.clone(),
        })
        .collect();
    let context = e.context();
    let name = e.fixture.name.clone();
    drop(e);

    rsx! {
        div { class: "pane-title", "{name}" }
        if rows.is_empty() {
            div { class: "empty", "Nothing here yet. Press L to draw a line." }
        }
        for row in rows {
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
                        span { class: "row-label", "{row.label}" }
                        span { class: "row-detail", "{row.detail}" }
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
    let f = &e.fixture;
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
    let fixture_name = f.name.clone();
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
        Some(vec![
            format!("Box {} × {}", f.bounds.width(), f.bounds.height()),
            format!("{} top-level things", f.components.len()),
            format!("{} lamps", f.lamp_count()),
            if outside == 0 {
                "every lamp inside the box".into()
            } else {
                format!("{outside} lamps outside the box — never lit")
            },
        ])
    } else {
        None
    };
    let only_lamps =
        !selection.is_empty() && selection.iter().all(|t| matches!(t, Target::Lamp { .. }));
    drop(e);

    rsx! {
        div { class: "crumbs",
            button {
                class: "crumb",
                tabindex: "-1",
                onmousedown: move |evt| evt.prevent_default(),
                onclick: move |_| ed.write().select(vec![]),
                "{fixture_name}"
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
            div { class: "section-title", "Fixture" }
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
            span { class: "lead", "{bar.lead}" }
            for (i, hint) in bar.hints.into_iter().enumerate() {
                span { key: "{i}", class: "hint", kbd { "{hint.keys}" } " {hint.does}" }
            }
            span { class: "spacer" }
            for (i, hint) in bar.view.into_iter().enumerate() {
                span { key: "v{i}", class: "hint view", kbd { "{hint.keys}" } " {hint.does}" }
            }
        }
    }
}
