//! WASM execution via wasmtime, implementing GlslExecutable.

use crate::test_run::compile::DEFAULT_MAX_INSTRUCTIONS;
use lp_glsl_abi::GlslValue;
use lp_glsl_diagnostics::{ErrorCode, GlslDiagnostics, GlslError};
use lp_glsl_exec::GlslExecutable;
use lp_glsl_naga::GlslType;
use lp_glsl_wasm::glsl_type_to_wasm_components;
use lp_glsl_wasm::{glsl_wasm, GlslWasmError, WasmExport, WasmOptions, SHADOW_STACK_GLOBAL_EXPORT};
use lps_types::{FnParam, LpsFnSig, LpsType, ParamQualifier};
use std::collections::HashMap;
use wasm_encoder::ValType as WasmValType;
use wasmtime::{Config, Engine, Instance, Store, Val};

use crate::test_run::wasm_link;

/// Q16.16 fixed-point scale factor.
const Q16_16_SCALE: f32 = 65536.0;

/// Executable WASM module that implements GlslExecutable via wasmtime.
pub struct WasmExecutable {
    store: Store<()>,
    instance: Instance,
    exports: HashMap<String, WasmExport>,
    signatures: HashMap<String, LpsFnSig>,
    float_mode: lp_glsl_naga::FloatMode,
    wasm_bytes: Vec<u8>,
    shadow_stack_base: Option<i32>,
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
        let signatures: HashMap<String, LpsFnSig> = module
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
            shadow_stack_base: module.shadow_stack_base,
        })
    }

    fn prepare_call(&mut self) -> Result<(), GlslError> {
        if let Some(base) = self.shadow_stack_base {
            let g = self
                .instance
                .get_global(&mut self.store, SHADOW_STACK_GLOBAL_EXPORT)
                .ok_or_else(|| {
                    GlslError::new(ErrorCode::E0400, "missing shadow stack global export")
                })?;
            g.set(&mut self.store, wasmtime::Val::I32(base))
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("failed to reset shadow stack pointer: {e}"),
                    )
                })?;
        }
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

        let wasm_args = build_wasm_args(export_info, args, self.float_mode)?;

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

fn wasm_export_to_signature(export: &WasmExport) -> LpsFnSig {
    LpsFnSig {
        name: export.name.clone(),
        return_type: to_frontend_type(&export.return_type),
        parameters: export
            .param_types
            .iter()
            .enumerate()
            .map(|(i, ty)| FnParam {
                name: format!("p{i}"),
                ty: to_frontend_type(ty),
                qualifier: ParamQualifier::In,
            })
            .collect(),
    }
}

fn to_frontend_type(ty: &GlslType) -> LpsType {
    match ty {
        GlslType::Void => LpsType::Void,
        GlslType::Float => LpsType::Float,
        GlslType::Int => LpsType::Int,
        GlslType::UInt => LpsType::UInt,
        GlslType::Bool => LpsType::Bool,
        GlslType::Vec2 => LpsType::Vec2,
        GlslType::Vec3 => LpsType::Vec3,
        GlslType::Vec4 => LpsType::Vec4,
        GlslType::IVec2 => LpsType::IVec2,
        GlslType::IVec3 => LpsType::IVec3,
        GlslType::IVec4 => LpsType::IVec4,
        GlslType::UVec2 => LpsType::UVec2,
        GlslType::UVec3 => LpsType::UVec3,
        GlslType::UVec4 => LpsType::UVec4,
        GlslType::BVec2 => LpsType::BVec2,
        GlslType::BVec3 => LpsType::BVec3,
        GlslType::BVec4 => LpsType::BVec4,
        GlslType::Mat2 => LpsType::Mat2,
        GlslType::Mat3 => LpsType::Mat3,
        GlslType::Mat4 => LpsType::Mat4,
        GlslType::Array { element, len } => {
            LpsType::Array(Box::new(to_frontend_type(element)), *len as usize)
        }
        GlslType::Struct { .. } => LpsType::Struct(0),
    }
}

fn encode_f32_wasm(f: f32, fm: lp_glsl_naga::FloatMode) -> Val {
    match fm {
        lp_glsl_naga::FloatMode::Q32 => Val::I32((f * Q16_16_SCALE) as i32),
        lp_glsl_naga::FloatMode::F32 => Val::F32(f.to_bits()),
    }
}

