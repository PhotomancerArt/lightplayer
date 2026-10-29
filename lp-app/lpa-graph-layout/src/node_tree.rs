//! The input's nodes as a tree: who sits in which container, in authoring
//! order. Both layouts walk it.

use std::collections::BTreeMap;

use crate::{LayoutInput, LayoutRole};

pub(crate) struct NodeTree {
    /// Each container's children, by index into the input's nodes; the
    /// root is `None`.
    pub children: BTreeMap<Option<usize>, Vec<usize>>,
    /// Each node's container.
    pub parent: Vec<Option<usize>>,
}

impl NodeTree {
    pub fn new(input: &LayoutInput) -> Self {
        let index: BTreeMap<&str, usize> = input
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut parent = vec![None; input.nodes.len()];
        for (i, n) in input.nodes.iter().enumerate() {
            parent[i] = n
                .parent
                .as_deref()
                .and_then(|p| index.get(p).copied())
                .filter(|&p| p != i && input.nodes[p].role == LayoutRole::Module);
        }
        // A loop of parents can never reach the root: break it there.
        for i in 0..parent.len() {
            let mut at = parent[i];
            let mut steps = 0;
            while let Some(p) = at {
                steps += 1;
                if steps > parent.len() {
                    parent[i] = None;
                    break;
                }
                at = parent[p];
            }
        }
        let mut children: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
        children.entry(None).or_default();
        for (i, p) in parent.iter().enumerate() {
            children.entry(*p).or_default().push(i);
        }
        Self { children, parent }
    }

    pub fn children_of(&self, container: Option<usize>) -> &[usize] {
        self.children.get(&container).map_or(&[], Vec::as_slice)
    }

    /// `node`'s ancestor (or itself) directly inside `container`, if it is
    /// in there at all.
    pub fn at_level(&self, node: usize, container: Option<usize>) -> Option<usize> {
        let mut at = node;
        loop {
            if self.parent[at] == container {
                return Some(at);
            }
            at = self.parent[at]?;
        }
    }
}
