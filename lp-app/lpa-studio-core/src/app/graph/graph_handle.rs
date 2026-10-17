//! The page's synchronous hold on the Graph's editor.
//!
//! The Graph is the mapping lab's canvas, and its editor
//! (`lab_mapping_model::Editor`) is core's: offers, saves and the board's
//! highlight are all built from it here. But a pointer move cannot wait
//! for the studio actor — the actor runs one batch at a time, a batch can
//! hold a device read, and every batch rebuilds the whole studio view — so
//! the page applies its pointer, key and layout facts to the editor
//! directly, through this handle, the way the lab's page does. It is the
//! same `Rc<RefCell<…>>` seam as [`StudioHandle`]'s delay cell: built by
//! core, handed to the page before the controller moves into the actor.
//!
//! Whatever an input changed that the rest of Studio must hear about (the
//! project, the selection) goes onto the actor's queue as a
//! [`GraphNote`](super::GraphNote), so saving and the board's highlight
//! stay on the actor's one ordered path.
//!
//! [`StudioHandle`]: crate::app::studio::studio_actor::StudioHandle

use std::cell::RefCell;
use std::rc::Rc;

use lab_mapping_model::{Editor, GraphInput, Project, Target};

use crate::app::graph::GraphNote;
use crate::app::studio::studio_command::StudioCommand;
use crate::app::studio::studio_view_channel::CommandSender;

/// Core's Graph editor, shared with the page. Cloning shares it.
#[derive(Clone)]
pub struct GraphHandle {
    editor: Rc<RefCell<Editor>>,
    tx: Rc<RefCell<Option<CommandSender>>>,
}

impl Default for GraphHandle {
    fn default() -> Self {
        Self {
            editor: Rc::new(RefCell::new(Editor::new(Project::new("")))),
            tx: Rc::new(RefCell::new(None)),
        }
    }
}

impl core::fmt::Debug for GraphHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("GraphHandle(..)")
    }
}

impl GraphHandle {
    /// The editor itself: the page reads it to draw, core to build offers,
    /// saves and the highlight. Never hold a borrow across an `.await`.
    pub fn editor(&self) -> &Rc<RefCell<Editor>> {
        &self.editor
    }

    /// Apply one input from the page. Returns whether it was used. When it
    /// changed the project or the selection, the actor hears so.
    pub fn input(&self, input: GraphInput) -> bool {
        let (used, note) = {
            let mut e = self.editor.borrow_mut();
            let revision = e.project.revision();
            let selection: Vec<Target> = e.selection.clone();
            let used = e.input(input);
            let note = GraphNote {
                edited: e.project.revision() != revision,
                selected: e.selection != selection,
            };
            (used, note)
        };
        if note.edited || note.selected {
            self.note(note);
        }
        used
    }

    /// Tell the actor what changed.
    pub(crate) fn note(&self, note: GraphNote) {
        if let Some(tx) = self.tx.borrow().as_ref() {
            tx.send(StudioCommand::Graph(note));
        }
    }

    /// The actor's queue, set when the actor is built.
    pub(crate) fn set_command_sender(&self, tx: CommandSender) {
        *self.tx.borrow_mut() = Some(tx);
    }
}
