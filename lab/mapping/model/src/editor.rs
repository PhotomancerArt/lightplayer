//! The editor: one state, driven by input events, with no IO.
//!
//! The web page forwards pointer, wheel and key events here and draws what
//! it finds. Everything that decides how the editor *feels* lives in this
//! crate, so it can be read, changed and tested without a browser.

use crate::camera::Camera;
use crate::component::{ComponentId, ComponentKind};
use crate::geom::{Rect, Vec2};
use crate::node::NodeId;
use crate::notice::Notice;
use crate::pick::{PickMode, level_candidates, pick};
use crate::project::{Project, union};
use crate::target::Target;

/// Modifier keys held during an event. `command` is ⌘ on a Mac (Ctrl
/// elsewhere); a trackpad pinch arrives as a wheel event with Ctrl held, so
/// both mean "zoom" on the wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
    pub ctrl: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Primary,
    Middle,
    /// Right-click: the "what's here" menu.
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Space,
    Char(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Line,
    Circle,
}

impl Tool {
    pub fn word(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Line => "line",
            Tool::Circle => "circle",
        }
    }
}

/// What the pointer is in the middle of.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    None,
    /// Button down, not yet moved far enough to be a drag.
    Press {
        start: Vec2,
        /// Something was under the cursor.
        over: bool,
        /// …and it was (inside) the selection.
        on_selection: bool,
        /// The press already chose (⌥-click); releasing does nothing more.
        decided: bool,
        mods: Mods,
    },
    /// Dragging the selection. `base` is the fixture before the drag, so the
    /// move is always "base + total delta" and never drifts.
    Moving {
        start_world: Vec2,
        base: Box<Project>,
        owners: Vec<ComponentId>,
    },
    /// Dragging a selection box from empty space.
    Boxing {
        start_world: Vec2,
        base_selection: Vec<Target>,
    },
    Panning {
        last: Vec2,
    },
    Drawing {
        start_world: Vec2,
    },
}

/// The right-click menu: everything under the cursor, every level, in tree
/// order. Opened only when asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    /// Where it opened, in screen pixels.
    pub at: Vec2,
    pub items: Vec<MenuItem>,
    /// The item under the pointer: outlined on the canvas.
    pub hover: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub target: Target,
    /// Indent: 0 for top-level things.
    pub depth: usize,
    pub label: String,
}

/// Around lamps framed with ⇧2, in world units, so one lamp isn't the
/// whole screen.
const FRAME_PAD: f64 = 6.0;
/// How far, in screen pixels, a press moves before it is a drag.
pub const DRAG_THRESHOLD_PX: f64 = 3.0;
const UNDO_LIMIT: usize = 200;

#[derive(Debug, Clone)]
pub struct Editor {
    pub project: Project,
    /// The one selection (D15): a set of targets, usually at one level.
    pub selection: Vec<Target>,
    pub camera: Camera,
    pub viewport: Vec2,
    pub tool: Tool,
    pub gesture: Gesture,
    /// The pointer, in screen pixels, while it is over the canvas.
    pub pointer: Option<Vec2>,
    pub mods: Mods,
    pub space_held: bool,
    /// Lamp spacing, in fixture units, for drawing.
    pub spacing: f64,
    pub notice: Option<Notice>,
    pub menu: Option<ContextMenu>,
    undo: Vec<(Project, Vec<Target>)>,
    redo: Vec<(Project, Vec<Target>)>,
    fitted: bool,
}

impl Editor {
    pub fn new(project: Project) -> Self {
        let viewport = Vec2::new(900.0, 600.0);
        let camera = Camera::fit(project.extent(), viewport, 40.0);
        Self {
            project,
            selection: Vec::new(),
            camera,
            viewport,
            tool: Tool::Select,
            gesture: Gesture::None,
            pointer: None,
            mods: Mods::default(),
            space_held: false,
            spacing: 2.4,
            notice: None,
            menu: None,
            undo: Vec::new(),
            redo: Vec::new(),
            fitted: false,
        }
    }

