//! The undo **correlation substrate** (unified-editor P2): a
//! session-global monotonic `edit_seq`, plus a bounded journal of
//! edit/switch events.
//!
//! Every event lands in ONE sequence as `(edit_seq, node, mode)`, so
//! streams COULD be correlated into a global timeline later. No global
//! replay exists; the UI ignores the journal entirely. Events arrive as
//! [`crate::ProjectEditorOp::EditorJournal`]; the Graph keys its file
//! reads on the sequence (`ProjectController::edit_seq`).

use std::collections::VecDeque;

use lpc_model::NodeId;

/// Journal capacity: enough to correlate a working session's recent
/// history, small enough to clone into every snapshot.
pub const EDIT_JOURNAL_CAP: usize = 256;

/// Which mode-scoped undo surface an event belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEditorMode {
    /// A fixture's mapping (geometry edits).
    Mapping,
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
    fn the_sequence_is_strictly_monotonic_across_events() {
        let mut journal = EditJournal::default();
        let a = journal.record(UiEditJournalEvent::Edit, None, UiEditorMode::Mapping);
        let b = journal.record(UiEditJournalEvent::ModeSwitch, None, UiEditorMode::Mapping);
        let c = journal.record(
            UiEditJournalEvent::Edit,
            Some(NodeId::new(7)),
            UiEditorMode::Mapping,
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
