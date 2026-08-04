//! Runnable browser [`WebAssembly::Instance`] with linked memory and exports.

use std::collections::HashMap;
use std::format;

use js_sys::{Function, Reflect, WebAssembly};
use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, LpsValueQ32, ParamQualifier};
use lpvm::{
    DEFAULT_VMCTX_FUEL, INVOCATION_INDEX_ARMED, LpsValueF32, LpvmBuffer, LpvmInstance,
    TRAP_CODE_NONE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_TRAP, decode_global_read, encode_global_write,
    encode_uniform_write, encode_uniform_write_q32, global_data_span, validate_compute_tick_sig,
    validate_render_samples_sig_ir, validate_render_texture_sig_ir,
};
use wasm_bindgen::{JsCast, JsValue};

use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport};

use super::BrowserLpvmModule;
use super::link;
use super::marshal::{
    browser_shadow_frame_close, browser_shadow_frame_open, build_js_args_for_call,
    build_js_args_q32_for_call, build_js_args_q32_scalar_only, build_js_args_scalar_only,
    decode_browser_sret_q32_return, js_result_to_lps_value, js_result_to_q32_words,
};
use crate::aggregate_abi::{decode_aggregate_std430_bytes, export_needs_shadow_marshal};
use lpir::LpirModule;

struct RenderTextureEntry {
    name: String,
    func: Function,
}

pub struct BrowserLpvmInstance {
    instance: WebAssembly::Instance,
    memory: Option<WebAssembly::Memory>,
    exports_obj: JsValue,
    exports: HashMap<String, WasmExport>,
    signatures: LpsModuleSig,
    shadow_stack_base: Option<i32>,
    float_mode: FloatMode,
    /// Backing storage for this instance's vmctx block. Shaders share the
    /// app's own linear memory, so the block is a plain Rust allocation whose
    /// address is the guest offset; `Vec<u128>` guarantees the 16-byte
    /// alignment the vmctx header wants. Owned so it lives exactly as long as
    /// the instance.
    vmctx_buf: Vec<u128>,
    /// Guest base address of `vmctx_buf` (header + uniforms + globals +
    /// snapshot). Passing a fixed base (the old `0`) made every instance —
    /// and the app itself — share low memory, so a second live shader
    /// silently clobbered the first one's uniforms and persistent globals.
    vmctx_base: usize,
    /// Byte offset from vmctx base to globals region
    globals_offset: usize,
    /// Byte offset from vmctx base to snapshot region
    snapshot_offset: usize,
    /// Size of globals region in bytes
    globals_size: usize,
    lpir: LpirModule,
    render_texture_cache: Option<RenderTextureEntry>,
    render_samples_cache: Option<RenderTextureEntry>,
}

impl BrowserLpvmInstance {
    pub(crate) fn new(module: &BrowserLpvmModule) -> Result<Self, WasmError> {
        let linked = link::instantiate_shader(module, &module.runtime.memory)?;
        let inst_js: JsValue = linked.instance.clone().into();
        let exports_obj = Reflect::get(&inst_js, &JsValue::from_str("exports"))
            .map_err(|e| WasmError::runtime(format!("instance.exports: {e:?}")))?;

        // Per-instance vmctx allocation, mirroring `NativeJitModule::instantiate`
        // and the wasmtime runtime.
        let total_size = module.signatures.vmctx_buffer_size();
        let vmctx_buf = vec![0u128; total_size.div_ceil(16).max(1)];
        let vmctx_base = vmctx_buf.as_ptr() as usize;
        i32::try_from(vmctx_base)
            .map_err(|_| WasmError::runtime("vmctx guest base exceeds i32 range"))?;

        let sigs = &module.signatures;
        let globals_offset = sigs.globals_offset();
        let snapshot_offset = sigs.snapshot_offset();
        let globals_size = sigs.globals_size();

        let mut inst = Self {
            instance: linked.instance,
            memory: linked.memory,
            exports_obj,
            exports: module.exports.clone(),
            signatures: module.signatures.clone(),
            shadow_stack_base: module.shadow_stack_base,
            float_mode: module.opts.float_mode,
            vmctx_buf,
            vmctx_base,
            globals_offset,
            snapshot_offset,
            globals_size,
            lpir: module.lpir.clone(),
            render_texture_cache: None,
            render_samples_cache: None,
        };

        // Auto-init globals: call __shader_init if it exists, then snapshot
        inst.init_globals()?;

        Ok(inst)
    }

