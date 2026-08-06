//! Editor selection: a set of shape-tree paths plus an optional vertex
//! sub-selection.
//!
//! The model (see `docs/adr/2026-08-05-map2d-editor-selection-tree-model.md`):
//!
//! - Selection is a set of [`ShapePath`]s. **An ancestor of a selected path
//!   is context (the *scope*), never a co-selection** — inserting a path
//!   evicts selected ancestors and descendants.
//! - All selected paths share one parent (**sibling-level multi-select**);
//!   inserting a path from a different parent replaces the selection.
//! - The scope is *derived*: the shared parent of the selected paths.
//!   There is no separate scope state to drift.
//!
//! Documents deliberately carry no node ids, so paths are positional and
//! the session remaps them on structural edits (delete/reorder) — those
//! fix-ups are host-tested.

use std::collections::BTreeSet;

use crate::editor_core::shape_path::ShapePath;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapSelection {
    /// Selected tree paths; invariants enforced by the mutators.
    paths: BTreeSet<ShapePath>,
    /// Selected vertex of a single selected path-shaped node.
    pub vertex: Option<usize>,
}

impl MapSelection {
    pub fn clear(&mut self) {
        self.paths.clear();
        self.vertex = None;
    }

    /// Replace the selection with one path.
    pub fn select_only_path(&mut self, path: ShapePath) {
        self.paths.clear();
        self.paths.insert(path);
        self.vertex = None;
    }

    /// Add a path, enforcing the invariants: evict ancestors/descendants of
    /// `path`; if `path`'s parent differs from the current shared parent,
    /// the selection is replaced (sibling-level multi-select only).
    pub fn insert_path(&mut self, path: ShapePath) {
        if self.shared_parent_differs(&path) {
            self.paths.clear();
        } else {
            self.paths
                .retain(|held| !held.is_ancestor_of(&path) && !path.is_ancestor_of(held));
        }
        self.paths.insert(path);
        self.vertex = None;
    }

    /// Toggle membership (same invariants as [`Self::insert_path`]).
    pub fn toggle_path(&mut self, path: ShapePath) {
        if !self.paths.remove(&path) {
            self.insert_path(path);
            return;
        }
        self.vertex = None;
    }

    /// Replace the selection with root paths for `indices` (marquee /
    /// select-all — top-level operations select at the top level).
    pub fn set_roots(&mut self, indices: impl IntoIterator<Item = usize>) {
        self.paths = indices.into_iter().map(ShapePath::root).collect();
        self.vertex = None;
    }

    /// Add root paths (additive marquee).
    pub fn extend_roots(&mut self, indices: impl IntoIterator<Item = usize>) {
        for index in indices {
            self.insert_path(ShapePath::root(index));
        }
    }

    #[must_use]
    pub fn paths(&self) -> impl Iterator<Item = &ShapePath> {
        self.paths.iter()
    }

    #[must_use]
    pub fn contains(&self, path: &ShapePath) -> bool {
        self.paths.contains(path)
    }

    /// The single selected path, when exactly one is selected.
    #[must_use]
    pub fn single(&self) -> Option<&ShapePath> {
        (self.paths.len() == 1)
            .then(|| self.paths.iter().next())
            .flatten()
    }

    /// The derived scope: the shared parent of the selected paths. `None`
    /// at root level or when nothing is selected.
    #[must_use]
    pub fn scope(&self) -> Option<ShapePath> {
        self.paths.iter().next().and_then(|path| path.parent())
    }

    /// Root object indices touched by the selection (each selected path's
    /// object), for top-level operations like reorder and structural remaps.
    #[must_use]
    pub fn root_objects(&self) -> BTreeSet<usize> {
        self.paths.iter().map(|path| path.object).collect()
    }

    /// Whether the root object `index` is selected *as an object* (a
    /// descended selection inside it does not count).
    #[must_use]
    pub fn contains_root(&self, index: usize) -> bool {
        self.paths.contains(&ShapePath::root(index))
    }

