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
        #[cfg(all(feature = "float-f32", target_arch = "riscv32"))]
        self.populate_soft_float();
    }

    /// Add the platform soft-float symbols the f32 lowering calls directly.
    ///
    /// These are not LightPlayer builtins and are not in [`BuiltinId`] — that is
    /// the point of roadmap D1. The addresses come from ordinary `extern "C"`
    /// declarations, so the *linker* answers where they live: on the ESP32-C6
    /// that is the chip's ROM `rvfplib` (`esp32c6.rom.rvfp.ld`, e.g.
    /// `__addsf3 = 0x400009f8`), and in a host-linked rv32 image it is Rust's
    /// `compiler_builtins`. Either way the JIT gets a real address and the
    /// firmware pays no flash for the implementation.
    ///
    /// rv32-only: Xtensa answers [`crate::isa::F32Lowering::Unsupported`], so
    /// nothing there can emit a call to one of these names. Adding the arm when
    /// an Xtensa float backend lands is a deliberate decision (the S3 has an
    /// FPU; soft float would be the wrong answer for it), not an oversight.
    #[cfg(all(feature = "float-f32", target_arch = "riscv32"))]
    fn populate_soft_float(&mut self) {
        // SAFETY (declaration only): these are the standard soft-float ABI
        // entry points. Nothing here calls them — the table stores addresses for
        // the JIT's auipc+jalr fixups, and the *generated* code performs the
        // calls with the ABI the lowering emitted.
        unsafe extern "C" {
            fn __addsf3(a: f32, b: f32) -> f32;
            fn __subsf3(a: f32, b: f32) -> f32;
            fn __mulsf3(a: f32, b: f32) -> f32;
            fn __divsf3(a: f32, b: f32) -> f32;
            fn __eqsf2(a: f32, b: f32) -> i32;
            fn __nesf2(a: f32, b: f32) -> i32;
            fn __ltsf2(a: f32, b: f32) -> i32;
            fn __lesf2(a: f32, b: f32) -> i32;
            fn __gtsf2(a: f32, b: f32) -> i32;
            fn __gesf2(a: f32, b: f32) -> i32;
            fn __floatsisf(a: i32) -> f32;
            fn __floatunsisf(a: u32) -> f32;
        }

        let entries: [(&str, usize); 12] = [
            ("__addsf3", __addsf3 as *const () as usize),
            ("__subsf3", __subsf3 as *const () as usize),
            ("__mulsf3", __mulsf3 as *const () as usize),
            ("__divsf3", __divsf3 as *const () as usize),
            ("__eqsf2", __eqsf2 as *const () as usize),
            ("__nesf2", __nesf2 as *const () as usize),
            ("__ltsf2", __ltsf2 as *const () as usize),
            ("__lesf2", __lesf2 as *const () as usize),
            ("__gtsf2", __gtsf2 as *const () as usize),
            ("__gesf2", __gesf2 as *const () as usize),
            ("__floatsisf", __floatsisf as *const () as usize),
            ("__floatunsisf", __floatunsisf as *const () as usize),
        ];
        for (name, addr) in entries {
            self.symbols.insert(String::from(name), addr);
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
