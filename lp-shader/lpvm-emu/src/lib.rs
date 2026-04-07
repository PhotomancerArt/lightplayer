//! LPVM implementations on top of [`lp_riscv_emu::Riscv32Emulator`].
//!
//! Shared guest memory for the engine lives at [`lp_riscv_emu::DEFAULT_SHARED_START`]. Use
//! [`EmuEngine`] / [`EmuModule`] / [`EmuInstance`] for the trait API, or
//! [`glsl_q32_call_emulated`] for the legacy Q32 filetest-style entry point.

extern crate alloc;

mod emu_run;
mod engine;
mod instance;
mod memory;
mod module;

pub use emu_run::{glsl_q32_call_emulated, run_loaded_function_i32, run_lpir_function_i32};
pub use engine::EmuEngine;
pub use instance::{EmuInstance, InstanceError};
pub use lpvm_cranelift::{
    CompileOptions, CompilerError, link_object_with_builtins, object_bytes_from_ir,
};
pub use memory::{DEFAULT_SHARED_CAPACITY, EmuSharedArena};
pub use module::EmuModule;

#[cfg(test)]
mod tests {
    use std::string::String;

    use lpir::parse_module;
    use lps_shared::lps_value_f32::LpsValueF32;
    use lps_shared::{FnParam, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
    use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};

    use super::*;

    #[test]
    fn emu_engine_memory_guest_base_aligned() {
        let engine = EmuEngine::new(CompileOptions::default());
        let b = engine.memory().alloc(8, 8).expect("alloc");
        assert_eq!(
            b.guest_base(),
            u64::from(lp_riscv_emu::DEFAULT_SHARED_START),
            "first bump slot should start at shared base"
        );
    }

    #[test]
    #[ignore = "requires lps-builtins-emu-app; build with scripts/build-builtins.sh"]
    fn emu_instance_call_add() {
        let ir = parse_module(
            r"func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
}",
        )
        .expect("parse");
        let meta = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("add"),
                parameters: vec![
                    FnParam {
                        name: String::from("a"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                    FnParam {
                        name: String::from("b"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                ],
                return_type: LpsType::Float,
            }],
        };
        let engine = EmuEngine::new(CompileOptions::default());
        let module = engine
            .compile(&ir, &meta)
            .expect("compile (needs builtins ELF)");
        let mut inst = module.instantiate().expect("instantiate");
        let v = inst
            .call("add", &[LpsValueF32::F32(1.0), LpsValueF32::F32(2.0)])
            .expect("call");
        match v {
            LpsValueF32::F32(x) => assert!((x - 3.0).abs() < 1e-3),
            _ => panic!("expected f32"),
        }
    }
}
