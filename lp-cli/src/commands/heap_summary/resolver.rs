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
    /// Sorted by addr ascending: (addr, end_addr, full_demangled, display_name)
    symbols: Vec<(u32, u32, String, String)>,
}

impl SymbolResolver {
    /// Load meta.json and build resolver. Symbols are sorted by address.
    pub fn load(meta_path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(meta_path)
            .with_context(|| format!("Failed to read {}", meta_path.display()))?;
        let meta: TraceMetaFile =
            serde_json::from_str(&content).context("Failed to parse meta.json")?;

        let mut symbols: Vec<(u32, u32, String, String)> = meta
            .symbols
            .into_iter()
            .filter(|s| s.size > 0)
            .map(|s| {
                let end = s.addr.saturating_add(s.size);
                let full = Self::demangle_name(&s.name);
                let display = Self::shorten_demangled(&full);
                (s.addr, end, full, display)
            })
            .collect();

        symbols.sort_by_key(|(addr, _, _, _)| *addr);

        Ok(Self { symbols })
    }

    /// Resolve an address to its display (shortened) name.
    pub fn resolve(&self, addr: u32) -> &str {
        self.lookup(addr)
            .map(|(_, display)| display.as_str())
            .unwrap_or("???")
    }

    /// Resolve an address to its full demangled name.
    fn resolve_full(&self, addr: u32) -> &str {
        self.lookup(addr)
            .map(|(full, _)| full.as_str())
            .unwrap_or("???")
    }

    fn lookup(&self, addr: u32) -> Option<(&String, &String)> {
        if self.symbols.is_empty() {
            return None;
        }
        let idx = match self.symbols.binary_search_by_key(&addr, |(a, _, _, _)| *a) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        let (_start, end, full, display) = &self.symbols[idx];
        if addr < *end {
            Some((full, display))
        } else {
            None
        }
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

    /// Check infra status using the full demangled name, which preserves
    /// trait impl structure like `<String as Clone>::clone`.
    fn is_infra(full_name: &str) -> bool {
        const INFRA_FRAGMENTS: &[&str] = &[
            "RawVecInner<",
            "RawVec<",
            "RawTable<",
            "as core::clone::Clone>::clone",
            "as core::fmt::Write>::write",
            "SmallVec<",
            "alloc::vec::Vec<",
            "alloc::string::String>::",
            "alloc::vec::",
            "alloc::string::",
            "hashbrown::",
        ];
        INFRA_FRAGMENTS.iter().any(|p| full_name.contains(p))
    }

    /// Resolve an address, stripping the monomorphization hash suffix.
    pub fn resolve_no_hash(&self, addr: u32) -> String {
        Self::strip_hash(self.resolve(addr))
    }

    /// Strip trailing ::h<hex> hash from a symbol name.
    pub fn strip_hash(name: &str) -> String {
        if let Some(pos) = name.rfind("::h") {
            let suffix = &name[pos + 3..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
                return name[..pos].to_string();
            }
        }
        name.to_string()
    }

    /// Walk frames (skipping frame 0) and return (origin, mechanism):
    /// - origin: first non-infra caller (display name, hash-stripped)
    /// - mechanism: the infra function (display name, hash-stripped), if any
    pub fn classify_alloc(&self, frames: &[u32]) -> (String, Option<String>) {
        let callers = if frames.len() > 1 {
            &frames[1..]
        } else if !frames.is_empty() {
            return (self.resolve_no_hash(frames[0]), None);
        } else {
            return ("???".to_string(), None);
        };

        let mut mechanism: Option<String> = None;
        for &addr in callers {
            let full = self.resolve_full(addr);
            if Self::is_infra(full) {
                if mechanism.is_none() {
                    mechanism = Some(Self::strip_hash(self.resolve(addr)));
                }
            } else {
                return (Self::strip_hash(self.resolve(addr)), mechanism);
            }
        }
        (self.resolve_no_hash(callers[0]), None)
    }

    fn demangle_name(raw: &str) -> String {
        if raw.starts_with("_Z") {
            format!("{}", demangle(raw))
        } else {
            raw.to_string()
        }
    }

    /// Shorten a demangled name for display.
    /// Handles trait impls: `<path::Type as path::Trait>::method` → `Type::method`
    /// Regular paths: take last 2 meaningful components.
    fn shorten_demangled(demangled: &str) -> String {
        if demangled.starts_with('<') {
            if let Some(short) = Self::shorten_trait_impl(demangled) {
                return short;
            }
        }
        Self::shorten_path(demangled)
    }

    /// `<alloc::string::String as core::clone::Clone>::clone::h...`
    /// → `String::clone::h...`
    fn shorten_trait_impl(s: &str) -> Option<String> {
        let close = find_matching_close(s, 0)?;
        let inner = &s[1..close];
        let rest = s[close + 1..].strip_prefix("::")?;

        // Find " as " at bracket depth 0 within `inner`
        let as_pos = find_as_at_depth0(inner)?;
        let self_type = inner[..as_pos].trim();

        let short_self = last_path_component(self_type);
        Some(format!("{short_self}::{rest}"))
    }

    /// Take the last 3 `::` segments, but respect `<>` nesting.
    fn shorten_path(s: &str) -> String {
        let components = split_path(s);
        if components.len() <= 3 {
            return s.to_string();
        }
        components[components.len() - 3..].join("::")
    }
}

/// Find the index of the `>` that matches the `<` at `s[start]`.
fn find_matching_close(s: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s[start..].char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find ` as ` within a string at angle-bracket depth 0.
fn find_as_at_depth0(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = s.as_bytes();
    for i in 0..s.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b' ' if depth == 0 => {
                if s[i..].starts_with(" as ") {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the last path component, preserving generics.
/// `alloc::string::String` → `String`
/// `alloc::vec::Vec<T,A>` → `Vec<T,A>`
fn last_path_component(path: &str) -> &str {
    let mut depth: i32 = 0;
    let bytes = path.as_bytes();
    let mut last_sep = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b':' => {
                last_sep = Some(i);
                i += 1; // skip second ':'
            }
            _ => {}
        }
        i += 1;
    }
    match last_sep {
        Some(pos) => &path[pos + 2..],
        None => path,
    }
}

/// Split a path by `::` respecting `<>` nesting.
fn split_path(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b':' => {
                parts.push(&s[start..i]);
                i += 2;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}
