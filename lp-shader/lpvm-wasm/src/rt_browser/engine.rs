//! [`lpvm::LpvmEngine`] / [`lpvm::LpvmModule`] for `wasm32` using `js_sys::WebAssembly`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use lpir::LpirModule;
use lps_builtins::ensure_builtins_referenced;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory};
use wasm_bindgen::JsValue;

use crate::compile::compile_lpir;
use crate::error::WasmError;
use crate::module::{EnvMemorySpec, WasmExport};
use crate::options::WasmOptions;

use super::instance::BrowserLpvmInstance;
use super::shared_runtime::{BrowserLpvmMemory, BrowserLpvmSharedRuntime};

thread_local! {
    static HOST_EXPORTS: RefCell<Option<JsValue>> = RefCell::new(None);
}

/// Call once after wasm-bindgen init, passing the embedding module's `instance.exports`
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
    runtime: Arc<BrowserLpvmSharedRuntime>,
    memory: BrowserLpvmMemory,
}

impl BrowserLpvmEngine {
    pub fn new(compile_options: WasmOptions) -> Result<Self, WasmError> {
        let runtime = BrowserLpvmSharedRuntime::new()?;
        let memory = BrowserLpvmMemory::new();
        Ok(Self {
            compile_options,
            runtime,
            memory,
        })
    }
}

pub struct BrowserLpvmModule {
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) env_memory: Option<EnvMemorySpec>,
    pub(crate) runtime: Arc<BrowserLpvmSharedRuntime>,
    pub(crate) signatures: LpsModuleSig,
    pub(crate) exports: HashMap<String, WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
    pub(crate) opts: WasmOptions,
}

impl LpvmEngine for BrowserLpvmEngine {
    type Module = BrowserLpvmModule;
    type Error = WasmError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let artifact = compile_lpir(ir, meta, &self.compile_options)?;
        let wm = artifact.wasm_module();
        if let Some(spec) = &wm.env_memory {
            let engine_spec = EnvMemorySpec::engine_initial_for_host();
            if spec.initial_pages > engine_spec.initial_pages {
                return Err(WasmError::runtime(format!(
                    "shader env.memory import requires minimum {} pages; engine has {}",
                    spec.initial_pages, engine_spec.initial_pages
                )));
            }
        }
        let exports: HashMap<_, _> = wm
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        Ok(BrowserLpvmModule {
            wasm_bytes: wm.bytes.clone(),
            env_memory: wm.env_memory,
            runtime: Arc::clone(&self.runtime),
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: wm.shadow_stack_base,
            opts: self.compile_options.clone(),
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
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
