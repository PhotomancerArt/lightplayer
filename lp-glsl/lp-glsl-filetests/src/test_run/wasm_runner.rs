//! WASM execution via wasmtime, implementing GlslExecutable.

use crate::test_run::compile::DEFAULT_MAX_INSTRUCTIONS;
use lp_glsl_cranelift::semantic::functions::FunctionSignature;
use lp_glsl_cranelift::semantic::types::Type;
use lp_glsl_cranelift::{ErrorCode, GlslDiagnostics, GlslError, GlslExecutable, GlslValue};
use lp_glsl_wasm::types::glsl_type_to_wasm_components;
use lp_glsl_wasm::{WasmOptions, glsl_wasm};
use std::collections::HashMap;
use wasm_encoder::ValType as WasmValType;
use wasmtime::{Config, Engine, Instance, Store};

use crate::test_run::wasm_link;

/// Q16.16 fixed-point scale factor.
const Q16_16_SCALE: f32 = 65536.0;

/// Executable WASM module that implements GlslExecutable via wasmtime.
pub struct WasmExecutable {
    store: Store<()>,
    instance: Instance,
    exports: HashMap<String, FunctionSignature>,
    float_mode: lp_glsl_wasm::FloatMode,
    wasm_bytes: Vec<u8>,
}

impl WasmExecutable {
    /// Compile GLSL source to WASM and create an executable.
    pub fn from_source(source: &str, options: WasmOptions) -> Result<Self, GlslDiagnostics> {
        let module = glsl_wasm(source, options.clone())?;
        let wasm_bytes = module.bytes.clone();

        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|e| {
            GlslDiagnostics::from(GlslError::new(
                ErrorCode::E0400,
                format!("failed to create WASM engine: {e}"),
            ))
        })?;
        let mut store = Store::new(&engine, ());
        let instance = wasm_link::instantiate_wasm_module(&engine, &mut store, &wasm_bytes)
            .map_err(|e| GlslDiagnostics::from(e))?;

        let exports: HashMap<String, FunctionSignature> = module
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.signature.clone()))
            .collect();

        Ok(Self {
            store,
            instance,
            exports,
            float_mode: options.float_mode,
            wasm_bytes,
        })
    }

    /// Set fuel before each call so execution does not hang on infinite loops.
    fn prepare_call(&mut self) -> Result<(), GlslError> {
        self.store
            .set_fuel(DEFAULT_MAX_INSTRUCTIONS)
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("failed to set fuel: {e}")))
    }

    fn call_wasm_multi(
        &mut self,
        name: &str,
        args: &[GlslValue],
    ) -> Result<Vec<wasmtime::Val>, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;

        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = sig
            .parameters
            .iter()
            .map(|p| glsl_param_to_wasm(&p.ty, self.float_mode))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.float_mode)?);
        }

        let result_types = glsl_type_to_wasm_components(&sig.return_type, self.float_mode);
        let mut results: Vec<wasmtime::Val> = result_types
            .iter()
            .map(|t| match t {
                WasmValType::I32 => wasmtime::Val::I32(0),
                WasmValType::F32 => wasmtime::Val::F32(0f32.to_bits()),
                _ => wasmtime::Val::I32(0),
            })
            .collect();

        self.prepare_call()?;
        func.call(&mut self.store, &wasm_args, &mut results)
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("WASM trap: {e}")))?;
        Ok(results)
    }
}

fn glsl_value_to_wasm(
    v: &GlslValue,
    expected: WasmValType,
    float_mode: lp_glsl_wasm::FloatMode,
) -> Result<wasmtime::Val, GlslError> {
    use wasmtime::Val;

    match expected {
        WasmValType::I32 => match v {
            GlslValue::I32(x) => Ok(Val::I32(*x)),
            GlslValue::U32(x) => Ok(Val::I32(*x as i32)),
            GlslValue::Bool(b) => Ok(Val::I32(if *b { 1 } else { 0 })),
            GlslValue::F32(f) => {
                if matches!(float_mode, lp_glsl_wasm::FloatMode::Q32) {
                    Ok(Val::I32((*f * Q16_16_SCALE) as i32))
                } else {
                    Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("float arg in i32 slot requires Q32 mode"),
                    ))
                }
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("unsupported GlslValue {v:?} for i32 param"),
            )),
        },
        WasmValType::F32 => match v {
            GlslValue::F32(f) => Ok(Val::F32(f.to_bits())),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("expected f32, got {v:?}"),
            )),
        },
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unsupported WASM param type {expected:?}"),
        )),
    }
}

