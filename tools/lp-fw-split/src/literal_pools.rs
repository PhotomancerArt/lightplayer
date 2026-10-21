//! Xtensa literal pools: a function's `.literal.X` belongs with its
//! `.text.X`.
//!
//! The compiler loads every address a function needs with `l32r` from the
//! function's own pool, and `l32r` reaches only 256 KiB **backwards**. So
//! the two must land in one region, the pool first:
//!
//! - **Reach**: [`tie_pools`] adds an edge from each `.text.X` to its
//!   `.literal.X`, so a core function's pool is core. Never the other way:
//!   coalescing gives a pool slots other functions' `l32r`s load, and a
//!   pool → code edge would make their callers reach this function
//!   ([`crate::xtensa_targets`]).
//! - **Placement**: the engine script writes each engine function's
//!   `.literal.X` rule immediately before its `.text.X` rule
//!   ([`crate::engine_script::EngineRules::from_split_for`]). A pool that
//!   relaxation emptied in pass 1 is no node there, but pass 2 starts from
//!   the object's own pools again, so the rule is written regardless.

use std::collections::HashMap;

use crate::section_graph::SectionGraph;

/// `.literal.X` for a `.text.X` (or `.literal` for `.text`).
pub fn pool_of(text_section: &str) -> Option<String> {
    text_section
        .strip_prefix(".text")
        .map(|rest| format!(".literal{rest}"))
}

/// An edge from each `.text.X` to its `.literal.X` in the same object; the
/// count added.
pub fn tie_pools(graph: &mut SectionGraph) -> usize {
    let mut pools = HashMap::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        let (obj, sec) = n.object_and_section();
        if sec.starts_with(".literal") {
            pools.insert((obj.to_string(), sec.to_string()), i);
        }
    }
    let mut ties = 0;
    for i in 0..graph.nodes.len() {
        let (obj, sec) = graph.nodes[i].object_and_section();
        let Some(pool) = pool_of(sec) else { continue };
        if let Some(&p) = pools.get(&(obj.to_string(), pool)) {
            graph.edges.entry(i).or_default().insert(p);
            ties += 1;
        }
    }
    ties
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section_graph::Node;

    #[test]
    fn code_reaches_its_pool_and_never_the_reverse() {
        let node = |vma, sec: &str| Node {
            vma,
            size: 0x10,
            out: ".text".into(),
            desc: format!("/x/a.rcgu.o:({sec})"),
        };
        let mut g = SectionGraph::build(
            vec![node(0x100, ".literal.f"), node(0x200, ".text.f")],
            &[],
            0,
        );
        assert_eq!(tie_pools(&mut g), 1);
        assert!(g.edges[&1].contains(&0), "code → pool");
        assert!(!g.edges.contains_key(&0), "never pool → code");
    }

    #[test]
    fn a_pool_is_named_after_its_code() {
        assert_eq!(pool_of(".text.f").as_deref(), Some(".literal.f"));
        assert_eq!(pool_of(".text").as_deref(), Some(".literal"));
        assert_eq!(
            pool_of(".text.unlikely.g").as_deref(),
            Some(".literal.unlikely.g")
        );
        assert_eq!(pool_of(".rodata.x"), None);
    }
}
