//! LPVM-backed filetest compilation: one module per `.glsl` file, fresh instance per `// run:`.

use lpir::{FloatMode as LpirFloatMode, IrModule};
use lps_shared::{LpsFnSig, LpsModuleSig};
use lpvm::{LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_cranelift::{CompileOptions, CraneliftEngine, CraneliftInstance, CraneliftModule};
use lpvm_emu::{EmuEngine, EmuInstance, EmuModule};
use lpvm_native::{NativeCompileOptions, NativeEmuEngine, NativeEmuInstance, NativeEmuModule};
use lpvm_wasm::{
    WasmOptions as LpvmWasmOptions,
    rt_wasmtime::{WasmLpvmEngine, WasmLpvmInstance, WasmLpvmModule},
};

use crate::targets::{Backend, FloatMode as TargetFloatMode, Target};

/// Compiled artifact for one test file and target.
pub enum CompiledShader {
    /// Host Cranelift JIT (`jit.q32`).
    Jit(CraneliftModule),
    /// Linked RV32 + shared arena via Cranelift (`rv32.q32`).
    Emu(EmuModule),
    /// Linked RV32 + shared arena via native backend (`rv32lp.q32`).
    Native(NativeEmuModule),
    /// wasmtime module (`wasm.q32`).
    Wasm(WasmLpvmModule),
}

/// Per-`// run:` instantiation (mutable VM context / store).
pub enum FiletestInstance {
    /// Host Cranelift JIT instance.
    Jit(CraneliftInstance),
    /// RV32 emulator instance with guest VMContext (Cranelift path).
    Emu(EmuInstance),
    /// RV32 emulator instance with guest VMContext (native path).
    Native(NativeEmuInstance),
    /// wasmtime-linked shader instance.
    Wasm(WasmLpvmInstance),
}

impl CompiledShader {
    pub(crate) fn module_sig(&self) -> &LpsModuleSig {
        match self {
            Self::Jit(m) => m.signatures(),
            Self::Emu(m) => m.signatures(),
            Self::Native(m) => m.signatures(),
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
            Self::Native(m) => {
                FiletestInstance::Native(m.instantiate().map_err(|e| anyhow::anyhow!("{e}"))?)
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
            Self::Native(i) => i.call(name, args).map_err(|e| e.to_string()),
            Self::Wasm(i) => i.call(name, args).map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn call_q32_flat(&mut self, name: &str, flat: &[i32]) -> Result<Vec<i32>, String> {
        match self {
            Self::Jit(i) => i.call_q32(name, flat).map_err(|e| e.to_string()),
            Self::Emu(i) => i.call_q32(name, flat).map_err(|e| e.to_string()),
            Self::Native(i) => i.call_q32(name, flat).map_err(|e| e.to_string()),
            Self::Wasm(i) => i.call_q32(name, flat).map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn debug_state(&self) -> Option<String> {
        match self {
            Self::Jit(_) => None,
            Self::Emu(i) => i.debug_state(),
            Self::Native(i) => i.debug_state(),
            Self::Wasm(_) => None,
        }
    }
}

fn lower_glsl(source: &str) -> anyhow::Result<(IrModule, LpsModuleSig)> {
    let naga = lps_frontend::compile(source).map_err(|e| anyhow::anyhow!("{e}"))?;
    lps_frontend::lower(&naga).map_err(|e| anyhow::anyhow!("{e}"))
}

impl CompiledShader {
    pub(crate) fn compile_glsl(source: &str, target: &Target) -> anyhow::Result<Self> {
        let (ir, meta) = lower_glsl(source)?;
        let fm = match target.float_mode {
            TargetFloatMode::Q32 => LpirFloatMode::Q32,
            TargetFloatMode::F32 => LpirFloatMode::F32,
        };
        let opts = CompileOptions {
            float_mode: fm,
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
            Backend::Rv32lp => {
                let native_opts = NativeCompileOptions {
                    float_mode: fm,
                    ..Default::default()
                };
                let engine = NativeEmuEngine::new(native_opts);
                Ok(Self::Native(engine.compile(&ir, &meta)?))
            }
            Backend::Wasm => {
                let wasm_opts = LpvmWasmOptions { float_mode: fm };
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
}
