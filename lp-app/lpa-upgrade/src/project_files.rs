//! A whole project as a flat path → bytes map.

use std::collections::BTreeMap;

/// The project manifest, at the package root.
pub const PROJECT_MANIFEST: &str = "project.json";

/// Every file in a project package, keyed by package-relative path.
///
/// This is the shape Studio already hands around
/// (`LibraryStore::read_all_files() -> Vec<(String, Vec<u8>)>`, paths with no
/// leading slash). Paths are stored exactly as given so a caller can write
/// the result back through the same keys it read; lookups tolerate a leading
/// `/` because the zip and device paths sometimes carry one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl ProjectFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) -> Option<Vec<u8>> {
        self.files.insert(path.into(), bytes)
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Paths in sorted order — the iteration order every step walks, so
    /// reports are deterministic.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
    }

    pub fn into_pairs(self) -> Vec<(String, Vec<u8>)> {
        self.files.into_iter().collect()
    }

    /// The `project.json` manifest bytes, with or without a leading slash.
    pub fn manifest(&self) -> Option<&[u8]> {
        self.paths()
            .find(|path| is_manifest_path(path))
            .and_then(|path| self.files.get(path).map(Vec::as_slice))
    }

    /// Replace a file's contents. Only used for files a step actually
    /// changed — untouched files are never rewritten, because the authored
    /// corpus is not canonically formatted and a canonicalizing pass would
    /// churn every project it touched.
    pub(crate) fn replace(&mut self, path: &str, bytes: Vec<u8>) {
        if let Some(slot) = self.files.get_mut(path) {
            *slot = bytes;
        }
    }
}

impl<P: Into<String>> FromIterator<(P, Vec<u8>)> for ProjectFiles {
    fn from_iter<I: IntoIterator<Item = (P, Vec<u8>)>>(iter: I) -> Self {
        Self {
            files: iter
                .into_iter()
                .map(|(path, bytes)| (path.into(), bytes))
                .collect(),
        }
    }
}

/// Whether `path` names the root manifest (`project.json`, or `/project.json`
/// from a source that keeps the leading slash).
pub(crate) fn is_manifest_path(path: &str) -> bool {
    path.trim_start_matches('/') == PROJECT_MANIFEST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_found_with_or_without_a_leading_slash() {
        let bare: ProjectFiles = [("project.json", b"{}".to_vec())].into_iter().collect();
        assert_eq!(bare.manifest(), Some(b"{}".as_slice()));

        let rooted: ProjectFiles = [("/project.json", b"{}".to_vec())].into_iter().collect();
        assert_eq!(rooted.manifest(), Some(b"{}".as_slice()));

        let nested: ProjectFiles = [("sub/project.json", b"{}".to_vec())].into_iter().collect();
        assert_eq!(nested.manifest(), None);
    }

    #[test]
    fn paths_come_back_exactly_as_inserted() {
        let files: ProjectFiles = [
            ("/project.json", b"{}".to_vec()),
            ("/shader.glsl", b"void main() {}".to_vec()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            files.paths().collect::<Vec<_>>(),
            vec!["/project.json", "/shader.glsl"]
        );
    }
}
