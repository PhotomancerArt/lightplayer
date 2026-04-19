//! LPVM-backed filetest compilation: one module per `.glsl` file, fresh instance per `// run:`.

use lp_riscv_emu::{CycleModel, LogLevel};
use lpir::{CompilerConfig, FloatMode as LpirFloatMode, LpirModule};
use lps_shared::{LpsFnSig, LpsModuleSig};
use lpvm::{LpsValueF32, LpsValueQ32, LpvmEngine, LpvmInstance, LpvmModule, ModuleDebugInfo};
use lpvm_cranelift::{CompileOptions, CraneliftEngine, CraneliftInstance, CraneliftModule};
use lpvm_emu::{EmuEngine, EmuInstance, EmuModule};
use lpvm_native::{
    NativeCompileOptions as FaCompileOptions, NativeEmuEngine as FaEmuEngine,
    NativeEmuInstance as FaEmuInstance, NativeEmuModule as FaEmuModule,
};
use lpvm_wasm::{
    WasmOptions as LpvmWasmOptions,
    rt_wasmtime::{WasmLpvmEngine, WasmLpvmInstance, WasmLpvmModule},
};

use crate::targets::{Backend, FloatMode as TargetFloatMode, Target};

/// Compiled artifact for one test file and target.
pub enum CompiledShader {
    /// Host Cranelift JIT (`jit.q32`).
    Jit(CraneliftModule),
    /// Linked RV32 + shared arena via Cranelift (`rv32c.q32`).
    Emu(EmuModule),
    /// Linked RV32 + shared arena via `lpvm-native` (`rv32n.q32`).
    NativeFa(FaEmuModule),
    /// wasmtime module (`wasm.q32`).
    Wasm(WasmLpvmModule),
}

/// Per-`// run:` instantiation (mutable VM context / store).
pub enum FiletestInstance {
    /// Host Cranelift JIT instance.
    Jit(CraneliftInstance),
    /// RV32 emulator instance with guest VMContext (Cranelift path).
    Emu(EmuInstance),
    /// RV32 emulator instance with guest VMContext (`lpvm-native` path).
    NativeFa(FaEmuInstance),
    /// wasmtime-linked shader instance.
    Wasm(WasmLpvmInstance),
}

impl CompiledShader {
    pub(crate) fn module_sig(&self) -> &LpsModuleSig {
        match self {
            Self::Jit(m) => m.signatures(),
            Self::Emu(m) => m.signatures(),
            Self::NativeFa(m) => m.signatures(),
            Self::Wasm(m) => m.signatures(),
        }
    }

    pub(crate) fn instantiate(&self) -> anyhow::Result<FiletestInstance> {
        Ok(match self {
            Self::Jit(m) => {
                FiletestInstance::Jit(m.instantiate().map_err(|e| anyhow::anyhow!("{e}"))?)
            }
            Self::Emu(m) => {
                FiletestInstance::Emu(m.instantiate().map_err(|e| anyhow::anyhow!("{e}"))?)
            }
            Self::NativeFa(m) => {
                FiletestInstance::NativeFa(m.instantiate().map_err(|e| anyhow::anyhow!("{e}"))?)
            }
            Self::Wasm(m) => {
                FiletestInstance::Wasm(m.instantiate().map_err(|e| anyhow::anyhow!("{e}"))?)
            }
        })
    }
}