impl GlslExecutable for WasmExecutable {
    fn call_void(&mut self, name: &str, args: &[GlslValue]) -> Result<(), GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = sig
            .parameters
            .iter()
            .map(|p| glsl_param_to_wasm(&p.ty, self.float_mode))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.float_mode)?);
        }

        self.prepare_call()?;
        func.call(&mut self.store, &wasm_args, &mut [])
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("WASM trap: {e}")))?;
        Ok(())
    }

    fn call_i32(&mut self, name: &str, args: &[GlslValue]) -> Result<i32, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = sig
            .parameters
            .iter()
            .map(|p| glsl_param_to_wasm(&p.ty, self.float_mode))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.float_mode)?);
        }

        self.prepare_call()?;
        let mut results = [wasmtime::Val::I32(0)];
        func.call(&mut self.store, &wasm_args, &mut results)
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("WASM trap: {e}")))?;

        match results[0] {
            wasmtime::Val::I32(i) => Ok(i),
            wasmtime::Val::F32(f) => Ok(f as i32),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("unexpected WASM result type"),
            )),
        }
    }

    fn call_f32(&mut self, name: &str, args: &[GlslValue]) -> Result<f32, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = sig
            .parameters
            .iter()
            .map(|p| glsl_param_to_wasm(&p.ty, self.float_mode))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.float_mode)?);
        }

        self.prepare_call()?;
        let mut results = [wasmtime::Val::F32(0f32.to_bits())];
        func.call(&mut self.store, &wasm_args, &mut results)
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("WASM trap: {e}")))?;

        match results[0] {
            wasmtime::Val::F32(bits) => Ok(f32::from_bits(bits)),
            wasmtime::Val::I32(i) => Ok(i as f32 / Q16_16_SCALE),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("unexpected WASM result type"),
            )),
        }
    }

    fn call_bool(&mut self, name: &str, args: &[GlslValue]) -> Result<bool, GlslError> {
        let i = self.call_i32(name, args)?;
        Ok(i != 0)
    }

    fn call_bvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&sig.return_type, dim),
            (Type::BVec2, 2) | (Type::BVec3, 3) | (Type::BVec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_bvec: function '{name}' returns {:?}, expected bvec{dim}",
                    sig.return_type
                ),
            ));
        }
        let results = self.call_wasm_multi(name, args)?;
        results
            .into_iter()
            .map(|r| match r {
                wasmtime::Val::I32(i) => Ok(i != 0),
                _ => Err(GlslError::new(
                    ErrorCode::E0400,
                    "WASM: unexpected result type in bvec call",
                )),
            })
            .collect()
    }

    fn call_ivec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&sig.return_type, dim),
            (Type::IVec2, 2) | (Type::IVec3, 3) | (Type::IVec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_ivec: function '{name}' returns {:?}, expected ivec{dim}",
                    sig.return_type
                ),
            ));
        }
        let results = self.call_wasm_multi(name, args)?;
        results
            .into_iter()
            .map(|r| match r {
                wasmtime::Val::I32(i) => Ok(i),
                _ => Err(GlslError::new(
                    ErrorCode::E0400,
                    "WASM: unexpected result type in ivec call",
                )),
            })
            .collect()
    }

    fn call_uvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&sig.return_type, dim),
            (Type::UVec2, 2) | (Type::UVec3, 3) | (Type::UVec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_uvec: function '{name}' returns {:?}, expected uvec{dim}",
                    sig.return_type
                ),
            ));
        }
        let results = self.call_wasm_multi(name, args)?;
        results
            .into_iter()
            .map(|r| match r {
                wasmtime::Val::I32(i) => Ok(i as u32),
                _ => Err(GlslError::new(
                    ErrorCode::E0400,
                    "WASM: unexpected result type in uvec call",
                )),
            })
            .collect()
    }

    fn call_vec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError> {
        let sig = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&sig.return_type, dim),
            (Type::Vec2, 2) | (Type::Vec3, 3) | (Type::Vec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_vec: function '{name}' returns {:?}, expected vec{dim}",
                    sig.return_type
                ),
            ));
        }
        let results = self.call_wasm_multi(name, args)?;
        let fm = self.float_mode;
        results
            .into_iter()
            .map(|r| match (r, fm) {
                (wasmtime::Val::I32(i), lp_glsl_wasm::FloatMode::Q32) => {
                    Ok(i as f32 / Q16_16_SCALE)
                }
                (wasmtime::Val::F32(bits), lp_glsl_wasm::FloatMode::Float) => {
                    Ok(f32::from_bits(bits))
                }
                _ => Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("WASM: unexpected result type in vec call (float_mode={fm:?})"),
                )),
            })
            .collect()
    }

    fn call_mat(
        &mut self,
        _name: &str,
        _args: &[GlslValue],
        _rows: usize,
        _cols: usize,
    ) -> Result<Vec<f32>, GlslError> {
        Err(GlslError::new(
            ErrorCode::E0400,
            "WASM: matrices not yet supported",
        ))
    }

    fn get_function_signature(&self, name: &str) -> Option<&FunctionSignature> {
        self.exports.get(name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.exports.keys().cloned().collect()
    }

    fn format_disassembly(&self) -> Option<String> {
        wasmprinter::print_bytes(&self.wasm_bytes).ok()
    }
}

fn glsl_param_to_wasm(ty: &Type, float_mode: lp_glsl_wasm::FloatMode) -> WasmValType {
    match ty {
        Type::Int | Type::UInt | Type::Bool => WasmValType::I32,
        Type::Float => match float_mode {
            lp_glsl_wasm::FloatMode::Q32 => WasmValType::I32,
            lp_glsl_wasm::FloatMode::Float => WasmValType::F32,
        },
        _ => panic!("WASM: unsupported param type {ty:?}"),
    }
}
