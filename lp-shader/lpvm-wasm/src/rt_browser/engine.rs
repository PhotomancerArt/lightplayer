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

    /// The one emit path both `compile` doors share; `opts.float_mode` is
    /// recorded on the module so the instance never has to ask the engine.
    fn compile_with_options(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        opts: WasmOptions,
    ) -> Result<BrowserLpvmModule, WasmError> {
        let artifact = compile_lpir(ir, meta, &opts)?;
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
            wasm_inst_count: wm.inst_count,
            env_memory: wm.env_memory,
            runtime: Arc::clone(&self.runtime),
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: wm.shadow_stack_base,
            opts,
            lpir: ir.clone(),
        })
    }
}

/// Compiled shader module: WASM bytes + metadata, ready to instantiate.
pub struct BrowserLpvmModule {
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) wasm_inst_count: usize,
    pub(crate) env_memory: Option<EnvMemorySpec>,
    pub(crate) runtime: Arc<BrowserLpvmSharedRuntime>,
    pub(crate) signatures: LpsModuleSig,
    pub(crate) exports: HashMap<String, WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
    pub(crate) opts: WasmOptions,
    pub(crate) lpir: LpirModule,
}

impl LpvmEngine for BrowserLpvmEngine {
    type Module = BrowserLpvmModule;
    type Error = WasmError;

    /// Both modes, per compile — not "the mode I was built with".
    ///
    /// Same emitter as `rt_wasmtime`, so the capability is the same; the
    /// runtime differs only in *which* wasm engine executes the bytes.
    /// [`Self::compile_with_params`] below honours the request, and the
    /// instance reads its mode off the compiled module's `opts`.
    fn supports_float_mode(&self, mode: lpir::FloatMode) -> bool {
        matches!(mode, lpir::FloatMode::Q32 | lpir::FloatMode::F32)
    }

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        self.compile_with_options(ir, meta, self.compile_options.clone())
    }

    fn compile_with_params(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        params: &lpvm::LpvmCompileParams,
    ) -> Result<Self::Module, Self::Error> {
        let mut opts = self.compile_options.clone();
        opts.config = params.config.clone();
        opts.float_mode = params.float_mode;
        self.compile_with_options(ir, meta, opts)
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

    /// See `rt_wasmtime`'s copy: the module's own `opts` answer, and the
    /// browser's wasm JIT is hardware FP just as wasmtime's is.
    fn float_impl(&self) -> lpvm::FloatImpl {
        match self.opts.float_mode {
            lpir::FloatMode::Q32 => lpvm::FloatImpl::Fixed,
            lpir::FloatMode::F32 => lpvm::FloatImpl::HardwareF32,
        }
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        BrowserLpvmInstance::new(self)
    }

    fn lpir_module(&self) -> Option<&LpirModule> {
        Some(&self.lpir)
    }

    fn code_size_bytes(&self) -> Option<usize> {
        Some(self.wasm_bytes.len())
    }

    fn final_instruction_count(&self) -> Option<usize> {
        Some(self.wasm_inst_count)
    }
}
