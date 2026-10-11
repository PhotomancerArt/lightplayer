//! What the screen says you can do right now (D20).
//!
//! The hint bar lists what the keys and mouse do *in this state*: this
//! selection, this tool, this gesture, these modifiers held. The cursor note
//! narrates a gesture while it happens. Both are derived from the editor,
//! like everything else, so a test can check them.
//!
//! The rule they serve: every action available in the current state is
//! visible in the hint bar.

use crate::editor::{Editor, Gesture, Tool};
use crate::fixture::Fixture;
use crate::props::common_props;
use crate::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    /// The keys or gesture, as written on screen: `⌫`, `⌘ click`, `− =`.
    pub keys: String,
    pub does: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HintBar {
    /// Where you are, in words: "circle1 selected", "Drawing a line".
    pub lead: String,
    pub hints: Vec<Hint>,
    /// Canvas navigation, shown on the right of the bar.
    pub view: Vec<Hint>,
}

fn h(keys: &str, does: impl Into<String>) -> Hint {
    Hint {
        keys: keys.into(),
        does: does.into(),
    }
}

impl Editor {
    pub fn hint_bar(&self) -> HintBar {
        let f = &self.fixture;
        let view = vec![
            h("scroll", "pan"),
            h("⌘ scroll · pinch", "zoom"),
            h("space-drag", "pan"),
            h("⇧1", "fit"),
            h("⌘Z", "undo"),
        ];
        let (lead, hints) = match &self.gesture {
            Gesture::Moving { owners, .. } => (
                format!(
                    "Moving {}",
                    list(&owners.iter().map(|o| o.to_string()).collect::<Vec<_>>())
                ),
                vec![h("release", "drop it here"), h("Esc", "put it back")],
            ),
            Gesture::Boxing { .. } => {
                let mut hints = vec![];
                if self.mods.alt {
                    hints.push(h("let go of ⌥", "take anything the box touches"));
                } else {
                    hints.push(h("hold ⌥", "take only what's fully inside"));
                }
                hints.push(h("⇧ at the start", "add to what's selected"));
                hints.push(h("Esc", "cancel"));
                ("Box-selecting".into(), hints)
            }
            Gesture::Panning { .. } => ("Panning".into(), vec![h("release", "stop")]),
            Gesture::Drawing { .. } => (
                format!("Drawing a {}", self.tool.word()),
                vec![h("release", "finish it"), h("Esc", "cancel")],
            ),
            Gesture::Press { .. } | Gesture::None => {
                if self.space_held {
                    (
                        "Hand".into(),
                        vec![
                            h("drag", "pan the canvas"),
                            h("let go of space", "back to selecting"),
                        ],
                    )
                } else {
                    match self.tool {
                        Tool::Line => (
                            "Line tool".into(),
                            vec![
                                h("drag", "draw a line — the lamp count follows its length"),
                                h("Esc · V", "back to selecting"),
                            ],
                        ),
                        Tool::Circle => (
                            "Circle tool".into(),
                            vec![
                                h(
                                    "drag",
                                    "draw a circle from its centre — the lamp count follows its size",
                                ),
                                h("Esc · V", "back to selecting"),
                            ],
                        ),
                        Tool::Select => (selection_lead(f, &self.selection), self.select_hints()),
                    }
                }
            }
        };
        HintBar { lead, hints, view }
    }

