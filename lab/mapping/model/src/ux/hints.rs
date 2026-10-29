//! What the screen says you can do right now (D20).
//!
//! The hint bar lists what the keys and mouse do *in this state*: this
//! selection, this tool, this gesture, these modifiers held. The cursor note
//! narrates a gesture while it happens. Both are derived from the editor,
//! like everything else, so a test can check them.
//!
//! The rule they serve: every action available in the current state is
//! visible in the hint bar. Each is named in a word or two ("select",
//! "go in", "delete"), never with the name of what it acts on: the
//! breadcrumb already says what's selected. A hint comes and goes only as
//! its action does, so the bar stays still while you point around.

use crate::domain::component::ComponentKind;
use crate::domain::patch_edits::OnWire;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::editor::{Editor, Gesture, Tool};
use crate::ux::graph_verb::GraphVerb;
use crate::ux::handle::{HandleKind, SizeEdge};
use crate::ux::patching::{ArmVerb, Armed, PairEnd};
use crate::ux::props::common_props;

#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    /// The keys or gesture, as written on screen: `⌫`, `⌘ click`, `− =`.
    pub keys: String,
    pub does: String,
    /// Mouse or keyboard: the bar shows them as separate rows.
    pub kind: HintKind,
    /// The verbs it names, for whoever presses verbs by name
    /// (`graph_verb.rs`): none for a gesture or a way of moving the view.
    pub verbs: Vec<GraphVerb>,
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
        verbs: Vec::new(),
    }
}

/// A hint that names verbs.
fn hv(keys: &str, does: impl Into<String>, verbs: &[GraphVerb]) -> Hint {
    Hint {
        verbs: verbs.to_vec(),
        ..h(keys, does)
    }
}

