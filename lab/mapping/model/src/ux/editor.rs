//! The editor: one state, driven by input events, with no IO.
//!
//! The web page forwards pointer, wheel and key events here and draws what
//! it finds. Everything that decides how the editor *feels* lives in this
//! crate, so it can be read, changed and tested without a browser.

use crate::domain::component::{Component, ComponentId, ComponentKind};
use crate::domain::geom::{Rect, Vec2, polyline_length};
use crate::domain::node::NodeId;
use crate::domain::notice::Notice;
use crate::domain::object::{Object, objects_of};
use crate::domain::project::{Project, union};
use crate::domain::target::Target;
use crate::ux::camera::Camera;
use crate::ux::debug_flags::DebugFlags;
use crate::ux::handle::{Handle, HandleKind};
use crate::ux::pick::{PickMode, level_candidates, pick};
use crate::ux::ui::{LayoutKind, SizeClass};

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
    /// Click point after point; double-click or Enter finishes.
    Path,
    /// Drag from the centre.
    Circle,
}

impl Tool {
    pub fn word(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Path => "path",
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
        /// It landed on a handle: a drag drags the handle; a click is an
        /// ordinary click on what's beneath.
        handle: Option<HandleKind>,
    },
    /// Dragging the selection. `base` is the project before the drag, so
    /// the move is always "base + total delta" and never drifts. Components
    /// move; nodes are pinned where they are dropped (D48), each from where
    /// it was, in its container's coordinates.
    Moving {
        start_world: Vec2,
        base: Box<Project>,
        owners: Vec<ComponentId>,
        nodes: Vec<(NodeId, Vec2)>,
    },
    /// Dragging a selection box from empty space.
    Boxing {
        start_world: Vec2,
        base_selection: Vec<Target>,
    },
    Panning {
        last: Vec2,
    },
    /// Dragging out a circle from its centre.
    Drawing {
        start_world: Vec2,
    },
    /// Dragging a handle. Like a move, always "base + total drag".
    Handle {
        handle: HandleKind,
        start_world: Vec2,
        base: Box<Project>,
    },
}

/// The right-click menu: what you can do right here, then everything under
/// the cursor, every level, in tree order. Opened only when asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    /// Where it opened, in screen pixels.
    pub at: Vec2,
    /// What you can do here: unpin a node, add or remove a path's point.
    pub actions: Vec<MenuAction>,
    pub items: Vec<MenuItem>,
    /// The item under the pointer: outlined on the canvas.
    pub hover: Option<usize>,
}

