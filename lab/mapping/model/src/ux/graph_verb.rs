//! The editor's verbs by name: everything a key does that a button, the
//! right-click list or an agent can do too.
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
    /// U: take the selected objects off the wire.
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
    /// ⇧U: every object of the fixture off the wire.
    UnpatchAll,
    /// ⌘Z
    Undo,
    /// ⇧⌘Z
    Redo,
}

impl GraphVerb {
    pub const ALL: [GraphVerb; 30] = [
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
    ];

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
            GraphVerb::Unpatch => "Unpatch",
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
            GraphVerb::UnpatchAll => "Unpatch all",
            GraphVerb::Undo => "Undo",
            GraphVerb::Redo => "Redo",
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
        }
    }

    /// The key press that does it.
    fn key(self) -> (Key, Mods) {
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
        match self {
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
        }
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

    /// Do what the verb's key does. Returns whether anything listened.
    pub fn press(&mut self, verb: GraphVerb) -> bool {
        let (key, mods) = verb.key();
        let used = self.key_down(key.clone(), mods);
        self.key_up(key, Mods::default());
        used
    }
}
