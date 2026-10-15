//! What the screen says you can do right now (D20).
//!
//! The hint bar lists what the keys and mouse do *in this state*: this
//! selection, this tool, this gesture, these modifiers held. The cursor note
//! narrates a gesture while it happens. Both are derived from the editor,
//! like everything else, so a test can check them.
//!
//! The rule they serve: every action available in the current state is
//! visible in the hint bar.

use crate::component::ComponentKind;
use crate::editor::{Editor, Gesture, Tool};
use crate::handle::{HandleKind, SizeEdge};
use crate::project::Project;
use crate::props::common_props;
use crate::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    /// The keys or gesture, as written on screen: `⌫`, `⌘ click`, `− =`.
    pub keys: String,
    pub does: String,
    /// Mouse or keyboard: the bar shows them as separate rows.
    pub kind: HintKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintKind {
    Mouse,
    Key,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HintBar {
    /// What you're doing, in words: "Drawing a path", "Moving circle1",
    /// "Box-selecting". Empty while you are plainly selecting: the
    /// breadcrumb already says what's selected, and the bar never repeats
    /// it.
    pub lead: String,
    pub hints: Vec<Hint>,
    /// Canvas navigation, shown on the right of the bar.
    pub view: Vec<Hint>,
}

fn h(keys: &str, does: impl Into<String>) -> Hint {
    let mouse = ["click", "drag", "release", "scroll", "pinch"]
        .iter()
        .any(|w| keys.contains(w));
    Hint {
        keys: keys.into(),
        does: does.into(),
        kind: if mouse {
            HintKind::Mouse
        } else {
            HintKind::Key
        },
    }
}

impl Editor {
    pub fn hint_bar(&self) -> HintBar {
        let view = vec![
            h("scroll", "pan"),
            h("⌘ scroll · pinch", "zoom"),
            h("space-drag", "pan"),
            h("⇧1", "fit"),
            h("⇧2", "frame the selection"),
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
            Gesture::Handle { handle, .. } => {
                let mut hints = vec![h("release", "drop it here"), h("Esc", "put it back")];
                if matches!(handle, HandleKind::FixtureSize { .. }) {
                    hints.insert(
                        1,
                        if self.mods.shift {
                            h("let go of ⇧", "any shape")
                        } else {
                            h("hold ⇧", "keep it square")
                        },
                    );
                }
                if matches!(
                    handle,
                    HandleKind::PathPoint { .. } | HandleKind::RingRadius { .. }
                ) {
                    hints.push(h("− =", "change the lamp count after"));
                }
                (handle_lead(&self.project, handle), hints)
            }
            Gesture::Press { .. } | Gesture::None => {
                if self.menu.is_some() {
                    (
                        "Choosing what's under the cursor".into(),
                        vec![
                            h("click", "select it"),
                            h("⇧ click", "add it"),
                            h("Esc", "close the list"),
                        ],
                    )
                } else if self.space_held {
                    (
                        "Hand".into(),
                        vec![
                            h("drag", "pan the canvas"),
                            h("let go of space", "back to selecting"),
                        ],
                    )
                } else {
                    match self.tool {
                        Tool::Path if self.path_draft.is_empty() => (
                            "Path tool".into(),
                            vec![
                                h("click", "the path's first point, inside a fixture"),
                                h("Esc · V", "back to selecting"),
                            ],
                        ),
                        Tool::Path => {
                            let lamps = self.draft_lamps();
                            (
                                "Drawing a path".into(),
                                vec![
                                    h("click", "add a point"),
                                    h("double-click · Enter", format!("finish it ({lamps} lamps)")),
                                    h("Esc", "take back the last point"),
                                ],
                            )
                        }
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
                        Tool::Select => (String::new(), self.select_hints()),
                    }
                }
            }
        };
        HintBar { lead, hints, view }
    }

    fn select_hints(&self) -> Vec<Hint> {
        let f = &self.project;
        let mut out = Vec::new();

        // A handle under the pointer: dragging it is what's on offer.
        let handle = self.hover_handle();
        if let Some(handle) = &handle {
            out.push(h("drag", handle.kind.does(f)));
        }

        // What a click would do, first: the hover outline in words.
        if let Some(target) = self.hover_target() {
            let going_in =
                self.selection.len() == 1 && f.parent(&target).as_ref() == self.selection.first();
            if self.mods.command {
                out.push(h(
                    "⌘ click",
                    format!("select {} directly", f.long_label(&target)),
                ));
            } else if self.mods.alt {
                out.push(h(
                    "⌥ click",
                    format!("select {} — the next one here", f.long_label(&target)),
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
                && let Some(deep) = self.hover_deep()
                && deep != target
            {
                out.push(h("⌘ click", format!("select {} directly", f.label(&deep))));
            }
            let here = self.hover_candidates().len();
            if here > 1 && !self.mods.alt {
                out.push(h("⌥ click", format!("the next of the {here} here")));
            }
            out.push(h("⇧ click", "add or remove"));
            out.push(h("right-click", "what's here"));
        }

        if self.selection.is_empty() {
            if self.hover_target().is_none() {
                out.push(h("click", "select something"));
            }
            out.push(h("drag on empty space", "box-select"));
            out.push(h("⌘A", "select everything"));
            out.push(h("P · O", "draw a path · a circle"));
            return out;
        }

        let first = &self.selection[0];
        let owners = self.move_owners();
        if !owners.is_empty() && handle.is_none() {
            out.push(h(
                "drag",
                format!(
                    "move {}",
                    list(&owners.iter().map(|o| o.to_string()).collect::<Vec<_>>())
                ),
            ));
        }
        match f.parent(first) {
            Some(p) => out.push(h("Esc", format!("up to {}", f.label(&p)))),
            None => out.push(h("Esc", "deselect")),
        }
        if let Some(child) = f.children(Some(first)).first() {
            let framed = if f.is_fixture(first) && !f.outputs_of(first.node().unwrap()).is_empty() {
                ", framed with its outputs"
            } else {
                ""
            };
            out.push(h(
                "Enter",
                format!(
                    "into {} — selects {}{framed}",
                    f.label(first),
                    f.label(child)
                ),
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
        if self.selection.iter().all(|t| f.has_direction(t)) {
            out.push(h("r", "reverse the data direction"));
        }
        out.push(h("⌘A", self.select_all_hint()));
        out
    }

    /// The note beside the cursor during a gesture.
    pub fn cursor_note(&self) -> Option<String> {
        let f = &self.project;
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
                self.draft()?;
                Some(format!(
                    "{} lamps · one every {} units",
                    self.draft_lamps(),
                    self.spacing
                ))
            }
            Gesture::None if self.tool == Tool::Path && !self.path_draft.is_empty() => {
                self.draft()?;
                Some(format!(
                    "{} lamps · one every {} units · double-click or Enter to finish",
                    self.draft_lamps(),
                    self.spacing
                ))
            }
            Gesture::Handle { handle, .. } => Some(self.handle_note(handle)),
            Gesture::None if self.menu.is_none() => {
                if let Some(edge) = self.hovered_edge() {
                    return Some(edge.carries);
                }
                let here = self.hover_candidates();
                if here.len() > 1 {
                    let word = f.kind_word(&here[0]);
                    let same = here.iter().all(|t| f.kind_word(t) == word);
                    let what = if same {
                        format!("{} {word}s", here.len())
                    } else {
                        format!("{} things", here.len())
                    };
                    Some(format!(
                        "{what} here · ⌥ click for the next · right-click to choose"
                    ))
                } else {
                    None
                }
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

impl Editor {
    /// How many lamps the draft has.
    fn draft_lamps(&self) -> usize {
        self.draft()
            .map(|kind| {
                crate::object::objects_of(&crate::Component {
                    id: crate::ComponentId::new("draft"),
                    kind,
                })
                .iter()
                .map(|o| o.lamps.len())
                .sum()
            })
            .unwrap_or(0)
    }

    /// The note beside the cursor while a handle is dragged: what it's
    /// changing, and that a count stays put.
    fn handle_note(&self, handle: &HandleKind) -> String {
        let f = &self.project;
        match handle {
            HandleKind::PathPoint { path, index } => {
                let (n, lamps) = match f.component(path).map(|c| &c.kind) {
                    Some(ComponentKind::Path { points, lamps }) => (points.len(), *lamps),
                    _ => (0, 0),
                };
                let what = if *index == 0 || *index + 1 == n {
                    "an end"
                } else {
                    "a point"
                };
                format!("Dragging {what} keeps {lamps} lamps · − = changes the count")
            }
            HandleKind::RingRadius { circle, ring } => match f.ring(circle, ring) {
                Some(r) => format!(
                    "Radius {:.1} · ring {ring} keeps {} lamps · − = changes the count",
                    r.radius, r.lamps
                ),
                None => String::new(),
            },
            HandleKind::FixtureSize { fixture, edge } => {
                let size = f.fixture(fixture).map(|x| x.size).unwrap_or_default();
                let tail = match (edge, self.mods.shift) {
                    (_, true) => "square",
                    (SizeEdge::Corner, false) => "hold ⇧ to keep it square",
                    (_, false) => "hold ⇧ to make it square",
                };
                format!("{:.0} × {:.0} · {tail}", size.x, size.y)
            }
        }
    }
}

/// The bar's lead while a handle is dragged: "Moving an end of path1".
fn handle_lead(f: &Project, handle: &HandleKind) -> String {
    match handle {
        HandleKind::PathPoint { path, index } => {
            let n = f.path_points(path).map_or(0, |p| p.len());
            if *index == 0 || *index + 1 == n {
                format!("Moving an end of {path}")
            } else {
                format!("Moving a point of {path}")
            }
        }
        HandleKind::RingRadius { circle, ring } => {
            format!("Changing the radius of ring {ring} of {circle}")
        }
        HandleKind::FixtureSize { fixture, .. } => format!("Resizing {}", f.node_name(fixture)),
    }
}

fn delete_hint(f: &Project, selection: &[Target]) -> String {
    if selection.iter().all(|t| matches!(t, Target::Lamp { .. })) {
        return "lamps can't be deleted — − shortens their path or ring".into();
    }
    match selection {
        [Target::Object(o)] => format!("remove ring {} from {}", o.key, o.component),
        [t @ Target::Port { output, .. }] => {
            format!("remove {} from {}", f.label(t), f.node_name(output))
        }
        [Target::Node(id)] if f.output(id).is_some() => {
            let fed = f
                .fixtures()
                .iter()
                .filter(|n| {
                    n.fixture()
                        .is_some_and(|x| x.rules.iter().any(|r| r.output() == id))
                })
                .count();
            if fed == 0 {
                format!("delete {}", f.node_name(id))
            } else {
                format!(
                    "delete {} — the {fed} fixture{} patched to it keep their rule, as a problem",
                    f.node_name(id),
                    if fed == 1 { "" } else { "s" }
                )
            }
        }
        [Target::Node(id)] if f.node(id).is_some_and(|n| !n.children().is_empty()) => {
            format!("delete {} and everything in it", f.node_name(id))
        }
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
