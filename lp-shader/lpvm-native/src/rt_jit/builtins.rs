//! Builtin symbol table for JIT relocation (filled once, then `O(log n)` lookup).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lps_builtin_ids::BuiltinId;
use lps_builtins::jit_builtin_code_ptr;

/// Maps `extern "C"` symbol name → address for auipc+jalr fixups.
pub struct BuiltinTable {
    symbols: BTreeMap<String, usize>,
}

impl BuiltinTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            symbols: BTreeMap::new(),
        }
    }

    /// Insert all builtins (uses the same symbol names as the ELF link path).
    pub fn populate(&mut self) {
        for bid in BuiltinId::all() {
            let p = jit_builtin_code_ptr(*bid);
            self.symbols.insert(String::from(bid.name()), p as usize);
        }
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<usize> {
        self.symbols.get(name).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// All `(name, addr)` pairs (e.g. for debugging).
    #[must_use]
    pub fn entries(&self) -> Vec<(&str, usize)> {
        self.symbols.iter().map(|(k, v)| (k.as_str(), *v)).collect()
    }
}

impl Default for BuiltinTable {
    fn default() -> Self {
        Self::new()
    }
}
