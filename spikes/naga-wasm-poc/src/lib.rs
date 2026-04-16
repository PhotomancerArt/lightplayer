//! Spike: parse a tiny GLSL function with Naga, emit WASM, run in tests.
//!
//! `#![no_std]` in non-test builds validates that `naga` with `glsl-in` builds without the Rust
//! standard library. Unit/integration tests compile with `std` (see `cfg_attr` below).

#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use alloc::{collections::BTreeMap, string::String, vec::Vec};
#[cfg(test)]
use std::{collections::BTreeMap, string::String, vec::Vec};

use core::fmt::{self, Write as _};

use naga::{
    BinaryOperator, Expression, Function, Handle, LocalVariable, Module, ScalarKind, Statement,
    TypeInner,
};
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function as WasmFunction, FunctionSection, Instruction,
    Module as WasmModule, TypeSection, ValType,
};

/// How `float` in GLSL is represented in WASM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericMode {
    /// WASM `f32` / `f32.add`.
    Float,
    /// 16.16 fixed point in WASM `i32` / `i32.add` (no saturation in this spike).
    Q32,
}

#[derive(Debug)]
pub enum CompileError {
    GlslParse(String),
    FunctionNotFound(String),
    MissingReturn,
    UnsupportedFeature(&'static str),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::GlslParse(msg) => write!(f, "GLSL parse: {msg}"),
            CompileError::FunctionNotFound(name) => {
                write!(f, "no function named `{name}` in module")
            }
            CompileError::MissingReturn => write!(f, "function body has no return value"),
            CompileError::UnsupportedFeature(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl core::error::Error for CompileError {}

pub struct CompileResult {
    pub wasm_bytes: Vec<u8>,
    pub export_name: String,
}

/// Compile `source` to a WASM module exporting `export_name` (must exist in GLSL).
pub fn compile(
    source: &str,
    export_name: &str,
    mode: NumericMode,
) -> Result<CompileResult, CompileError> {
    let module = parse_glsl(source)?;
    let func = find_function(&module, export_name)?;
    let wasm_bytes = emit_module(&module, func, export_name, mode)?;
    Ok(CompileResult {
        wasm_bytes,
        export_name: String::from(export_name),
    })
}

fn parse_glsl(source: &str) -> Result<Module, CompileError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Vertex);
    frontend.parse(&options, source).map_err(|e| {
        let mut msg = String::new();
        let _ = write!(&mut msg, "{e}");
        CompileError::GlslParse(msg)
    })
}

fn find_function<'a>(module: &'a Module, name: &str) -> Result<&'a Function, CompileError> {
    for (_, f) in module.functions.iter() {
        if f.name.as_deref() == Some(name) {
            return Ok(f);
        }
    }
    Err(CompileError::FunctionNotFound(String::from(name)))
}

fn find_return_expr(block: &naga::Block) -> Option<Handle<Expression>> {
    for stmt in block.iter() {
        match stmt {
            Statement::Return { value } => return *value,
            Statement::Block(inner) => {
                if let Some(h) = find_return_expr(inner) {
                    return Some(h);
                }
            }
            _ => {}
        }
    }
    None
}

