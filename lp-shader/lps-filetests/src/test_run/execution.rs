//! Shared execution logic for filetests (`execute_function` / `execute_render`).

use anyhow::Result;

use lps_shared::{LpsFnSig, LpsType};
use lpvm::{LpsValueF32, decode_q32_return, flat_q32_words_from_f32_args, q32_to_lps_value_f32};

use crate::targets::{FloatMode as TargetFloatMode, Target};
use crate::test_run::filetest_lpvm::FiletestInstance;
use lp_riscv_emu::CycleModel;

/// Execute a function by name with arguments and return the result as a [`LpsValueF32`].
pub fn execute_function(
    inst: &mut FiletestInstance,
    target: &Target,
    gfn: &LpsFnSig,
    name: &str,
    args: &[LpsValueF32],
    cycle_model: CycleModel,
) -> Result<LpsValueF32> {
    let return_ty = gfn.return_type.clone();

    let mut inner = || -> Result<LpsValueF32> {
        match target.float_mode {
            TargetFloatMode::Q32 => {
                let flat = flat_q32_words_from_f32_args(&gfn.parameters, args)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let words = inst
                    .call_q32_flat(name, &flat, cycle_model)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                match return_ty {
                    LpsType::Void => Ok(LpsValueF32::F32(0.0)),
                    _ => {
                        let q = decode_q32_return(&return_ty, &words)
                            .map_err(|e| anyhow::anyhow!("{e}"))?;
                        q32_to_lps_value_f32(&return_ty, q).map_err(|e| anyhow::anyhow!("{e}"))
                    }
                }
            }
            TargetFloatMode::F32 => inst.call(name, args).map_err(|e| anyhow::anyhow!("{e}")),
        }
    };

    match inner() {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = format!("{e:#}");
            if let Some(s) = inst.debug_state() {
                Err(anyhow::anyhow!("{msg}\n{s}"))
            } else {
                Err(anyhow::anyhow!("{msg}"))
            }
        }
    }
}