fn wasm_val_to_f32(v: &Val, fm: lp_glsl_naga::FloatMode) -> Result<f32, GlslError> {
    match (v, fm) {
        (Val::I32(i), lp_glsl_naga::FloatMode::Q32) => Ok(*i as f32 / Q16_16_SCALE),
        (Val::F32(bits), lp_glsl_naga::FloatMode::F32) => Ok(f32::from_bits(*bits)),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("WASM: unexpected value for float (float_mode={fm:?})"),
        )),
    }
}

/// Flatten a logical [`GlslValue`] to WASM values for `ty` (matches [`glsl_type_to_wasm_components`] order).
fn glsl_value_to_wasm_flat(
    ty: &GlslType,
    v: &GlslValue,
    fm: lp_glsl_naga::FloatMode,
) -> Result<Vec<Val>, GlslError> {
    use GlslType::*;
    Ok(match (ty, v) {
        (Float, GlslValue::F32(f)) => vec![encode_f32_wasm(*f, fm)],
        (Int, GlslValue::I32(i)) => vec![Val::I32(*i)],
        (UInt, GlslValue::U32(u)) => vec![Val::I32(*u as i32)],
        (Bool, GlslValue::Bool(b)) => vec![Val::I32(if *b { 1 } else { 0 })],
        (Vec2, GlslValue::Vec2(a)) => vec![encode_f32_wasm(a[0], fm), encode_f32_wasm(a[1], fm)],
        (Vec3, GlslValue::Vec3(a)) => vec![
            encode_f32_wasm(a[0], fm),
            encode_f32_wasm(a[1], fm),
            encode_f32_wasm(a[2], fm),
        ],
        (Vec4, GlslValue::Vec4(a)) => vec![
            encode_f32_wasm(a[0], fm),
            encode_f32_wasm(a[1], fm),
            encode_f32_wasm(a[2], fm),
            encode_f32_wasm(a[3], fm),
        ],
        (IVec2, GlslValue::IVec2(a)) => vec![Val::I32(a[0]), Val::I32(a[1])],
        (IVec3, GlslValue::IVec3(a)) => vec![Val::I32(a[0]), Val::I32(a[1]), Val::I32(a[2])],
        (IVec4, GlslValue::IVec4(a)) => vec![
            Val::I32(a[0]),
            Val::I32(a[1]),
            Val::I32(a[2]),
            Val::I32(a[3]),
        ],
        (UVec2, GlslValue::UVec2(a)) => vec![Val::I32(a[0] as i32), Val::I32(a[1] as i32)],
        (UVec3, GlslValue::UVec3(a)) => vec![
            Val::I32(a[0] as i32),
            Val::I32(a[1] as i32),
            Val::I32(a[2] as i32),
        ],
        (UVec4, GlslValue::UVec4(a)) => vec![
            Val::I32(a[0] as i32),
            Val::I32(a[1] as i32),
            Val::I32(a[2] as i32),
            Val::I32(a[3] as i32),
        ],
        (BVec2, GlslValue::BVec2(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
        ],
        (BVec3, GlslValue::BVec3(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
            Val::I32(if a[2] { 1 } else { 0 }),
        ],
        (BVec4, GlslValue::BVec4(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
            Val::I32(if a[2] { 1 } else { 0 }),
            Val::I32(if a[3] { 1 } else { 0 }),
        ],
        (Mat2, GlslValue::Mat2x2(m)) => vec![
            encode_f32_wasm(m[0][0], fm),
            encode_f32_wasm(m[0][1], fm),
            encode_f32_wasm(m[1][0], fm),
            encode_f32_wasm(m[1][1], fm),
        ],
        (Mat3, GlslValue::Mat3x3(m)) => {
            let mut v = Vec::with_capacity(9);
            for col in m.iter() {
                for x in col.iter() {
                    v.push(encode_f32_wasm(*x, fm));
                }
            }
            v
        }
        (Mat4, GlslValue::Mat4x4(m)) => {
            let mut v = Vec::with_capacity(16);
            for col in m.iter() {
                for x in col.iter() {
                    v.push(encode_f32_wasm(*x, fm));
                }
            }
            v
        }
        (Array { element, len }, GlslValue::Array(items)) => {
            if items.len() != *len as usize {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "array value length {} does not match type length {}",
                        items.len(),
                        len
                    ),
                ));
            }
            let mut out = Vec::new();
            for it in items.iter() {
                out.extend(glsl_value_to_wasm_flat(element, it, fm)?);
            }
            out
        }
        (Struct { members, .. }, GlslValue::Struct { fields, .. }) => {
            if members.len() != fields.len() {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "struct field count {} does not match type field count {}",
                        fields.len(),
                        members.len()
                    ),
                ));
            }
            let mut out = Vec::new();
            for (m, (_, fv)) in members.iter().zip(fields.iter()) {
                out.extend(glsl_value_to_wasm_flat(&m.ty, fv, fm)?);
            }
            out
        }
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("WASM: value {v:?} does not match parameter type {ty:?}"),
            ));
        }
    })
}

