//! [`lpvm::LpvmEngine`] / [`lpvm::LpvmModule`] for `wasm32` using `js_sys::WebAssembly`.

use std::cell::RefCell;
use std::collections::HashMap;

use lpir::IrModule;
use lps_builtins::ensure_builtins_referenced;
use lps_shared::LpsModuleSig;
use lpvm::{BumpLpvmMemory, LpvmEngine, LpvmMemory};
use wasm_bindgen::JsValue;

use crate::compile::compile_lpir;
use crate::error::WasmError;
use crate::module::{EnvMemorySpec, WasmExport};
use crate::options::WasmOptions;

use super::instance::BrowserLpvmInstance;

const DEFAULT_LPVM_SHARED_MEMORY_BYTES: usize = 256 * 1024;

thread_local! {
    static HOST_EXPORTS: RefCell<Option<JsValue>> = RefCell::new(None);
}

/// Call once after wasm-bindgen init, passing the embedding module’s `instance.exports`
/// (so `builtins.*` imports resolve to `lps-builtins` symbols linked into the same wasm).
pub fn init_host_exports(exports: JsValue) {
    HOST_EXPORTS.with(|e| *e.borrow_mut() = Some(exports));
    ensure_builtins_referenced();
}

pub(crate) fn host_exports() -> Result<JsValue, WasmError> {
    HOST_EXPORTS.with(|e| {
        e.borrow()
            .clone()
            .ok_or_else(|| WasmError::runtime("init_host_exports not called"))
    })
}

pub struct BrowserLpvmEngine {
    compile_options: WasmOptions,
    shared_memory: BumpLpvmMemory,
}

impl BrowserLpvmEngine {
    pub fn new(compile_options: WasmOptions) -> Self {
        Self {
            compile_options,
            shared_memory: BumpLpvmMemory::new(DEFAULT_LPVM_SHARED_MEMORY_BYTES),
        }
    }
}

pub struct BrowserLpvmModule {
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) env_memory: Option<EnvMemorySpec>,
    pub(crate) signatures: LpsModuleSig,
    pub(crate) exports: HashMap<String, WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
    pub(crate) opts: WasmOptions,
}

impl LpvmEngine for BrowserLpvmEngine {
    type Module = BrowserLpvmModule;
    type Error = WasmError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let artifact = compile_lpir(ir, meta, &self.compile_options)?;
        let wm = artifact.wasm_module();
        let exports: HashMap<_, _> = wm
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        Ok(BrowserLpvmModule {
            wasm_bytes: wm.bytes.clone(),
            env_memory: wm.env_memory,
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: wm.shadow_stack_base,
            opts: self.compile_options,
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.shared_memory
    }
}

impl lpvm::LpvmModule for BrowserLpvmModule {
    type Instance = BrowserLpvmInstance;
    type Error = WasmError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        BrowserLpvmInstance::new(self)
    }
}
