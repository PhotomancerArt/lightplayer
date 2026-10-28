//! The editor's verbs by name: everything a key does that a button, the
//! right-click list or an agent can do too, and the verbs that take values
//! — patching (`graph_edit.rs`) and layout (`graph_layout_edit.rs`), lean
//! Q77 — which a key, an inspector row and an offer all press through one
//! function. Delete is both: ⌫ deletes the selection, and its offer takes
//! what to delete as a value.
//!
//! The hint bar names each verb it shows ([`Hint::verbs`]), and
//! [`Editor::verbs`] lists the ones on offer right now from the same
//! hints, so the bar and whoever presses verbs by name (Studio's offers)
//! see one set. [`Editor::press`] does what the verb's key does, through
//! the very same key handler. Moving the view (pan, zoom, fit) is not a
//! verb: it changes how things look, never what they are.
//!
//! [`Hint::verbs`]: crate::ux::hints::Hint::verbs

use crate::ux::editor::{Editor, Key, Mods};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GraphVerb {
    /// Esc: up a level, or deselect at the top.
    Up,
    /// Enter: into the selection.
    GoIn,
    /// ←: the previous sibling.
    Previous,
    /// →: the next sibling.
    Next,
    /// ⌫
    Delete,
    /// −: one fewer (lamps, pixels, copies).
    Fewer,
    /// =: one more.
    More,
    /// r: which way the data runs.
    Reverse,
    /// ⌥U: give a pinned node back to the layout.
    Unpin,
    /// ⌘A: climb a level of everything.
    SelectAll,
    /// V
    SelectTool,
    /// P
    PathTool,
    /// O
    CircleTool,
    /// D: Move & draw on or off.
    MoveDraw,
    /// A: arm a patch from the selection (`patching.rs`).
    Patch,
    /// M: patch, then on to the next free segment, still armed.
    PatchNext,
    /// U: Back to the flow: the selected segments' own starts go, and each
    /// takes what its flow gives it again.
    Unpatch,
    /// S: arm a swap.
    Swap,
    /// K: turn one lamp.
    Turn,
    /// L: turn one lamp back.
    TurnBack,
    /// ;: turn a third.
    TurnThird,
    /// ': turn a third back.
    TurnThirdBack,
    /// ]: shift a pixel up the port.
    Shift,
    /// [: shift a pixel down the port.
    ShiftBack,
    /// ⇧→: the next place on the wire.
    WireNext,
    /// ⇧←: the previous place on the wire.
    WirePrevious,
    /// .: the next port's first free segment.
    NextPort,
    /// ⇧U: everything off: every segment of the fixture off the line, to
    /// patch by hand from nothing.
    UnpatchAll,
    /// ⌘Z
    Undo,
    /// ⇧⌘Z
    Redo,
    // ---- the patching verbs that take values (`graph_edit.rs`) ----------
    /// A segment's output.
    SetOutput,
    /// A segment's port: at a port, the next port, or stays here.
    SetPort,
    /// Where on its port a segment starts, in lamps.
    SetStartsAt,
    /// Which way a node's lamps run on the line.
    SetDirection,
    /// A segment's lamps shifted round, in lamps.
    SetOffset,
    /// Off the line.
    TurnOff,
    /// On the line again.
    TurnOn,
    /// Drop what is set here, one field or all.
    BackToTheFlow,
    /// A fixture's or a container's flow.
    SetFlow,
    /// Where a fixture starts.
    SetFixtureStart,
    /// Where a part starts, in every copy.
    SetPartStart,
    /// Take away an override whose node is gone.
    RemoveOverride,
    // ---- the layout verbs that take values (`graph_layout_edit.rs`) -----
    /// Give a part another id; every override on it follows.
    Rename,
    /// Put a part in a new repeat of some copies.
    Repeat,
    /// Keep only the part a repeat repeats (copy 0).
    Unrepeat,
    /// Make every copy of a repeat a real part (`x@k` → `x-k`).
    Expand,
    /// How many: a repeat's copies, a path's, a polygon's or a ring's
    /// lamps.
    SetCount,
    /// A ring added to a circle.
    AddRing,
    /// A one-lamp ring at a circle's centre.
    AddCentreLamp,
    /// A ring taken from its circle.
    RemoveRing,
    /// One segment, or one per subpath or ring.
    SetSplit,
    /// Where a shape's own lamp order starts: a circle's outside or
    /// inside, a path's first or last point.
    SetDrawnFrom,
    /// How far a ring is from its circle's centre.
    SetRadius,
    /// Which way round a circle's rings run.
    SetWayRound,
    /// A grid's columns, rows and spacing, a filled polygon's spacing and
    /// angle.
    SetLattice,
    /// Whether a grid's or a filled polygon's rows run back and forth.
    SetRouting,
    /// A fixture's box: its width and height.
    SetFixtureSize,
    /// Which byte of each lamp is red, green and blue.
    SetColourOrder,
    /// How many lamps a port's strip has.
    SetPortLamps,
}

