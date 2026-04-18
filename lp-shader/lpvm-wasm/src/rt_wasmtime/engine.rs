//! [`lpvm::LpvmEngine`] backed by wasmtime.

use std::collections::HashMap;
use std::format;
use std::sync::Arc;

use lpir::LpirModule;
use lps_builtins::ensure_builtins_referenced;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory};
use wasmtime::{Engine, Module};

use crate::compile::compile_lpir;
use crate::error::WasmError;
use crate::module::EnvMemorySpec;
use crate::options::WasmOptions;

use super::instance::WasmLpvmInstance;
use super::link::shader_env_memory_minimum_pages;
use super::{WasmLpvmSharedRuntime, WasmtimeLpvmMemory};

/// wasmtime engine; compiles LPIR with fixed [`WasmOptions`].
pub struct WasmLpvmEngine {
    engine: Engine,
    compile_options: WasmOptions,
    runtime: Arc<WasmLpvmSharedRuntime>,
    memory: WasmtimeLpvmMemory,
}

impl WasmLpvmEngine {
    /// New engine (builtins are linked natively from `lps-builtins`).
    pub fn new(compile_options: WasmOptions) -> Result<Self, WasmError> {
        ensure_builtins_referenced();
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|e| WasmError::runtime(format!("failed to create WASM engine: {e}")))?;
        let runtime = WasmLpvmSharedRuntime::new(&engine)?;
        let memory = WasmtimeLpvmMemory::new(Arc::clone(&runtime));
        Ok(Self {
            engine,
            compile_options,
            runtime,
            memory,
        })
    }
}

/// Linked shader module: WASM bytes + metadata, ready to [`LpvmModule::instantiate`].
#[derive(Clone)]
pub struct WasmLpvmModule {
    pub(crate) engine: Engine,
    pub(crate) runtime: Arc<WasmLpvmSharedRuntime>,
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) signatures: LpsModuleSig,
    pub(crate) exports: HashMap<String, crate::module::WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
    pub(crate) opts: WasmOptions,
    pub(crate) lpir: LpirModule,
}

impl WasmLpvmModule {
    /// Ensure the shader parses under this engine (validates once at compile time).
    pub(crate) fn validate_shader(engine: &Engine, bytes: &[u8]) -> Result<(), WasmError> {
        Module::new(engine, bytes)
            .map(|_| ())
            .map_err(|e| WasmError::runtime(format!("shader WASM parse/validate failed: {e}")))
    }

    pub(crate) fn validate_memory_import(engine: &Engine, bytes: &[u8]) -> Result<(), WasmError> {
        let m = Module::new(engine, bytes)
            .map_err(|e| WasmError::runtime(format!("shader WASM parse (memory check): {e}")))?;
        if let Some(min) = shader_env_memory_minimum_pages(&m) {
            let need = u32::try_from(min).unwrap_or(u32::MAX);
            let have = EnvMemorySpec::engine_initial_for_host().initial_pages;
            if need > have {
                return Err(WasmError::runtime(format!(
                    "shader env.memory import requires minimum {need} pages; engine has {have}"
                )));
            }
        }
        Ok(())
    }
}

impl LpvmEngine for WasmLpvmEngine {
    type Module = WasmLpvmModule;
    type Error = WasmError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let artifact = compile_lpir(ir, meta, &self.compile_options)?;
        let bytes = artifact.wasm_module().bytes.clone();
        WasmLpvmModule::validate_shader(&self.engine, &bytes)?;
        WasmLpvmModule::validate_memory_import(&self.engine, &bytes)?;
        let exports: HashMap<_, _> = artifact
            .wasm_module()
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        Ok(WasmLpvmModule {
            engine: self.engine.clone(),
            runtime: Arc::clone(&self.runtime),
            wasm_bytes: bytes,
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: artifact.wasm_module().shadow_stack_base,
            opts: self.compile_options.clone(),
            lpir: ir.clone(),
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}

impl lpvm::LpvmModule for WasmLpvmModule {
    type Instance = WasmLpvmInstance;
    type Error = WasmError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        WasmLpvmInstance::new(self)
    }
}
