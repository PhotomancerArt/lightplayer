//! The undo **correlation substrate** (unified-editor P2): a
//! session-global monotonic `edit_seq` stamped onto every undo step in
//! every stack, plus a bounded journal of edit/switch events.
//!
//! Undo itself stays MODE-SCOPED (ratified v1): mapping ⌘Z is the focused
//! fixture's session undo, patching and arrange ⌘Z are controller byte
//! stacks. This module does not change that — it only guarantees that
//! every step everywhere carries `(edit_seq, node, mode)` from ONE
//! sequence, and that node/mode switches land in the same sequence, so the
//! streams COULD be correlated into a global timeline later. No global
//! replay exists in this pass; v1 UI ignores the journal entirely.
//!
//! The mapping editor's session (`lpa-mapping-editor`) stays
//! project-unaware: its step commits are stamped at the GLUE layer — the
//! shell observes them and dispatches
//! [`crate::ProjectEditorOp::EditorJournal`] events (wired in P5).

use std::collections::VecDeque;

use lpc_model::{ArtifactLocation, NodeId};

/// Journal capacity: enough to correlate a working session's recent
/// history, small enough to clone into every snapshot.
pub const EDIT_JOURNAL_CAP: usize = 256;

/// Which mode-scoped undo surface an event belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEditorMode {
    /// The focused fixture's `MapEditorSession` (geometry edits).
    Mapping,
    /// The patch-verb byte stack (wiring edits).
    Patching,
    /// The `editor.json` byte stack (canvas placement edits).
    Arrange,
}

/// What happened, in correlation grain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEditJournalEvent {
    /// An undoable step committed somewhere (any stack).
    Edit,
    /// The shell's focused node changed.
    NodeSwitch,
    /// The shell's mode changed.
    ModeSwitch,
}

/// One journal entry: the stamp every undo step also carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiEditJournalEntry {
    /// Position in the session-global sequence (strictly increasing).
    pub seq: u64,
    pub event: UiEditJournalEvent,
    /// The node the event concerns, when one does (runtime id — the
    /// journal is session-scoped, exactly like the id).
    pub node: Option<NodeId>,
    pub mode: UiEditorMode,
}

/// One committed step on a controller byte stack: the `(edit_seq, node,
/// mode)` stamp plus the two-sided `(artifact, before, after)` snapshots
/// (the #409 lesson: never snapshot from a cold cache).
#[derive(Clone, Debug, PartialEq)]
pub struct EditStep {
    pub seq: u64,
    pub node: Option<NodeId>,
    pub mode: UiEditorMode,
    pub writes: Vec<(ArtifactLocation, Vec<u8>, Vec<u8>)>,
}

/// The sequence + journal pair the controller owns.
#[derive(Debug, Default)]
pub struct EditJournal {
    seq: u64,
    entries: VecDeque<UiEditJournalEntry>,
}

impl EditJournal {
    /// Record an event, minting the next `edit_seq` — the ONLY place the
    /// sequence advances.
    pub fn record(
        &mut self,
        event: UiEditJournalEvent,
        node: Option<NodeId>,
        mode: UiEditorMode,
    ) -> u64 {
        self.seq += 1;
        if self.entries.len() == EDIT_JOURNAL_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(UiEditJournalEntry {
            seq: self.seq,
            event,
            node,
            mode,
        });
        self.seq
    }

    /// The last minted sequence value (0 = nothing recorded yet).
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// The retained entries, oldest first.
    #[must_use]
    pub fn entries(&self) -> Vec<UiEditJournalEntry> {
        self.entries.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequence_is_strictly_monotonic_across_modes() {
        let mut journal = EditJournal::default();
        let a = journal.record(UiEditJournalEvent::Edit, None, UiEditorMode::Patching);
        let b = journal.record(UiEditJournalEvent::ModeSwitch, None, UiEditorMode::Arrange);
        let c = journal.record(
            UiEditJournalEvent::Edit,
            Some(NodeId::new(7)),
            UiEditorMode::Arrange,
        );
        assert!(a < b && b < c);
        assert_eq!(journal.seq(), c);
        let entries = journal.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].node, Some(NodeId::new(7)));
    }

    #[test]
    fn the_journal_ring_is_bounded_but_the_sequence_never_resets() {
        let mut journal = EditJournal::default();
        for _ in 0..(EDIT_JOURNAL_CAP + 10) {
            journal.record(UiEditJournalEvent::Edit, None, UiEditorMode::Mapping);
        }
        assert_eq!(journal.entries().len(), EDIT_JOURNAL_CAP);
        assert_eq!(journal.seq(), (EDIT_JOURNAL_CAP + 10) as u64);
        assert_eq!(
            journal.entries().first().unwrap().seq,
            11,
            "oldest entries fall off; their sequence positions do not recycle"
        );
    }
}