    // ---- what the page reads -------------------------------------------

    /// The pointer, if hovering means anything right now (select tool, no
    /// gesture, no menu, not panning).
    pub(crate) fn hover_point(&self) -> Option<Vec2> {
        let idle = matches!(self.gesture, Gesture::None) && self.tool == Tool::Select;
        if !idle || self.space_held || self.menu.is_some() {
            return None;
        }
        self.pointer
    }

    /// What a click right now would select — the hover outline (D17).
    pub fn hover_target(&self) -> Option<Target> {
        let p = self.hover_point()?;
        pick(
            &self.project,
            &self.camera,
            &self.selection,
            p,
            self.click_mode(self.mods),
        )
    }

    /// What a ⌘-click right now would select.
    pub fn hover_deep(&self) -> Option<Target> {
        let p = self.hover_point()?;
        pick(
            &self.project,
            &self.camera,
            &self.selection,
            p,
            PickMode::Deep,
        )
    }

    /// The things at the click's level under the pointer, nearest first —
    /// more than one means ⌥-click has somewhere to go.
    pub fn hover_candidates(&self) -> Vec<Target> {
        match self.hover_point() {
            Some(p) => level_candidates(&self.project, &self.camera, &self.selection, p),
            None => Vec::new(),
        }
    }

    /// The level the selection lives in: its parent, or `None` for the
    /// fixture's top level.
    pub fn context(&self) -> Option<Target> {
        self.selection.first().and_then(|s| self.project.parent(s))
    }

    /// The selection box, in fixture units, while one is being dragged.
    pub fn selection_box(&self) -> Option<Rect> {
        match &self.gesture {
            Gesture::Boxing { start_world, .. } => Some(Rect::from_corners(
                *start_world,
                self.camera.to_world(self.pointer?),
            )),
            _ => None,
        }
    }

