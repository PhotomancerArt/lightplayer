//! Where a project's files come from: a directory on disk, or files held
//! in memory (the page, which has no disk, bakes its projects in).
//!
//! Paths are the project's own: relative to its root, `/`-separated, as
//! `module.json` refs write them (`./dome/module.json` → `dome/module.json`).

use std::collections::BTreeMap;
use std::path::PathBuf;

pub trait Source {
    /// One file's text, or `None` when there is no such file.
    fn read(&self, path: &str) -> Option<String>;
}

/// Files held in memory, by path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemFiles {
    pub files: BTreeMap<String, String>,
}

impl MemFiles {
    pub fn new(files: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        Self {
            files: files
                .into_iter()
                .map(|(p, t)| (p.into(), t.into()))
                .collect(),
        }
    }
}

impl Source for MemFiles {
    fn read(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }
}

/// A project directory on disk.
#[derive(Debug, Clone)]
pub struct DirFiles {
    pub root: PathBuf,
}

impl DirFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Source for DirFiles {
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }
}

/// `child`, written relative to the file at `from` (`./x.json` from
/// `dome/module.json` is `dome/x.json`).
pub fn join(from: &str, child: &str) -> String {
    let dir = match from.rfind('/') {
        Some(i) => &from[..i],
        None => "",
    };
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in child.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The name of a file at `path`, as a ref from a file beside it.
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
