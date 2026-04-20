//! Runnable browser [`WebAssembly::Instance`] with linked memory and exports.

use std::collections::HashMap;
use std::format;

use js_sys::{Function, Reflect, WebAssembly};
use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, LpsValueQ32, ParamQualifier};
use lpvm::{
    LpsValueF32, LpvmBuffer, LpvmInstance, encode_uniform_write, encode_uniform_write_q32,
    validate_render_texture_sig_ir,
};
use wasm_bindgen::{JsCast, JsValue};

use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport};

use super::BrowserLpvmModule;
use super::link;
use super::marshal::{
    build_js_args, build_js_args_q32_flat, js_result_to_lps_value, js_result_to_q32_words,
};
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
    lpir: LpirModule,
    render_texture_cache: Option<RenderTextureEntry>,
}

impl BrowserLpvmInstance {
    pub(crate) fn new(module: &BrowserLpvmModule) -> Result<Self, WasmError> {
        let linked = link::instantiate_shader(module, &module.runtime.memory)?;
        let inst_js: JsValue = linked.instance.clone().into();
        let exports_obj = Reflect::get(&inst_js, &JsValue::from_str("exports"))
            .map_err(|e| WasmError::runtime(format!("instance.exports: {e:?}")))?;

        Ok(Self {
            instance: linked.instance,
            memory: linked.memory,
            exports_obj,
            exports: module.exports.clone(),
            signatures: module.signatures.clone(),
            shadow_stack_base: module.shadow_stack_base,
            float_mode: module.opts.float_mode,
            lpir: module.lpir.clone(),
            render_texture_cache: None,
        })
    }

    fn prepare_call(&self) -> Result<(), WasmError> {
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
        if end > len {
            return Err(WasmError::runtime(format!(
                "linear memory too small: need {end} have {len}"
            )));
        }
        let view = js_sys::Uint8Array::new_with_byte_offset_and_length(
            &ab,
            offset as u32,
            data.len() as u32,
        );
        view.copy_from(data);
        Ok(())
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

        if matches!(export.return_type, LpsType::Void) {
            return Err(WasmError::runtime(
                "void return is not represented as LpsValue; use a typed return",
            ));
        }

        let return_ty = export.return_type.clone();
        let js_args = build_js_args(
            &export.param_types,
            export.params.len(),
            args,
            self.float_mode,
        )?;

        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(name))
            .map_err(|e| WasmError::runtime(format!("get export {name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("'{name}' is not a function")))?;

        self.prepare_call()?;
        let result = func
            .apply(&JsValue::NULL, &js_args)
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;

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
        let js_args = build_js_args_q32_flat(&export.param_types, export.params.len(), args)?;

        let func_val = Reflect::get(&self.exports_obj, &JsValue::from_str(name))
            .map_err(|e| WasmError::runtime(format!("get export {name}: {e:?}")))?;
        let func: Function = func_val
            .dyn_into()
            .map_err(|_| WasmError::runtime(format!("'{name}' is not a function")))?;

        self.prepare_call()?;
        let result = func
            .apply(&JsValue::NULL, &js_args)
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;

        if matches!(return_ty, LpsType::Void) {
            return Ok(Vec::new());
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
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "BrowserLpvmInstance::call_render_texture requires FloatMode::Q32",
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
        js_args.push(&JsValue::from_f64(0.0));
        js_args.push(&JsValue::from_f64(f64::from(tex_offset)));
        js_args.push(&JsValue::from_f64(f64::from(width as i32)));
        js_args.push(&JsValue::from_f64(f64::from(height as i32)));

        self.prepare_call()?;
        func.apply(&JsValue::NULL, &js_args)
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e:?}")))?;
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
}
