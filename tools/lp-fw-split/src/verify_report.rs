//! The verifier: on the pass-2 link, no core node may sit in the engine
//! region — the core would then call into flash the loader never maps.
//!
//! It also reports what is evidence rather than a gate: the edges from core
//! to engine (zero: the door is data, read through a plain address), the
//! engine-only C archive members left in the core region (expected), and
//! the largest owners on each side by crate — the shader compiler's crates
//! among the engine's.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use crate::reachability::Split;
use crate::section_graph::SectionGraph;

/// What the verifier found.
pub struct Verification {
    /// Core nodes at or above the engine base, by name.
    pub core_in_engine: Vec<String>,
    /// Engine nodes still below the engine base, and their bytes.
    pub engine_in_core_region: usize,
    pub engine_in_core_region_bytes: u64,
    /// Relocation edges from a core node to an engine node.
    pub core_to_engine_edges: usize,
    pub engine_to_core_edges: usize,
    pub core_flash_bytes: u64,
    pub engine_flash_bytes: u64,
    pub core_by_crate: Vec<(String, u64)>,
    pub engine_by_crate: Vec<(String, u64)>,
    pub nodes: usize,
    pub relocs: usize,
    pub unresolved: usize,
}

impl Verification {
    pub fn run(
        graph: &SectionGraph,
        split: &Split,
        engine_base: u64,
        names: &BTreeMap<u64, Vec<(u64, String)>>,
    ) -> Self {
        let named = name_nodes(graph, names);
        let mut v = Self {
            core_in_engine: Vec::new(),
            engine_in_core_region: 0,
            engine_in_core_region_bytes: 0,
            core_to_engine_edges: 0,
            engine_to_core_edges: 0,
            core_flash_bytes: 0,
            engine_flash_bytes: 0,
            core_by_crate: Vec::new(),
            engine_by_crate: Vec::new(),
            nodes: graph.nodes.len(),
            relocs: graph.relocs,
            unresolved: graph.unresolved,
        };
        let mut core_crates: HashMap<String, u64> = HashMap::new();
        let mut engine_crates: HashMap<String, u64> = HashMap::new();
        for (i, n) in graph.nodes.iter().enumerate() {
            if !n.is_flash() {
                continue;
            }
            let core = split.is_core(i);
            let (_, crate_name) = &named[i];
            if core {
                v.core_flash_bytes += n.size;
                *core_crates.entry(crate_name.clone()).or_default() += n.size;
                if n.vma >= engine_base {
                    v.core_in_engine.push(named[i].0.clone());
                }
            } else {
                v.engine_flash_bytes += n.size;
                *engine_crates.entry(crate_name.clone()).or_default() += n.size;
                if n.vma < engine_base {
                    v.engine_in_core_region += 1;
                    v.engine_in_core_region_bytes += n.size;
                }
            }
        }
        for (s, targets) in &graph.edges {
            for t in targets {
                match (split.is_core(*s), split.is_core(*t)) {
                    (true, false) => v.core_to_engine_edges += 1,
                    (false, true) => v.engine_to_core_edges += 1,
                    _ => {}
                }
            }
        }
        v.core_by_crate = ranked(core_crates);
        v.engine_by_crate = ranked(engine_crates);
        v
    }

    pub fn passed(&self) -> bool {
        self.core_in_engine.is_empty()
    }

    /// The one line CI and the size check quote.
    pub fn verdict_line(&self) -> String {
        format!(
            "== verify: core nodes in engine region: {}; engine nodes left in core region: {} ({} B)",
            self.core_in_engine.len(),
            self.engine_in_core_region,
            self.engine_in_core_region_bytes
        )
    }

    /// `verify.txt`.
    pub fn report(&self, top: usize) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{}", self.verdict_line());
        for name in self.core_in_engine.iter().take(20) {
            let _ = writeln!(s, "  CORE IN ENGINE REGION: {name}");
        }
        let _ = writeln!(
            s,
            "\nnodes {} · relocations {} ({} unresolved)",
            self.nodes, self.relocs, self.unresolved
        );
        let _ = writeln!(
            s,
            "edges core -> engine: {} (expected 0: the core reads the engine header as data)",
            self.core_to_engine_edges
        );
        let _ = writeln!(
            s,
            "edges engine -> core: {} (expected many: the engine calls the core freely)",
            self.engine_to_core_edges
        );
        let _ = writeln!(
            s,
            "flash: core {} B · engine {} B",
            self.core_flash_bytes, self.engine_flash_bytes
        );
        let _ = writeln!(s, "\n== engine flash by crate ==");
        for (k, v) in self.engine_by_crate.iter().take(top) {
            let _ = writeln!(s, "{v:9} {k}");
        }
        let _ = writeln!(s, "\n== core flash by crate ==");
        for (k, v) in self.core_by_crate.iter().take(top) {
            let _ = writeln!(s, "{v:9} {k}");
        }
        s
    }

    /// Bytes the engine holds of a crate.
    pub fn engine_bytes_of(&self, krate: &str) -> u64 {
        self.engine_by_crate
            .iter()
            .find(|(k, _)| k == krate)
            .map_or(0, |(_, v)| *v)
    }
}