fn build_wasm_args(
    export_info: &WasmExport,
    args: &[GlslValue],
    fm: lp_glsl_naga::FloatMode,
) -> Result<Vec<Val>, GlslError> {
    if args.len() != export_info.param_types.len() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "wrong argument count: expected {}, got {}",
                export_info.param_types.len(),
                args.len()
            ),
        ));
    }
    let mut wasm_args = Vec::new();
    // Prepend VMContext (i32) as first argument for all shader function calls.
    // VMContext is always the first parameter in the WASM signature.
    wasm_args.push(Val::I32(0)); // Dummy VMContext pointer
    for (v, ty) in args.iter().zip(export_info.param_types.iter()) {
        wasm_args.extend(glsl_value_to_wasm_flat(ty, v, fm)?);
    }
    if wasm_args.len() != export_info.params.len() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "internal: flattened arg count {} != export param slots {}",
                wasm_args.len(),
                export_info.params.len()
            ),
        ));
    }
    Ok(wasm_args)
}

/// Decode flattened WASM result values into a [`GlslValue`]; returns `(value, slots_consumed)`.
fn wasm_vals_to_glsl_value(
    ty: &GlslType,
    vals: &[Val],
    fm: lp_glsl_naga::FloatMode,
) -> Result<(GlslValue, usize), GlslError> {
    use GlslType::*;
    match ty {
        Void => Err(GlslError::new(
            ErrorCode::E0400,
            "WASM: void type in wasm_vals_to_glsl_value",
        )),
        Float => {
            let f = wasm_val_to_f32(&vals[0], fm)?;
            Ok((GlslValue::F32(f), 1))
        }
        Int => match vals.first() {
            Some(Val::I32(i)) => Ok((GlslValue::I32(*i), 1)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 for int return",
            )),
        },
        UInt => match vals.first() {
            Some(Val::I32(i)) => Ok((GlslValue::U32(*i as u32), 1)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 for uint return",
            )),
        },
        Bool => match vals.first() {
            Some(Val::I32(i)) => Ok((GlslValue::Bool(*i != 0), 1)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 for bool return",
            )),
        },
        Vec2 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            Ok((GlslValue::Vec2([a, b]), 2))
        }
        Vec3 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            let c = wasm_val_to_f32(&vals[2], fm)?;
            Ok((GlslValue::Vec3([a, b, c]), 3))
        }
        Vec4 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            let c = wasm_val_to_f32(&vals[2], fm)?;
            let d = wasm_val_to_f32(&vals[3], fm)?;
            Ok((GlslValue::Vec4([a, b, c, d]), 4))
        }
        IVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((GlslValue::IVec2([*a, *b]), 2)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 pair for ivec2",
            )),
        },
        IVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => Ok((GlslValue::IVec3([*a, *b, *c]), 3)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 triple for ivec3",
            )),
        },
        IVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => {
                Ok((GlslValue::IVec4([*a, *b, *c, *d]), 4))
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected four i32 for ivec4",
            )),
        },
        UVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((GlslValue::UVec2([*a as u32, *b as u32]), 2)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 pair for uvec2",
            )),
        },
        UVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => {
                Ok((GlslValue::UVec3([*a as u32, *b as u32, *c as u32]), 3))
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 triple for uvec3",
            )),
        },
        UVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => Ok((
                GlslValue::UVec4([*a as u32, *b as u32, *c as u32, *d as u32]),
                4,
            )),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected four i32 for uvec4",
            )),
        },
        BVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((GlslValue::BVec2([*a != 0, *b != 0]), 2)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 pair for bvec2",
            )),
        },
        BVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => {
                Ok((GlslValue::BVec3([*a != 0, *b != 0, *c != 0]), 3))
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected i32 triple for bvec3",
            )),
        },
        BVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => {
                Ok((GlslValue::BVec4([*a != 0, *b != 0, *c != 0, *d != 0]), 4))
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: expected four i32 for bvec4",
            )),
        },
        Mat2 => {
            let mut col0 = [0f32; 2];
            let mut col1 = [0f32; 2];
            col0[0] = wasm_val_to_f32(&vals[0], fm)?;
            col0[1] = wasm_val_to_f32(&vals[1], fm)?;
            col1[0] = wasm_val_to_f32(&vals[2], fm)?;
            col1[1] = wasm_val_to_f32(&vals[3], fm)?;
            Ok((GlslValue::Mat2x2([col0, col1]), 4))
        }
        Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                for row in 0..3 {
                    m[col][row] = wasm_val_to_f32(&vals[col * 3 + row], fm)?;
                }
            }
            Ok((GlslValue::Mat3x3(m), 9))
        }
        Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for col in 0..4 {
                for row in 0..4 {
                    m[col][row] = wasm_val_to_f32(&vals[col * 4 + row], fm)?;
                }
            }
            Ok((GlslValue::Mat4x4(m), 16))
        }
        Array { element, len } => {
            let mut off = 0;
            let mut elems = Vec::with_capacity(*len as usize);
            for _ in 0..*len {
                let (v, n) = wasm_vals_to_glsl_value(element, &vals[off..], fm)?;
                off += n;
                elems.push(v);
            }
            Ok((GlslValue::Array(elems.into_boxed_slice()), off))
        }
        Struct { name, members } => {
            let mut off = 0;
            let mut fields = Vec::with_capacity(members.len());
            for m in members {
                let key = m
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("_{}", fields.len()));
                let (v, n) = wasm_vals_to_glsl_value(&m.ty, &vals[off..], fm)?;
                off += n;
                fields.push((key, v));
            }
            Ok((
                GlslValue::Struct {
                    name: name.clone(),
                    fields,
                },
                off,
            ))
        }
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

        let wasm_args = build_wasm_args(export_info, args, self.float_mode)?;

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

        let wasm_args = build_wasm_args(export_info, args, self.float_mode)?;

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

        let wasm_args = build_wasm_args(export_info, args, self.float_mode)?;

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
                (wasmtime::Val::F32(bits), lp_glsl_naga::FloatMode::F32) => {
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
        name: &str,
        args: &[GlslValue],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f32>, GlslError> {
        let export_info = self.exports.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
        })?;
        let ok = matches!(
            (&export_info.return_type, rows, cols),
            (GlslType::Mat2, 2, 2) | (GlslType::Mat3, 3, 3) | (GlslType::Mat4, 4, 4)
        );
        if !ok {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_mat: function '{name}' returns {:?}, expected mat{rows}x{cols}",
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
                (wasmtime::Val::F32(bits), lp_glsl_naga::FloatMode::F32) => {
                    Ok(f32::from_bits(bits))
                }
                _ => Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("WASM: unexpected result type in mat call (float_mode={fm:?})"),
                )),
            })
            .collect()
    }

    fn call_array(
        &mut self,
        name: &str,
        args: &[GlslValue],
        elem_ty: &LpsType,
        len: usize,
    ) -> Result<Vec<GlslValue>, GlslError> {
        let return_type = self
            .exports
            .get(name)
            .map(|e| e.return_type.clone())
            .ok_or_else(|| {
                GlslError::new(ErrorCode::E0101, format!("function '{name}' not found"))
            })?;
        let expected = LpsType::Array(Box::new(elem_ty.clone()), len);
        if to_frontend_type(&return_type) != expected {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_array: function '{name}' returns {:?}, expected {:?}",
                    return_type, expected
                ),
            ));
        }
        let results = self.call_wasm_multi(name, args)?;
        let (val, consumed) = wasm_vals_to_glsl_value(&return_type, &results, self.float_mode)?;
        if consumed != results.len() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "WASM: return slot count mismatch: decoded {consumed}, got {}",
                    results.len()
                ),
            ));
        }
        match val {
            GlslValue::Array(items) => Ok(Vec::from(items)),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "WASM: internal: array return was not decoded as Array",
            )),
        }
    }

    fn get_function_signature(&self, name: &str) -> Option<&LpsFnSig> {
        self.signatures.get(name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.exports.keys().cloned().collect()
    }

    fn format_disassembly(&self) -> Option<String> {
        wasmprinter::print_bytes(&self.wasm_bytes).ok()
    }
}
