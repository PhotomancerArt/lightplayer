# Phase 2: `JitSymbols` overlay (pure data)

> Read `00-notes.md` and `00-design.md` for shared context.

## Scope of phase

Create the in-memory `JitSymbols` overlay used by `ProfileSession` and the
on-disk JSON serializer. This phase is pure data — no syscall wiring, no
session integration yet. Phase 3 wires it up.

### In scope

- New file `lp-riscv/lp-riscv-emu/src/profile/jit_symbols.rs` with:
  - `pub struct JitSymbols` (owns a `Vec<JitSymbolRecord>`).
  - `pub struct JitSymbolRecord { base_addr, offset, size, name,
    loaded_at_cycle, unloaded_at_cycle }`.
  - `JitSymbols::new() -> Self`.
  - `JitSymbols::add_module(&mut self, base, entries: &[(offset, size,
    name)], cycle)` appends records.
  - `JitSymbols::lookup(&self, pc) -> Option<&str>` — linear scan,
    latest-wins on overlap (later inserts shadow earlier ones).
  - `JitSymbols::to_json(&self) -> serde_json::Value` — array of
    `{addr, size, name, loaded_at_cycle, unloaded_at_cycle}` objects.
- `pub mod jit_symbols;` and a `pub use` in
  `lp-riscv/lp-riscv-emu/src/profile/mod.rs` (so phase 3 can import it).
- Unit tests covering: lookup hit (single module), lookup miss (PC
  outside any module), latest-wins semantics across two modules, JSON
  roundtrip via serde for a small fixture.

### Out of scope

- Owning a `JitSymbols` from `ProfileSession` (phase 3).
- Mutating `meta.json` at finish (phase 3).
- Any syscall handler (phase 3).
- Cycle-keyed lookup (m5 ignores cycle fields at lookup time).

## Code Organization Reminders

- Tests at the **top** in a `mod tests` block (matches `.cursorrules`).
- Public types and methods first; helper functions at the **bottom**.
- One concept per file. Don't sprinkle JIT-symbol logic into other
  modules; it all lives in `jit_symbols.rs`.
- Use builder-style or short helper functions in tests to avoid
  duplication.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** wire `JitSymbols` into `ProfileSession` here — that's phase 3.
- Do **not** suppress warnings.
- Do **not** weaken or skip tests.
- If anything is ambiguous, stop and report.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### File skeleton

`lp-riscv/lp-riscv-emu/src/profile/jit_symbols.rs`:

```rust
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
                        "name": r.name,
                        "loaded_at_cycle": r.loaded_at_cycle,
                        "unloaded_at_cycle": r.unloaded_at_cycle,
                    })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(offset: u32, size: u32, name: &str) -> (u32, u32, String) {
        (offset, size, name.into())
    }

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
}
```

### Module wiring

In `lp-riscv/lp-riscv-emu/src/profile/mod.rs`, add a `pub mod
jit_symbols;` next to the existing `pub mod alloc; pub mod cpu; pub mod
events; pub mod perf_event;`. Add a `pub use jit_symbols::{JitSymbols,
JitSymbolRecord};` next to the existing re-exports near the top of
that file. Do **not** add a field to `ProfileSession` here — phase 3
does that.

### Dependencies

`lp-riscv-emu` already depends on `serde_json`. No `Cargo.toml`
changes expected. If your build complains otherwise, stop and report.

## Validate

```bash
cargo test -p lp-riscv-emu --lib jit_symbols
cargo build -p lp-riscv-emu
```

Both must succeed cleanly with no warnings.