    /// The component being drawn, before it exists.
    pub fn draft(&self) -> Option<ComponentKind> {
        let Gesture::Drawing { start_world } = &self.gesture else {
            return None;
        };
        let now = self.camera.to_world(self.pointer?);
        Some(self.drawn(*start_world, now))
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---- input ---------------------------------------------------------

    pub fn resize(&mut self, size: Vec2) {
        self.viewport = size;
        if !self.fitted {
            self.fit();
            self.fitted = true;
        }
    }

    /// ⇧1: everything on the canvas.
    pub fn fit(&mut self) {
        self.camera = Camera::fit(self.project.extent(), self.viewport, 40.0);
    }

    /// ⇧2: the selection. A fixture is framed with the outputs it feeds,
    /// so you see both ends of its patch; anything else, on its own.
    pub fn frame_selection(&mut self) {
        let rects = self.selection.iter().filter_map(|t| self.framing_rect(t));
        match union(rects) {
            Some(r) => self.camera = Camera::fit(r, self.viewport, 40.0),
            None => {
                self.notice = Some(Notice::refused(
                    "Nothing to frame",
                    "nothing is selected",
                    "select something first; ⇧1 fits everything",
                ))
            }
        }
    }

    /// What ⇧2 frames for one target, in world units.
    pub fn framing_rect(&self, t: &Target) -> Option<Rect> {
        let p = &self.project;
        match t {
            Target::Node(id) if p.fixture(id).is_some() => union(
                std::iter::once(id.clone())
                    .chain(p.outputs_of(id))
                    .filter_map(|n| p.node_rect(&n)),
            ),
            Target::Node(id) => p.node_rect(id),
            Target::Port { output, key } => p.port_rect(output, key),
            other => p.lamps_rect(other).map(|r| {
                let pad = Vec2::new(FRAME_PAD, FRAME_PAD);
                Rect::new(r.min - pad, r.max + pad)
            }),
        }
    }

    pub fn pointer_down(&mut self, at: Vec2, button: Button, mods: Mods) {
        self.mods = mods;
        self.pointer = Some(at);
        self.notice = None;
        if button == Button::Secondary {
            self.open_menu(at);
            return;
        }
        if self.menu.take().is_some() {
            // A click away from the menu just closes it.
            return;
        }
        if button == Button::Middle || self.space_held {
            self.gesture = Gesture::Panning { last: at };
            return;
        }
        match self.tool {
            Tool::Line | Tool::Circle => {
                self.gesture = Gesture::Drawing {
                    start_world: self.camera.to_world(at),
                };
            }
            Tool::Select => {
                let under = self.project.things_at(&self.camera, at);
                let over = !under.is_empty();
                let on_selection = under.iter().any(|t| self.selection.contains(t));
                let mut decided = false;
                if over && mods.alt {
                    // ⌥: the next overlapping thing at this level.
                    if let Some(t) = pick(
                        &self.project,
                        &self.camera,
                        &self.selection,
                        at,
                        PickMode::Next,
                    ) {
                        self.selection = vec![t];
                    }
                    decided = true;
                } else if over && !on_selection {
                    // Pressing something new selects it straight away, so a
                    // drag moves it. Pressing the selection waits: it may be
                    // a drag (move it) or a click (go into it).
                    if let Some(t) = pick(
                        &self.project,
                        &self.camera,
                        &self.selection,
                        at,
                        self.press_mode(mods),
                    ) {
                        if mods.shift {
                            self.toggle(t);
                        } else {
                            self.selection = vec![t];
                        }
                    }
                }
                self.gesture = Gesture::Press {
                    start: at,
                    over,
                    on_selection: on_selection || decided,
                    decided,
                    mods,
                };
            }
        }
    }

    pub fn pointer_move(&mut self, at: Vec2, mods: Mods) {
        let prev = self.pointer.replace(at);
        self.mods = mods;
        match self.gesture.clone() {
            Gesture::Press {
                start,
                over,
                mods: press_mods,
                ..
            } => {
                if start.distance(at) < DRAG_THRESHOLD_PX {
                    return;
                }
                let start_world = self.camera.to_world(start);
                if over && !self.selection.is_empty() {
                    self.begin_move(start_world);
                    self.drag_to(at);
                } else {
                    let base_selection = if press_mods.shift {
                        self.selection.clone()
                    } else {
                        Vec::new()
                    };
                    self.gesture = Gesture::Boxing {
                        start_world,
                        base_selection,
                    };
                    self.update_box();
                }
            }
            Gesture::Moving { .. } => self.drag_to(at),
            Gesture::Boxing { .. } => self.update_box(),
            Gesture::Panning { last } => {
                self.camera.pan(at - last);
                self.gesture = Gesture::Panning { last: at };
            }
            Gesture::Drawing { .. } | Gesture::None => {
                let _ = prev;
            }
        }
    }

    pub fn pointer_up(&mut self, at: Vec2, mods: Mods) {
        self.pointer = Some(at);
        self.mods = mods;
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::Press { decided: true, .. } => {}
            Gesture::Press {
                over,
                on_selection,
                mods: press_mods,
                ..
            } => {
                if over && on_selection {
                    // A click on the selection: go in, add/remove, or go deep.
                    if let Some(t) = pick(
                        &self.project,
                        &self.camera,
                        &self.selection,
                        at,
                        self.click_mode(press_mods),
                    ) {
                        if press_mods.shift {
                            self.toggle(t);
                        } else {
                            self.selection = vec![t];
                        }
                    }
                } else if !over && !press_mods.shift {
                    self.selection.clear();
                }
                // Otherwise it was selected on press.
            }
            Gesture::Drawing { start_world } => {
                self.finish_drawing(start_world, self.camera.to_world(at))
            }
            Gesture::Moving { .. }
            | Gesture::Boxing { .. }
            | Gesture::Panning { .. }
            | Gesture::None => {}
        }
    }

    /// The pointer left the canvas: finish whatever it was doing.
    /// The pointer left the canvas. Hover stops; a drag in progress does
    /// NOT — the page keeps sending moves from the whole window, and the
    /// drag carries on (or finishes) when the pointer comes back.
    pub fn pointer_leave(&mut self) {
        if matches!(self.gesture, Gesture::None) {
            self.pointer = None;
        }
    }

