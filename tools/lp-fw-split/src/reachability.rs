//! Core and engine: what the core's roots reach, and everything else.
//!
//! The core's roots are the reset entry, the app descriptor, every
//! RAM-resident code section (the bootloader loads those, so they are core
//! whatever reaches them), and any extra root a caller names. The engine
//! header is not a root and nothing in the core names it — the core reads it
//! through a plain address — so the walk never crosses into the engine
//! through it. Placement is by reachability, never by crate: a crate's code
//! lands wherever its callers are (the spike's first trap).

use std::collections::HashSet;

use crate::section_graph::SectionGraph;

/// The two halves of one link.
pub struct Split {
    /// Node indices reachable from the core's roots.
    pub core: HashSet<usize>,
}

impl Split {
    /// Walk the graph from the core's roots, plus the nodes `extra_roots`
    /// selects.
    pub fn compute(graph: &SectionGraph, extra_roots: &[usize]) -> Self {
        let mut roots: Vec<usize> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_ram_code() || n.out == ".flash.appdesc")
            .map(|(i, _)| i)
            .collect();
        if let Some(e) = graph.node_at(graph.entry as i64) {
            roots.push(e);
        }
        roots.extend_from_slice(extra_roots);
        let mut core = HashSet::new();
        let mut stack = roots;
        while let Some(i) = stack.pop() {
            if !core.insert(i) {
                continue;
            }
            if let Some(targets) = graph.edges.get(&i) {
                stack.extend(targets.iter().copied().filter(|t| !core.contains(t)));
            }
        }
        Self { core }
    }

    pub fn is_core(&self, i: usize) -> bool {
        self.core.contains(&i)
    }

    /// Engine nodes, by index.
    pub fn engine(&self, graph: &SectionGraph) -> Vec<usize> {
        (0..graph.nodes.len())
            .filter(|i| !self.is_core(*i))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section_graph::Node;

    fn node(vma: u64, out: &str) -> Node {
        Node {
            vma,
            size: 0x10,
            out: out.into(),
            desc: format!("/x/a.o:(.s{vma:x})"),
        }
    }

    #[test]
    fn the_core_is_what_the_entry_and_ram_code_reach() {
        // entry 0x100 -> 0x200 -> 0x300; .rwtext 0x50 -> 0x400; 0x500 alone;
        // the engine header (0x600) reaches 0x700, which nothing else does.
        let g = SectionGraph::build(
            vec![
                node(0x50, ".rwtext"),
                node(0x100, ".text"),
                node(0x200, ".text"),
                node(0x300, ".rodata"),
                node(0x400, ".text"),
                node(0x500, ".text"),
                node(0x600, ".engine_rodata"),
                node(0x700, ".text"),
            ],
            &[
                (0x100, 0x200),
                (0x200, 0x300),
                (0x50, 0x400),
                (0x600, 0x700),
                (0x700, 0x300),
            ],
            0x100,
        );
        let split = Split::compute(&g, &[]);
        let core: Vec<u64> = {
            let mut v: Vec<u64> = split.core.iter().map(|i| g.nodes[*i].vma).collect();
            v.sort();
            v
        };
        assert_eq!(core, [0x50, 0x100, 0x200, 0x300, 0x400]);
        let engine: Vec<u64> = split.engine(&g).iter().map(|i| g.nodes[*i].vma).collect();
        assert_eq!(engine, [0x500, 0x600, 0x700]);
    }

    #[test]
    fn an_extra_root_is_core() {
        let g = SectionGraph::build(
            vec![node(0x100, ".text"), node(0x200, ".rodata")],
            &[],
            0x100,
        );
        let split = Split::compute(&g, &[1]);
        assert!(split.is_core(1));
    }
}