impl GraphVerb {
    pub const ALL: [GraphVerb; 59] = [
        GraphVerb::Up,
        GraphVerb::GoIn,
        GraphVerb::Previous,
        GraphVerb::Next,
        GraphVerb::Delete,
        GraphVerb::Fewer,
        GraphVerb::More,
        GraphVerb::Reverse,
        GraphVerb::Unpin,
        GraphVerb::SelectAll,
        GraphVerb::SelectTool,
        GraphVerb::PathTool,
        GraphVerb::CircleTool,
        GraphVerb::MoveDraw,
        GraphVerb::Patch,
        GraphVerb::PatchNext,
        GraphVerb::Unpatch,
        GraphVerb::Swap,
        GraphVerb::Turn,
        GraphVerb::TurnBack,
        GraphVerb::TurnThird,
        GraphVerb::TurnThirdBack,
        GraphVerb::Shift,
        GraphVerb::ShiftBack,
        GraphVerb::WireNext,
        GraphVerb::WirePrevious,
        GraphVerb::NextPort,
        GraphVerb::UnpatchAll,
        GraphVerb::Undo,
        GraphVerb::Redo,
        GraphVerb::SetOutput,
        GraphVerb::SetPort,
        GraphVerb::SetStartsAt,
        GraphVerb::SetDirection,
        GraphVerb::SetOffset,
        GraphVerb::TurnOff,
        GraphVerb::TurnOn,
        GraphVerb::BackToTheFlow,
        GraphVerb::SetFlow,
        GraphVerb::SetFixtureStart,
        GraphVerb::SetPartStart,
        GraphVerb::RemoveOverride,
        GraphVerb::Rename,
        GraphVerb::Repeat,
        GraphVerb::Unrepeat,
        GraphVerb::Expand,
        GraphVerb::SetCount,
        GraphVerb::AddRing,
        GraphVerb::AddCentreLamp,
        GraphVerb::RemoveRing,
        GraphVerb::SetSplit,
        GraphVerb::SetDrawnFrom,
        GraphVerb::SetRadius,
        GraphVerb::SetWayRound,
        GraphVerb::SetLattice,
        GraphVerb::SetRouting,
        GraphVerb::SetFixtureSize,
        GraphVerb::SetColourOrder,
        GraphVerb::SetPortLamps,
    ];