    /// Whether a drag is in progress, so the page knows to follow the
    /// pointer outside the canvas.
    pub fn dragging(&self) -> bool {
        !matches!(self.gesture, Gesture::None)
    }

    /// Scroll pans; ⌘-scroll or a pinch (Ctrl-scroll) zooms at the cursor.
    pub fn wheel(&mut self, at: Vec2, delta: Vec2, mods: Mods) {
        self.pointer = Some(at);
        self.mods = mods;
        if mods.command || mods.ctrl {
            self.camera.zoom_at(at, (-delta.y * 0.01).exp());
        } else {
            self.camera.pan(delta * -1.0);
        }
    }

    pub fn modifiers(&mut self, mods: Mods) {
        self.mods = mods;
    }

    /// Returns true when the key did something (the page then stops the
    /// browser's own handling of it).
    pub fn key_down(&mut self, key: Key, mods: Mods) -> bool {
        self.mods = mods;
        self.notice = None;
        match key {
            Key::Space => {
                self.space_held = true;
                true
            }
            Key::Escape => {
                if self.menu.take().is_none() {
                    self.escape();
                }
                true
            }
            Key::Enter => {
                self.enter();
                true
            }
            Key::Delete => {
                self.delete_selection();
                true
            }
            Key::Left | Key::Up => {
                self.step(-1);
                true
            }
            Key::Right | Key::Down => {
                self.step(1);
                true
            }
            Key::Char(c) => self.char_key(c.to_ascii_lowercase(), mods),
        }
    }

    pub fn key_up(&mut self, key: Key, mods: Mods) {
        self.mods = mods;
        if key == Key::Space {
            self.space_held = false;
        }
    }

    fn char_key(&mut self, c: char, mods: Mods) -> bool {
        match (c, mods.command, mods.shift) {
            ('z', true, false) => self.undo(),
            ('z', true, true) => self.redo(),
            ('a', true, _) => self.select_all(),
            ('v', false, _) => self.set_tool(Tool::Select),
            ('l', false, _) => self.set_tool(Tool::Line),
            ('o', false, _) => self.set_tool(Tool::Circle),
            ('-' | '_', false, _) => self.nudge(-1),
            ('=' | '+', false, _) => self.nudge(1),
            ('r', false, _) => self.reverse(),
            ('1' | '!', false, true) => self.fit(),
            ('2' | '@', false, true) => self.frame_selection(),
            _ => return false,
        }
        true
    }

    // ---- selection -----------------------------------------------------

    fn click_mode(&self, mods: Mods) -> PickMode {
        if mods.command {
            PickMode::Deep
        } else if mods.alt {
            PickMode::Next
        } else {
            PickMode::Click
        }
    }

    // ---- the right-click menu ---------------------------------------------

    pub fn open_menu(&mut self, at: Vec2) {
        let f = &self.project;
        let found = f.things_at(&self.camera, at);
        // Indent from the shallowest thing listed, so the list reads like a
        // little tree of its own.
        let top = found.iter().map(|t| f.chain(t).len()).min().unwrap_or(1);
        let items: Vec<MenuItem> = found
            .into_iter()
            .map(|t| MenuItem {
                depth: f.chain(&t).len() - top,
                label: f.label(&t),
                target: t,
            })
            .collect();
        self.gesture = Gesture::None;
        if items.is_empty() {
            self.menu = None;
            self.notice = Some(Notice::info("Nothing under the cursor here"));
            return;
        }
        self.menu = Some(ContextMenu {
            at,
            items,
            hover: None,
        });
    }

    pub fn menu_hover(&mut self, item: Option<usize>) {
        if let Some(m) = &mut self.menu {
            m.hover = item;
        }
    }

