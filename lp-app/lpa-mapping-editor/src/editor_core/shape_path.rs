//! Tree addressing for document shapes.
//!
//! The document is a tree: a root list of objects, each a chain of shape
//! nodes ([`Map2dShape::Repeat`] is the only structural parent today, with
//! exactly one child). A [`ShapePath`] names one node in that tree, and it
//! is the unit of selection — see the selection/tree ADR
//! (`docs/adr/2026-08-05-map2d-editor-selection-tree-model.md`).
//!
//! Every consumer must handle arbitrary child arity even though today's
//! arity is 0 or 1: a future `Group(Vec<...>)` format addition must be
//! purely additive here.

use lpc_mapping::{Map2dDoc, Map2dShape};

/// One node in the document's shape tree: a root object index plus the
/// child-index steps descending from the object's shape.
///
/// Documents deliberately carry no node ids, so paths are positional and
/// can dangle after structural edits; consumers resolve with
/// [`Self::resolve`] and drop paths that miss rather than panicking.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapePath {
    /// Wiring-order object index.
    pub object: usize,
    /// Child-index steps below the object's root shape; empty = the object
    /// itself.
    pub descent: Vec<usize>,
}

impl ShapePath {
    #[must_use]
    pub fn root(object: usize) -> Self {
        Self {
            object,
            descent: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.descent.is_empty()
    }

    /// The enclosing node, `None` at root. This is the selection's *scope*
    /// when non-root — context, never a co-selection.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.descent.is_empty() {
            return None;
        }
        let mut descent = self.descent.clone();
        descent.pop();
        Some(Self {
            object: self.object,
            descent,
        })
    }

    #[must_use]
    pub fn child(&self, step: usize) -> Self {
        let mut descent = self.descent.clone();
        descent.push(step);
        Self {
            object: self.object,
            descent,
        }
    }

    /// Strict-ancestor test (a path is not its own ancestor).
    #[must_use]
    pub fn is_ancestor_of(&self, other: &Self) -> bool {
        self.object == other.object
            && self.descent.len() < other.descent.len()
            && other.descent[..self.descent.len()] == self.descent[..]
    }

    /// The shape node this path names, `None` when it dangles.
    #[must_use]
    pub fn resolve<'a>(&self, doc: &'a Map2dDoc) -> Option<&'a Map2dShape> {
        let mut node = &doc.objects.get(self.object)?.shape;
        for step in &self.descent {
            node = structural_child(node, *step)?;
        }
        Some(node)
    }

    /// Mutable resolve; same dangle semantics.
    #[must_use]
    pub fn resolve_mut<'a>(&self, doc: &'a mut Map2dDoc) -> Option<&'a mut Map2dShape> {
        let mut node = &mut doc.objects.get_mut(self.object)?.shape;
        for step in &self.descent {
            node = structural_child_mut(node, *step)?;
        }
        Some(node)
    }

    /// A path is valid when it names a live node in `doc`.
    #[must_use]
    pub fn is_valid(&self, doc: &Map2dDoc) -> bool {
        self.resolve(doc).is_some()
    }
}

/// Number of structural children of a shape node. `Repeat` has one; every
/// leaf shape — grid, ring, path, polygon, filled polygon — has none. A
/// future `Group` slots in here and everything above keeps working.
#[must_use]
pub fn structural_child_count(shape: &Map2dShape) -> usize {
    match shape {
        Map2dShape::Repeat(_) => 1,
        Map2dShape::Grid(_)
        | Map2dShape::Ring(_)
        | Map2dShape::Path(_)
        | Map2dShape::Polygon(_)
        | Map2dShape::FilledPolygon(_) => 0,
    }
}

#[must_use]
pub fn structural_child(shape: &Map2dShape, step: usize) -> Option<&Map2dShape> {
    match shape {
        Map2dShape::Repeat(repeat) if step == 0 => Some(&repeat.shape),
        _ => None,
    }
}

#[must_use]
pub fn structural_child_mut(shape: &mut Map2dShape, step: usize) -> Option<&mut Map2dShape> {
    match shape {
        Map2dShape::Repeat(repeat) if step == 0 => Some(&mut repeat.shape),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::corpus;

    fn repeated_doc() -> Map2dDoc {
        corpus::repeated_sector()
    }

    #[test]
    fn root_resolves_to_the_object_shape() {
        let doc = repeated_doc();
        assert!(matches!(
            ShapePath::root(0).resolve(&doc),
            Some(Map2dShape::Repeat(_))
        ));
    }

    #[test]
    fn descent_resolves_through_a_repeat() {
        let doc = repeated_doc();
        let inner = ShapePath::root(0).child(0);
        assert!(matches!(inner.resolve(&doc), Some(Map2dShape::Path(_))));
    }

    #[test]
    fn dangling_paths_resolve_to_none_not_a_panic() {
        let doc = repeated_doc();
        assert_eq!(ShapePath::root(7).resolve(&doc), None);
        assert_eq!(ShapePath::root(0).child(1).resolve(&doc), None);
        assert_eq!(ShapePath::root(0).child(0).child(0).resolve(&doc), None);
    }

    #[test]
    fn parent_and_ancestor_relations() {
        let inner = ShapePath::root(2).child(0);
        assert_eq!(inner.parent(), Some(ShapePath::root(2)));
        assert_eq!(ShapePath::root(2).parent(), None);
        assert!(ShapePath::root(2).is_ancestor_of(&inner));
        assert!(!inner.is_ancestor_of(&ShapePath::root(2)));
        assert!(!ShapePath::root(1).is_ancestor_of(&inner));
        let path = ShapePath::root(2);
        assert!(
            !path.is_ancestor_of(&path),
            "a path is not its own ancestor"
        );
    }

    #[test]
    fn resolve_mut_edits_the_authored_inner_shape() {
        let mut doc = repeated_doc();
        let inner = ShapePath::root(0).child(0);
        if let Some(Map2dShape::Path(path)) = inner.resolve_mut(&mut doc) {
            path.points[0] = [1.0, 2.0];
        } else {
            panic!("inner path resolves mutably");
        }
        let Some(Map2dShape::Repeat(repeat)) = ShapePath::root(0).resolve(&doc) else {
            panic!("root is the repeat");
        };
        let Map2dShape::Path(path) = repeat.shape.as_ref() else {
            panic!("inner is the path");
        };
        assert_eq!(path.points[0], [1.0, 2.0]);
    }
}
