//! Q32 pipeline probes: compile a GLSL module of small `probe_*` functions
//! that call the `lpfn_*` builtins, through the normal Q32 pipeline, and run
//! it on the `wasm.q32` host target (the host-side execution path — the
//! rv32n.q32 device path links the identical Rust builtins and is covered by
//! the existing `filetests/lpfn/` corpus).

use anyhow::Context;
use lp_collection::VecMap;
use lp_riscv_emu::{CycleModel, LogLevel};
use lpir::CompilerConfig;
use lps_shared::LpsValueF32;

use crate::targets::Target;
use crate::test_run::execution::execute_function;
use crate::test_run::filetest_lpvm::{CompiledShader, FiletestInstance};

/// Compiled Q32 probe module plus a reusable instance.
pub struct Q32Probes {
    shader: CompiledShader,
    target: &'static Target,
    inst: FiletestInstance,
}

impl Q32Probes {
    /// Compile `probes` (GLSL calling `lpfn_*` builtins) on `wasm.q32`.
    pub fn build(probes: &str) -> anyhow::Result<Self> {
        let target = Target::from_name("wasm.q32").map_err(|e| anyhow::anyhow!(e))?;
        let shader = CompiledShader::compile_glsl(
            probes,
            target,
            LogLevel::None,
            &CompilerConfig::default(),
            &VecMap::new(),
        )?;
        let inst = shader.instantiate()?;
        Ok(Self {
            shader,
            target,
            inst,
        })
    }

    /// Call a probe with f32-typed args (converted to Q32 exactly the way
    /// shader calls are marshalled); returns flattened f32 results.
    pub fn run(&mut self, probe: &str, args: &[LpsValueF32]) -> anyhow::Result<Vec<f32>> {
        let sig = self
            .shader
            .get_function_signature(probe)
            .with_context(|| format!("no probe function {probe}"))?;
        let value = execute_function(
            &mut self.inst,
            self.target,
            sig,
            probe,
            args,
            CycleModel::InstructionCount,
        )?;
        flatten(&value).with_context(|| format!("{probe}: unsupported return value"))
    }
}

fn flatten(v: &LpsValueF32) -> Option<Vec<f32>> {
    Some(match v {
        LpsValueF32::F32(f) => vec![*f],
        LpsValueF32::Vec2(a) => a.to_vec(),
        LpsValueF32::Vec3(a) => a.to_vec(),
        LpsValueF32::Vec4(a) => a.to_vec(),
        _ => return None,
    })
}