    /// Pick a menu item: select it, or with ⇧ add it.
    pub fn menu_choose(&mut self, item: usize, shift: bool) {
        let Some(m) = self.menu.take() else { return };
        let Some(it) = m.items.get(item) else { return };
        if shift {
            self.toggle(it.target.clone());
        } else {
            self.selection = vec![it.target.clone()];
        }
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    fn press_mode(&self, mods: Mods) -> PickMode {
        if mods.command {
            PickMode::Deep
        } else {
            PickMode::Press
        }
    }

    fn toggle(&mut self, t: Target) {
        if let Some(i) = self.selection.iter().position(|s| *s == t) {
            self.selection.remove(i);
        } else {
            self.selection.push(t);
        }
    }

    /// Select exactly these (the tree pane and the breadcrumb do this).
    pub fn select(&mut self, targets: Vec<Target>) {
        self.notice = None;
        self.selection = targets;
    }

    /// ⇧-click in the tree pane.
    pub fn toggle_select(&mut self, t: Target) {
        self.notice = None;
        self.toggle(t);
    }

    pub fn escape(&mut self) {
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::Moving { base, .. } => {
                self.project = *base;
                self.undo.pop();
                return;
            }
            Gesture::Boxing { base_selection, .. } => {
                self.selection = base_selection;
                return;
            }
            Gesture::Drawing { .. } => return,
            Gesture::Press { .. } | Gesture::Panning { .. } | Gesture::None => {}
        }
        if self.tool != Tool::Select {
            self.tool = Tool::Select;
            return;
        }
        if let Some(first) = self.selection.first().cloned() {
            self.selection = self.project.parent(&first).into_iter().collect();
        }
    }

    /// Enter: go into the selection. Opening a fixture also frames it with
    /// its outputs (D42), so the patch it is about to show is on screen.
    pub fn enter(&mut self) {
        let Some(first) = self.selection.first().cloned() else {
            return;
        };
        match self.project.children(Some(&first)).into_iter().next() {
            Some(child) => {
                if self.project.is_fixture(&first)
                    && let Some(r) = self.framing_rect(&first)
                {
                    self.camera = Camera::fit(r, self.viewport, 40.0);
                }
                self.selection = vec![child];
            }
            None => {
                self.notice = Some(Notice::info(format!(
                    "{} is as deep as it goes",
                    self.project.long_label(&first)
                )))
            }
        }
    }

    /// Arrow keys: the previous or next sibling at the selection's level.
    pub fn step(&mut self, dir: i64) {
        let Some(first) = self.selection.first().cloned() else {
            self.selection = self.project.children(None).into_iter().take(1).collect();
            return;
        };
        let sibs = self.project.siblings(&first);
        let Some(i) = sibs.iter().position(|s| *s == first) else {
            return;
        };
        let j = i as i64 + dir;
        if j < 0 || j >= sibs.len() as i64 {
            let end = if dir < 0 { "first" } else { "last" };
            self.notice = Some(Notice::info(format!(
                "{} is the {end} one here",
                self.project.long_label(&first)
            )));
            self.selection = vec![first];
            return;
        }
        self.selection = vec![sibs[j as usize].clone()];
    }

    /// ⌘A: everything at the selection's level. Pressed again once that
    /// level is all selected, it climbs one level, so ⌘A ⌘A … always ends
    /// at the whole project — and ⌘A ⌫ from nothing selected empties every
    /// fixture.
    pub fn select_all(&mut self) {
        // The levels ⌘A can select, from where you are outward: first the
        // levels inside a fixture, then the levels of nodes (inside modules,
        // an output's ports), then "everything you drew, in every fixture",
        // and last the project's own nodes.
        let p = &self.project;
        let mut levels: Vec<Vec<Target>> = Vec::new();
        // A selection spread over several levels (everything drawn) is
        // already outside them all.
        let mut context = self.context();
        if self.selection.iter().any(|t| p.parent(t) != context) {
            context = None;
        }
        while let Some(c) = context {
            levels.push(p.children(Some(&c)));
            context = p.parent(&c);
        }
        levels.push(
            p.pick_level(None)
                .into_iter()
                .filter(|t| matches!(t, Target::Component(_)))
                .collect(),
        );
        levels.push(p.children(None));
        let next = levels
            .into_iter()
            .find(|level| !level.is_empty() && !level.iter().all(|t| self.selection.contains(t)));
        if let Some(level) = next {
            self.selection = level;
        }
    }

