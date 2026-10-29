//! The page's hold on an editor it does not own.
//!
//! The components in this crate draw an [`Editor`] and say what happened
//! to it, but the editor belongs to the host: Studio's core keeps the
//! Graph's. A [`GraphEditor`] is that seam:
//!
//! - **reading:** [`GraphEditor::read`] borrows the editor and subscribes
//!   the reading component to its tick, so it draws again when the editor
//!   changes; [`GraphEditor::peek`] borrows without subscribing;
//! - **writing:** [`GraphEditor::input`] hands a [`GraphInput`] to the
//!   host's sink, which applies it however the host applies inputs, then
//!   moves the tick.
//!
//! A host that changes the editor some other way (Studio rebuilding it for
//! a new revision of the project) calls [`GraphEditor::changed`].
//!
//! Never hold a [`GraphEditor::read`] across an [`GraphEditor::input`]:
//! the editor is one `RefCell`, as a `Signal` would be.

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_graph::{Editor, GraphInput};

/// Applies an input to the host's editor; returns whether it was used.
pub type GraphSink = Rc<dyn Fn(GraphInput) -> bool>;

#[derive(Clone)]
pub struct GraphEditor {
    editor: Rc<RefCell<Editor>>,
    tick: Signal<u64>,
    sink: GraphSink,
}

impl PartialEq for GraphEditor {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.editor, &other.editor) && self.tick == other.tick
    }
}

impl GraphEditor {
    /// Hold `editor`, moving `tick` whenever it changes, and send every
    /// input to `sink`.
    pub fn new(editor: Rc<RefCell<Editor>>, tick: Signal<u64>, sink: GraphSink) -> Self {
        Self { editor, tick, sink }
    }

    /// The editor, and draw again when it changes.
    pub fn read(&self) -> Ref<'_, Editor> {
        let _ = self.tick.read();
        self.editor.borrow()
    }

    /// The editor, without drawing again when it changes.
    pub fn peek(&self) -> Ref<'_, Editor> {
        self.editor.borrow()
    }

    /// Say what happened: the host applies it, and whatever reads the
    /// editor draws again. Returns whether the input was used.
    pub fn input(&self, input: GraphInput) -> bool {
        let used = (self.sink)(input);
        self.changed();
        used
    }

    /// The editor changed some other way: draw again.
    pub fn changed(&self) {
        let mut tick = self.tick;
        tick += 1;
    }
}
