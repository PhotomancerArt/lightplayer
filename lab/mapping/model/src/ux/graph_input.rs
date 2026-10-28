//! Everything a page tells the editor, as one plain value.
//!
//! A page (the lab's, or Studio's Graph tab) never calls the editor's
//! methods itself: it turns each browser event, and each fact it worked
//! out (where the layout put the cards, the edge under the pointer, the
//! screen's size class), into a [`GraphInput`] and hands it to its host.
//! The lab applies it to its own editor at once; Studio hands it to core,
//! which owns the editor there. One function, [`Editor::input`], applies
//! any of them, so both hosts mean the same thing by the same input.

use crate::domain::geom::Vec2;
use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::ux::debug_flags::Presentation;
use crate::domain::target::Target;
use crate::ux::camera::Camera;
use crate::ux::editor::{Button, Editor, Key, Mods, Tool};
use crate::ux::graph_verb::GraphVerb;
use crate::ux::patch_section::PatchInput;
use crate::ux::port_grouping::PortGrouping;
use crate::ux::props::PropEdit;
use crate::ux::ui::{LayoutKind, SizeClass};

/// One thing the page says. Points are in screen pixels, relative to the
/// canvas.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphInput {
    PointerDown {
        at: Vec2,
        button: Button,
        mods: Mods,
    },
    PointerMove {
        at: Vec2,
        mods: Mods,
    },
    PointerUp {
        at: Vec2,
        mods: Mods,
    },
    /// The pointer left the canvas with no button held.
    PointerLeave,
    DoubleClick {
        at: Vec2,
        mods: Mods,
    },
    Wheel {
        at: Vec2,
        delta: Vec2,
        mods: Mods,
    },
    KeyDown {
        key: Key,
        mods: Mods,
    },
    KeyUp {
        key: Key,
        mods: Mods,
    },
    /// ⌘, ⌥ or ⇧ went down or up on its own.
    Modifiers(Mods),
    /// The canvas's size, in screen pixels.
    Resize(Vec2),
    /// Where the layout put every node: a fact the page worked out.
    Placement(lab_layout::Placement),
    /// The edge under the pointer, as the page hit-tests it.
    HoveredEdge(Option<(NodeId, NodeId)>),
    /// The screen's size class, from the app's width.
    SizeClass(SizeClass),
    /// The toolbar's Canvas | Stack; `None` gives the choice back to the
    /// screen size.
    SetLayout(Option<LayoutKind>),
    /// Frame everything (a switch back to the canvas does).
    Fit,
    /// The stack's camera, which follows its native scroll.
    Camera(Camera),
    /// A row in the tree, or a crumb, chose this.
    Select(Vec<Target>),
    /// ⇧ on a row: add or remove it.
    ToggleSelect(Target),
    /// The inspector edited one property of everything selected.
    EditProp {
        id: &'static str,
        edit: PropEdit,
    },
    /// The right-click list: the item under the pointer.
    MenuHover(Option<usize>),
    /// The right-click list: one of what it can do here.
    MenuAct(usize),
    /// The right-click list: one of what's under the cursor (⇧ adds it).
    MenuChoose {
        item: usize,
        shift: bool,
    },
    Tool(Tool),
    MoveDraw(bool),
    Undo,
    Redo,
    /// Debug's Pinning.
    Pinning(bool),
    /// Debug's Size.
    ForceSizeClass(Option<SizeClass>),
    /// Debug's Presentation: how the inspector shows a start against its
    /// flow (view state; the page remembers it per browser).
    Presentation(Presentation),
    /// Debug's Cursor ops: the patcher's steps under the inspector.
    CursorOps(bool),
    /// A problem's Remove: take away an override whose node is gone (one
    /// undo step). An offer from P06.
    RemoveOverride(TreeRef),
    /// A verb by name, as its key does it.
    Verb(GraphVerb),
    /// How output cards group a port's pixels.
    Grouping(PortGrouping),
    /// The inspector's Patch section edited the selection's placements.
    Patch(PatchInput),
}

impl Editor {
    /// Apply one input. Returns whether it was used, so the page knows to
    /// stop the browser doing its own thing with a key or a double-click.
    pub fn input(&mut self, input: GraphInput) -> bool {
        match input {
            GraphInput::PointerDown { at, button, mods } => self.pointer_down(at, button, mods),
            GraphInput::PointerMove { at, mods } => self.pointer_move(at, mods),
            GraphInput::PointerUp { at, mods } => self.pointer_up(at, mods),
            GraphInput::PointerLeave => self.pointer_leave(),
            GraphInput::DoubleClick { at, mods } => return self.double_click(at, mods),
            GraphInput::Wheel { at, delta, mods } => self.wheel(at, delta, mods),
            GraphInput::KeyDown { key, mods } => return self.key_down(key, mods),
            GraphInput::KeyUp { key, mods } => self.key_up(key, mods),
            GraphInput::Modifiers(mods) => self.modifiers(mods),
            GraphInput::Resize(size) => self.resize(size),
            GraphInput::Placement(placement) => self.set_placement(&placement),
            GraphInput::HoveredEdge(edge) => self.set_hovered_edge(edge),
            GraphInput::SizeClass(class) => self.report_size_class(class),
            GraphInput::SetLayout(layout) => self.set_layout(layout),
            GraphInput::Fit => self.fit(),
            GraphInput::Camera(camera) => self.camera = camera,
            GraphInput::Select(targets) => self.select(targets),
            GraphInput::ToggleSelect(target) => self.toggle_select(target),
            GraphInput::EditProp { id, edit } => self.edit_selection_prop(id, edit),
            GraphInput::MenuHover(item) => self.menu_hover(item),
            GraphInput::MenuAct(i) => self.menu_act(i),
            GraphInput::MenuChoose { item, shift } => self.menu_choose(item, shift),
            GraphInput::Tool(tool) => self.set_tool(tool),
            GraphInput::MoveDraw(on) => self.set_move_draw(on),
            GraphInput::Undo => self.undo(),
            GraphInput::Redo => self.redo(),
            GraphInput::Pinning(on) => self.set_pinning(on),
            GraphInput::ForceSizeClass(class) => self.force_size_class(class),
            GraphInput::Presentation(presentation) => self.debug.presentation = presentation,
            GraphInput::CursorOps(on) => self.debug.cursor_ops = on,
            GraphInput::RemoveOverride(node) => self.remove_gone_override(&node),
            GraphInput::Verb(verb) => return self.press(verb),
            GraphInput::Grouping(g) => self.grouping = g,
            GraphInput::Patch(input) => self.patch_input(input),
        }
        true
    }
}