    /// What ⌘A would do now, in words.
    pub fn select_all_hint(&self) -> String {
        let mut probe = self.clone();
        probe.select_all();
        let p = &probe.project;
        if probe.selection.is_empty() {
            return "nothing to select yet".into();
        }
        if probe.selection == self.selection {
            return "everything is selected".into();
        }
        if probe.selection.iter().all(|t| matches!(t, Target::Node(_))) {
            let parent = p.parent(&probe.selection[0]);
            let one_parent = probe.selection.iter().all(|t| p.parent(t) == parent);
            return match parent {
                Some(m) if one_parent => format!("select everything in {}", p.label(&m)),
                _ => "select the nodes themselves".into(),
            };
        }
        let mut parents: Vec<Option<Target>> = Vec::new();
        for t in &probe.selection {
            let parent = p.parent(t);
            if !parents.contains(&parent) {
                parents.push(parent);
            }
        }
        match parents.as_slice() {
            [Some(c)] => format!(
                "select every {} in {}",
                p.kind_word(&probe.selection[0]),
                p.label(c)
            ),
            _ => "select everything you drew, in every fixture".into(),
        }
    }

    fn update_box(&mut self) {
        let Some(rect) = self.selection_box() else {
            return;
        };
        let Gesture::Boxing { base_selection, .. } = &self.gesture else {
            return;
        };
        let mut selection = base_selection.clone();
        let enclosed = self.mods.alt;
        // The box works at the level you are in: lamps inside a ring,
        // what's drawn at the top (frames are see-through to it). Nodes
        // are picked by their title, never caught by a box.
        let context = base_selection.first().and_then(|s| self.project.parent(s));
        let level: Vec<Target> = self
            .project
            .pick_level(context.as_ref())
            .into_iter()
            .filter(|t| !matches!(t, Target::Node(_)))
            .collect();
        for t in level {
            let lamps = self.project.lamps_under(&t);
            let caught = if enclosed {
                !lamps.is_empty() && lamps.iter().all(|p| rect.contains(*p))
            } else {
                lamps.iter().any(|p| rect.contains(*p))
            };
            if caught && !selection.contains(&t) {
                selection.push(t);
            }
        }
        self.selection = selection;
    }

    /// After an edit or undo: a selected lamp past the end of a shortened
    /// line becomes its last lamp (the selection stays where you were
    /// working), and anything else that no longer exists is dropped.
    fn prune_selection(&mut self) {
        let f = &self.project;
        for t in &mut self.selection {
            if let Target::Lamp { object, index } = t {
                let n = f.object(object).map(|o| o.lamps.len() as u32).unwrap_or(0);
                if n > 0 && *index >= n {
                    *index = n - 1;
                }
            }
        }
        self.selection.retain(|t| f.exists(t));
        let mut seen = Vec::new();
        self.selection.retain(|t| {
            let new = !seen.contains(t);
            seen.push(t.clone());
            new
        });
    }

    // ---- moving --------------------------------------------------------

    fn begin_move(&mut self, start_world: Vec2) {
        let owners = self.move_owners();
        if owners.is_empty() {
            self.notice = Some(Notice::refused(
                "Nodes don't move by dragging",
                "the canvas lays nodes out by itself, in columns",
                "drag what's inside a fixture, or change its size in the inspector",
            ));
        } else {
            self.checkpoint();
        }
        self.gesture = Gesture::Moving {
            start_world,
            base: Box::new(self.project.clone()),
            owners,
        };
    }

