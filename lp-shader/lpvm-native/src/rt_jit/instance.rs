//! [`LpvmInstance`] for direct JIT calls (register args only; see `invoke_flat` limits).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpir::FloatMode;
use lps_shared::{LpsType, LpsValueQ32, ParamQualifier, lps_value_f32::LpsValueF32};
use lpvm::{
    CallError, LpvmBuffer, LpvmInstance, decode_q32_return, encode_uniform_write,
    encode_uniform_write_q32, flat_q32_words_from_f32_args, glsl_component_count,
    q32_to_lps_value_f32, validate_render_texture_sig_ir,
};

use crate::error::NativeError;
use crate::isa::IsaTarget;

use super::call::rv32_jalr_a0_a7;
use super::module::{NativeJitDirectCall, NativeJitModule};

struct RenderTextureEntry {
    name: String,
    entry_pc: usize,
}

/// Per-instance state: [`NativeJitModule`] plus guest vmctx pointer.
pub struct NativeJitInstance {
    pub(crate) module: NativeJitModule,
    pub(crate) vmctx_guest: u32,
    /// Byte offset from vmctx base to globals region
    pub(crate) globals_offset: u32,
    /// Byte offset from vmctx base to snapshot region
    pub(crate) snapshot_offset: u32,
    /// Size of globals region in bytes
    pub(crate) globals_size: u32,
    render_texture_cache: Option<RenderTextureEntry>,
}

impl NativeJitInstance {
    /// Initialize globals by calling `__shader_init` if it exists,
    /// then memcpy globals -> snapshot to capture the initialized state.
    pub fn init_globals(&mut self) -> Result<(), NativeError> {
        // Call __shader_init if it exists (it may not be present if there are no globals with initializers)
        if self.module.entry_offset("__shader_init").is_some() {
            self.invoke_flat("__shader_init", &[])?;
        }

        // Copy globals region to snapshot region
        self.snapshot_globals();
        Ok(())
    }

    /// Reset globals by memcpy snapshot -> globals.
    /// This is a no-op if globals_size == 0.
    pub fn reset_globals(&mut self) {
        if self.globals_size == 0 {
            return;
        }

        let base = self.vmctx_guest as *mut u8;
        let globals_ptr = unsafe { base.add(self.globals_offset as usize) };
        let snapshot_ptr = unsafe { base.add(self.snapshot_offset as usize) };

        unsafe {
            core::ptr::copy_nonoverlapping(snapshot_ptr, globals_ptr, self.globals_size as usize);
        }
    }

    /// Copy globals region to snapshot region (for init).
    fn snapshot_globals(&mut self) {
        if self.globals_size == 0 {
            return;
        }

        let base = self.vmctx_guest as *mut u8;
        let globals_ptr = unsafe { base.add(self.globals_offset as usize) };
        let snapshot_ptr = unsafe { base.add(self.snapshot_offset as usize) };

        unsafe {
            core::ptr::copy_nonoverlapping(globals_ptr, snapshot_ptr, self.globals_size as usize);
        }
    }

