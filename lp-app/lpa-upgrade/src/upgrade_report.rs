//! What an upgrade did, in a form a UI can show and a test can assert.

/// A machine-readable account of one `v_from → v_to` migration run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeReport {
    /// The format the project was authored at.
    pub from: u32,
    /// The format it is at now.
    pub to: u32,
    /// Package-relative paths whose bytes changed, in sorted order. A file
    /// absent from this list is byte-identical to its input.
    pub changed_files: Vec<String>,
    /// One line per transformation applied, naming the file and the edit.
    pub notes: Vec<String>,
    /// Things a human should look at afterwards. Not failures — the upgrade
    /// succeeded — but places where the automatic answer is conservative.
    pub warnings: Vec<String>,
}

impl UpgradeReport {
    pub(crate) fn new(from: u32) -> Self {
        Self {
            from,
            to: from,
            ..Self::default()
        }
    }

    /// Whether the upgrade rewrote anything at all.
    pub fn is_empty(&self) -> bool {
        self.changed_files.is_empty()
    }

    pub(crate) fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    pub(crate) fn warn(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Record that `path`'s bytes changed. Idempotent: a file touched by two
    /// steps in one chain is listed once.
    pub(crate) fn record_changed(&mut self, path: &str) {
        if let Err(index) = self
            .changed_files
            .binary_search_by(|p| p.as_str().cmp(path))
        {
            self.changed_files.insert(index, String::from(path));
        }
    }
}
