//! Map guest PCs to ELF symbol names (interval lookup).

use std::borrow::Cow;

use lp_riscv_emu::profile::{PcSymbolizer, TraceSymbol};
use serde::Deserialize;

/// One dynamic (JIT) symbol record from `meta.json` (`dynamic_symbols` array).
#[derive(Debug, Clone, Deserialize)]
pub struct DynamicSymbol {
    pub addr: u64,
    pub size: u64,
    pub name: String,
    #[serde(default)]
    pub loaded_at_cycle: Option<u64>,
    #[serde(default)]
    pub unloaded_at_cycle: Option<u64>,
}

/// Symbol rows from `meta.json` (same shape as [`TraceSymbol`] on the wire).
#[derive(Debug, Clone, Deserialize)]
pub struct TraceSymbolMeta {
    pub name: String,
    pub addr: u32,
    pub size: u32,
}

/// Deserialize fields needed to build a [`Symbolizer`] from a full `meta.json` blob.
/// Unknown top-level fields are ignored by serde.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileTraceMeta {
    #[serde(default)]
    pub symbols: Vec<TraceSymbolMeta>,
    #[serde(default)]
    pub dynamic_symbols: Vec<DynamicSymbol>,
}

impl ProfileTraceMeta {
    /// Builds a symbolizer over static ELF symbols plus JIT `dynamic_symbols` (static wins on overlap).
    pub fn build_symbolizer(&self) -> Symbolizer {
        let syms: Vec<TraceSymbol> = self
            .symbols
            .iter()
            .map(|s| TraceSymbol {
                name: s.name.clone(),
                addr: s.addr,
                size: s.size,
            })
            .collect();
        Symbolizer::new(&syms, &self.dynamic_symbols)
    }
}

/// Deserializes `meta.json` text and returns a [`Symbolizer`].
pub fn symbolizer_from_meta_json_str(json: &str) -> serde_json::Result<Symbolizer> {
    let meta: ProfileTraceMeta = serde_json::from_str(json)?;
    for entry in &meta.dynamic_symbols {
        // Field presence in schema; m5 PC lookup ignores cycle metadata.
        let _ = (entry.loaded_at_cycle, entry.unloaded_at_cycle);
    }
    Ok(meta.build_symbolizer())
}

pub struct Symbolizer {
    static_intervals: Vec<(u32, u32, String)>,
    dynamic_intervals: Vec<(u32, u32, String)>,
}

impl Symbolizer {
    /// PC interval tables: static ELF symbols first in [`Self::lookup`], then JIT `dynamic_symbols`.
    pub fn new(static_symbols: &[TraceSymbol], dynamic_symbols: &[DynamicSymbol]) -> Self {
        Self {
            static_intervals: build_static_intervals(static_symbols),
            dynamic_intervals: build_dynamic_intervals(dynamic_symbols),
        }
    }

    pub fn lookup(&self, pc: u32) -> Cow<'_, str> {
        if pc == 0 {
            return Cow::Borrowed("<root>");
        }
        if let Some(name) = lookup_interval(&self.static_intervals, pc) {
            return Cow::Borrowed(name);
        }
        if let Some(name) = lookup_interval(&self.dynamic_intervals, pc) {
            return Cow::Borrowed(name);
        }
        if pc >= 0x8000_0000 {
            Cow::Owned(format!("<jit:{pc:#010x}>"))
        } else {
            Cow::Owned(format!("<unknown:{pc:#010x}>"))
        }
    }

    /// Lowest address of the ELF or dynamic symbol interval containing `pc`, or `pc` itself if none.
    pub fn entry_lo_for_pc(&self, pc: u32) -> u32 {
        if pc == 0 {
            return 0;
        }
        if let Some(lo) = entry_lo_in_intervals(&self.static_intervals, pc) {
            return lo;
        }
        if let Some(lo) = entry_lo_in_intervals(&self.dynamic_intervals, pc) {
            return lo;
        }
        pc
    }
}

impl PcSymbolizer for Symbolizer {
    fn symbolize(&self, pc: u32) -> Cow<'_, str> {
        self.lookup(pc)
    }

    fn entry_lo_for_pc(&self, pc: u32) -> u32 {
        Symbolizer::entry_lo_for_pc(self, pc)
    }
}

fn build_static_intervals(symbols: &[TraceSymbol]) -> Vec<(u32, u32, String)> {
    let mut v: Vec<_> = symbols
        .iter()
        .filter(|s| s.size > 0)
        .map(|s| {
            let lo = s.addr;
            let hi = s.addr.saturating_add(s.size);
            (lo, hi, s.name.clone())
        })
        .collect();
    v.sort_unstable_by_key(|(lo, _, _)| *lo);
    v
}

fn build_dynamic_intervals(symbols: &[DynamicSymbol]) -> Vec<(u32, u32, String)> {
    let mut v: Vec<_> = symbols
        .iter()
        .filter(|d| d.size > 0)
        .filter_map(|d| {
            let lo = u32::try_from(d.addr).ok()?;
            let size = u32::try_from(d.size).ok()?;
            let hi = lo.saturating_add(size);
            Some((lo, hi, format!("{JIT_DISPLAY_PREFIX}{}", d.name)))
        })
        .collect();
    v.sort_unstable_by_key(|(lo, _, _)| *lo);
    v
}

