//! Core and engine: what the core's roots reach, and everything else.
//!
//! The core's roots are the reset entry, the app descriptor, every
//! RAM-resident code section (the bootloader loads those, so they are core
//! whatever reaches them), and any extra root a caller names. The one RAM
//! code that is not a root is code a linker rule moved there for speed (the
//! S3's IRAM hot set, [`Node::is_hot_ram_code`]): it is core if the core
//! reaches it, and engine code that happens to ride in the core's RAM
//! segment if only the engine does. Rooting it would pull everything it calls
//! — most of the engine — into the core. The engine
//! header is not a root and nothing in the core names it — the core reads it
//! through a plain address — so the walk never crosses into the engine
//! through it. Placement is by reachability, never by crate: a crate's code
//! lands wherever its callers are (the spike's first trap).

use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(doc)]
use crate::section_graph::Node;
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
        let mut core = HashSet::new();
        let mut stack = roots(graph, extra_roots);
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

/// The core's roots: every RAM-resident code section, the app descriptor,
/// the reset entry's section, and `extra_roots`.
fn roots(graph: &SectionGraph, extra_roots: &[usize]) -> Vec<usize> {
    let mut roots: Vec<usize> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| (n.is_ram_code() && !n.is_hot_ram_code()) || n.out == ".flash.appdesc")
        .map(|(i, _)| i)
        .collect();
    if let Some(e) = graph.node_at(graph.entry as i64) {
        roots.push(e);
    }
    roots.extend_from_slice(extra_roots);
    roots
}

/// How the core reaches the first node `wanted` accepts: the shortest chain
/// of node indices from a root to it, or `None` when the core does not
/// reach one (it is engine). The question to ask when the core is larger
/// than it should be.
pub fn path_to(
    graph: &SectionGraph,
    extra_roots: &[usize],
    wanted: &dyn Fn(usize) -> bool,
) -> Option<Vec<usize>> {
    let mut parent = HashMap::new();
    let mut queue = VecDeque::new();
    for r in roots(graph, extra_roots) {
        if parent.insert(r, None).is_none() {
            queue.push_back(r);
        }
    }
    while let Some(i) = queue.pop_front() {
        if wanted(i) {
            let mut chain = vec![i];
            let mut at = parent[&i];
            while let Some(p) = at {
                chain.push(p);
                at = parent[&p];
            }
            chain.reverse();
            return Some(chain);
        }
        for t in graph.edges.get(&i).into_iter().flatten() {
            if !parent.contains_key(t) {
                parent.insert(*t, Some(i));
                queue.push_back(*t);
            }
        }
    }
    None
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
    fn hot_ram_code_is_core_only_when_the_core_reaches_it() {
        // A Rust function's own `.text.*` moved into `.rwtext` (0x50) calls
        // engine code (0x400); only the engine (0x500) calls it. Another hot
        // one (0x60) the core calls (0x100 -> 0x60) is core, and so is what
        // it calls (0x200). A `#[ram]` function (0x70) is a root as ever.
        let hot = |vma: u64| Node {
            vma,
            size: 0x10,
            out: ".rwtext".into(),
            desc: format!("/x/fw-1234.fw.abc-cgu.0.rcgu.o:(.text._RNv{vma:x})"),
        };
        let g = SectionGraph::build(
            vec![
                hot(0x50),
                hot(0x60),
                Node {
                    vma: 0x70,
                    size: 0x10,
                    out: ".rwtext".into(),
                    desc: "/x/fw-1234.fw.abc-cgu.0.rcgu.o:(.rwtext._RNvisr)".into(),
                },
                node(0x100, ".text"),
                node(0x200, ".text"),
                node(0x300, ".text"),
                node(0x400, ".text"),
                node(0x500, ".text"),
            ],
            &[
                (0x50, 0x400),
                (0x500, 0x50),
                (0x100, 0x60),
                (0x60, 0x200),
                (0x70, 0x300),
            ],
            0x100,
        );
        let split = Split::compute(&g, &[]);
        let mut core: Vec<u64> = split.core.iter().map(|i| g.nodes[*i].vma).collect();
        core.sort();
        assert_eq!(core, [0x60, 0x70, 0x100, 0x200, 0x300]);
    }

    #[test]
    fn the_path_names_how_the_core_reaches_a_node() {
        let g = SectionGraph::build(
            vec![
                node(0x100, ".text"),
                node(0x200, ".text"),
                node(0x300, ".text"),
                node(0x400, ".text"),
            ],
            &[(0x100, 0x200), (0x200, 0x300)],
            0x100,
        );
        let path = path_to(&g, &[], &|i| g.nodes[i].vma == 0x300).unwrap();
        let vmas: Vec<u64> = path.iter().map(|i| g.nodes[*i].vma).collect();
        assert_eq!(vmas, [0x100, 0x200, 0x300]);
        assert!(path_to(&g, &[], &|i| g.nodes[i].vma == 0x400).is_none());
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