/// One thing the menu can do, with the key or gesture that does the same.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuAction {
    pub label: String,
    /// The shortcut, as the hint bar writes it: `U`, `double-click`.
    pub keys: String,
    pub command: MenuCommand,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuCommand {
    /// Give a pinned node back to the layout.
    Unpin(NodeId),
    /// Add a point to a path on segment `segment`, at `at` (world units).
    AddPoint {
        path: ComponentId,
        segment: usize,
        at: Vec2,
    },
    /// Take a point away from a path.
    RemovePoint { path: ComponentId, index: usize },
    /// Make a path's segment a jumper (wire, no lamps), or lit again.
    ToggleJumper { path: ComponentId, segment: usize },
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
/// Path points closer than this on screen are one point: the two presses
/// of a double-click land on the same spot.
pub const SAME_POINT_PX: f64 = 3.0;
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
    /// The path being clicked out, in world units, before it exists.
    pub path_draft: Vec<Vec2>,
    /// "Move & draw" (D50): on, the drawing can be moved, drawn and
    /// reshaped; off, it holds still and only selection and patching work
    /// (the dome). M toggles it.
    pub move_draw: bool,
    pub debug: DebugFlags,
    /// The project runs for real (Studio's Graph: a board, or Studio's
    /// preview host): a shader's picture is its own, not the lab's
    /// stand-in, so nothing says it is one.
    pub runs_for_real: bool,
    /// The layout you chose; `None` until you choose one, and the screen's
    /// size class picks.
    layout_choice: Option<LayoutKind>,
    /// The screen's size class, as the page reports it.
    screen_class: SizeClass,
    /// The edge under the pointer, as the view reports it
    /// (`edges.rs`).
    pub(crate) edge_hover: Option<(NodeId, NodeId)>,
    /// The selection before each of the last two presses: a double-click
    /// adds a point to a path that was shaped BEFORE its two clicks went
    /// into it.
    press_log: Vec<Vec<Target>>,
    undo: Vec<(Project, Vec<Target>)>,
    redo: Vec<(Project, Vec<Target>)>,
    /// The camera still frames everything as the editor put it: until you
    /// move the view, a new canvas size frames everything again.
    framed: bool,
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
            path_draft: Vec::new(),
            move_draw: true,
            debug: DebugFlags::default(),
            runs_for_real: false,
            layout_choice: None,
            screen_class: SizeClass::default(),
            edge_hover: None,
            press_log: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            framed: true,
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

    /// What's being drawn, before it exists, as objects in world units: a
    /// circle being dragged, or a path's clicked points plus the segment
    /// to the cursor.
    pub fn draft(&self) -> Option<Vec<Object>> {
        let (fixture, kind) = self.draft_into()?;
        let frame = self.project.frame(&fixture);
        let c = Component {
            id: ComponentId::new("draft"),
            kind,
        };
        Some(
            objects_of(&c)
                .into_iter()
                .map(|o| crate::domain::project::placed(o, frame))
                .collect(),
        )
    }

    /// The fixture the draft goes into.
    pub fn draft_fixture(&self) -> Option<NodeId> {
        self.draft_into().map(|(f, _)| f)
    }

    /// The handles the selection shows (none while a tool is drawing, or
    /// with Move & draw off).
    pub fn handles(&self) -> Vec<Handle> {
        if self.tool != Tool::Select || !self.move_draw {
            return Vec::new();
        }
        self.project.handles(&self.selection)
    }

    /// The handle a press right now would grab.
    pub fn handle_at(&self, at: Vec2) -> Option<Handle> {
        let cam = &self.camera;
        self.handles()
            .into_iter()
            .filter(|h| h.hit(cam, at))
            .min_by(|a, b| {
                let d = |h: &Handle| cam.to_screen(h.at).distance(at);
                d(a).total_cmp(&d(b))
            })
    }

    /// The handle under the pointer, while hovering means anything — the
    /// page outlines it and the hint bar says what dragging it does.
    pub fn hover_handle(&self) -> Option<Handle> {
        self.handle_at(self.hover_point()?)
    }

    /// The handle being dragged right now.
    pub fn dragged_handle(&self) -> Option<&HandleKind> {
        match &self.gesture {
            Gesture::Handle { handle, .. } => Some(handle),
            _ => None,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---- input ---------------------------------------------------------

    /// The canvas's size on screen. Until you move the view, the canvas
    /// keeps everything framed (in the stack, the page moves the camera).
    pub fn resize(&mut self, size: Vec2) {
        self.viewport = size;
        if self.framed && self.layout() == LayoutKind::Canvas {
            self.fit();
        }
    }

    /// ⇧1: everything on the canvas.
    pub fn fit(&mut self) {
        self.camera = Camera::fit(self.project.extent(), self.viewport, 40.0);
        self.framed = true;
    }

    /// ⇧2: the selection. A fixture is framed with the outputs it feeds,
    /// or its rules name (an empty fixture still shows where its lamps
    /// will go), so you see both ends of its patch; anything else, on its
    /// own.
    pub fn frame_selection(&mut self) {
        let rects = self.selection.iter().filter_map(|t| self.framing_rect(t));
        match union(rects) {
            Some(r) => {
                self.camera = Camera::fit(r, self.viewport, 40.0);
                self.framed = false;
            }
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
            Target::Node(id) if let Some(f) = p.fixture(id) => union(
                std::iter::once(id.clone())
                    .chain(p.outputs_of(id))
                    .chain(f.rules.iter().map(|r| r.output().clone()))
                    .filter_map(|n| p.node_rect(&n)),
            ),
            Target::Node(id) => p.node_rect(id),
            Target::Port { output, key } => p.port_rect(output, key),
            Target::Pixel { output, key, index } => p.pixel_rect(output, key, *index),
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
            Tool::Circle => {
                self.gesture = Gesture::Drawing {
                    start_world: self.camera.to_world(at),
                };
            }
            Tool::Path => self.path_click(at),
            Tool::Select => {
                self.press_log.push(self.selection.clone());
                if self.press_log.len() > 2 {
                    self.press_log.remove(0);
                }
                let under = self.project.things_at(&self.camera, at);
                // Something is under the press only where the press would
                // grab it. A fixture's empty inside, or a drawn thing's span
                // out on an output, is empty space to a drag (it draws a
                // selection box, and never drags the drawing from the
                // output); a click there still selects (on release).
                let over = pick(
                    &self.project,
                    &self.camera,
                    &self.selection,
                    at,
                    self.press_mode(mods),
                )
                .is_some_and(|t| self.project.hit_directly(&self.camera, &t, at));
                let on_selection = under.iter().any(|t| self.selection.contains(t));
                let handle = self.handle_at(at).map(|h| h.kind);
                let mut decided = false;
                if let (true, Some(HandleKind::PathPoint { path, index })) = (mods.alt, &handle) {
                    // ⌥-click on a point's handle takes the point away.
                    let (path, index) = (path.clone(), *index);
                    self.remove_point(&path, index);
                    self.gesture = Gesture::Press {
                        start: at,
                        over,
                        on_selection: true,
                        decided: true,
                        mods,
                        handle: None,
                    };
                    return;
                }
                if handle.is_some() {
                    // A handle waits: a drag drags it, a click falls through
                    // to what's beneath on release.
                } else if over && mods.alt {
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
                    self.press_select(at, mods);
                }
                self.gesture = Gesture::Press {
                    start: at,
                    over,
                    on_selection: on_selection || decided,
                    decided,
                    mods,
                    handle,
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
                handle,
                ..
            } => {
                if start.distance(at) < DRAG_THRESHOLD_PX {
                    return;
                }
                let start_world = self.camera.to_world(start);
                if let Some(handle) = handle {
                    self.checkpoint();
                    self.gesture = Gesture::Handle {
                        handle,
                        start_world,
                        base: Box::new(self.project.clone()),
                    };
                    self.drag_handle_to(at);
                } else if over && !self.selection.is_empty() {
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
            Gesture::Handle { .. } => self.drag_handle_to(at),
            Gesture::Boxing { .. } => self.update_box(),
            Gesture::Panning { last } => {
                self.camera.pan(at - last);
                self.framed = false;
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
                handle,
                ..
            } => {
                if handle.is_some() && over && !on_selection {
                    // A click on a handle over something new: what a press
                    // there would have selected.
                    self.press_select(at, press_mods);
                } else if over && on_selection {
                    // A click on the selection: go in, add/remove, or go deep.
                    if let Some(t) = pick(
                        &self.project,
                        &self.camera,
                        &self.selection,
                        at,
                        self.click_mode(press_mods),
                    ) {
                        if press_mods.shift {
                            self.shift_add(t);
                        } else {
                            self.selection = vec![t];
                        }
                    }
                } else if !over {
                    // Empty space, or a fixture's empty inside.
                    let pick = pick(
                        &self.project,
                        &self.camera,
                        &self.selection,
                        at,
                        self.click_mode(press_mods),
                    );
                    match pick {
                        Some(f) if press_mods.shift => self.shift_add(f),
                        Some(f) => self.selection = vec![f],
                        None if !press_mods.shift => self.selection.clear(),
                        None => {}
                    }
                }
                // Otherwise it was selected on press.
            }
            Gesture::Drawing { start_world } => {
                self.finish_drawing(start_world, self.camera.to_world(at))
            }
            Gesture::Moving { .. }
            | Gesture::Handle { .. }
            | Gesture::Boxing { .. }
            | Gesture::Panning { .. }
            | Gesture::None => {}
        }
    }

    /// A double-click: with the path tool, finish the path (its two presses
    /// have already landed as one point). Selecting, on a segment of a path
    /// that was shaped before the double-click (selected, or a lamp of it),
    /// add a point there — the product's grammar. Returns whether it did
    /// anything.
    pub fn double_click(&mut self, at: Vec2, mods: Mods) -> bool {
        self.pointer = Some(at);
        self.mods = mods;
        if self.tool == Tool::Path {
            self.finish_path();
            return true;
        }
        let before = match self.press_log.as_slice() {
            [first, _] => first.clone(),
            _ => self.selection.clone(),
        };
        let shaped = self.shaped_paths(&before);
        let hit = shaped
            .iter()
            .filter_map(|p| {
                let (segment, q) = self.project.segment_at(&self.camera, p, at)?;
                Some((p.clone(), segment, q))
            })
            .min_by(|a, b| {
                let d = |q: Vec2| self.camera.to_screen(q).distance(at);
                d(a.2).total_cmp(&d(b.2))
            });
        match hit {
            Some((path, segment, q)) => {
                self.add_point(&path, segment, q);
                true
            }
            None => false,
        }
    }

    /// The paths whose points show handles for this selection: a selected
    /// path, or one a selected lamp is on.
    pub fn shaped_paths(&self, selection: &[Target]) -> Vec<ComponentId> {
        let mut out: Vec<ComponentId> = Vec::new();
        for h in self.project.handles(selection) {
            if let HandleKind::PathPoint { path, .. } = h.kind
                && !out.contains(&path)
            {
                out.push(path);
            }
        }
        out
    }

    /// Add a point to a path at `at` (world) on segment `segment`, and
    /// select the path so its new handle shows. Refused on top of a point
    /// it already has.
    pub fn add_point(&mut self, path: &ComponentId, segment: usize, at: Vec2) {
        if !self.move_draw {
            self.refuse_move_draw(&format!("add a point to {path}"));
            return;
        }
        let frame = self.project.component_frame(path);
        let near = self.project.path_points(path).is_some_and(|ps| {
            ps.iter()
                .any(|q| frame.to_world(*q).distance(at) * self.camera.scale < SAME_POINT_PX)
        });
        if near {
            self.notice = Some(Notice::refused(
                format!("No point added to {path}"),
                "it already has a point right there",
                "double-click further along a segment; drag the point to move it",
            ));
            return;
        }
        self.checkpoint();
        if self.project.insert_path_point(path, segment, at) {
            self.selection = vec![Target::Component(path.clone())];
            let n = self.project.path_points(path).map_or(0, |p| p.len());
            self.notice = Some(Notice::info(format!(
                "Added a point to {path} — {n} points, the same lamps"
            )));
        }
    }

    /// Make a path's segment a jumper (wire that carries no lamps: a corner,
    /// a run between windows) or lit again. The lamps keep their count and
    /// respace over the lit segments, as the product places them.
    pub fn toggle_jumper(&mut self, path: &ComponentId, segment: usize) {
        if !self.move_draw {
            self.refuse_move_draw(&format!("change {path}'s jumpers"));
            return;
        }
        self.checkpoint();
        match self.project.toggle_path_jumper(path, segment) {
            Some(true) => {
                self.selection = vec![Target::Component(path.clone())];
                self.notice = Some(Notice::info(format!(
                    "Segment {} of {path} is a jumper: no lamps on it, the same lamps along the rest",
                    segment + 1
                )));
            }
            Some(false) => {
                self.selection = vec![Target::Component(path.clone())];
                self.notice = Some(Notice::info(format!(
                    "Segment {} of {path} is lit again",
                    segment + 1
                )));
            }
            None => {
                self.notice = Some(Notice::refused(
                    format!("Segment {} of {path} can't be a jumper", segment + 1),
                    "a path keeps at least one lit segment",
                    "light another segment first",
                ));
            }
        }
    }

    /// `J`: the segment under the pointer, of a selected path (or the
    /// path under the pointer), becomes a jumper or lit again.
    fn jumper_under_pointer(&mut self) {
        let Some(at) = self.pointer else { return };
        let p = &self.project;
        let is_path = |id: &ComponentId| {
            p.component(id)
                .is_some_and(|c| c.kind.points().is_some() && c.kind.min_points() == 2)
        };
        let found = p.things_at(&self.camera, at).into_iter().find_map(|t| match t {
            Target::Component(id) if is_path(&id) => {
                p.segment_at(&self.camera, &id, at).map(|(s, _)| (id, s))
            }
            _ => None,
        });
        if let Some((path, segment)) = found {
            self.toggle_jumper(&path, segment);
        }
    }

    /// Take a point away from a path (or a polygon); refused below two
    /// points (three).
    pub fn remove_point(&mut self, path: &ComponentId, index: usize) {
        if !self.move_draw {
            self.refuse_move_draw(&format!("remove a point of {path}"));
            return;
        }
        let n = self.project.path_points(path).map_or(0, |p| p.len());
        let (min, word) = match self.project.component(path) {
            Some(c) => (c.kind.min_points(), c.kind_word()),
            None => (2, "path"),
        };
        if n <= min {
            let least = if min == 3 { "three" } else { "two" };
            self.notice = Some(Notice::refused(
                format!("Can't remove a point of {path}"),
                format!("a {word} needs at least {least} points"),
                format!("delete {path} itself (select it, then ⌫), or drag this point instead"),
            ));
            return;
        }
        self.checkpoint();
        if self.project.remove_path_point(path, index) {
            self.after_edit();
            self.notice = Some(Notice::info(format!(
                "Removed a point of {path} — {} points, the same lamps",
                n - 1
            )));
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
        self.framed = false;
    }

    pub fn modifiers(&mut self, mods: Mods) {
        self.mods = mods;
        // ⇧ pressed or let go mid-resize changes the shape at once.
        if let (Gesture::Handle { .. }, Some(p)) = (&self.gesture, self.pointer) {
            self.drag_handle_to(p);
        }
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
                if self.tool == Tool::Path {
                    self.finish_path();
                } else {
                    self.enter();
                }
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
            ('p' | 'l', false, _) => self.set_tool(Tool::Path),
            ('o', false, _) => self.set_tool(Tool::Circle),
            ('-' | '_', false, _) => self.nudge(-1),
            ('=' | '+', false, _) => self.nudge(1),
            ('r', false, _) => self.reverse(),
            ('u', false, _) => self.unpin_selection(),
            ('j', false, _) => self.jumper_under_pointer(),
            ('m', false, _) => self.set_move_draw(!self.move_draw),
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
        let actions = self.menu_actions(at);
        self.gesture = Gesture::None;
        if items.is_empty() && actions.is_empty() {
            self.menu = None;
            self.notice = Some(Notice::info("Nothing under the cursor here"));
            return;
        }
        self.menu = Some(ContextMenu {
            at,
            actions,
            items,
            hover: None,
        });
    }

    /// What the menu offers to do at `at` (screen): remove the point whose
    /// handle is there, add a point to a path's segment there, unpin each
    /// pinned node the cursor is on.
    fn menu_actions(&self, at: Vec2) -> Vec<MenuAction> {
        let p = &self.project;
        let world = self.camera.to_world(at);
        let mut out = Vec::new();
        // Every one of them moves or reshapes something.
        if !self.move_draw {
            return out;
        }
        if let Some(Handle {
            kind: HandleKind::PathPoint { path, index },
            ..
        }) = self.handle_at(at)
        {
            out.push(MenuAction {
                label: format!("Remove point {} of {path}", index + 1),
                keys: "⌥ click".into(),
                command: MenuCommand::RemovePoint { path, index },
            });
        }
        for t in p.things_at(&self.camera, at) {
            if let Target::Component(id) = &t
                && let Some((segment, q)) = p.segment_at(&self.camera, id, at)
            {
                let is_path = p.component(id).is_some_and(|c| c.kind.points().is_some());
                out.push(MenuAction {
                    label: format!("Add a point to {id} here"),
                    keys: "double-click".into(),
                    command: MenuCommand::AddPoint {
                        path: id.clone(),
                        segment,
                        at: q,
                    },
                });
                let jumper = p.path_jumpers(id).contains(&(segment as u32));
                if !is_path || p.component(id).is_some_and(|c| c.kind.min_points() > 2) {
                    continue;
                }
                out.push(MenuAction {
                    label: if jumper {
                        "Light this segment".to_string()
                    } else {
                        "Make this segment a jumper".to_string()
                    },
                    keys: "J".into(),
                    command: MenuCommand::ToggleJumper {
                        path: id.clone(),
                        segment,
                    },
                });
            }
        }
        for n in p.all_nodes().into_iter().filter(|_| self.debug.pinning) {
            if n.pinned.is_some() && p.node_rect(&n.id).is_some_and(|r| r.contains(world)) {
                out.push(MenuAction {
                    label: format!("Unpin {}", n.name),
                    keys: "U".into(),
                    command: MenuCommand::Unpin(n.id.clone()),
                });
            }
        }
        out
    }

    /// Do one of the menu's actions.
    pub fn menu_act(&mut self, i: usize) {
        let Some(m) = self.menu.take() else { return };
        let Some(a) = m.actions.get(i) else { return };
        match a.command.clone() {
            MenuCommand::Unpin(id) => self.unpin(&[id]),
            MenuCommand::AddPoint { path, segment, at } => self.add_point(&path, segment, at),
            MenuCommand::RemovePoint { path, index } => self.remove_point(&path, index),
            MenuCommand::ToggleJumper { path, segment } => self.toggle_jumper(&path, segment),
        }
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

    /// What pressing at `at` selects straight away: the thing there (⇧ adds
    /// or removes it).
    fn press_select(&mut self, at: Vec2, mods: Mods) {
        if let Some(t) = pick(
            &self.project,
            &self.camera,
            &self.selection,
            at,
            self.press_mode(mods),
        ) {
            if mods.shift {
                self.shift_add(t);
            } else {
                self.selection = vec![t];
            }
        }
    }

    /// ⇧-click: add or remove — except on a port's pixels, where ⇧-click
    /// selects the run from the first pixel you picked to this one (D47).
    fn shift_add(&mut self, t: Target) {
        if let Target::Pixel { output, key, index } = &t {
            let anchor = self.selection.iter().find_map(|s| match s {
                Target::Pixel {
                    output: o,
                    key: k,
                    index: i,
                } if o == output && k == key => Some(*i),
                _ => None,
            });
            if let Some(a) = anchor {
                let run: Vec<u32> = if a <= *index {
                    (a..=*index).collect()
                } else {
                    (*index..=a).rev().collect()
                };
                self.selection = run
                    .into_iter()
                    .map(|i| Target::Pixel {
                        output: output.clone(),
                        key: key.clone(),
                        index: i,
                    })
                    .collect();
                return;
            }
        }
        self.toggle(t);
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
            Gesture::Moving { base, .. } | Gesture::Handle { base, .. } => {
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
        // Drawing a path, Esc takes back one point; with none left, it
        // leaves the tool.
        if self.path_draft.pop().is_some() {
            return;
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
                    self.framed = false;
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
        if let Some(level) = self.select_all_next() {
            self.selection = level;
        }
    }

    /// What ⌘A would select now, without selecting it.
    fn select_all_next(&self) -> Option<Vec<Target>> {
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
        levels
            .into_iter()
            .find(|level| !level.is_empty() && !level.iter().all(|t| self.selection.contains(t)))
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
    /// path becomes its last lamp, and a pixel past the end of a shortened
    /// port its last pixel (the selection stays where you were working);
    /// anything else that no longer exists is dropped.
    fn prune_selection(&mut self) {
        let f = &self.project;
        for t in &mut self.selection {
            let (n, index) = match t {
                Target::Lamp { object, index } => (
                    f.object(object).map(|o| o.lamps.len() as u32).unwrap_or(0),
                    index,
                ),
                Target::Pixel { output, key, index } => (f.port_pixels(output, key), index),
                _ => continue,
            };
            if n > 0 && *index >= n {
                *index = n - 1;
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
        let mut owners = self.move_owners();
        let mut nodes: Vec<(NodeId, Vec2)> = self
            .move_nodes()
            .into_iter()
            .filter_map(|id| Some((id.clone(), self.project.node_position(&id)?)))
            .collect();
        if !self.move_draw {
            // Nothing moves: the drag runs, and does nothing.
            let names: Vec<String> = owners
                .iter()
                .map(|o| o.to_string())
                .chain(nodes.iter().map(|(id, _)| self.project.node_name(id)))
                .collect();
            self.refuse_move_draw(&format!("move {}", names.join(", ")));
            owners.clear();
            nodes.clear();
        }
        if !owners.is_empty() || !nodes.is_empty() {
            self.checkpoint();
        } else if self.move_draw && !self.dragged_nodes().is_empty() {
            self.notice = Some(Notice::info(
                "The layout places nodes — dragging one moves nothing · pinning is under Debug",
            ));
        }
        self.gesture = Gesture::Moving {
            start_world,
            base: Box::new(self.project.clone()),
            owners,
            nodes,
        };
    }

    /// The nodes a move of the selection pins (D48): each selected node,
    /// and the output of a selected port or pixel — minus any inside a
    /// module that is also moving. None while pinning is off (D64).
    pub fn move_nodes(&self) -> Vec<NodeId> {
        if !self.debug.pinning {
            return Vec::new();
        }
        self.dragged_nodes()
    }

    /// The nodes a drag of the selection takes hold of, whether or not
    /// pinning lets it move them.
    pub(crate) fn dragged_nodes(&self) -> Vec<NodeId> {
        let p = &self.project;
        let mut nodes: Vec<NodeId> = Vec::new();
        for t in &self.selection {
            let id = match t {
                Target::Node(id) => id,
                Target::Port { output, .. } | Target::Pixel { output, .. } => output,
                _ => continue,
            };
            if !nodes.contains(id) {
                nodes.push(id.clone());
            }
        }
        nodes
            .iter()
            .filter(|n| {
                let mut cur = p.parent_module(n).cloned();
                while let Some(m) = cur {
                    if nodes.contains(&m) {
                        return false;
                    }
                    cur = p.parent_module(&m).cloned();
                }
                true
            })
            .cloned()
            .collect()
    }

    /// U: give the selected nodes (or the outputs of selected ports and
    /// pixels) back to the automatic layout.
    pub fn unpin_selection(&mut self) {
        if !self.debug.pinning {
            self.notice = Some(Notice::refused(
                "Nothing to unpin",
                "pinning is off: the layout places every node",
                "turn on pinning under Debug to place nodes by hand",
            ));
            return;
        }
        if !self.move_draw {
            self.refuse_move_draw("unpin");
            return;
        }
        let ids = self.move_nodes();
        self.unpin(&ids);
    }

    fn unpin(&mut self, ids: &[NodeId]) {
        let pinned: Vec<NodeId> = ids
            .iter()
            .filter(|id| self.project.node(id).is_some_and(|n| n.pinned.is_some()))
            .cloned()
            .collect();
        if pinned.is_empty() {
            self.notice = Some(Notice::refused(
                "Nothing to unpin",
                if ids.is_empty() {
                    "no node is selected"
                } else {
                    "the layout already places it"
                },
                "drag a node by its title (or an output by its card) to pin it",
            ));
            return;
        }
        self.checkpoint();
        for id in &pinned {
            self.project.unpin(id);
        }
        let names: Vec<String> = pinned.iter().map(|id| self.project.node_name(id)).collect();
        self.notice = Some(Notice::info(format!(
            "Unpinned {} — the layout places it again",
            names.join(", ")
        )));
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
            nodes,
        } = &self.gesture
        else {
            return;
        };
        let delta = self.camera.to_world(at) - *start_world;
        let mut f = (**base).clone();
        for o in owners {
            // The same move in the component's fixture's own units.
            let d = base.component_frame(o).delta_local(delta);
            if let Some(c) = f.component_mut(o) {
                c.translate(d);
            }
        }
        for (id, from) in nodes {
            f.pin(id, *from + delta);
        }
        self.project = f;
    }

    // ---- drawing -------------------------------------------------------

    pub fn set_tool(&mut self, tool: Tool) {
        if tool != Tool::Select && !self.move_draw {
            self.refuse_move_draw(&format!("draw a {}", tool.word()));
            return;
        }
        self.tool = tool;
        self.gesture = Gesture::None;
        self.path_draft.clear();
    }

    /// M, or the toolbar's "Move & draw": turn moving, drawing and
    /// reshaping on or off (D50). Off leaves any drawing tool.
    pub fn set_move_draw(&mut self, on: bool) {
        self.move_draw = on;
        if !on {
            self.tool = Tool::Select;
            self.path_draft.clear();
            self.gesture = Gesture::None;
        }
        self.notice = Some(Notice::info(if on {
            "Move & draw on — drag to move, P and O draw"
        } else {
            "Move & draw off — select and patch only; nothing moves · M turns it back on"
        }));
    }

    /// The size class the canvas is drawn for: Debug's forced one, or the
    /// screen's.
    pub fn size_class(&self) -> SizeClass {
        self.debug.size_class.unwrap_or(self.screen_class)
    }

    /// The layout in use: the one you chose, or the size class's default
    /// (the stack on a phone).
    pub fn layout(&self) -> LayoutKind {
        self.layout_choice
            .unwrap_or_else(|| self.size_class().default_layout())
    }

    /// Whether the layout is one you chose.
    pub fn layout_chosen(&self) -> bool {
        self.layout_choice.is_some()
    }

    /// The node the selection is in: the selected node itself, or the
    /// fixture or output something selected sits in. The stack scrolls it
    /// into view.
    pub fn focus_node(&self) -> Option<NodeId> {
        let t = self.selection.first()?;
        match t {
            Target::Node(id) => Some(id.clone()),
            Target::Port { output, .. } | Target::Pixel { output, .. } => Some(output.clone()),
            other => self.project.fixture_of_target(other),
        }
    }

    /// Take the view's choices from the editor this one replaces (opening
    /// another project): the layout you chose, the screen, the viewport and
    /// the Debug switches. The camera is new.
    pub fn with_view_of(mut self, other: &Editor) -> Editor {
        self.layout_choice = other.layout_choice;
        self.screen_class = other.screen_class;
        self.viewport = other.viewport;
        self.debug = other.debug;
        self
    }

    /// The page says how big the screen is, as a size class.
    pub fn report_size_class(&mut self, class: SizeClass) {
        self.screen_class = class;
    }

    /// Choose a layout (the toolbar's Canvas | Stack); `None` gives the
    /// choice back to the screen size. The page remembers it per browser;
    /// it is never saved in the project.
    pub fn set_layout(&mut self, layout: Option<LayoutKind>) {
        self.layout_choice = layout;
    }

    /// Debug: draw as if the screen were this size, or as it is.
    pub fn force_size_class(&mut self, class: Option<SizeClass>) {
        self.debug.size_class = class;
    }

    /// Debug's "Pinning": on, dragging a node pins it and the layout
    /// honours pins; off, the layout places every node (D64). Pins stay in
    /// the project either way.
    pub fn set_pinning(&mut self, on: bool) {
        self.debug.pinning = on;
        self.notice = Some(Notice::info(if on {
            "Pinning on — drag a node to pin it, U unpins it"
        } else {
            "Pinning off — the layout places every node"
        }));
    }

    /// Say that Move & draw is off, naming the toggle.
    pub(crate) fn refuse_move_draw(&mut self, what: &str) {
        self.notice = Some(Notice::refused(
            format!("Can't {what}"),
            "Move & draw is off",
            "M turns it on (or Move & draw in the toolbar)",
        ));
    }

    /// The lamp count a drawn path of this length gets: one every `spacing`.
    pub fn lamps_for_length(&self, length: f64) -> u32 {
        ((length / self.spacing).round() as u32 + 1).max(2)
    }

    /// A circle dragged from `start` to `now`, in one fixture's coordinates.
    fn drawn_circle(&self, start: Vec2, now: Vec2) -> ComponentKind {
        let r = start.distance(now).max(0.5);
        let lamps = ((std::f64::consts::TAU * r / self.spacing).round() as u32).max(3);
        ComponentKind::circle(start, &[(r, lamps)])
    }

    /// A path through these points (one fixture's coordinates), its lamp
    /// count from its length.
    fn drawn_path(&self, points: Vec<Vec2>) -> ComponentKind {
        let lamps = self.lamps_for_length(polyline_length(&points));
        ComponentKind::path(points, lamps)
    }

    /// Where the draft goes, and what it is in that fixture's own
    /// coordinates: a circle being dragged, or the clicked-out path with the
    /// cursor as its next point. `None` when there's no draft or it starts
    /// outside every fixture. Finishing a drawing and the simulated board's
    /// preview of it (`board.rs`) both ask here, so the preview is
    /// exactly what finishing makes.
    pub(crate) fn draft_into(&self) -> Option<(NodeId, ComponentKind)> {
        if let Gesture::Drawing { start_world } = &self.gesture {
            let now = self.camera.to_world(self.pointer?);
            let fixture = self.project.fixture_at(*start_world)?;
            let frame = self.project.frame(&fixture);
            return Some((
                fixture,
                self.drawn_circle(frame.to_local(*start_world), frame.to_local(now)),
            ));
        }
        if self.tool != Tool::Path {
            return None;
        }
        let mut points = self.path_draft.clone();
        if let Some(p) = self.pointer {
            points.push(self.camera.to_world(p));
        }
        let points = self.distinct(points);
        if points.len() < 2 {
            return None;
        }
        let fixture = self.project.fixture_at(points[0])?;
        let frame = self.project.frame(&fixture);
        let local = points.into_iter().map(|p| frame.to_local(p)).collect();
        Some((fixture, self.drawn_path(local)))
    }

    /// Points with any that sit on the one before (on screen) dropped.
    fn distinct(&self, points: Vec<Vec2>) -> Vec<Vec2> {
        let mut out: Vec<Vec2> = Vec::new();
        for p in points {
            let same = out
                .last()
                .is_some_and(|q| q.distance(p) * self.camera.scale < SAME_POINT_PX);
            if !same {
                out.push(p);
            }
        }
        out
    }

    /// A click with the path tool: the first point must be in a fixture;
    /// every click after it adds a point.
    fn path_click(&mut self, at: Vec2) {
        let p = self.camera.to_world(at);
        if self.path_draft.is_empty() && self.project.fixture_at(p).is_none() {
            self.notice = Some(Notice::refused(
                "Nothing drawn",
                "a path starts inside a fixture's box",
                "click inside a fixture for the path's first point",
            ));
            return;
        }
        self.path_draft.push(p);
    }

    /// Double-click or Enter: make the clicked-out path. It needs two
    /// different points; with fewer it is refused and the draft stays.
    fn finish_path(&mut self) {
        let draft = std::mem::take(&mut self.path_draft);
        let points = self.distinct(draft);
        if points.len() < 2 {
            self.path_draft = points;
            self.notice = Some(Notice::refused(
                "Nothing drawn",
                "a path needs two different points",
                "click where it starts, click where it ends, then double-click or press Enter",
            ));
            return;
        }
        let Some(fixture) = self.project.fixture_at(points[0]) else {
            return;
        };
        let frame = self.project.frame(&fixture);
        let kind = self.drawn_path(points.into_iter().map(|p| frame.to_local(p)).collect());
        self.checkpoint();
        if let Some(id) = self.project.add(&fixture, kind) {
            self.selection = vec![Target::Component(id)];
        }
        self.tool = Tool::Select;
    }

    fn finish_drawing(&mut self, start: Vec2, end: Vec2) {
        if start.distance(end) * self.camera.scale < 4.0 {
            self.notice = Some(Notice::refused(
                "Nothing drawn",
                "a click alone doesn't say how big it is",
                "press at the centre and drag out to draw a circle",
            ));
            return;
        }
        let Some(fixture) = self.project.fixture_at(start) else {
            self.notice = Some(Notice::refused(
                "Nothing drawn",
                "a drawing starts inside a fixture's box",
                "press inside a fixture, then drag to draw a circle",
            ));
            return;
        };
        let frame = self.project.frame(&fixture);
        let kind = self.drawn_circle(frame.to_local(start), frame.to_local(end));
        self.checkpoint();
        if let Some(id) = self.project.add(&fixture, kind) {
            self.selection = vec![Target::Component(id)];
        }
        self.tool = Tool::Select;
    }

    // ---- handles -------------------------------------------------------

    fn drag_handle_to(&mut self, at: Vec2) {
        let Gesture::Handle {
            handle,
            start_world,
            base,
        } = &self.gesture
        else {
            return;
        };
        let mut p = (**base).clone();
        p.drag_handle(
            base,
            handle,
            *start_world,
            self.camera.to_world(at),
            self.mods.shift,
        );
        self.project = p;
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