    /// Every verb that takes values, offered with its parameters: the
    /// patching verbs, then the layout verbs.
    pub const WITH_VALUES: [GraphVerb; 30] = [
        GraphVerb::SetOutput,
        GraphVerb::SetPort,
        GraphVerb::SetStartsAt,
        GraphVerb::SetDirection,
        GraphVerb::SetOffset,
        GraphVerb::TurnOff,
        GraphVerb::TurnOn,
        GraphVerb::BackToTheFlow,
        GraphVerb::SetFlow,
        GraphVerb::SetFixtureStart,
        GraphVerb::SetPartStart,
        GraphVerb::RemoveOverride,
        GraphVerb::Rename,
        GraphVerb::Repeat,
        GraphVerb::Unrepeat,
        GraphVerb::Expand,
        GraphVerb::SetCount,
        GraphVerb::AddRing,
        GraphVerb::AddCentreLamp,
        GraphVerb::RemoveRing,
        GraphVerb::SetSplit,
        GraphVerb::SetDrawnFrom,
        GraphVerb::SetRadius,
        GraphVerb::SetWayRound,
        GraphVerb::SetLattice,
        GraphVerb::SetRouting,
        GraphVerb::SetFixtureSize,
        GraphVerb::SetColourOrder,
        GraphVerb::SetPortLamps,
        GraphVerb::Delete,
    ];

    /// The layout verbs that take values (Delete among them; its key
    /// deletes the selection).
    pub const LAYOUT: [GraphVerb; 18] = [
        GraphVerb::Rename,
        GraphVerb::Repeat,
        GraphVerb::Unrepeat,
        GraphVerb::Expand,
        GraphVerb::SetCount,
        GraphVerb::AddRing,
        GraphVerb::AddCentreLamp,
        GraphVerb::RemoveRing,
        GraphVerb::SetSplit,
        GraphVerb::SetDrawnFrom,
        GraphVerb::SetRadius,
        GraphVerb::SetWayRound,
        GraphVerb::SetLattice,
        GraphVerb::SetRouting,
        GraphVerb::SetFixtureSize,
        GraphVerb::SetColourOrder,
        GraphVerb::SetPortLamps,
        GraphVerb::Delete,
    ];

    /// The patching verbs that take values: never a key of their own.
    pub const PATCHING: [GraphVerb; 12] = [
        GraphVerb::SetOutput,
        GraphVerb::SetPort,
        GraphVerb::SetStartsAt,
        GraphVerb::SetDirection,
        GraphVerb::SetOffset,
        GraphVerb::TurnOff,
        GraphVerb::TurnOn,
        GraphVerb::BackToTheFlow,
        GraphVerb::SetFlow,
        GraphVerb::SetFixtureStart,
        GraphVerb::SetPartStart,
        GraphVerb::RemoveOverride,
    ];

    /// Whether it takes values (and so is offered with parameters; only
    /// Delete also has a key, which acts on the selection).
    pub fn takes_values(self) -> bool {
        GraphVerb::WITH_VALUES.contains(&self)
    }