fn emit_module(
    module: &Module,
    func: &Function,
    export_name: &str,
    mode: NumericMode,
) -> Result<Vec<u8>, CompileError> {
    let ret = find_return_expr(&func.body).ok_or(CompileError::MissingReturn)?;

    let (param_ty, result_ty) = wasm_val_types(mode)?;
    for arg in &func.arguments {
        let inner = &module.types[arg.ty].inner;
        if !matches!(
            inner,
            TypeInner::Scalar(s) if s.kind == ScalarKind::Float && s.width == 4
        ) {
            return Err(CompileError::UnsupportedFeature(
                "only float (f32) parameters supported",
            ));
        }
    }
    if let Some(res) = &func.result {
        let inner = &module.types[res.ty].inner;
        if !matches!(
            inner,
            TypeInner::Scalar(s) if s.kind == ScalarKind::Float && s.width == 4
        ) {
            return Err(CompileError::UnsupportedFeature(
                "only float (f32) return supported",
            ));
        }
    } else {
        return Err(CompileError::UnsupportedFeature(
            "function must return float",
        ));
    }

    let num_params = func.arguments.len();
    let param_aliases = param_local_to_argument(func);
    let mut wasm_fn = WasmFunction::new([]);
    emit_expr(module, func, ret, &mut wasm_fn, mode, &param_aliases)?;
    wasm_fn.instruction(&Instruction::Return);
    wasm_fn.instruction(&Instruction::End);

    let mut types = TypeSection::new();
    let params: Vec<ValType> = (0..num_params).map(|_| param_ty).collect();
    types.ty().function(params, [result_ty]);

    let mut func_sec = FunctionSection::new();
    func_sec.function(0);

    let mut exports = ExportSection::new();
    exports.export(export_name, ExportKind::Func, 0);

    let mut code = CodeSection::new();
    code.function(&wasm_fn);

    let mut out = WasmModule::new();
    out.section(&types);
    out.section(&func_sec);
    out.section(&exports);
    out.section(&code);

    Ok(out.finish())
}

/// Naga's GLSL frontend models `in` parameters as `LocalVariable`s initialized from
/// `FunctionArgument` via `Store`. WASM params already live in `local.get 0..n`, so we map each
/// such `LocalVariable` back to its argument index and emit `local.get` for loads.
fn param_local_to_argument(func: &Function) -> BTreeMap<Handle<LocalVariable>, u32> {
    let mut m = BTreeMap::new();
    fn walk(block: &naga::Block, func: &Function, m: &mut BTreeMap<Handle<LocalVariable>, u32>) {
        for stmt in block.iter() {
            match stmt {
                Statement::Store { pointer, value } => {
                    if let (Expression::LocalVariable(lv), Expression::FunctionArgument(idx)) =
                        (&func.expressions[*pointer], &func.expressions[*value])
                    {
                        m.insert(*lv, *idx);
                    }
                }
                Statement::Block(inner) => walk(inner, func, m),
                _ => {}
            }
        }
    }
    walk(&func.body, func, &mut m);
    m
}

fn wasm_val_types(mode: NumericMode) -> Result<(ValType, ValType), CompileError> {
    let v = match mode {
        NumericMode::Float => ValType::F32,
        NumericMode::Q32 => ValType::I32,
    };
    Ok((v, v))
}

fn emit_expr(
    _module: &Module,
    func: &Function,
    expr: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: NumericMode,
    param_aliases: &BTreeMap<Handle<LocalVariable>, u32>,
) -> Result<(), CompileError> {
    match &func.expressions[expr] {
        Expression::FunctionArgument(index) => {
            wasm_fn.instruction(&Instruction::LocalGet(*index));
            Ok(())
        }
        Expression::Load { pointer } => match &func.expressions[*pointer] {
            Expression::LocalVariable(lv) => {
                let idx = param_aliases
                    .get(lv)
                    .ok_or(CompileError::UnsupportedFeature(
                        "load from non-parameter local",
                    ))?;
                wasm_fn.instruction(&Instruction::LocalGet(*idx));
                Ok(())
            }
            _ => Err(CompileError::UnsupportedFeature(
                "load from non-local pointer",
            )),
        },
        Expression::Binary { op, left, right } => {
            if *op != BinaryOperator::Add {
                return Err(CompileError::UnsupportedFeature("only `+` supported"));
            }
            emit_expr(_module, func, *left, wasm_fn, mode, param_aliases)?;
            emit_expr(_module, func, *right, wasm_fn, mode, param_aliases)?;
            match mode {
                NumericMode::Float => {
                    wasm_fn.instruction(&Instruction::F32Add);
                }
                NumericMode::Q32 => {
                    wasm_fn.instruction(&Instruction::I32Add);
                }
            }
            Ok(())
        }
        _ => Err(CompileError::UnsupportedFeature(
            "unsupported expression (expected load, argument, or float add)",
        )),
    }
}
