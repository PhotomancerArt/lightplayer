//! Symbol resolution for heap trace analysis.
//!
//! Loads symbol list from meta.json and resolves instruction addresses to
//! demangled function names for backtrace display.

use anyhow::Context;
use rustc_demangle::demangle;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct TraceMetaFile {
    symbols: Vec<SymbolEntry>,
}

#[derive(Debug, Deserialize)]
struct SymbolEntry {
    addr: u32,
    size: u32,
    name: String,
}

/// Resolves frame addresses to symbol names using a sorted symbol table.
pub struct SymbolResolver {
    /// Sorted by addr ascending: (addr, end_addr, display_name)
    /// end_addr = addr + size for range check
    symbols: Vec<(u32, u32, String)>,
}

impl SymbolResolver {
    /// Load meta.json and build resolver. Symbols are sorted by address.
    pub fn load(meta_path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(meta_path)
            .with_context(|| format!("Failed to read {}", meta_path.display()))?;
        let meta: TraceMetaFile =
            serde_json::from_str(&content).context("Failed to parse meta.json")?;

        let mut symbols: Vec<(u32, u32, String)> = meta
            .symbols
            .into_iter()
            .filter(|s| s.size > 0)
            .map(|s| {
                let end = s.addr.saturating_add(s.size);
                let display = Self::shorten_name(&s.name);
                (s.addr, end, display)
            })
            .collect();

        symbols.sort_by_key(|(addr, _, _)| *addr);

        Ok(Self { symbols })
    }

    /// Resolve an address to its containing symbol.
    /// Returns "???" if not found (e.g. RAM address or outside code).
    pub fn resolve(&self, addr: u32) -> &str {
        if self.symbols.is_empty() {
            return "???";
        }

        let idx = match self.symbols.binary_search_by_key(&addr, |(a, _, _)| *a) {
            Ok(i) => i,
            Err(0) => return "???",
            Err(i) => i - 1,
        };
        let (_start, end, name) = &self.symbols[idx];
        if addr < *end { name.as_str() } else { "???" }
    }

    /// Format a callstack: innermost first, joined by " <- "
    pub fn format_callstack(&self, frames: &[u32], max_frames: usize) -> String {
        let take = frames.len().min(max_frames);
        frames[..take]
            .iter()
            .map(|&addr| self.resolve(addr))
            .collect::<Vec<_>>()
            .join(" <- ")
    }

    /// Walk frames (skipping frame 0 which is the TrackingAllocator) and return
    /// the first caller that isn't a known infrastructure/generic function.
    /// Falls back to the immediate caller if every frame is infra.
    pub fn resolve_past_infra(&self, frames: &[u32]) -> &str {
        let callers = if frames.len() > 1 {
            &frames[1..]
        } else {
            return self.resolve_first(frames);
        };
        for &addr in callers {
            let name = self.resolve(addr);
            if !Self::is_infra(name) {
                return name;
            }
        }
        self.resolve(callers[0])
    }

    fn resolve_first(&self, frames: &[u32]) -> &str {
        if frames.is_empty() {
            "???"
        } else {
            self.resolve(frames[0])
        }
    }

    fn is_infra(name: &str) -> bool {
        const INFRA_PREFIXES: &[&str] = &[
            "RawVecInner<",
            "RawVec<",
            "RawTable<",
            "Clone>::clone",
            "SmallVec<",
            "Vec<",
            "HashMap<",
            "HashSet<",
            "BTreeMap<",
            "BTreeSet<",
            "String>::",
            "Write>::write",
            "alloc::vec::",
            "alloc::string::",
            "hashbrown::",
        ];
        INFRA_PREFIXES.iter().any(|p| name.starts_with(p))
    }

    fn shorten_name(name: &str) -> String {
        let demangled = if name.starts_with("_Z") {
            format!("{}", demangle(name))
        } else {
            name.to_string()
        };

        let parts: Vec<&str> = demangled.split("::").collect();
        if parts.len() <= 3 {
            return demangled;
        }
        parts[parts.len().saturating_sub(3)..].join("::")
    }
}