    /// Initialize globals by calling `__shader_init` if it exists,
    /// then memcpy globals -> snapshot to capture the initialized state
    /// (mirrors `rt_wasmtime` and lpvm-native's `rt_jit`).
    pub fn init_globals(&mut self) -> Result<(), WasmError> {
        // Call __shader_init if it exists (it may not be present if there are no globals with initializers)
        if self.exports.contains_key("__shader_init") {
            let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str("__shader_init"))
                .map_err(|e| WasmError::runtime(format!("get export __shader_init: {e:?}")))?;
            let func: Function = func_val
                .dyn_into()
                .map_err(|_| WasmError::runtime("`__shader_init` is not a function"))?;

            self.prepare_call()?;
            // Pass this instance's vmctx pointer as first argument, same as
            // other shader calls
            let js_args = js_sys::Array::new();
            js_args.push(&JsValue::from_f64(self.vmctx_base as f64));
            let call_result = func.apply(&JsValue::NULL, &js_args);
            self.take_trap()?;
            call_result
                .map_err(|e| WasmError::runtime(format!("WASM trap in __shader_init: {e:?}")))?;
        }

        // Copy globals region to snapshot region
        self.snapshot_globals()?;
        Ok(())
    }

    /// Reset globals by memcpy snapshot -> globals so each shader call sees
    /// the initialized state (per-pixel isolation). No-op if globals_size == 0.
    fn reset_globals(&mut self) -> Result<(), WasmError> {
        if self.globals_size == 0 {
            return Ok(());
        }
        let bytes = self.vmctx_read_bytes(self.snapshot_offset, self.globals_size)?;
        self.vmctx_write_bytes(self.globals_offset, &bytes)
    }

    /// Copy globals region to snapshot region (for init).
    fn snapshot_globals(&mut self) -> Result<(), WasmError> {
        if self.globals_size == 0 {
            return Ok(());
        }
        let bytes = self.vmctx_read_bytes(self.globals_offset, self.globals_size)?;
        self.vmctx_write_bytes(self.snapshot_offset, &bytes)
    }

    /// Arm the vmctx fuel/trap words and reset the shadow stack before a
    /// guest entry: full tank in the fuel low u32, host-armed invocation
    /// index in the high u32, no trap (mirrors `rt_wasmtime`). Render
    /// wrappers immediately re-arm per pixel/sample with
    /// `DEFAULT_INVOCATION_FUEL`; `metadata` (vmctx+12) is left untouched.
    ///
    /// Arming is mandatory with fuel-instrumented modules: the emitted
    /// checks are check-then-decrement, so an unarmed zero fuel word traps
    /// at the very first function entry.
    fn prepare_call(&mut self) -> Result<(), WasmError> {
        let mut header = [0u8; 12];
        header[0..4].copy_from_slice(&(DEFAULT_VMCTX_FUEL as u32).to_le_bytes());
        header[4..8].copy_from_slice(&INVOCATION_INDEX_ARMED.to_le_bytes());
        header[8..12].copy_from_slice(&TRAP_CODE_NONE.to_le_bytes());
        debug_assert_eq!(VMCTX_OFFSET_FUEL, 0);
        debug_assert_eq!(VMCTX_OFFSET_TRAP, 8);
        self.vmctx_write_bytes(VMCTX_OFFSET_FUEL, &header)?;
        if let Some(base) = self.shadow_stack_base {
            let global = Reflect::get(
                &self.exports_obj,
                &JsValue::from_str(SHADOW_STACK_GLOBAL_EXPORT),
            )
            .map_err(|e| WasmError::runtime(format!("get shadow stack global: {e:?}")))?;
            Reflect::set(
                &global,
                &JsValue::from_str("value"),
                &JsValue::from_f64(base as f64),
            )
            .map_err(|e| WasmError::runtime(format!("set shadow stack: {e:?}")))?;
        }
        Ok(())
    }

    fn vmctx_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), WasmError> {
        let total = self.signatures.vmctx_buffer_size();
        let end = offset
            .checked_add(data.len())
            .ok_or_else(|| WasmError::runtime("vmctx write: offset overflow"))?;
        if end > total {
            return Err(WasmError::runtime(format!(
                "vmctx write out of bounds: end {end} total {total}"
            )));
        }
        let mem = self
            .memory
            .as_ref()
            .ok_or_else(|| WasmError::runtime("no linear memory for vmctx write"))?;
        let ab: js_sys::ArrayBuffer = mem
            .buffer()
            .dyn_into()
            .map_err(|_| WasmError::runtime("memory.buffer is not ArrayBuffer"))?;
        let len = ab.byte_length() as usize;
        if self.vmctx_base + end > len {
            return Err(WasmError::runtime(format!(
                "linear memory too small: need {} have {len}",
                self.vmctx_base + end
            )));
        }
        let view = js_sys::Uint8Array::new_with_byte_offset_and_length(
            &ab,
            (self.vmctx_base + offset) as u32,
            data.len() as u32,
        );
        view.copy_from(data);
        Ok(())
    }

    fn vmctx_read_bytes(&mut self, offset: usize, len: usize) -> Result<Vec<u8>, WasmError> {
        let total = self.signatures.vmctx_buffer_size();
        let end = offset
            .checked_add(len)
            .ok_or_else(|| WasmError::runtime("vmctx read: offset overflow"))?;
        if end > total {
            return Err(WasmError::runtime(format!(
                "vmctx read out of bounds: end {end} total {total}"
            )));
        }
        let mem = self
            .memory
            .as_ref()
            .ok_or_else(|| WasmError::runtime("no linear memory for vmctx read"))?;
        let ab: js_sys::ArrayBuffer = mem
            .buffer()
            .dyn_into()
            .map_err(|_| WasmError::runtime("memory.buffer is not ArrayBuffer"))?;
        let mem_len = ab.byte_length() as usize;
        if self.vmctx_base + end > mem_len {
            return Err(WasmError::runtime(format!(
                "linear memory too small: need {} have {mem_len}",
                self.vmctx_base + end
            )));
        }
        let view = js_sys::Uint8Array::new_with_byte_offset_and_length(
            &ab,
            (self.vmctx_base + offset) as u32,
            len as u32,
        );
        let mut bytes = vec![0u8; len];
        view.copy_to(&mut bytes);
        Ok(bytes)
    }

    /// Read the vmctx trap slot after a guest entry; nonzero → typed
    /// [`WasmError::Trap`] carrying the invocation index (fuel high u32).
    ///
    /// Must run on BOTH `Ok` and `Err` call returns: an emitted fuel check
    /// aborts with `unreachable`, which surfaces here as an opaque JS
    /// `RuntimeError` — classification is by the slot, never the JS error.
    /// A JS error with a clean trap slot stays the generic "WASM trap: …"
    /// runtime error (a genuine non-fuel trap). Return values from a trapped
    /// call are garbage; callers must discard them.
    fn take_trap(&mut self) -> Result<(), WasmError> {
        let words = self.vmctx_read_bytes(VMCTX_OFFSET_FUEL + 4, 8)?;
        let invocation = u32::from_le_bytes(words[0..4].try_into().expect("4 bytes"));
        let trap = u32::from_le_bytes(words[4..8].try_into().expect("4 bytes"));
        if trap == TRAP_CODE_NONE {
            Ok(())
        } else {
            Err(WasmError::Trap {
                code: trap,
                invocation,
            })
        }
    }

    fn resolve_render_texture(&mut self, fn_name: &str) -> Result<Function, WasmError> {
        if let Some(entry) = &self.render_texture_cache {
            if entry.name == fn_name {
                return Ok(entry.func.clone());
            }
        }

        let ir_fn = self
            .lpir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| WasmError::runtime(format!("function `{fn_name}` not in LPIR")))?;
        validate_render_texture_sig_ir(ir_fn)
            .map_err(|e| WasmError::runtime(format!("render-texture sig invalid: {e}")))?;

        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(fn_name))
            .map_err(|e| WasmError::runtime(format!("get export {fn_name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("`{fn_name}` is not a function")))?;

        let func_ret = func.clone();
        self.render_texture_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            func,
        });
        Ok(func_ret)
    }

    fn resolve_render_samples(&mut self, fn_name: &str) -> Result<Function, WasmError> {
        if let Some(entry) = &self.render_samples_cache {
            if entry.name == fn_name {
                return Ok(entry.func.clone());
            }
        }

        let ir_fn = self
            .lpir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| WasmError::runtime(format!("function `{fn_name}` not in LPIR")))?;
        validate_render_samples_sig_ir(ir_fn)
            .map_err(|e| WasmError::runtime(format!("render-samples sig invalid: {e}")))?;

        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(fn_name))
            .map_err(|e| WasmError::runtime(format!("get export {fn_name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("`{fn_name}` is not a function")))?;

        let func_ret = func.clone();
        self.render_samples_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            func,
        });
        Ok(func_ret)
    }

    pub fn js_instance(&self) -> &WebAssembly::Instance {
        &self.instance
    }

    pub fn js_memory(&self) -> Option<&WebAssembly::Memory> {
        self.memory.as_ref()
    }

    pub fn js_exports(&self) -> &JsValue {
        &self.exports_obj
    }
}

