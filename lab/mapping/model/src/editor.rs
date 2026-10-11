//! The editor: one state, driven by input events, with no IO.
//!
//! The web page forwards pointer, wheel and key events here and draws what
//! it finds. Everything that decides how the editor *feels* lives in this
//! crate, so it can be read, changed and tested without a browser.

use crate::camera::Camera;
use crate::component::{ComponentId, ComponentKind};
use crate::fixture::Fixture;
use crate::geom::{Rect, Vec2};
use crate::hit_test::hit;
use crate::notice::Notice;
use crate::pick::{PickMode, pick};
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
        hit: Option<Target>,
        on_selection: bool,
        mods: Mods,
    },
    /// Dragging the selection. `base` is the fixture before the drag, so the
    /// move is always "base + total delta" and never drifts.
    Moving {
        start_world: Vec2,
        base: Box<Fixture>,
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

/// How far, in screen pixels, a press moves before it is a drag.
pub const DRAG_THRESHOLD_PX: f64 = 3.0;
const UNDO_LIMIT: usize = 200;

#[derive(Debug, Clone)]
pub struct Editor {
    pub fixture: Fixture,
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
    undo: Vec<(Fixture, Vec<Target>)>,
    redo: Vec<(Fixture, Vec<Target>)>,
    fitted: bool,
}

impl Editor {
    pub fn new(fixture: Fixture) -> Self {
        let viewport = Vec2::new(900.0, 600.0);
        let camera = Camera::fit(fixture.bounds, viewport, 40.0);
        Self {
            fixture,
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
            undo: Vec::new(),
            redo: Vec::new(),
            fitted: false,
        }
    }

    // ---- what the page reads -------------------------------------------

    /// The deepest thing under the pointer.
    pub fn hover_hit(&self) -> Option<Target> {
        if !matches!(self.gesture, Gesture::None) || self.tool != Tool::Select || self.space_held {
            return None;
        }
        hit(&self.fixture, &self.camera, self.pointer?)
    }

    /// What a click right now would select — the hover outline (D17).
    pub fn hover_target(&self) -> Option<Target> {
        let h = self.hover_hit()?;
        Some(pick(
            &self.fixture,
            &self.selection,
            &h,
            self.click_mode(self.mods),
        ))
    }

    /// The level the selection lives in: its parent, or `None` for the
    /// fixture's top level.
    pub fn context(&self) -> Option<Target> {
        self.selection.first().and_then(|s| self.fixture.parent(s))
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

    pub fn fit(&mut self) {
        self.camera = Camera::fit(self.fixture.bounds, self.viewport, 40.0);
    }

    pub fn pointer_down(&mut self, at: Vec2, button: Button, mods: Mods) {
        self.mods = mods;
        self.pointer = Some(at);
        self.notice = None;
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
                let h = hit(&self.fixture, &self.camera, at);
                let on_selection = h.as_ref().is_some_and(|h| {
                    self.fixture
                        .chain(h)
                        .iter()
                        .any(|t| self.selection.contains(t))
                });
                // Pressing something new selects it straight away, so a
                // drag moves it. Pressing the selection waits: it may be a
                // drag (move it) or a click (go into it).
                if let Some(h) = &h
                    && !on_selection
                {
                    let t = pick(&self.fixture, &self.selection, h, self.press_mode(mods));
                    if mods.shift {
                        self.toggle(t);
                    } else {
                        self.selection = vec![t];
                    }
                }
                self.gesture = Gesture::Press {
                    start: at,
                    hit: h,
                    on_selection,
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
                hit,
                mods: press_mods,
                ..
            } => {
                if start.distance(at) < DRAG_THRESHOLD_PX {
                    return;
                }
                let start_world = self.camera.to_world(start);
                if hit.is_some() && !self.selection.is_empty() {
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
            Gesture::Press {
                hit,
                on_selection,
                mods: press_mods,
                ..
            } => match hit {
                // A click on the selection: go in, add/remove, or go deep.
                Some(h) if on_selection => {
                    let t = pick(
                        &self.fixture,
                        &self.selection,
                        &h,
                        self.click_mode(press_mods),
                    );
                    if press_mods.shift {
                        self.toggle(t);
                    } else {
                        self.selection = vec![t];
                    }
                }
                Some(_) => {} // selected on press
                None => {
                    if !press_mods.shift {
                        self.selection.clear();
                    }
                }
            },
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
    pub fn pointer_leave(&mut self) {
        if let Some(at) = self.pointer {
            if matches!(
                self.gesture,
                Gesture::Press { .. } | Gesture::Drawing { .. }
            ) {
                self.gesture = Gesture::None;
            } else {
                self.pointer_up(at, self.mods);
            }
        }
        self.pointer = None;
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
                self.escape();
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
            _ => return false,
        }
        true
    }

    // ---- selection -----------------------------------------------------

    fn click_mode(&self, mods: Mods) -> PickMode {
        if mods.command {
            PickMode::Deep
        } else {
            PickMode::Click
        }
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
                self.fixture = *base;
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
            self.selection = self.fixture.parent(&first).into_iter().collect();
        }
    }

    pub fn enter(&mut self) {
        let Some(first) = self.selection.first().cloned() else {
            return;
        };
        match self.fixture.children(Some(&first)).into_iter().next() {
            Some(child) => self.selection = vec![child],
            None => {
                self.notice = Some(Notice::info(format!(
                    "{} is as deep as it goes",
                    self.fixture.long_label(&first)
                )))
            }
        }
    }

    /// Arrow keys: the previous or next sibling at the selection's level.
    pub fn step(&mut self, dir: i64) {
        let Some(first) = self.selection.first().cloned() else {
            self.selection = self.fixture.children(None).into_iter().take(1).collect();
            return;
        };
        let sibs = self.fixture.siblings(&first);
        let Some(i) = sibs.iter().position(|s| *s == first) else {
            return;
        };
        let j = i as i64 + dir;
        if j < 0 || j >= sibs.len() as i64 {
            let end = if dir < 0 { "first" } else { "last" };
            self.notice = Some(Notice::info(format!(
                "{} is the {end} one here",
                self.fixture.long_label(&first)
            )));
            self.selection = vec![first];
            return;
        }
        self.selection = vec![sibs[j as usize].clone()];
    }

    /// ⌘A: everything at the selection's level. Pressed again once that
    /// level is all selected, it climbs one level, so ⌘A ⌘A … always ends
    /// at the whole fixture — and ⌘A ⌫ from nothing selected deletes
    /// everything.
    pub fn select_all(&mut self) {
        let mut context = self.context();
        loop {
            let level = self.fixture.children(context.as_ref());
            let all_there = !level.is_empty() && level.iter().all(|t| self.selection.contains(t));
            match (&context, all_there) {
                (Some(c), true) => context = self.fixture.parent(c),
                _ => {
                    self.selection = level;
                    return;
                }
            }
        }
    }

    /// What ⌘A would do now, in words.
    pub fn select_all_hint(&self) -> String {
        let mut probe = self.clone();
        probe.select_all();
        if probe.selection.is_empty() {
            return "nothing to select yet".into();
        }
        if probe.selection == self.selection {
            return "everything is selected".into();
        }
        match probe.context() {
            None => "select everything".into(),
            Some(c) => format!(
                "select every {} in {}",
                probe.fixture.kind_word(&probe.selection[0]),
                self.fixture.label(&c)
            ),
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
        // components at the top.
        let context = base_selection.first().and_then(|s| self.fixture.parent(s));
        for t in self.fixture.children(context.as_ref()) {
            let lamps = self.fixture.lamps_under(&t);
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
        let f = &self.fixture;
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
        self.checkpoint();
        let owners = self.move_owners();
        self.gesture = Gesture::Moving {
            start_world,
            base: Box::new(self.fixture.clone()),
            owners,
        };
    }

    /// The authored components a move of the selection moves: each target's
    /// owner, minus any whose group is also moving.
    pub fn move_owners(&self) -> Vec<ComponentId> {
        let mut owners: Vec<ComponentId> = Vec::new();
        for t in &self.selection {
            let o = t.owner().clone();
            if !owners.contains(&o) {
                owners.push(o);
            }
        }
        let f = &self.fixture;
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
        self.fixture = f;
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

    fn finish_drawing(&mut self, start: Vec2, end: Vec2) {
        if start.distance(end) * self.camera.scale < 4.0 {
            self.notice = Some(Notice::refused(
                format!("Nothing drawn"),
                "a click alone doesn't say how long it is",
                format!("press and drag to draw a {}", self.tool.word()),
            ));
            return;
        }
        let kind = self.drawn(start, end);
        self.checkpoint();
        let id = self.fixture.add(kind);
        self.selection = vec![Target::Component(id)];
        self.tool = Tool::Select;
    }

    // ---- history -------------------------------------------------------

    /// Record the state before an edit.
    pub(crate) fn checkpoint(&mut self) {
        self.undo
            .push((self.fixture.clone(), self.selection.clone()));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        match self.undo.pop() {
            Some((f, s)) => {
                self.redo.push((
                    std::mem::replace(&mut self.fixture, f),
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
                    std::mem::replace(&mut self.fixture, f),
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
