//! WASM execution via wasmtime, implementing GlslExecutable.

use crate::test_run::compile::DEFAULT_MAX_INSTRUCTIONS;
use lp_glsl_cranelift::semantic::functions::{FunctionSignature, ParamQualifier, Parameter};
use lp_glsl_cranelift::semantic::types::Type;
use lp_glsl_cranelift::{ErrorCode, GlslDiagnostics, GlslError, GlslExecutable, GlslValue};
use lp_glsl_naga::GlslType;
use lp_glsl_wasm::types::glsl_type_to_wasm_components;
use lp_glsl_wasm::{GlslWasmError, WasmExport, WasmOptions, glsl_wasm};
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
    exports: HashMap<String, WasmExport>,
    signatures: HashMap<String, FunctionSignature>,
    float_mode: lp_glsl_naga::FloatMode,
    wasm_bytes: Vec<u8>,
}

impl WasmExecutable {
    /// Compile GLSL source to WASM and create an executable.
    pub fn from_source(source: &str, options: WasmOptions) -> Result<Self, GlslDiagnostics> {
        let module = glsl_wasm(source, options.clone()).map_err(glsl_wasm_error_to_diagnostics)?;
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
        let (instance, _memory) =
            wasm_link::instantiate_wasm_module(&engine, &mut store, &wasm_bytes)
                .map_err(|e| GlslDiagnostics::from(e))?;

        let exports: HashMap<String, WasmExport> = module
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.clone()))
            .collect();
        let signatures: HashMap<String, FunctionSignature> = module
            .exports
            .iter()
            .map(|e| (e.name.clone(), wasm_export_to_signature(e)))
            .collect();

        Ok(Self {
            store,
            instance,
            exports,
            signatures,
            float_mode: options.float_mode,
            wasm_bytes,
        })
    }

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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;

        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = export_info.params.clone();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.float_mode)?);
        }

        let result_types = glsl_type_to_wasm_components(&export_info.return_type, self.float_mode);
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

fn glsl_wasm_error_to_diagnostics(e: GlslWasmError) -> GlslDiagnostics {
    GlslDiagnostics::from(GlslError::new(ErrorCode::E0400, e.to_string()))
}

fn wasm_export_to_signature(export: &WasmExport) -> FunctionSignature {
    FunctionSignature {
        name: export.name.clone(),
        return_type: to_frontend_type(&export.return_type),
        parameters: export
            .param_types
            .iter()
            .enumerate()
            .map(|(i, ty)| Parameter {
                name: format!("p{i}"),
                ty: to_frontend_type(ty),
                qualifier: ParamQualifier::In,
            })
            .collect(),
    }
}

fn to_frontend_type(ty: &GlslType) -> Type {
    match ty {
        GlslType::Void => Type::Void,
        GlslType::Float => Type::Float,
        GlslType::Int => Type::Int,
        GlslType::UInt => Type::UInt,
        GlslType::Bool => Type::Bool,
        GlslType::Vec2 => Type::Vec2,
        GlslType::Vec3 => Type::Vec3,
        GlslType::Vec4 => Type::Vec4,
        GlslType::IVec2 => Type::IVec2,
        GlslType::IVec3 => Type::IVec3,
        GlslType::IVec4 => Type::IVec4,
        GlslType::UVec2 => Type::UVec2,
        GlslType::UVec3 => Type::UVec3,
        GlslType::UVec4 => Type::UVec4,
        GlslType::BVec2 => Type::BVec2,
        GlslType::BVec3 => Type::BVec3,
        GlslType::BVec4 => Type::BVec4,
    }
}

fn glsl_value_to_wasm(
    v: &GlslValue,
    expected: WasmValType,
    float_mode: lp_glsl_naga::FloatMode,
) -> Result<wasmtime::Val, GlslError> {
    use wasmtime::Val;

    match expected {
        WasmValType::I32 => match v {
            GlslValue::I32(x) => Ok(Val::I32(*x)),
            GlslValue::U32(x) => Ok(Val::I32(*x as i32)),
            GlslValue::Bool(b) => Ok(Val::I32(if *b { 1 } else { 0 })),
            GlslValue::F32(f) => {
                if matches!(float_mode, lp_glsl_naga::FloatMode::Q32) {
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = export_info.params.clone();
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = export_info.params.clone();
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;

        let param_types: Vec<WasmValType> = export_info.params.clone();
        let mut wasm_args: Vec<wasmtime::Val> = Vec::with_capacity(args.len());
        for (v, t) in args.iter().zip(param_types.iter()) {
            wasm_args.push(glsl_value_to_wasm(v, *t, self.float_mode)?);
        }

        let return_type = export_info.return_type.clone();
        self.prepare_call()?;
        let result_types = glsl_type_to_wasm_components(&return_type, self.float_mode);
        let mut results: Vec<wasmtime::Val> = result_types
            .iter()
            .map(|t| match t {
                WasmValType::I32 => wasmtime::Val::I32(0),
                WasmValType::F32 => wasmtime::Val::F32(0f32.to_bits()),
                _ => wasmtime::Val::I32(0),
            })
            .collect();
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&export_info.return_type, dim),
            (GlslType::BVec2, 2) | (GlslType::BVec3, 3) | (GlslType::BVec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_bvec: function '{name}' returns {:?}, expected bvec{dim}",
                    export_info.return_type
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&export_info.return_type, dim),
            (GlslType::IVec2, 2) | (GlslType::IVec3, 3) | (GlslType::IVec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_ivec: function '{name}' returns {:?}, expected ivec{dim}",
                    export_info.return_type
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&export_info.return_type, dim),
            (GlslType::UVec2, 2) | (GlslType::UVec3, 3) | (GlslType::UVec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_uvec: function '{name}' returns {:?}, expected uvec{dim}",
                    export_info.return_type
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
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&export_info.return_type, dim),
            (GlslType::Vec2, 2) | (GlslType::Vec3, 3) | (GlslType::Vec4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_vec: function '{name}' returns {:?}, expected vec{dim}",
                    export_info.return_type
                ),
            ));
        }
        let results = self.call_wasm_multi(name, args)?;
        let fm = self.float_mode;
        results
            .into_iter()
            .map(|r| match (r, fm) {
                (wasmtime::Val::I32(i), lp_glsl_naga::FloatMode::Q32) => {
                    Ok(i as f32 / Q16_16_SCALE)
                }
                (wasmtime::Val::F32(bits), lp_glsl_naga::FloatMode::Float) => {
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
        self.signatures.get(name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.exports.keys().cloned().collect()
    }

    fn format_disassembly(&self) -> Option<String> {
        wasmprinter::print_bytes(&self.wasm_bytes).ok()
    }
}