impl LpvmInstance for BrowserLpvmInstance {
    type Error = WasmError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        let fn_sig = self
            .signatures
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        for p in &fn_sig.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(WasmError::runtime(
                    "out/inout parameters are not supported for direct calling.",
                ));
            }
        }

        let export = self.exports.get(name).cloned().ok_or_else(|| {
            WasmError::runtime(format!("function '{name}' not found in WASM export table"))
        })?;

        // Void exports run and report `0.0` — see the matching comment in
        // `rt_wasmtime::instance`. The two wasm runtimes must agree, or the
        // browser firmware answers an f32 shader call differently from the
        // host one the filetests gate on.
        let returns_void = matches!(export.return_type, LpsType::Void);

        let return_ty = export.return_type.clone();
        let needs_shadow = export_needs_shadow_marshal(&export);
        if needs_shadow && self.shadow_stack_base.is_none() {
            return Err(WasmError::runtime(
                "aggregate/sret calling convention requires an exported shadow stack global",
            ));
        }

        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(name))
            .map_err(|e| WasmError::runtime(format!("get export {name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("'{name}' is not a function")))?;

        // Reset globals before each shader call to ensure per-pixel isolation
        self.reset_globals()?;
        self.prepare_call()?;

        let shadow_frame = if needs_shadow {
            Some(browser_shadow_frame_open(&self.exports_obj)?)
        } else {
            None
        };

        let (js_args, sret_plan) = if needs_shadow {
            let mem = self
                .memory
                .as_ref()
                .ok_or_else(|| WasmError::runtime("no linear memory for aggregate call"))?;
            build_js_args_for_call(
                &self.exports_obj,
                mem,
                &export,
                args,
                self.float_mode,
                &return_ty,
                self.vmctx_base as f64,
            )?
        } else {
            (
                build_js_args_scalar_only(
                    &export.param_types,
                    export.params.len(),
                    args,
                    self.float_mode,
                    self.vmctx_base as f64,
                )?,
                None,
            )
        };

        let call_result = func.apply(&JsValue::NULL, &js_args);
        self.take_trap()?;
        let result = call_result.map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;

        if let Some(frame) = shadow_frame {
            browser_shadow_frame_close(&self.exports_obj, frame)?;
        }

        if returns_void {
            return Ok(LpsValueF32::F32(0.0));
        }

        if export.uses_sret {
            let plan = sret_plan.ok_or_else(|| {
                WasmError::runtime("internal: sret export without sret allocation plan")
            })?;
            let mem = self
                .memory
                .as_ref()
                .ok_or_else(|| WasmError::runtime("no linear memory for sret read"))?;
            let bytes = super::marshal::browser_memory_read(mem, plan.ptr, plan.size)?;
            return decode_aggregate_std430_bytes(&return_ty, &bytes, self.float_mode);
        }

        js_result_to_lps_value(&return_ty, &result, self.float_mode)
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "BrowserLpvmInstance::call_q32 requires FloatMode::Q32",
            ));
        }

        let fn_sig = self
            .signatures
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        for p in &fn_sig.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(WasmError::runtime(
                    "out/inout parameters are not supported for direct calling.",
                ));
            }
        }

        let export = self.exports.get(name).cloned().ok_or_else(|| {
            WasmError::runtime(format!("function '{name}' not found in WASM export table"))
        })?;

        let return_ty = export.return_type.clone();
        let needs_shadow = export_needs_shadow_marshal(&export);
        if needs_shadow && self.shadow_stack_base.is_none() {
            return Err(WasmError::runtime(
                "aggregate/sret calling convention requires an exported shadow stack global",
            ));
        }

        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(name))
            .map_err(|e| WasmError::runtime(format!("get export {name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("'{name}' is not a function")))?;

        // Reset globals before each shader call to ensure per-pixel isolation
        self.reset_globals()?;
        self.prepare_call()?;

        let shadow_frame = if needs_shadow {
            Some(browser_shadow_frame_open(&self.exports_obj)?)
        } else {
            None
        };

        let (js_args, sret_plan) = if needs_shadow {
            let mem = self
                .memory
                .as_ref()
                .ok_or_else(|| WasmError::runtime("no linear memory for aggregate call"))?;
            build_js_args_q32_for_call(
                &self.exports_obj,
                mem,
                &export,
                args,
                &return_ty,
                self.vmctx_base as f64,
            )?
        } else {
            (
                build_js_args_q32_scalar_only(
                    &export.param_types,
                    export.params.len(),
                    args,
                    self.vmctx_base as f64,
                )?,
                None,
            )
        };

        let call_result = func.apply(&JsValue::NULL, &js_args);
        self.take_trap()?;
        let result = call_result.map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;

        if let Some(frame) = shadow_frame {
            browser_shadow_frame_close(&self.exports_obj, frame)?;
        }

        if matches!(return_ty, LpsType::Void) {
            return Ok(Vec::new());
        }

        if export.uses_sret {
            let plan = sret_plan.ok_or_else(|| {
                WasmError::runtime("internal: sret export without sret allocation plan")
            })?;
            let mem = self
                .memory
                .as_ref()
                .ok_or_else(|| WasmError::runtime("no linear memory for sret read"))?;
            return decode_browser_sret_q32_return(mem, &plan, &return_ty);
        }

        js_result_to_q32_words(&return_ty, &result, self.float_mode)
    }

    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error> {
        // Deliberately still Q32-only, unlike lpvm-native's two backends — but
        // NOT because anything is unimplemented. The emitter this tier shares
        // with `rt_wasmtime` resolves f32 builtin ids fine (M5); the whole
        // `wasm.f32` corpus compiles and runs, 850/850 files, 0 compile-fail.
        // The old "no f32 builtin lowering" reason was measured false on
        // 2026-08-02 — do not restate it.
        //
        // Two honest reasons this one stays. (1) On `rt_wasmtime`, the same
        // emitted module rendered *one count low* against the rv32-emulator
        // oracle with the guard removed — the known wasmtime last-bit
        // divergence; see `rt_wasmtime/instance.rs`. (2) That was **not**
        // measured here: this runtime executes in the browser's own wasm
        // engine, so its numeric agreement is unverified rather than known-bad.
        // Refusing is the conservative read of an unmeasured tier, and a Float
        // shader still previews on the GPU tier. Lift this when someone
        // actually measures the browser tier — not by analogy to wasmtime.
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "BrowserLpvmInstance::call_render_texture requires FloatMode::Q32 \
                 (float shaders preview on the GPU tier; the CPU preview tier's \
                 f32 numeric agreement is unverified)",
            ));
        }

        let func = self.resolve_render_texture(fn_name)?;
        let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| {
            WasmError::runtime(format!(
                "texture guest base {:#x} exceeds i32 range",
                texture.guest_base()
            ))
        })?;

        let js_args = js_sys::Array::new();
        js_args.push(&JsValue::from_f64(self.vmctx_base as f64));
        js_args.push(&JsValue::from_f64(f64::from(tex_offset)));
        js_args.push(&JsValue::from_f64(f64::from(width as i32)));
        js_args.push(&JsValue::from_f64(f64::from(height as i32)));

        self.reset_globals()?;
        self.prepare_call()?;
        let call_result = func.apply(&JsValue::NULL, &js_args);
        self.take_trap()?;
        call_result.map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;
        Ok(())
    }

    fn call_render_samples(
        &mut self,
        fn_name: &str,
        points: &mut LpvmBuffer,
        out: &mut LpvmBuffer,
        count: u32,
    ) -> Result<(), Self::Error> {
        // See `call_render_texture` for why this one stays — an unverified
        // tier, not a missing capability.
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "BrowserLpvmInstance::call_render_samples requires FloatMode::Q32 \
                 (float shaders preview on the GPU tier; the CPU preview tier's \
                 f32 numeric agreement is unverified)",
            ));
        }

        let func = self.resolve_render_samples(fn_name)?;
        let points_offset = i32::try_from(points.guest_base()).map_err(|_| {
            WasmError::runtime(format!(
                "points guest base {:#x} exceeds i32 range",
                points.guest_base()
            ))
        })?;
        let out_offset = i32::try_from(out.guest_base()).map_err(|_| {
            WasmError::runtime(format!(
                "sample output guest base {:#x} exceeds i32 range",
                out.guest_base()
            ))
        })?;

        let js_args = js_sys::Array::new();
        js_args.push(&JsValue::from_f64(self.vmctx_base as f64));
        js_args.push(&JsValue::from_f64(f64::from(points_offset)));
        js_args.push(&JsValue::from_f64(f64::from(out_offset)));
        js_args.push(&JsValue::from_f64(f64::from(count as i32)));

        self.reset_globals()?;
        self.prepare_call()?;
        let call_result = func.apply(&JsValue::NULL, &js_args);
        self.take_trap()?;
        call_result.map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;
        Ok(())
    }

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write(&self.signatures, path, value, self.float_mode)
            .map_err(|e| WasmError::runtime(format!("set_uniform: {e}")))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write_q32(&self.signatures, path, value)
            .map_err(|e| WasmError::runtime(format!("set_uniform_q32: {e}")))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_global(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_global_write(&self.signatures, path, value, self.float_mode)
            .map_err(|e| WasmError::runtime(format!("set_global: {e}")))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn get_global(&mut self, path: &str) -> Result<LpsValueF32, Self::Error> {
        let span = global_data_span(&self.signatures, path)
            .map_err(|e| WasmError::runtime(format!("get_global: {e}")))?;
        let bytes = self.vmctx_read_bytes(span.offset, span.len)?;
        decode_global_read(&span.ty, &bytes, self.float_mode)
            .map_err(|e| WasmError::runtime(format!("get_global: {e}")))
    }

    fn call_compute_tick(&mut self, name: &str) -> Result<(), Self::Error> {
        let fn_sig = self
            .signatures
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;
        validate_compute_tick_sig(fn_sig)
            .map_err(|e| WasmError::runtime(format!("{name}: {e}")))?;
        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(name))
            .map_err(|e| WasmError::runtime(format!("get export {name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("`{name}` is not a function")))?;
        self.prepare_call()?;
        let args = js_sys::Array::new();
        args.push(&JsValue::from_f64(self.vmctx_base as f64));
        let call_result = func.apply(&JsValue::NULL, &args);
        self.take_trap()?;
        call_result.map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;
        Ok(())
    }
}
