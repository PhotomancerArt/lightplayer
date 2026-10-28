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
    /// Package-relative paths a step added, in sorted order (a fixture's
    /// hand-placed lamps moved into their own `*.map2d.json`, v11→v12).
    pub added_files: Vec<String>,
    /// Package-relative paths a step removed, in sorted order (a retired
    /// `*.patch.json`, v11→v12).
    pub removed_files: Vec<String>,
    /// One line per transformation applied, naming the file and the edit.
    pub notes: Vec<String>,
    /// Things a human should look at afterwards. Not failures — the upgrade
    /// succeeded — but places where the automatic answer is conservative.
    pub warnings: Vec<String>,
}

impl UpgradeReport {
    /// A report of nothing done yet, for a project at `from`.
    pub fn new(from: u32) -> Self {
        Self {
            from,
            to: from,
            ..Self::default()
        }
    }

    /// Whether the upgrade rewrote, added or removed anything at all.
    pub fn is_empty(&self) -> bool {
        self.changed_files.is_empty()
            && self.added_files.is_empty()
            && self.removed_files.is_empty()
    }

    pub fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    pub fn warn(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Record that `path`'s bytes changed. Idempotent: a file touched by two
    /// steps in one chain is listed once.
    pub fn record_changed(&mut self, path: &str) {
        insert_sorted(&mut self.changed_files, path);
    }

    /// Record that `path` was added. Idempotent, sorted.
    pub fn record_added(&mut self, path: &str) {
        insert_sorted(&mut self.added_files, path);
    }

    /// Record that `path` was removed. Idempotent, sorted.
    pub fn record_removed(&mut self, path: &str) {
        insert_sorted(&mut self.removed_files, path);
    }
}

fn insert_sorted(paths: &mut Vec<String>, path: &str) {
    if let Err(index) = paths.binary_search_by(|p| p.as_str().cmp(path)) {
        paths.insert(index, String::from(path));
    }
}
