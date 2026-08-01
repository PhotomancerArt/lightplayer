//! Builtin symbol table for JIT relocation (filled once, then `O(log n)` lookup).

use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lps_builtin_ids::BuiltinId;
use lps_builtins::jit_builtin_code_ptr;

/// Maps `extern "C"` symbol name → address for auipc+jalr fixups.
pub struct BuiltinTable {
    symbols: VecMap<String, usize>,
}

impl BuiltinTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            symbols: VecMap::new(),
        }
    }

    /// Insert every builtin this image actually links (same symbol names as the
    /// ELF link path).
    ///
    /// Builtins behind a disabled feature — the native-f32 family without
    /// `float-f32` — have no address and are skipped. A shader that genuinely
    /// needs one then fails relocation with a named "unknown builtin symbol",
    /// which is diagnosable; the alternative of asking for the address anyway
    /// aborts a Fixed-only firmware image at boot, because this loop walks
    /// every id.
    pub fn populate(&mut self) {
        for bid in BuiltinId::all() {
            if let Some(p) = jit_builtin_code_ptr(*bid) {
                self.symbols.insert(String::from(bid.name()), p as usize);
            }
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