    fn select_hints(&self) -> Vec<Hint> {
        let f = &self.fixture;
        let mut out = Vec::new();

        // What a click would do, first: the hover outline in words.
        if let Some(target) = self.hover_target() {
            let going_in =
                self.selection.len() == 1 && f.parent(&target).as_ref() == self.selection.first();
            if self.mods.command {
                out.push(h(
                    "⌘ click",
                    format!("select {} directly", f.long_label(&target)),
                ));
            } else if going_in {
                out.push(h(
                    "click",
                    format!(
                        "go into {} — selects {}",
                        f.label(&self.selection[0]),
                        f.label(&target)
                    ),
                ));
            } else {
                out.push(h("click", format!("select {}", f.long_label(&target))));
            }
            if !self.mods.command
                && let Some(deep) = self.hover_hit()
                && deep != target
            {
                out.push(h("⌘ click", format!("select {} directly", f.label(&deep))));
            }
            out.push(h("⇧ click", "add to or remove from the selection"));
        }

        if self.selection.is_empty() {
            if self.hover_target().is_none() {
                out.push(h("click", "select something"));
            }
            out.push(h("drag on empty space", "box-select"));
            out.push(h("⌘A", "select everything"));
            out.push(h("L · O", "draw a line · a circle"));
            return out;
        }

        let first = &self.selection[0];
        let owners = self.move_owners();
        out.push(h(
            "drag",
            format!(
                "move {}",
                list(&owners.iter().map(|o| o.to_string()).collect::<Vec<_>>())
            ),
        ));
        match f.parent(first) {
            Some(p) => out.push(h("Esc", format!("up to {}", f.label(&p)))),
            None => out.push(h("Esc", "deselect")),
        }
        if let Some(child) = f.children(Some(first)).first() {
            out.push(h(
                "Enter",
                format!("into {} — selects {}", f.label(first), f.label(child)),
            ));
        }
        let word = f.kind_word(first);
        if f.siblings(first).len() > 1 {
            out.push(h("← →", format!("previous · next {word}")));
        }
        out.push(h("⌫", delete_hint(f, &self.selection)));

        let edit: Vec<Target> = self
            .selection
            .iter()
            .map(|t| match t {
                Target::Lamp { object, .. } => f.object_target(object),
                other => other.clone(),
            })
            .collect();
        if let Some(p) = common_props(f, &edit).iter().find(|p| p.desc.nudge) {
            let whose = if self
                .selection
                .iter()
                .any(|t| matches!(t, Target::Lamp { .. }))
            {
                format!(" of {}", f.label(&edit[0]))
            } else {
                String::new()
            };
            out.push(h(
                "− =",
                format!(
                    "{}{whose} ({})",
                    p.desc.label.to_lowercase(),
                    p.value.describe()
                ),
            ));
        }
        if word != "group" {
            out.push(h("r", "reverse the data direction"));
        }
        out.push(h("⌘A", self.select_all_hint()));
        out
    }

    /// The note beside the cursor during a gesture.
    pub fn cursor_note(&self) -> Option<String> {
        let f = &self.fixture;
        match &self.gesture {
            Gesture::Boxing { .. } => {
                let n = self.selection.len();
                let word = self
                    .selection
                    .first()
                    .map(|t| f.kind_word(t))
                    .unwrap_or("thing");
                let how = if self.mods.alt {
                    "fully inside the box"
                } else {
                    "the box touches"
                };
                let tail = if self.mods.alt {
                    "let go of ⌥ for anything it touches"
                } else {
                    "hold ⌥ for only those fully inside"
                };
                Some(format!(
                    "{n} {}{} {how} · {tail}",
                    word,
                    if n == 1 { "" } else { "s" }
                ))
            }
            Gesture::Drawing { .. } => {
                let kind = self.draft()?;
                let lamps: usize = crate::object::objects_of(&crate::Component {
                    id: crate::ComponentId::new("draft"),
                    kind,
                })
                .iter()
                .map(|o| o.lamps.len())
                .sum();
                Some(format!("{lamps} lamps · one every {} units", self.spacing))
            }
            Gesture::Moving { .. } => {
                if self.selection.iter().any(|t| !t.is_authored()) {
                    Some("rings and lamps move with the thing that makes them".into())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn selection_lead(f: &Fixture, selection: &[Target]) -> String {
    match selection {
        [] => format!("Nothing selected · {} · {} lamps", f.name, f.lamp_count()),
        [one] => format!("{} selected", f.long_label(one)),
        many => {
            let word = f.kind_word(&many[0]);
            let same = many.iter().all(|t| f.kind_word(t) == word);
            if same {
                format!("{} {word}s selected", many.len())
            } else {
                format!("{} things selected", many.len())
            }
        }
    }
}

fn delete_hint(f: &Fixture, selection: &[Target]) -> String {
    if selection.iter().all(|t| matches!(t, Target::Lamp { .. })) {
        return "lamps can't be deleted — − shortens their line".into();
    }
    match selection {
        [Target::Object(o)] => format!("remove ring {} from {}", o.key, o.component),
        [one] => format!("delete {}", f.label(one)),
        many => format!("delete these {}", many.len()),
    }
}

fn list(items: &[String]) -> String {
    match items.len() {
        0 => "nothing".into(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        n => format!("{n} things"),
    }
}
