//! WASM execution via wasmtime, implementing GlslExecutable.

use lp_glsl_cranelift::semantic::functions::FunctionSignature;
use lp_glsl_cranelift::semantic::types::Type;
use lp_glsl_cranelift::{ErrorCode, GlslDiagnostics, GlslError, GlslExecutable, GlslValue};
use lp_glsl_wasm::{WasmOptions, glsl_wasm};
use std::collections::HashMap;
use wasm_encoder::ValType as WasmValType;
use wasmtime::{Engine, Instance, Module, Store};

/// Q16.16 fixed-point scale factor.
const Q16_16_SCALE: f32 = 65536.0;

/// Executable WASM module that implements GlslExecutable via wasmtime.
pub struct WasmExecutable {
    store: Store<()>,
    instance: Instance,
    exports: HashMap<String, FunctionSignature>,
    decimal_format: lp_glsl_wasm::DecimalFormat,
}

impl WasmExecutable {
    /// Compile GLSL source to WASM and create an executable.
    pub fn from_source(source: &str, options: WasmOptions) -> Result<Self, GlslDiagnostics> {
        let module = glsl_wasm(source, options.clone())?;
        let engine = Engine::default();
        let mut store = Store::new(&engine, ());
        let wasm_module = Module::new(&engine, &module.bytes).map_err(|e| {
            GlslDiagnostics::from(GlslError::new(
                ErrorCode::E0400,
                format!("failed to load WASM module: {e}"),
            ))
        })?;
        let instance = wasmtime::Instance::new(&mut store, &wasm_module, &[]).map_err(|e| {
            GlslDiagnostics::from(GlslError::new(
                ErrorCode::E0400,
                format!("failed to instantiate WASM: {e}"),
            ))
        })?;

        let exports: HashMap<String, FunctionSignature> = module
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.signature.clone()))
            .collect();

        Ok(Self {
            store,
            instance,
            exports,
            decimal_format: options.decimal_format,
        })
    }
}

fn glsl_value_to_wasm(
    v: &GlslValue,
    expected: WasmValType,
    decimal_format: lp_glsl_wasm::DecimalFormat,
) -> Result<wasmtime::Val, GlslError> {
    use wasmtime::Val;

    match expected {
        WasmValType::I32 => match v {
            GlslValue::I32(x) => Ok(Val::I32(*x)),
            GlslValue::U32(x) => Ok(Val::I32(*x as i32)),
            GlslValue::Bool(b) => Ok(Val::I32(if *b { 1 } else { 0 })),
            GlslValue::F32(f) => {
                if matches!(decimal_format, lp_glsl_wasm::DecimalFormat::Q32) {
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
                format!("unsupported GlslValue {:?} for i32 param", v),
            )),
        },
        WasmValType::F32 => match v {
            GlslValue::F32(f) => Ok(Val::F32(f.to_bits())),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("expected f32, got {:?}", v),
            )),
        },
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unsupported WASM param type {:?}", expected),
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
            .map(|p| glsl_param_to_wasm(&p.ty, self.decimal_format))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.decimal_format)?);
        }

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
            .map(|p| glsl_param_to_wasm(&p.ty, self.decimal_format))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.decimal_format)?);
        }

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
            .map(|p| glsl_param_to_wasm(&p.ty, self.decimal_format))
            .collect();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.decimal_format)?);
        }

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
        _name: &str,
        _args: &[GlslValue],
        _dim: usize,
    ) -> Result<Vec<bool>, GlslError> {
        Err(GlslError::new(
            ErrorCode::E0400,
            "WASM: vectors not yet supported",
        ))
    }

    fn call_ivec(
        &mut self,
        _name: &str,
        _args: &[GlslValue],
        _dim: usize,
    ) -> Result<Vec<i32>, GlslError> {
        Err(GlslError::new(
            ErrorCode::E0400,
            "WASM: vectors not yet supported",
        ))
    }

    fn call_uvec(
        &mut self,
        _name: &str,
        _args: &[GlslValue],
        _dim: usize,
    ) -> Result<Vec<u32>, GlslError> {
        Err(GlslError::new(
            ErrorCode::E0400,
            "WASM: vectors not yet supported",
        ))
    }

    fn call_vec(
        &mut self,
        _name: &str,
        _args: &[GlslValue],
        _dim: usize,
    ) -> Result<Vec<f32>, GlslError> {
        Err(GlslError::new(
            ErrorCode::E0400,
            "WASM: vectors not yet supported",
        ))
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
}

fn glsl_param_to_wasm(ty: &Type, decimal_format: lp_glsl_wasm::DecimalFormat) -> WasmValType {
    match ty {
        Type::Int | Type::UInt | Type::Bool => WasmValType::I32,
        Type::Float => match decimal_format {
            lp_glsl_wasm::DecimalFormat::Q32 => WasmValType::I32,
            lp_glsl_wasm::DecimalFormat::Float => WasmValType::F32,
        },
        _ => panic!("WASM: unsupported param type {:?}", ty),
    }
}