    /// The verb's name, as a path segment: lowercase words joined by `-`,
    /// never a dot.
    pub fn name(self) -> &'static str {
        match self {
            GraphVerb::Up => "up",
            GraphVerb::GoIn => "go-in",
            GraphVerb::Previous => "previous",
            GraphVerb::Next => "next",
            GraphVerb::Delete => "delete",
            GraphVerb::Fewer => "fewer",
            GraphVerb::More => "more",
            GraphVerb::Reverse => "reverse",
            GraphVerb::Unpin => "unpin",
            GraphVerb::SelectAll => "select-all",
            GraphVerb::SelectTool => "select-tool",
            GraphVerb::PathTool => "path-tool",
            GraphVerb::CircleTool => "circle-tool",
            GraphVerb::MoveDraw => "move-and-draw",
            GraphVerb::Patch => "patch",
            GraphVerb::PatchNext => "patch-next",
            GraphVerb::Unpatch => "unpatch",
            GraphVerb::Swap => "swap",
            GraphVerb::Turn => "turn",
            GraphVerb::TurnBack => "turn-back",
            GraphVerb::TurnThird => "turn-third",
            GraphVerb::TurnThirdBack => "turn-third-back",
            GraphVerb::Shift => "shift",
            GraphVerb::ShiftBack => "shift-back",
            GraphVerb::WireNext => "wire-next",
            GraphVerb::WirePrevious => "wire-previous",
            GraphVerb::NextPort => "next-port",
            GraphVerb::UnpatchAll => "unpatch-all",
            GraphVerb::Undo => "undo",
            GraphVerb::Redo => "redo",
            GraphVerb::SetOutput => "set-output",
            GraphVerb::SetPort => "set-port",
            GraphVerb::SetStartsAt => "set-starts-at",
            GraphVerb::SetDirection => "set-direction",
            GraphVerb::SetOffset => "set-offset",
            GraphVerb::TurnOff => "turn-off",
            GraphVerb::TurnOn => "turn-on",
            GraphVerb::BackToTheFlow => "back-to-the-flow",
            GraphVerb::SetFlow => "set-flow",
            GraphVerb::SetFixtureStart => "set-fixture-start",
            GraphVerb::SetPartStart => "set-part-start",
            GraphVerb::RemoveOverride => "remove-override",
            GraphVerb::Rename => "rename",
            GraphVerb::Repeat => "repeat",
            GraphVerb::Unrepeat => "unrepeat",
            GraphVerb::Expand => "expand",
            GraphVerb::SetCount => "set-count",
            GraphVerb::AddRing => "add-ring",
            GraphVerb::AddCentreLamp => "add-centre-lamp",
            GraphVerb::RemoveRing => "remove-ring",
            GraphVerb::SetSplit => "set-split",
            GraphVerb::SetDrawnFrom => "set-drawn-from",
            GraphVerb::SetRadius => "set-radius",
            GraphVerb::SetWayRound => "set-way-round",
            GraphVerb::SetLattice => "set-lattice",
            GraphVerb::SetRouting => "set-routing",
            GraphVerb::SetFixtureSize => "set-fixture-size",
            GraphVerb::SetColourOrder => "set-colour-order",
            GraphVerb::SetPortLamps => "set-port-lamps",
        }
    }

    /// The verb named `name`.
    pub fn from_name(name: &str) -> Option<GraphVerb> {
        GraphVerb::ALL.into_iter().find(|v| v.name() == name)
    }

    /// The verb on a button: a word or two, never naming what it acts on.
    pub fn label(self) -> &'static str {
        match self {
            GraphVerb::Up => "Up",
            GraphVerb::GoIn => "Go in",
            GraphVerb::Previous => "Previous",
            GraphVerb::Next => "Next",
            GraphVerb::Delete => "Delete",
            GraphVerb::Fewer => "Fewer",
            GraphVerb::More => "More",
            GraphVerb::Reverse => "Reverse",
            GraphVerb::Unpin => "Unpin",
            GraphVerb::SelectAll => "Select all",
            GraphVerb::SelectTool => "Select",
            GraphVerb::PathTool => "Path",
            GraphVerb::CircleTool => "Circle",
            GraphVerb::MoveDraw => "Move & draw",
            GraphVerb::Patch => "Patch",
            GraphVerb::PatchNext => "Patch & next",
            // U puts the selection's starts back (output, port and lamp);
            // `back-to-the-flow` takes a node and which of what is set
            // there goes.
            GraphVerb::Unpatch => "Start back to the flow",
            GraphVerb::Swap => "Swap",
            GraphVerb::Turn => "Turn",
            GraphVerb::TurnBack => "Turn back",
            GraphVerb::TurnThird => "Turn ⅓",
            GraphVerb::TurnThirdBack => "Turn ⅓ back",
            GraphVerb::Shift => "Shift",
            GraphVerb::ShiftBack => "Shift back",
            GraphVerb::WireNext => "Next on wire",
            GraphVerb::WirePrevious => "Previous on wire",
            GraphVerb::NextPort => "Next port",
            GraphVerb::UnpatchAll => "Everything off",
            GraphVerb::Undo => "Undo",
            GraphVerb::Redo => "Redo",
            GraphVerb::SetOutput => "Output",
            GraphVerb::SetPort => "Port",
            GraphVerb::SetStartsAt => "Starts at",
            GraphVerb::SetDirection => "Direction",
            GraphVerb::SetOffset => "Offset",
            GraphVerb::TurnOff => "Turn off",
            GraphVerb::TurnOn => "Turn on",
            GraphVerb::BackToTheFlow => "Back to the flow",
            GraphVerb::SetFlow => "Flow",
            GraphVerb::SetFixtureStart => "Fixture start",
            GraphVerb::SetPartStart => "Part start",
            GraphVerb::RemoveOverride => "Remove override",
            GraphVerb::Rename => "Rename",
            GraphVerb::Repeat => "Repeat",
            GraphVerb::Unrepeat => "Unrepeat",
            GraphVerb::Expand => "Expand",
            GraphVerb::SetCount => "Count",
            GraphVerb::AddRing => "Add ring",
            GraphVerb::AddCentreLamp => "Add centre lamp",
            GraphVerb::RemoveRing => "Remove ring",
            GraphVerb::SetSplit => "Split",
            GraphVerb::SetDrawnFrom => "Drawn from",
            GraphVerb::SetRadius => "Radius",
            GraphVerb::SetWayRound => "Way round",
            GraphVerb::SetLattice => "Lattice",
            GraphVerb::SetRouting => "Rows run",
            GraphVerb::SetFixtureSize => "Size",
            GraphVerb::SetColourOrder => "Colour order",
            GraphVerb::SetPortLamps => "Port lamps",
        }
    }

    /// The key it is, as the hint bar writes it.
    pub fn keys(self) -> &'static str {
        match self {
            GraphVerb::Up => "Esc",
            GraphVerb::GoIn => "Enter",
            GraphVerb::Previous => "←",
            GraphVerb::Next => "→",
            GraphVerb::Delete => "⌫",
            GraphVerb::Fewer => "−",
            GraphVerb::More => "=",
            GraphVerb::Reverse => "R",
            GraphVerb::Unpin => "⌥U",
            GraphVerb::SelectAll => "⌘A",
            GraphVerb::SelectTool => "V",
            GraphVerb::PathTool => "P",
            GraphVerb::CircleTool => "O",
            GraphVerb::MoveDraw => "D",
            GraphVerb::Patch => "A",
            GraphVerb::PatchNext => "M",
            GraphVerb::Unpatch => "U",
            GraphVerb::Swap => "S",
            GraphVerb::Turn => "K",
            GraphVerb::TurnBack => "L",
            GraphVerb::TurnThird => ";",
            GraphVerb::TurnThirdBack => "'",
            GraphVerb::Shift => "]",
            GraphVerb::ShiftBack => "[",
            GraphVerb::WireNext => "⇧→",
            GraphVerb::WirePrevious => "⇧←",
            GraphVerb::NextPort => ".",
            GraphVerb::UnpatchAll => "⇧U",
            GraphVerb::Undo => "⌘Z",
            GraphVerb::Redo => "⇧⌘Z",
            // No key of their own: a row or an offer gives them values.
            GraphVerb::SetOutput
            | GraphVerb::SetPort
            | GraphVerb::SetStartsAt
            | GraphVerb::SetDirection
            | GraphVerb::SetOffset
            | GraphVerb::TurnOff
            | GraphVerb::TurnOn
            | GraphVerb::BackToTheFlow
            | GraphVerb::SetFlow
            | GraphVerb::SetFixtureStart
            | GraphVerb::SetPartStart
            | GraphVerb::RemoveOverride
            | GraphVerb::Rename
            | GraphVerb::Repeat
            | GraphVerb::Unrepeat
            | GraphVerb::Expand
            | GraphVerb::SetCount
            | GraphVerb::AddRing
            | GraphVerb::AddCentreLamp
            | GraphVerb::RemoveRing
            | GraphVerb::SetSplit
            | GraphVerb::SetDrawnFrom
            | GraphVerb::SetRadius
            | GraphVerb::SetWayRound
            | GraphVerb::SetLattice
            | GraphVerb::SetRouting
            | GraphVerb::SetFixtureSize
            | GraphVerb::SetColourOrder
            | GraphVerb::SetPortLamps => "",
        }
    }

    /// The key press that does it; `None` for a verb that takes values
    /// (Delete excepted: ⌫ deletes the selection).
    fn key(self) -> Option<(Key, Mods)> {
        let plain = Mods::default();
        let command = Mods {
            command: true,
            ..Mods::default()
        };
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        Some(match self {
            GraphVerb::Up => (Key::Escape, plain),
            GraphVerb::GoIn => (Key::Enter, plain),
            GraphVerb::Previous => (Key::Left, plain),
            GraphVerb::Next => (Key::Right, plain),
            GraphVerb::Delete => (Key::Delete, plain),
            GraphVerb::Fewer => (Key::Char('-'), plain),
            GraphVerb::More => (Key::Char('='), plain),
            GraphVerb::Reverse => (Key::Char('r'), plain),
            GraphVerb::Unpin => (Key::Char('u'), alt),
            GraphVerb::SelectAll => (Key::Char('a'), command),
            GraphVerb::SelectTool => (Key::Char('v'), plain),
            GraphVerb::PathTool => (Key::Char('p'), plain),
            GraphVerb::CircleTool => (Key::Char('o'), plain),
            GraphVerb::MoveDraw => (Key::Char('d'), plain),
            GraphVerb::Patch => (Key::Char('a'), plain),
            GraphVerb::PatchNext => (Key::Char('m'), plain),
            GraphVerb::Unpatch => (Key::Char('u'), plain),
            GraphVerb::Swap => (Key::Char('s'), plain),
            GraphVerb::Turn => (Key::Char('k'), plain),
            GraphVerb::TurnBack => (Key::Char('l'), plain),
            GraphVerb::TurnThird => (Key::Char(';'), plain),
            GraphVerb::TurnThirdBack => (Key::Char('\''), plain),
            GraphVerb::Shift => (Key::Char(']'), plain),
            GraphVerb::ShiftBack => (Key::Char('['), plain),
            GraphVerb::WireNext => (Key::Right, shift),
            GraphVerb::WirePrevious => (Key::Left, shift),
            GraphVerb::NextPort => (Key::Char('.'), plain),
            GraphVerb::UnpatchAll => (Key::Char('u'), shift),
            GraphVerb::Undo => (Key::Char('z'), command),
            GraphVerb::Redo => (
                Key::Char('z'),
                Mods {
                    shift: true,
                    ..command
                },
            ),
            _ => return None,
        })
    }
}

impl Editor {
    /// The verbs on offer right now: every one the hint bar names, and
    /// undo and redo while there is something to undo or redo.
    pub fn verbs(&self) -> Vec<GraphVerb> {
        let bar = self.hint_bar();
        let mut out: Vec<GraphVerb> = bar
            .hints
            .iter()
            .flat_map(|h| h.verbs.iter().copied())
            .filter(|v| !matches!(v, GraphVerb::Undo | GraphVerb::Redo))
            .collect();
        if self.can_undo() {
            out.push(GraphVerb::Undo);
        }
        if self.can_redo() {
            out.push(GraphVerb::Redo);
        }
        out.sort();
        out.dedup();
        out
    }

    /// Do what the verb's key does. Returns whether anything listened: a
    /// verb that takes values has no key, and is made with
    /// [`Editor::edit`].
    pub fn press(&mut self, verb: GraphVerb) -> bool {
        let Some((key, mods)) = verb.key() else {
            return false;
        };
        let used = self.key_down(key.clone(), mods);
        self.key_up(key, Mods::default());
        used
    }
}
