//! [`lpvm::LpvmEngine`] backed by wasmtime.

use std::collections::HashMap;
use std::format;

use lpir::IrModule;
use lps_builtins::ensure_builtins_referenced;
use lps_shared::LpsModuleSig;
use lpvm::LpvmEngine;
use wasmtime::{Engine, Module};

use crate::compile::compile_lpir;
use crate::error::WasmError;
use crate::options::WasmOptions;

use super::instance::WasmLpvmInstance;

/// wasmtime engine; compiles LPIR with fixed [`WasmOptions`].
pub struct WasmLpvmEngine {
    engine: Engine,
    compile_options: WasmOptions,
}

impl WasmLpvmEngine {
    /// New engine (builtins are linked natively from `lps-builtins`).
    pub fn new(compile_options: WasmOptions) -> Result<Self, WasmError> {
        ensure_builtins_referenced();
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|e| WasmError::runtime(format!("failed to create WASM engine: {e}")))?;
        Ok(Self {
            engine,
            compile_options,
        })
    }
}

/// Linked shader module: WASM bytes + metadata, ready to [`LpvmModule::instantiate`].
pub struct WasmLpvmModule {
    pub(crate) engine: Engine,
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) signatures: LpsModuleSig,
    pub(crate) exports: HashMap<String, crate::module::WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
    pub(crate) opts: WasmOptions,
}

impl WasmLpvmModule {
    /// Ensure the shader parses under this engine (validates once at compile time).
    pub(crate) fn validate_shader(engine: &Engine, bytes: &[u8]) -> Result<(), WasmError> {
        Module::new(engine, bytes)
            .map(|_| ())
            .map_err(|e| WasmError::runtime(format!("shader WASM parse/validate failed: {e}")))
    }
}

impl LpvmEngine for WasmLpvmEngine {
    type Module = WasmLpvmModule;
    type Error = WasmError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let artifact = compile_lpir(ir, meta, &self.compile_options)?;
        let bytes = artifact.wasm_module().bytes.clone();
        WasmLpvmModule::validate_shader(&self.engine, &bytes)?;
        let exports: HashMap<_, _> = artifact
            .wasm_module()
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        Ok(WasmLpvmModule {
            engine: self.engine.clone(),
            wasm_bytes: bytes,
            signatures: artifact.signatures().clone(),
            exports,
            shadow_stack_base: artifact.wasm_module().shadow_stack_base,
            opts: self.compile_options,
        })
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