impl Editor {
    pub fn hint_bar(&self) -> HintBar {
        let view = vec![
            h("scroll", "pan"),
            h("⌘ scroll · pinch", "zoom"),
            h("space-drag", "pan"),
            h("⇧1", "fit"),
            h("⇧2", "frame selection"),
            hv("⌘Z", "undo", &[GraphVerb::Undo]),
        ];
        if let Some(armed) = &self.armed {
            return HintBar {
                lead: self.armed_lead(armed),
                hints: self.armed_hints(),
                view,
            };
        }
        let (lead, hints) = match &self.gesture {
            Gesture::Moving { .. } => (
                "Moving".into(),
                vec![h("release", "drop"), h("Esc", "put back")],
            ),
            Gesture::Boxing { .. } => (
                "Box-selecting".into(),
                vec![
                    h("hold ⌥", "only what's inside"),
                    h("⇧ at the start", "add"),
                    h("Esc", "cancel"),
                ],
            ),
            Gesture::Panning { .. } => ("Panning".into(), vec![h("release", "stop")]),
            Gesture::Drawing { .. } => (
                format!("Drawing a {}", self.tool.word()),
                vec![h("release", "finish"), h("Esc", "cancel")],
            ),
            Gesture::Handle { handle, .. } => {
                let mut hints = vec![h("release", "drop")];
                if matches!(handle, HandleKind::FixtureSize { .. }) {
                    hints.push(h("hold ⇧", "square"));
                }
                if matches!(
                    handle,
                    HandleKind::PathPoint { .. } | HandleKind::RingRadius { .. }
                ) {
                    hints.push(h("− =", "lamps, after"));
                }
                hints.push(h("Esc", "put back"));
                (handle.lead().into(), hints)
            }
            Gesture::Press { .. } | Gesture::None => {
                if self.menu.is_some() {
                    (
                        "Choosing".into(),
                        vec![h("click", "select"), h("⇧ click", "add"), h("Esc", "close")],
                    )
                } else if self.space_held {
                    (
                        "Hand".into(),
                        vec![h("drag", "pan"), h("let go of space", "back")],
                    )
                } else {
                    match self.tool {
                        Tool::Path if self.path_draft.is_empty() => (
                            "Path tool".into(),
                            vec![
                                h("click", "first point"),
                                hv("Esc · V", "done", &[GraphVerb::SelectTool]),
                            ],
                        ),
                        Tool::Path => (
                            "Drawing a path".into(),
                            vec![
                                h("click", "add point"),
                                h("double-click · Enter", "finish"),
                                h("Esc", "undo point"),
                            ],
                        ),
                        Tool::Circle => (
                            "Circle tool".into(),
                            vec![
                                h("drag", "draw"),
                                hv("Esc · V", "done", &[GraphVerb::SelectTool]),
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
        let sel = &self.selection;
        let mut out = vec![
            h("click", "select"),
            h("⇧ click", "add"),
            h("⌘ click", "select deepest"),
            h("⌥ click", "next one under"),
        ];

        // Dragging: a handle under the pointer, else the selection.
        let movable =
            !self.move_owners().is_empty() || (self.debug.pinning && !self.move_nodes().is_empty());
        if let Some(handle) = self.hover_handle() {
            out.push(h("drag", handle.kind.verb()));
            if let HandleKind::PathPoint { path, .. } = &handle.kind
                && f.path_points(path).is_some_and(|p| p.len() > 2)
            {
                out.push(h("⌥ click", "remove point"));
            }
        } else if self.move_draw && movable {
            out.push(h("drag", "move"));
        }
        out.push(h("drag on empty space", "box-select"));
        if self.move_draw && !self.shaped_paths(sel).is_empty() {
            out.push(h("double-click", "add point"));
            out.push(h("J", "jumper"));
        }
        out.push(h("right-click", "what's here"));

        if let Some(first) = sel.first() {
            out.push(hv(
                "Esc",
                if f.parent(first).is_some() {
                    "up"
                } else {
                    "deselect"
                },
                &[GraphVerb::Up],
            ));
            if !f.children(Some(first)).is_empty() {
                out.push(hv("Enter", "go in", &[GraphVerb::GoIn]));
            }
            if f.siblings(first).len() > 1 {
                out.push(hv(
                    "← →",
                    "previous · next",
                    &[GraphVerb::Previous, GraphVerb::Next],
                ));
            }
            if deletable(f, sel) {
                out.push(hv("⌫", "delete", &[GraphVerb::Delete]));
            }
            let edit: Vec<Target> = sel
                .iter()
                .map(|t| match t {
                    Target::Lamp { object, .. } => f.object_target(object),
                    Target::Pixel { .. } => t.port_of_pixel().unwrap_or_else(|| t.clone()),
                    other => other.clone(),
                })
                .collect();
            if let Some(p) = common_props(f, &edit)
                .iter()
                .find(|p| p.desc.nudge && (self.move_draw || !p.desc.reshapes))
            {
                out.push(hv(
                    "− =",
                    p.desc.label.to_lowercase(),
                    &[GraphVerb::Fewer, GraphVerb::More],
                ));
            }
            if self.move_draw && sel.iter().all(|t| f.has_direction(t)) {
                out.push(hv("R", "reverse", &[GraphVerb::Reverse]));
            }
            if self.move_draw
                && self
                    .move_nodes()
                    .iter()
                    .any(|n| f.node(n).is_some_and(|x| x.pinned.is_some()))
            {
                out.push(hv("⌥U", "unpin", &[GraphVerb::Unpin]));
            }
            out.extend(self.patch_hints());
        }
        out.push(hv("⌘A", "select all", &[GraphVerb::SelectAll]));
        if self.move_draw {
            out.push(hv(
                "P · O",
                "path · circle",
                &[GraphVerb::PathTool, GraphVerb::CircleTool],
            ));
        }
        out.push(hv("D", "move & draw", &[GraphVerb::MoveDraw]));
        out
    }

    /// The patching keys for the selection (`patching.rs`): what a hand
    /// patch can do to it right now.
    fn patch_hints(&self) -> Vec<Hint> {
        let p = &self.project;
        let mut out = Vec::new();
        let objects = self.selected_objects();
        let on_wire = !objects.is_empty()
            && objects.len() == self.selection.len()
            && objects
                .iter()
                .all(|o| !matches!(p.on_wire(o), OnWire::Nowhere));
        if let Some(end) = self.selected_end() {
            out.push(hv(
                "A · M",
                "patch",
                &[GraphVerb::Patch, GraphVerb::PatchNext],
            ));
            if matches!(end, PairEnd::Object(_)) && on_wire {
                out.push(hv("S", "swap", &[GraphVerb::Swap]));
            }
        }
        if on_wire {
            if !self.move_draw {
                out.push(hv("R", "reverse", &[GraphVerb::Reverse]));
            }
            out.push(hv("K L", "turn", &[GraphVerb::Turn, GraphVerb::TurnBack]));
            out.push(hv(
                "; '",
                "turn ⅓",
                &[GraphVerb::TurnThird, GraphVerb::TurnThirdBack],
            ));
            out.push(hv(
                "[ ]",
                "shift",
                &[GraphVerb::ShiftBack, GraphVerb::Shift],
            ));
            if objects
                .iter()
                .all(|o| matches!(p.on_wire(o), OnWire::Placed { .. }))
            {
                out.push(hv("U", "unpatch", &[GraphVerb::Unpatch]));
            }
        }
        let on_ports = !p.wire_ports().is_empty()
            && (on_wire
                || self.selected_end().is_some()
                || self
                    .selection
                    .iter()
                    .any(|t| matches!(t, Target::Port { .. } | Target::Pixel { .. })));
        if on_ports && self.wire_has_places() {
            out.push(hv(
                "⇧← ⇧→",
                "along the wire",
                &[GraphVerb::WirePrevious, GraphVerb::WireNext],
            ));
        }
        if on_ports && self.next_port_segment().is_some() {
            out.push(hv(".", "next port", &[GraphVerb::NextPort]));
        }
        if let Some(fixture) = self.selection.first().and_then(|t| p.fixture_of_target(t)) {
            out.push(hv(
                "⇧A",
                if p.is_auto(&fixture) {
                    "auto off"
                } else {
                    "auto on"
                },
                &[GraphVerb::Auto],
            ));
            let placed = p.fixture(&fixture).is_some_and(|f| {
                f.places.iter().any(|r| {
                    r.on != crate::domain::patch_place::PatchTarget::Fixture && r.wire.is_some()
                })
            });
            if placed {
                out.push(hv("⇧U", "unpatch all", &[GraphVerb::UnpatchAll]));
            }
        }
        out
    }

    /// The bar's lead while armed: what the pair is waiting for.
    /// It names the candidate in focus too, the cursor note's words, since
    /// at the keyboard the cursor may be nowhere near.
    fn armed_lead(&self, armed: &Armed) -> String {
        if let Some(note) = self.cursor_note() {
            return note;
        }
        let from = self.end_words(&armed.from);
        match armed.verb {
            ArmVerb::Patch => format!("Patching {from} · nothing to pair with"),
            ArmVerb::Swap => format!("Swapping {from} · nothing to swap with"),
        }
    }

    fn armed_hints(&self) -> Vec<Hint> {
        let swap = self.armed.as_ref().is_some_and(|a| a.verb == ArmVerb::Swap);
        let mut out = vec![
            h("← →", "candidates"),
            h("click", if swap { "swap with this" } else { "patch here" }),
            h("Enter", if swap { "swap" } else { "patch" }),
        ];
        if !swap {
            out.push(hv("M", "patch & next", &[GraphVerb::PatchNext]));
        }
        out.push(h("Esc", "cancel"));
        out
    }

    /// The note beside the cursor during a gesture.
    pub fn cursor_note(&self) -> Option<String> {
        let f = &self.project;
        if let Some(armed) = &self.armed {
            let other = self.focused_candidate()?;
            let (verb, a, b) = match armed.verb {
                ArmVerb::Patch => ("Patch", &armed.from, &other),
                ArmVerb::Swap => ("Swap", &armed.from, &other),
            };
            // Say the object first: "Patch panel 7 to Box 1 port 2 …".
            let (object, place) = match (a, b) {
                (PairEnd::Segment { .. }, o) => (o, a),
                _ => (a, b),
            };
            let joint = if armed.verb == ArmVerb::Swap {
                "with"
            } else {
                "to"
            };
            return Some(format!(
                "{verb} {} {joint} {}",
                self.end_words(object),
                self.end_words(place)
            ));
        }
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
            Gesture::Moving { owners, nodes, .. } => {
                if owners.is_empty()
                    && let [(id, _)] = nodes.as_slice()
                {
                    Some(format!(
                        "Dropping pins {} here · U gives it back to the layout",
                        f.node_name(id)
                    ))
                } else if self
                    .selection
                    .iter()
                    .any(|t| matches!(t, Target::Object(_) | Target::Lamp { .. }))
                {
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
            .map(|objects| objects.iter().map(|o| o.lamps.len()).sum())
            .unwrap_or(0)
    }

    /// The note beside the cursor while a handle is dragged: what it's
    /// changing, and that a count stays put.
    fn handle_note(&self, handle: &HandleKind) -> String {
        let f = &self.project;
        match handle {
            HandleKind::PathPoint { path, index } => {
                let (n, lamps) = match f.component(path).map(|c| &c.kind) {
                    Some(ComponentKind::Path { points, lamps, .. })
                    | Some(ComponentKind::Polygon { points, lamps }) => (points.len(), *lamps),
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

/// Whether ⌫ deletes the selection: lamps, pixels and an array's copies
/// can't be (their count is the thing to change).
fn deletable(f: &Project, selection: &[Target]) -> bool {
    if selection
        .iter()
        .all(|t| matches!(t, Target::Lamp { .. } | Target::Pixel { .. }))
    {
        return false;
    }
    !matches!(selection, [Target::Object(o)] if f.ring(&o.component, &o.key).is_none())
}