    fn resolve_render_texture(&mut self, fn_name: &str) -> Result<usize, NativeError> {
        if let Some(entry) = &self.render_texture_cache {
            if entry.name == fn_name {
                return Ok(entry.entry_pc);
            }
        }

        let ir_fn = self
            .module
            .inner
            .ir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| NativeError::Call(CallError::MissingMetadata(String::from(fn_name))))?;
        validate_render_texture_sig_ir(ir_fn).map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "render-texture sig invalid: {e}"
            )))
        })?;

        let entry_off = self.module.entry_offset(fn_name).ok_or_else(|| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "symbol `{fn_name}` not in JIT image"
            )))
        })?;
        let entry_pc = unsafe { self.module.buffer().entry_ptr(entry_off) as usize };

        self.render_texture_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            entry_pc,
        });
        Ok(entry_pc)
    }

    /// Fast direct call using cached handle (zero lookups, zero allocations).
    ///
    /// Uses stack-allocated buffers for args and returns. Caller provides output buffer.
    /// This is the preferred path for per-pixel shader calls.
    pub fn call_direct(
        &mut self,
        handle: &NativeJitDirectCall,
        args: &[i32],
        out: &mut [i32],
    ) -> Result<(), NativeError> {
        // Validate
        if args.len() != handle.arg_count {
            return Err(NativeError::Call(CallError::Arity {
                expected: handle.arg_count,
                got: args.len(),
            }));
        }
        if out.len() != handle.ret_count {
            return Err(NativeError::Call(CallError::TypeMismatch(alloc::format!(
                "return buffer length {} does not match {} return word(s)",
                out.len(),
                handle.ret_count
            ))));
        }

        // Pack args: vmctx + user args (stack-allocated, max 8 words)
        let full_len = 1 + args.len();
        if full_len > 8 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeJitInstance::call_direct: more than 8 argument words need stack args (not implemented)",
            ))));
        }

        let mut full: [i32; 8] = [0; 8];
        full[0] = self.vmctx_guest as i32;
        for (i, arg) in args.iter().enumerate() {
            full[1 + i] = *arg;
        }

        let entry = unsafe { self.module.buffer().entry_ptr(handle.entry_offset) as usize };

        if handle.is_sret {
            // sret path: a0 = sret buffer pointer, a1-a7 = args
            if full_len > 7 {
                return Err(NativeError::Call(CallError::Unsupported(String::from(
                    "NativeJitInstance::call_direct: sret + more than 7 argument words need stack args (not implemented)",
                ))));
            }

            // Use out buffer as sret buffer
            let sret_ptr = out.as_mut_ptr() as usize;
            // Note: full[0] is vmctx, so we pass full[0..7] into a1-a7
            let (a0, a1, a2, a3, a4, a5, a6, a7) = pack_regs_sret_direct(sret_ptr as i32, &full);
            unsafe {
                rv32_jalr_a0_a7(entry, a0, a1, a2, a3, a4, a5, a6, a7);
            }
        } else {
            // Direct return path: returns in a0, a1
            let (a0, a1, a2, a3, a4, a5, a6, a7) = pack_regs_direct_arr(&full);
            let (r0, r1) = unsafe { rv32_jalr_a0_a7(entry, a0, a1, a2, a3, a4, a5, a6, a7) };

            match handle.ret_count {
                0 => {}
                1 => out[0] = r0,
                2 => {
                    out[0] = r0;
                    out[1] = r1;
                }
                _ => {
                    return Err(NativeError::Call(CallError::Unsupported(format!(
                        "NativeJitInstance::call_direct: expected sret for {} return words",
                        handle.ret_count
                    ))));
                }
            }
        }

        Ok(())
    }

    fn invoke_flat(&mut self, name: &str, flat: &[i32]) -> Result<Vec<i32>, NativeError> {
        let ir_func = self
            .module
            .inner
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;

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
        let func_abi = match self.module.inner.isa {
            IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(&gfn, slots),
        };
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

    fn vmctx_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), NativeError> {
        let total = self.module.inner.meta.vmctx_buffer_size();
        let end = offset.checked_add(data.len()).ok_or_else(|| {
            NativeError::Call(CallError::Unsupported(String::from(
                "vmctx write: offset overflow",
            )))
        })?;
        if end > total {
            return Err(NativeError::Call(CallError::Unsupported(alloc::format!(
                "vmctx write out of bounds: end {end} total {total}"
            ))));
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                (self.vmctx_guest as *mut u8).add(offset),
                data.len(),
            );
        }
        Ok(())
    }
}

fn pack_regs_direct(words: &[i32]) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    let mut r = [0i32; 8];
    for (i, w) in words.iter().enumerate().take(8) {
        r[i] = *w;
    }
    (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7])
}

fn pack_regs_direct_arr(words: &[i32; 8]) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    (
        words[0], words[1], words[2], words[3], words[4], words[5], words[6], words[7],
    )
}

fn pack_regs_sret(sret: i32, words: &[i32]) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    let mut r = [0i32; 8];
    r[0] = sret;
    for (i, w) in words.iter().enumerate().take(7) {
        r[1 + i] = *w;
    }
    (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7])
}

fn pack_regs_sret_direct(sret: i32, words: &[i32; 8]) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    // words[0] is vmctx, words[1..] are user args
    // sret goes to a0, vmctx goes to a1, user args to a2-a7
    (
        sret, words[0], // vmctx -> a1
        words[1], // user arg 0 -> a2
        words[2], // user arg 1 -> a3
        words[3], // user arg 2 -> a4
        words[4], // user arg 3 -> a5
        words[5], // user arg 4 -> a6
        words[6], // user arg 5 -> a7
    )
}

impl LpvmInstance for NativeJitInstance {
    type Error = NativeError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        // Reset globals before each call to ensure fresh state
        self.reset_globals();
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
        let ir_func = self
            .module
            .inner
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;
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
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

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

        let ir_func = self
            .module
            .inner
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(String::from(name)))?;
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

    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error> {
        self.reset_globals();

        if self.module.inner.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeJitInstance::call_render_texture requires FloatMode::Q32",
            ))));
        }

        let entry = self.resolve_render_texture(fn_name)?;

        let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "texture guest base {:#x} exceeds i32 range",
                texture.guest_base()
            )))
        })?;
        let vmctx = self.vmctx_guest as i32;

        #[cfg(target_arch = "riscv32")]
        unsafe {
            crate::rt_jit::call::rv32_jalr_a0_a7(
                entry,
                vmctx,
                tex_offset,
                width as i32,
                height as i32,
                0,
                0,
                0,
                0,
            );
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = (entry, vmctx, tex_offset, width, height);
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeJitInstance::call_render_texture requires riscv32 host",
            ))));
        }

        Ok(())
    }

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write(
            &self.module.inner.meta,
            path,
            value,
            self.module.inner.options.float_mode,
        )
        .map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!("set_uniform: {e}")))
        })?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error> {
        let (off, bytes) =
            encode_uniform_write_q32(&self.module.inner.meta, path, value).map_err(|e| {
                NativeError::Call(CallError::Unsupported(alloc::format!(
                    "set_uniform_q32: {e}"
                )))
            })?;
        self.vmctx_write_bytes(off, &bytes)
    }
}