fn ranked(m: HashMap<String, u64>) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// `(name, crate)` per node: the largest symbol at the first symbol address
/// inside the section names it; its crate is the demangled path's first
/// segment, or the C object it came from.
fn name_nodes(
    graph: &SectionGraph,
    names: &BTreeMap<u64, Vec<(u64, String)>>,
) -> Vec<(String, String)> {
    graph
        .nodes
        .iter()
        .map(|n| {
            let (obj, sec) = n.object_and_section();
            let name = names
                .range(n.vma..n.vma + n.size)
                .next()
                .and_then(|(_, cands)| cands.iter().max_by_key(|(size, _)| *size))
                .map_or_else(|| sec.to_string(), |(_, name)| name.clone());
            let krate = if crate::engine_script::is_rust_object(obj) {
                crate_of(&name).unwrap_or("(rust-anon)").to_string()
            } else {
                let file = obj.rsplit('/').next().unwrap_or(obj);
                let file: String = file.chars().take(48).collect();
                format!("(C:{file})")
            };
            (name, krate)
        })
        .collect()
}

/// The crate a demangled path starts in: `lpvm_native::…`,
/// `<lps_glsl::X as core::fmt::Debug>::fmt`, `<&mut dyn foo::Bar …`.
pub fn crate_of(name: &str) -> Option<&str> {
    let mut s = name.trim_start_matches('<');
    for prefix in ["&mut ", "&", "dyn "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    let end = s.find("::")?;
    let krate = &s[..end];
    let mut chars = krate.chars();
    let first = chars.next()?;
    let ok = (first.is_ascii_lowercase() || first == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then_some(krate)
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
            desc: format!("/x/a.cgu.0.rcgu.o:(.s{vma:x})"),
        }
    }

    #[test]
    fn a_core_node_in_the_engine_region_fails_the_verifier() {
        // The entry reaches a node linked inside the engine region: the
        // verifier must refuse it and name it.
        let g = SectionGraph::build(
            vec![
                node(0x4200_0100, ".text"),
                node(0x4240_0010, ".engine_text"),
            ],
            &[(0x4200_0100, 0x4240_0010)],
            0x4200_0100,
        );
        let split = Split::compute(&g, &[]);
        let names = BTreeMap::from([(0x4240_0010, vec![(0x10, "lps_glsl::parse".to_string())])]);
        let v = Verification::run(&g, &split, 0x4240_0000, &names);
        assert!(!v.passed());
        assert_eq!(v.core_in_engine, ["lps_glsl::parse"]);
        assert_eq!(v.core_to_engine_edges, 0, "both ends are core");
        assert!(v.verdict_line().contains("core nodes in engine region: 1;"));
    }

    #[test]
    fn an_engine_only_in_the_engine_region_passes_and_is_attributed() {
        let g = SectionGraph::build(
            vec![
                node(0x4200_0100, ".text"),
                node(0x4240_0000, ".engine_rodata"),
                node(0x4240_0010, ".engine_text"),
            ],
            &[(0x4240_0000, 0x4240_0010), (0x4240_0010, 0x4200_0100)],
            0x4200_0100,
        );
        let split = Split::compute(&g, &[]);
        let names = BTreeMap::from([
            (
                0x4240_0010,
                vec![(0x10, "<lpvm_native::Jit as core::Foo>::go".to_string())],
            ),
            (0x4200_0100, vec![(0x10, "fw_esp32c6::main".to_string())]),
        ]);
        let v = Verification::run(&g, &split, 0x4240_0000, &names);
        assert!(v.passed());
        assert_eq!(v.engine_bytes_of("lpvm_native"), 0x10);
        assert_eq!(v.engine_to_core_edges, 1);
        assert_eq!(v.core_to_engine_edges, 0);
    }

    #[test]
    fn crates_come_from_the_path() {
        assert_eq!(crate_of("lps_glsl::lexer::next"), Some("lps_glsl"));
        assert_eq!(
            crate_of("<lpvm_native::A as core::B>::c"),
            Some("lpvm_native")
        );
        assert_eq!(crate_of("<&mut dyn foo::Bar as x::Y>::z"), Some("foo"));
        assert_eq!(crate_of("memcpy"), None);
    }
}