    /// The authored components a move of the selection moves: each target's
    /// owner, minus any whose group is also moving.
    pub fn move_owners(&self) -> Vec<ComponentId> {
        let mut owners: Vec<ComponentId> = Vec::new();
        for t in &self.selection {
            if let Some(o) = t.owner()
                && !owners.contains(o)
            {
                owners.push(o.clone());
            }
        }
        let f = &self.project;
        owners
            .iter()
            .filter(|o| {
                let mut cur = f.parent_group(o).cloned();
                while let Some(g) = cur {
                    if owners.contains(&g) {
                        return false;
                    }
                    cur = f.parent_group(&g).cloned();
                }
                true
            })
            .cloned()
            .collect()
    }

    fn drag_to(&mut self, at: Vec2) {
        let Gesture::Moving {
            start_world,
            base,
            owners,
        } = &self.gesture
        else {
            return;
        };
        let delta = self.camera.to_world(at) - *start_world;
        let mut f = (**base).clone();
        for o in owners {
            if let Some(c) = f.component_mut(o) {
                c.translate(delta);
            }
        }
        self.project = f;
    }

    // ---- drawing -------------------------------------------------------

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.gesture = Gesture::None;
    }

    /// The lamp count a drawn line of this length gets: one every `spacing`.
    pub fn lamps_for_length(&self, length: f64) -> u32 {
        ((length / self.spacing).round() as u32 + 1).max(2)
    }

    fn drawn(&self, start: Vec2, now: Vec2) -> ComponentKind {
        match self.tool {
            Tool::Circle => {
                let r = start.distance(now).max(0.5);
                let lamps = ((std::f64::consts::TAU * r / self.spacing).round() as u32).max(3);
                ComponentKind::circle(start, &[(r, lamps)])
            }
            _ => ComponentKind::line(start, now, self.lamps_for_length(start.distance(now))),
        }
    }

    /// Where a drag from `start` to `end` (world units) puts its drawing:
    /// the fixture it starts in, and the component in that fixture's own
    /// coordinates. `None` when it starts outside every fixture. Finishing
    /// a drag and the simulated board's preview of it (`device_frame.rs`)
    /// both ask here, so the preview is exactly what the release makes.
    pub(crate) fn drawn_into(&self, start: Vec2, end: Vec2) -> Option<(NodeId, ComponentKind)> {
        let fixture = self.project.fixture_at(start)?;
        let origin = self.project.origin(&fixture);
        Some((fixture, self.drawn(start - origin, end - origin)))
    }

    fn finish_drawing(&mut self, start: Vec2, end: Vec2) {
        if start.distance(end) * self.camera.scale < 4.0 {
            self.notice = Some(Notice::refused(
                "Nothing drawn",
                "a click alone doesn't say how long it is",
                format!("press and drag to draw a {}", self.tool.word()),
            ));
            return;
        }
        let Some((fixture, kind)) = self.drawn_into(start, end) else {
            self.notice = Some(Notice::refused(
                "Nothing drawn",
                "a drawing starts inside a fixture's box",
                format!(
                    "press inside a fixture, then drag to draw a {}",
                    self.tool.word()
                ),
            ));
            return;
        };
        self.checkpoint();
        if let Some(id) = self.project.add(&fixture, kind) {
            self.selection = vec![Target::Component(id)];
        }
        self.tool = Tool::Select;
    }

    // ---- history -------------------------------------------------------

    /// Record the state before an edit.
    pub(crate) fn checkpoint(&mut self) {
        self.undo
            .push((self.project.clone(), self.selection.clone()));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        match self.undo.pop() {
            Some((f, s)) => {
                self.redo.push((
                    std::mem::replace(&mut self.project, f),
                    std::mem::replace(&mut self.selection, s),
                ));
                self.prune_selection();
            }
            None => self.notice = Some(Notice::info("Nothing to undo")),
        }
    }

    pub fn redo(&mut self) {
        match self.redo.pop() {
            Some((f, s)) => {
                self.undo.push((
                    std::mem::replace(&mut self.project, f),
                    std::mem::replace(&mut self.selection, s),
                ));
                self.prune_selection();
            }
            None => self.notice = Some(Notice::info("Nothing to redo")),
        }
    }

    pub(crate) fn after_edit(&mut self) {
        self.prune_selection();
    }
}
