//! LPVM backend selection by target architecture.
//!
//! | Target                                  | Backend                            |
//! |-----------------------------------------|------------------------------------|
//! | `cfg(target_arch = "riscv32")`          | `lpvm-native::rt_jit`              |
//! | `cfg(target_arch = "wasm32")`           | `lpvm-wasm::rt_browser`            |
//! | catchall (host)                         | `lpvm-wasm::rt_wasmtime`           |
//!
//! Picked at compile time. There is no Cargo feature for selecting
//! a backend; the dep blocks in `Cargo.toml` already gate which
//! crate is in scope. `LpvmBackend` is the type alias users see;
//! `new_backend()` is the constructor. Both are crate-internal.

#[cfg(target_arch = "riscv32")]
mod imp {
    use alloc::sync::Arc;

    use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

    pub type LpvmBackend = NativeJitEngine;

    pub fn new_backend() -> LpvmBackend {
        lps_builtins::ensure_builtins_referenced();
        let mut table = BuiltinTable::new();
        table.populate();
        NativeJitEngine::new(Arc::new(table), NativeCompileOptions::default())
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use lpvm_wasm::WasmOptions;
    use lpvm_wasm::rt_browser::BrowserLpvmEngine;

    pub type LpvmBackend = BrowserLpvmEngine;

    pub fn new_backend() -> LpvmBackend {
        BrowserLpvmEngine::new(WasmOptions::default())
            .expect("BrowserLpvmEngine::new with default WasmOptions")
    }
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
mod imp {
    use lpvm_wasm::WasmOptions;
    use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

    pub type LpvmBackend = WasmLpvmEngine;

    pub fn new_backend() -> LpvmBackend {
        WasmLpvmEngine::new(WasmOptions::default())
            .expect("WasmLpvmEngine::new with default WasmOptions")
    }
}

pub(crate) use imp::{LpvmBackend, new_backend};