/// Visual prefix applied to JIT-emitted (dynamic) symbol names so they're
/// distinguishable from static ELF symbols in reports and flame charts.
/// Pre-baked into each dynamic interval's `String` so [`Symbolizer::lookup`]
/// can still return `Cow::Borrowed`.
const JIT_DISPLAY_PREFIX: &str = "[jit] ";

fn lookup_interval<S: AsRef<str>>(intervals: &[(u32, u32, S)], pc: u32) -> Option<&str> {
    if pc == 0 {
        return None;
    }
    let idx = intervals.partition_point(|t| t.0 <= pc).saturating_sub(1);
    if let Some((lo, hi, name)) = intervals.get(idx) {
        if pc >= *lo && pc < *hi {
            return Some(name.as_ref());
        }
    }
    None
}

fn entry_lo_in_intervals<S: AsRef<str>>(intervals: &[(u32, u32, S)], pc: u32) -> Option<u32> {
    if pc == 0 {
        return None;
    }
    let idx = intervals.partition_point(|t| t.0 <= pc).saturating_sub(1);
    if let Some((lo, hi, _)) = intervals.get(idx) {
        if pc >= *lo && pc < *hi {
            return Some(*lo);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(addr: u32, size: u32, name: &str) -> TraceSymbol {
        TraceSymbol {
            addr,
            size,
            name: name.into(),
        }
    }

    fn dyn_sym(addr: u32, size: u32, name: &str) -> DynamicSymbol {
        DynamicSymbol {
            addr: u64::from(addr),
            size: u64::from(size),
            name: name.into(),
            loaded_at_cycle: None,
            unloaded_at_cycle: None,
        }
    }

    fn for_test(static_syms: Vec<TraceSymbol>, dynamic: Vec<DynamicSymbol>) -> Symbolizer {
        Symbolizer::new(&static_syms, &dynamic)
    }

    #[test]
    fn lookup_static_symbol_wins() {
        let s = for_test(
            vec![sym(0x1000, 0x10, "static_fn")],
            vec![dyn_sym(0x1000, 0x10, "dyn_fn")],
        );
        assert_eq!(s.lookup(0x1000).as_ref(), "static_fn");
    }

    #[test]
    fn lookup_dynamic_symbol_resolves_with_jit_prefix() {
        let s = for_test(vec![], vec![dyn_sym(0x8000_0000, 0x40, "palette_warm")]);
        assert_eq!(s.lookup(0x8000_0010).as_ref(), "[jit] palette_warm");
    }

    #[test]
    fn lookup_falls_back_when_unresolved() {
        let s = for_test(vec![], vec![]);
        assert_eq!(s.lookup(0x1234).as_ref(), "<unknown:0x00001234>");
    }

    #[test]
    fn lookup_static_does_not_shadow_disjoint_dynamic() {
        let s = for_test(
            vec![sym(0x1000, 0x10, "static_fn")],
            vec![dyn_sym(0x8000_0000, 0x10, "dyn_fn")],
        );
        assert_eq!(s.lookup(0x8000_0008).as_ref(), "[jit] dyn_fn");
    }

    #[test]
    fn meta_json_static_and_dynamic_lookups() {
        let json = r#"{
            "symbols": [{"name":"rom_fn","addr":4096,"size":32}],
            "dynamic_symbols": [
                {"addr":2147483648,"size":64,"name":"jit_shader","loaded_at_cycle":1,"unloaded_at_cycle":null}
            ]
        }"#;
        let sym = symbolizer_from_meta_json_str(json).expect("parse meta");
        assert_eq!(sym.lookup(0x1000).as_ref(), "rom_fn");
        assert_eq!(sym.lookup(0x8000_0008).as_ref(), "[jit] jit_shader");
    }

    fn fixture() -> Vec<TraceSymbol> {
        vec![
            TraceSymbol {
                addr: 0x1000,
                size: 0x40,
                name: "alpha".into(),
            },
            TraceSymbol {
                addr: 0x1100,
                size: 0x80,
                name: "beta".into(),
            },
            TraceSymbol {
                addr: 0x2000,
                size: 0x10,
                name: "gamma".into(),
            },
        ]
    }

    #[test]
    fn pc_zero_is_root() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0), "<root>");
    }

    #[test]
    fn exact_addr_hit() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x1000).as_ref(), "alpha");
    }

    #[test]
    fn last_byte_hit() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x103f).as_ref(), "alpha");
    }

    #[test]
    fn one_past_end_misses() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x1040).as_ref(), "<unknown:0x00001040>");
    }

    #[test]
    fn between_symbols_misses() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x1080).as_ref(), "<unknown:0x00001080>");
    }

    #[test]
    fn ram_pc_is_jit() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x8000_0000 + 0x1234).as_ref(), "<jit:0x80001234>");
    }

    #[test]
    fn rom_pc_is_unknown() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x500).as_ref(), "<unknown:0x00000500>");
    }

    #[test]
    fn boundary_addr_plus_size_minus_one() {
        let f = fixture();
        let s = Symbolizer::new(&f, &[]);
        assert_eq!(s.lookup(0x2000 + 0x10 - 1).as_ref(), "gamma");
    }
}
