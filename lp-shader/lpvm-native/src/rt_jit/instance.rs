//! [`LpvmInstance`] for direct JIT calls (register args only; see `invoke_flat` limits).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpir::FloatMode;
use lps_shared::{LpsType, ParamQualifier, lps_value_f32::LpsValueF32};
use lpvm::{
    CallError, LpvmInstance, decode_q32_return, flat_q32_words_from_f32_args, glsl_component_count,
    q32_to_lps_value_f32,
};

use crate::error::NativeError;
use crate::isa::rv32::abi::func_abi_rv32;

use super::call::rv32_jalr_a0_a7;
use super::module::NativeJitModule;

/// Per-instance state: [`NativeJitModule`] plus guest vmctx pointer.
pub struct NativeJitInstance {
    pub(crate) module: NativeJitModule,
    pub(crate) vmctx_guest: u32,
}

impl NativeJitInstance {
    fn invoke_flat(&mut self, name: &str, flat: &[i32]) -> Result<Vec<i32>, NativeError> {
        let idx = self
            .module
            .inner
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;
        let ir_func = &self.module.inner.ir.functions[idx];

        let gfn = self
            .module
            .inner
            .meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;

        let slots = ir_func.total_param_slots() as usize;
        let func_abi = func_abi_rv32(&gfn, slots);
        let is_sret = func_abi.is_sret();
        let n_ret = ir_func.return_types.len();

        let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());
        full.push(self.vmctx_guest as i32);
        full.extend_from_slice(flat);

        let entry_off = self
            .module
            .entry_offset(name)
            .ok_or_else(|| CallError::Unsupported(format!("symbol `{name}` not in JIT image")))?;
        let entry = unsafe { self.module.buffer().entry_ptr(entry_off) as usize };

        if is_sret {
            if full.len() > 7 {
                return Err(NativeError::Call(CallError::Unsupported(String::from(
                    "NativeJitInstance: sret + more than 7 argument words need stack args (not implemented)",
                ))));
            }
            let n_words = func_abi.sret_word_count().unwrap_or(0) as usize;
            let n_buf = n_words.max(n_ret).max(1);
            let mut sret_buf = alloc::vec![0i32; n_buf];
            let sret_ptr = sret_buf.as_mut_ptr() as usize;
            let (a0, a1, a2, a3, a4, a5, a6, a7) = pack_regs_sret(sret_ptr as i32, &full);
            unsafe {
                rv32_jalr_a0_a7(entry, a0, a1, a2, a3, a4, a5, a6, a7);
            }
            sret_buf.truncate(n_ret);
            return Ok(sret_buf);
        }

        if full.len() > 8 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeJitInstance: more than 8 argument words need stack args (not implemented)",
            ))));
        }

        let (a0, a1, a2, a3, a4, a5, a6, a7) = pack_regs_direct(&full);
        let (r0, r1) = unsafe { rv32_jalr_a0_a7(entry, a0, a1, a2, a3, a4, a5, a6, a7) };

        match n_ret {
            0 => Ok(Vec::new()),
            1 => Ok(alloc::vec![r0]),
            2 => Ok(alloc::vec![r0, r1]),
            _ => Err(NativeError::Call(CallError::Unsupported(format!(
                "NativeJitInstance: expected sret for {n_ret} return words"
            )))),
        }
    }
}

fn pack_regs_direct(words: &[i32]) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    let mut r = [0i32; 8];
    for (i, w) in words.iter().enumerate().take(8) {
        r[i] = *w;
    }
    (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7])
}

fn pack_regs_sret(sret: i32, words: &[i32]) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    let mut r = [0i32; 8];
    r[0] = sret;
    for (i, w) in words.iter().enumerate().take(7) {
        r[1 + i] = *w;
    }
    (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7])
}

impl LpvmInstance for NativeJitInstance {
    type Error = NativeError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        if self.module.inner.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeJitInstance::call requires FloatMode::Q32",
            ))));
        }

        let gfn = self
            .module
            .inner
            .meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(NativeError::Call(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))));
            }
        }

        if gfn.return_type == LpsType::Void {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "void return is not represented as LpsValue; use a typed return",
            ))));
        }

        if gfn.parameters.len() != args.len() {
            return Err(NativeError::Call(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            }));
        }

        let flat = flat_q32_words_from_f32_args(&gfn.parameters, args)?;
        let idx = self
            .module
            .inner
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;
        let ir_func = &self.module.inner.ir.functions[idx];
        let param_count = ir_func.param_count as usize;
        if flat.len() != param_count {
            return Err(NativeError::Call(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))));
        }

        let words = self.invoke_flat(name, &flat)?;
        let gq = decode_q32_return(&gfn.return_type, &words)?;
        q32_to_lps_value_f32(&gfn.return_type, gq)
            .map_err(|e| NativeError::Call(CallError::TypeMismatch(e.to_string())))
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        if self.module.inner.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeJitInstance::call_q32 requires FloatMode::Q32",
            ))));
        }

        let gfn = self
            .module
            .inner
            .meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(NativeError::Call(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))));
            }
        }

        let idx = self
            .module
            .inner
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;
        let ir_func = &self.module.inner.ir.functions[idx];
        let param_count = ir_func.param_count as usize;

        let expected_words: usize = gfn
            .parameters
            .iter()
            .map(|p| glsl_component_count(&p.ty))
            .sum();
        if args.len() != expected_words {
            return Err(NativeError::Call(CallError::Arity {
                expected: expected_words,
                got: args.len(),
            }));
        }
        if args.len() != param_count {
            return Err(NativeError::Call(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                args.len(),
                param_count
            ))));
        }

        let words = self.invoke_flat(name, args)?;
        if gfn.return_type == LpsType::Void {
            return Ok(Vec::new());
        }
        Ok(words)
    }
}
