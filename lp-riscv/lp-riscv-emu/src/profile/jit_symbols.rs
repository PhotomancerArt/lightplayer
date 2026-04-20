//! JIT-emitted symbol overlay.
//!
//! Populated by the host `SYSCALL_JIT_MAP_LOAD` handler (see
//! `lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs`). Consulted at
//! report time by the lp-cli profile symbolizer and the alloc-trace
//! `SymbolResolver`.
//!
//! m5 lookup is a linear scan with latest-wins semantics on PC overlap.
//! The cycle metadata (`loaded_at_cycle`, `unloaded_at_cycle`) is
//! recorded but not yet consulted at lookup time — reserved for future
//! timeline-aware lookup / `SYSCALL_JIT_MAP_UNLOAD` wiring.

use ::alloc::string::String;
use ::alloc::vec::Vec;
use serde_json::{Value, json};

#[cfg(test)]
mod tests {
    use super::*;
    use ::alloc::vec;
    use serde::Deserialize;

    #[test]
    fn lookup_within_module_returns_name() {
        let mut s = JitSymbols::new();
        s.add_module(
            0x8000_0000,
            &[entry(0, 0x40, "alpha"), entry(0x40, 0x20, "beta")],
            0,
        );
        assert_eq!(s.lookup(0x8000_0000), Some("alpha"));
        assert_eq!(s.lookup(0x8000_003f), Some("alpha"));
        assert_eq!(s.lookup(0x8000_0040), Some("beta"));
        assert_eq!(s.lookup(0x8000_005f), Some("beta"));
    }

    #[test]
    fn lookup_outside_any_module_returns_none() {
        let mut s = JitSymbols::new();
        s.add_module(0x8000_0000, &[entry(0, 0x10, "alpha")], 0);
        assert_eq!(s.lookup(0x8000_0010), None);
        assert_eq!(s.lookup(0x7fff_ffff), None);
    }

    #[test]
    fn latest_module_wins_on_overlap() {
        let mut s = JitSymbols::new();
        s.add_module(0x8000_0000, &[entry(0, 0x10, "old")], 1);
        s.add_module(0x8000_0000, &[entry(0, 0x10, "new")], 2);
        assert_eq!(s.lookup(0x8000_0000), Some("new"));
    }

    #[test]
    fn to_json_matches_schema() {
        let mut s = JitSymbols::new();
        s.add_module(0x8000_0000, &[entry(0x10, 0x20, "palette_warm")], 12_345);
        let v = s.to_json();
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        let obj = &arr[0];
        assert_eq!(obj["addr"].as_u64(), Some(0x8000_0010));
        assert_eq!(obj["size"].as_u64(), Some(0x20));
        assert_eq!(obj["name"].as_str(), Some("palette_warm"));
        assert_eq!(obj["loaded_at_cycle"].as_u64(), Some(12_345));
        assert!(obj["unloaded_at_cycle"].is_null());
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct JsonFixtureRow {
        addr: u64,
        size: u64,
        name: String,
        loaded_at_cycle: u64,
        unloaded_at_cycle: Option<u64>,
    }

    #[test]
    fn json_roundtrip_via_serde_fixture() {
        let mut s = JitSymbols::new();
        s.add_module(0x8000_0000, &[entry(0, 16, "a"), entry(16, 8, "b")], 99);
        let v = s.to_json();
        let parsed: Vec<JsonFixtureRow> =
            serde_json::from_value(v).expect("deserialize dynamic_symbols JSON");
        assert_eq!(
            parsed,
            vec![
                JsonFixtureRow {
                    addr: 0x8000_0000,
                    size: 16,
                    name: "a".into(),
                    loaded_at_cycle: 99,
                    unloaded_at_cycle: None,
                },
                JsonFixtureRow {
                    addr: 0x8000_0010,
                    size: 8,
                    name: "b".into(),
                    loaded_at_cycle: 99,
                    unloaded_at_cycle: None,
                },
            ]
        );
    }

    fn entry(offset: u32, size: u32, name: &str) -> (u32, u32, String) {
        (offset, size, name.into())
    }
}

#[derive(Debug, Default)]
pub struct JitSymbols {
    entries: Vec<JitSymbolRecord>,
}

#[derive(Debug, Clone)]
pub struct JitSymbolRecord {
    pub base_addr: u32,
    pub offset: u32,
    pub size: u32,
    pub name: String,
    pub loaded_at_cycle: u64,
    /// Always `None` in m5 (UNLOAD deferred). Carried for forward compat.
    pub unloaded_at_cycle: Option<u64>,
}

impl JitSymbols {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append all functions of one freshly-loaded JIT module.
    ///
    /// `entries` is `(offset_within_module, size_bytes, name)`.
    pub fn add_module(&mut self, base: u32, entries: &[(u32, u32, String)], cycle: u64) {
        self.entries.reserve(entries.len());
        for (offset, size, name) in entries {
            self.entries.push(JitSymbolRecord {
                base_addr: base,
                offset: *offset,
                size: *size,
                name: name.clone(),
                loaded_at_cycle: cycle,
                unloaded_at_cycle: None,
            });
        }
    }

    /// Linear scan; latest-inserted match wins on PC overlap.
    pub fn lookup(&self, pc: u32) -> Option<&str> {
        for record in self.entries.iter().rev() {
            let lo = record.base_addr.wrapping_add(record.offset);
            let hi = lo.wrapping_add(record.size);
            if pc >= lo && pc < hi {
                return Some(record.name.as_str());
            }
        }
        None
    }

    /// All recorded records (used by tests; phase 3 uses this for
    /// `meta.json` serialization via [`Self::to_json`]).
    pub fn records(&self) -> &[JitSymbolRecord] {
        &self.entries
    }

    /// Render as a JSON array suitable for the `dynamic_symbols` field
    /// in `meta.json`. Schema:
    ///
    /// ```json
    /// [{ "addr": 0x..., "size": 124, "name": "palette_warm",
    ///    "loaded_at_cycle": 12345, "unloaded_at_cycle": null }]
    /// ```
    pub fn to_json(&self) -> Value {
        Value::Array(
            self.entries
                .iter()
                .map(|r| {
                    json!({
                        "addr": r.base_addr.wrapping_add(r.offset),
                        "size": r.size,
                        "name": &r.name,
                        "loaded_at_cycle": r.loaded_at_cycle,
                        "unloaded_at_cycle": r.unloaded_at_cycle,
                    })
                })
                .collect(),
        )
    }
}
