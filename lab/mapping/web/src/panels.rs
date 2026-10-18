//! The tree, the inspector and the hint bar, drawn from the view tree as
//! core gives them: rows, sections, hints. Nothing here decides what to
//! say.

use dioxus::prelude::*;
use lab_mapping_model::{
    GraphInput, Hint, HintBar as Bar, HintKind, Notice, NoticeKind, Prop, PropEdit, PropKind,
    PropValue, UiInspector, UiItem, UiTree,
};

use crate::graph_editor::GraphEditor;

#[component]
pub fn TreePane(ed: GraphEditor, tree: Memo<UiTree>) -> Element {
    let tree = tree.read().clone();
    rsx! {
        div { class: "pane-title", "{tree.title}" }
        if let Some(empty) = tree.empty {
            div { class: "empty", "{empty}" }
        }
        for row in tree.rows.into_iter() {
            {
                let t = row.target.clone();
                let ed = ed.clone();
                let mut class = String::from("row");
                if row.selected {
                    class.push_str(" row-selected");
                } else if row.lit {
                    class.push_str(" row-lit");
                }
                if row.context {
                    class.push_str(" row-context");
                }
                if row.derived {
                    class.push_str(" row-derived");
                }
                if !row.kind.is_empty() {
                    class.push_str(" row-");
                    class.push_str(row.kind);
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
                                ed.input(GraphInput::ToggleSelect(t.clone()));
                            } else {
                                ed.input(GraphInput::Select(vec![t.clone()]));
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
pub fn Inspector(ed: GraphEditor, inspector: Memo<UiInspector>) -> Element {
    let inspector = inspector.read().clone();
    rsx! {
        div { class: "crumbs",
            for (i, c) in inspector.crumbs.into_iter().enumerate() {
                {
                    let ed = ed.clone();
                    rsx! {
                        span { key: "c{i}", class: "crumb-step",
                            if i > 0 {
                                span { class: "crumb-sep", "›" }
                            }
                            button {
                                class: "crumb",
                                tabindex: "-1",
                                onmousedown: move |evt| evt.prevent_default(),
                                onclick: move |_| {
                                    ed.input(GraphInput::Select(c.target.iter().cloned().collect()));
                                },
                                "{c.label}"
                            }
                        }
                    }
                }
            }
            if let Some(many) = inspector.many {
                span { class: "crumb-step",
                    span { class: "crumb-sep", "›" }
                    span { class: "crumb crumb-many", "{many}" }
                }
            }
        }
        for (i, section) in inspector.sections.into_iter().enumerate() {
            div { key: "s{i}",
                if let Some(title) = section.title {
                    div { class: "section-title", "{title}" }
                }
                for (j, item) in section.items.into_iter().enumerate() {
                    InspectorItem { key: "{j}", ed: ed.clone(), item }
                }
            }
        }
    }
}

/// One line of the inspector, as core gives it.
#[component]
fn InspectorItem(ed: GraphEditor, item: UiItem) -> Element {
    match item {
        UiItem::Fact(text) => rsx! { div { class: "fact", "{text}" } },
        UiItem::Rule(text) => rsx! { div { class: "fact patch-rule", "{text}" } },
        UiItem::Hint(text) => rsx! { div { class: "hint-inline", "{text}" } },
        UiItem::Code(code) => rsx! { pre { class: "glsl", "{code}" } },
        UiItem::Port { name, fill, ratio } => rsx! {
            div { class: "port-fact",
                span { "{name}" }
                span { class: "port-fact-fill", "{fill}" }
                div { class: "port-fact-bar", div { class: "port-fact-bar-in", style: "width: {ratio * 100.0}%" } }
            }
        },
        UiItem::Problem(pr) => rsx! {
            div { class: "notice refused problem",
                div { class: "notice-text", span { class: "notice-tag", "problem: " } "{pr.text}" }
                if let Some(why) = pr.why {
                    div { class: "notice-line", span { class: "notice-eq", "= why: " } "{why}" }
                }
                if let Some(help) = pr.help {
                    div { class: "notice-line", span { class: "notice-eq", "= help: " } "{help}" }
                }
            }
        },
        UiItem::Prop { prop, locked } => rsx! { PropRow { ed: ed.clone(), prop, locked } },
    }
}

/// One property, laid out from its descriptor alone.
#[component]
fn PropRow(ed: GraphEditor, prop: Prop, locked: bool) -> Element {
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
            let (down, typed, up) = (ed.clone(), ed.clone(), ed.clone());
            rsx! {
                div { class: "num",
                    button {
                        tabindex: "-1",
                        disabled: locked,
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| {
                            down.input(GraphInput::EditProp { id, edit: PropEdit::Nudge(-1) });
                        },
                        "−"
                    }
                    input {
                        r#type: "number",
                        disabled: locked,
                        step: "{step}",
                        value: "{shown}",
                        placeholder: "{value}",
                        // Typing here must not drive the canvas.
                        onkeydown: move |evt| evt.stop_propagation(),
                        onchange: move |evt| {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                let edit = if is_count { PropEdit::SetCount(v.round() as i64) } else { PropEdit::SetNumber(v) };
                                typed.input(GraphInput::EditProp { id, edit });
                            }
                        },
                    }
                    button {
                        tabindex: "-1",
                        disabled: locked,
                        onmousedown: move |evt| evt.prevent_default(),
                        onclick: move |_| {
                            up.input(GraphInput::EditProp { id, edit: PropEdit::Nudge(1) });
                        },
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
                    onclick: move |_| {
                        ed.input(GraphInput::EditProp { id, edit: PropEdit::SetFlag(!on) });
                    },
                    "{label}"
                }
            }
        }
    };
    rsx! {
        div {
            class: "prop",
            title: if locked { "Move & draw is off · M turns it on" } else { "" },
            span { class: "prop-label", "{prop.desc.label}" }
            {control}
            if mixed {
                span { class: "prop-mixed", "{value}" }
            }
        }
    }
}

#[component]
pub fn HintBar(hints: Memo<(Bar, Option<Notice>)>) -> Element {
    let (bar, notice) = hints.read().clone();
    let (mouse, keys): (Vec<Hint>, Vec<Hint>) = bar
        .hints
        .iter()
        .cloned()
        .partition(|h| h.kind == HintKind::Mouse);
    rsx! {
        if let Some(n) = notice {
            div { class: if n.kind == NoticeKind::Refused { "notice refused" } else { "notice" },
                // The text already says "Can't …": no tag repeating it.
                div { class: "notice-text", "{n.text}" }
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