    /// The single selected ROOT object, when the selection is exactly one
    /// root path (compat surface for top-level-only consumers).
    #[must_use]
    pub fn single_root(&self) -> Option<usize> {
        self.single()
            .filter(|path| path.is_root())
            .map(|path| path.object)
    }

    /// Root-level convenience: replace the selection with one object
    /// (equivalent to selecting its root path).
    pub fn select_only(&mut self, index: usize) {
        self.select_only_path(ShapePath::root(index));
    }

    /// Root-level convenience: toggle one object's root path.
    pub fn toggle(&mut self, index: usize) {
        self.toggle_path(ShapePath::root(index));
    }

    /// Whether ANY selected path lives on object `index` (root or
    /// descended) — the canvas-highlight notion of "this object is part of
    /// the selection".
    #[must_use]
    pub fn object_selected(&self, index: usize) -> bool {
        self.paths.iter().any(|path| path.object == index)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Drop paths that no longer resolve in `doc` (structural edits can
    /// leave dangles; selection never panics over one).
    pub fn retain_valid(&mut self, doc: &lpc_mapping::Map2dDoc) {
        self.paths.retain(|path| path.is_valid(doc));
        if self.paths.is_empty() {
            self.vertex = None;
        }
    }

    /// Remap root object indices after a structural edit, dropping paths
    /// whose object was removed (`remap` returns `None`).
    pub fn remap_objects(&mut self, remap: impl Fn(usize) -> Option<usize>) {
        self.paths = self
            .paths
            .iter()
            .filter_map(|path| {
                remap(path.object).map(|object| ShapePath {
                    object,
                    descent: path.descent.clone(),
                })
            })
            .collect();
        if self.paths.is_empty() {
            self.vertex = None;
        }
    }

    fn shared_parent_differs(&self, path: &ShapePath) -> bool {
        self.paths
            .iter()
            .next()
            .is_some_and(|held| held.parent() != path.parent())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_evicts_ancestors_and_descendants() {
        let mut selection = MapSelection::default();
        selection.select_only_path(ShapePath::root(0));
        // Selecting the child evicts the parent — context, not co-selection.
        selection.insert_path(ShapePath::root(0).child(0));
        assert!(selection.contains(&ShapePath::root(0).child(0)));
        assert!(!selection.contains(&ShapePath::root(0)));
        // And selecting the parent back evicts the child.
        selection.insert_path(ShapePath::root(0));
        assert!(selection.contains(&ShapePath::root(0)));
        assert_eq!(selection.len(), 1);
    }

    #[test]
    fn multi_select_stays_sibling_level() {
        let mut selection = MapSelection::default();
        selection.select_only_path(ShapePath::root(0));
        selection.insert_path(ShapePath::root(2));
        assert_eq!(selection.len(), 2, "root siblings co-select");
        // A descended path has a different parent: selection is replaced.
        selection.insert_path(ShapePath::root(1).child(0));
        assert_eq!(selection.len(), 1);
        assert!(selection.contains(&ShapePath::root(1).child(0)));
    }

    #[test]
    fn scope_is_derived_from_the_shared_parent() {
        let mut selection = MapSelection::default();
        assert_eq!(selection.scope(), None);
        selection.select_only_path(ShapePath::root(3));
        assert_eq!(selection.scope(), None, "root selection has no scope");
        selection.select_only_path(ShapePath::root(3).child(0));
        assert_eq!(selection.scope(), Some(ShapePath::root(3)));
    }

    #[test]
    fn toggle_removes_and_inserts_with_invariants() {
        let mut selection = MapSelection::default();
        selection.toggle_path(ShapePath::root(1));
        assert!(selection.contains_root(1));
        selection.toggle_path(ShapePath::root(1));
        assert!(selection.is_empty());
    }

    #[test]
    fn remap_drops_removed_objects_and_shifts_survivors() {
        let mut selection = MapSelection::default();
        selection.set_roots([1, 3]);
        // Object 2 deleted: 3 shifts to 2; 1 unchanged.
        selection.remap_objects(|index| match index {
            2 => None,
            i if i > 2 => Some(i - 1),
            i => Some(i),
        });
        assert_eq!(selection.root_objects(), BTreeSet::from([1, 2]));
    }
}
