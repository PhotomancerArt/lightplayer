//! The open project's files, as the mapping lab's files crate reads them.
//!
//! `lab_mapping_files::Opened::open` walks a project from `project.json`
//! and asks for each file it needs by path. Studio holds a file's text
//! once something has fetched it (the project controller's asset cache:
//! an edit not yet saved, the overlay, or the saved body). A file not
//! fetched yet is a **miss**: the source answers "no such file" for now
//! and remembers the path, the controller fetches every miss from the
//! board, and the open runs again. A path the board says isn't there is
//! **absent**, and is answered "no such file" for good.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use lab_mapping_files::Source;
use lpc_model::ArtifactLocation;

use crate::app::project::ProjectController;

/// One read of the project's files.
pub(crate) struct GraphSource<'a> {
    project: &'a ProjectController,
    absent: &'a BTreeSet<String>,
    /// Every path asked for, and what it said (`None`: no such file).
    read: RefCell<BTreeMap<String, Option<String>>>,
    misses: RefCell<BTreeSet<String>>,
}

impl<'a> GraphSource<'a> {
    pub(crate) fn new(project: &'a ProjectController, absent: &'a BTreeSet<String>) -> Self {
        Self {
            project,
            absent,
            read: RefCell::new(BTreeMap::new()),
            misses: RefCell::new(BTreeSet::new()),
        }
    }

    /// The paths asked for that Studio hasn't fetched yet.
    pub(crate) fn misses(&self) -> BTreeSet<String> {
        self.misses.borrow().clone()
    }

    /// Every path asked for, and what it said.
    pub(crate) fn into_read(self) -> BTreeMap<String, Option<String>> {
        self.read.into_inner()
    }

    /// What a file says now, if Studio holds it: `Some(None)` for a file
    /// that isn't there, `None` for one not fetched yet.
    pub(crate) fn text_of(
        project: &ProjectController,
        absent: &BTreeSet<String>,
        path: &str,
    ) -> Option<Option<String>> {
        if absent.contains(path) {
            return Some(None);
        }
        let content = project.asset_content_cached(&location(path))?;
        Some(content.text().map(str::to_string))
    }
}

impl Source for GraphSource<'_> {
    fn read(&self, path: &str) -> Option<String> {
        match Self::text_of(self.project, self.absent, path) {
            Some(text) => {
                self.read
                    .borrow_mut()
                    .insert(path.to_string(), text.clone());
                text
            }
            None => {
                self.misses.borrow_mut().insert(path.to_string());
                None
            }
        }
    }
}

/// A project path (`dome/module.json`) as the artifact Studio files it
/// under (`/dome/module.json`).
pub(crate) fn location(path: &str) -> ArtifactLocation {
    ArtifactLocation::file(format!("/{}", path.trim_start_matches('/')))
}