impl FiletestInstance {
    pub(crate) fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, String> {
        match self {
            Self::Jit(i) => i.call(name, args).map_err(|e| e.to_string()),
            Self::Emu(i) => i.call(name, args).map_err(|e| e.to_string()),
            Self::NativeFa(i) => i.call(name, args).map_err(|e| e.to_string()),
            Self::Wasm(i) => i.call(name, args).map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn call_q32_flat(
        &mut self,
        name: &str,
        flat: &[i32],
        cycle_model: CycleModel,
    ) -> Result<Vec<i32>, String> {
        match self {
            Self::Jit(i) => i.call_q32(name, flat).map_err(|e| e.to_string()),
            Self::Emu(i) => i
                .call_q32_with_cycle_model(name, flat, cycle_model)
                .map_err(|e| e.to_string()),
            Self::NativeFa(i) => i
                .call_q32_with_cycle_model(name, flat, cycle_model)
                .map_err(|e| e.to_string()),
            Self::Wasm(i) => i.call_q32(name, flat).map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), String> {
        match self {
            Self::Jit(i) => i.set_uniform(path, value).map_err(|e| e.to_string()),
            Self::Emu(i) => i.set_uniform(path, value).map_err(|e| e.to_string()),
            Self::NativeFa(i) => i.set_uniform(path, value).map_err(|e| e.to_string()),
            Self::Wasm(i) => i.set_uniform(path, value).map_err(|e| e.to_string()),
        }
    }

    /// Pre-encoded Q32 uniforms (filetests use [`Self::set_uniform`]; this mirrors `LpvmInstance`).
    #[allow(
        dead_code,
        reason = "mirrors LpvmInstance::set_uniform_q32; filetests use f32 set_uniform only"
    )]
    pub(crate) fn set_uniform_q32(
        &mut self,
        path: &str,
        value: &LpsValueQ32,
    ) -> Result<(), String> {
        match self {
            Self::Jit(i) => i.set_uniform_q32(path, value).map_err(|e| e.to_string()),
            Self::Emu(i) => i.set_uniform_q32(path, value).map_err(|e| e.to_string()),
            Self::NativeFa(i) => i.set_uniform_q32(path, value).map_err(|e| e.to_string()),
            Self::Wasm(i) => i.set_uniform_q32(path, value).map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn debug_state(&self) -> Option<String> {
        match self {
            Self::Jit(_) => None,
            Self::Emu(i) => i.debug_state(),
            Self::NativeFa(i) => i.debug_state(),
            Self::Wasm(_) => None,
        }
    }

    pub(crate) fn last_guest_instruction_count(&self) -> Option<u64> {
        match self {
            Self::Jit(i) => i.last_guest_instruction_count(),
            Self::Emu(i) => i.last_guest_instruction_count(),
            Self::NativeFa(i) => i.last_guest_instruction_count(),
            Self::Wasm(i) => i.last_guest_instruction_count(),
        }
    }

    pub(crate) fn last_guest_cycle_count(&self) -> Option<u64> {
        match self {
            Self::Jit(i) => i.last_guest_cycle_count(),
            Self::Emu(i) => i.last_guest_cycle_count(),
            Self::NativeFa(i) => i.last_guest_cycle_count(),
            Self::Wasm(i) => i.last_guest_cycle_count(),
        }
    }
}

fn lower_glsl(source: &str) -> anyhow::Result<(LpirModule, LpsModuleSig)> {
    let naga = lps_frontend::compile(source).map_err(|e| anyhow::anyhow!("{e}"))?;
    lps_frontend::lower(&naga).map_err(|e| anyhow::anyhow!("{e}"))
}

impl CompiledShader {
    pub(crate) fn compile_glsl(
        source: &str,
        target: &Target,
        emu_log_level: LogLevel,
        compiler_config: &CompilerConfig,
    ) -> anyhow::Result<Self> {
        let (ir, meta) = lower_glsl(source)?;
        let fm = match target.float_mode {
            TargetFloatMode::Q32 => LpirFloatMode::Q32,
            TargetFloatMode::F32 => LpirFloatMode::F32,
        };
        let opts = CompileOptions {
            float_mode: fm,
            emu_trace_instructions: emu_log_level == LogLevel::Instructions,
            config: compiler_config.clone(),
            ..Default::default()
        };
        match target.backend {
            Backend::Jit => {
                let engine = CraneliftEngine::new(opts);
                Ok(Self::Jit(engine.compile(&ir, &meta)?))
            }
            Backend::Rv32 => {
                let engine = EmuEngine::new(opts);
                Ok(Self::Emu(engine.compile(&ir, &meta)?))
            }
            Backend::Rv32fa => {
                let alloc_trace = std::env::var("LPVM_ALLOC_TRACE").unwrap_or_default() == "1";
                let native_opts = FaCompileOptions {
                    float_mode: fm,
                    emu_trace_instructions: opts.emu_trace_instructions,
                    alloc_trace,
                    config: compiler_config.clone(),
                    ..Default::default()
                };
                let engine = FaEmuEngine::new(native_opts);
                Ok(Self::NativeFa(engine.compile(&ir, &meta)?))
            }
            Backend::Wasm => {
                let wasm_opts = LpvmWasmOptions {
                    float_mode: fm,
                    config: compiler_config.clone(),
                    ..Default::default()
                };
                let engine = WasmLpvmEngine::new(wasm_opts).map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(Self::Wasm(engine.compile(&ir, &meta)?))
            }
        }
    }
}

impl CompiledShader {
    pub(crate) fn get_function_signature(&self, name: &str) -> Option<&LpsFnSig> {
        self.module_sig().functions.iter().find(|f| f.name == name)
    }

    /// Get structured debug info for the compiled module.
    /// Returns None if the backend doesn't provide debug info.
    pub(crate) fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        match self {
            Self::Jit(_) => None,
            Self::Emu(m) => m.debug_info(),
            Self::NativeFa(m) => m.debug_info(),
            Self::Wasm(_) => None,
        }
    }

    pub(crate) fn lpir_module(&self) -> Option<&LpirModule> {
        match self {
            Self::Emu(m) => m.lpir_module(),
            Self::NativeFa(m) => m.lpir_module(),
            Self::Jit(_) | Self::Wasm(_) => None,
        }
    }
}
