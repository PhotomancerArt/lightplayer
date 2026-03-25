//! Host JIT module: finalized code, GLSL metadata, signatures.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use cranelift_codegen::ir::{FuncRef, Signature, StackSlot, StackSlotData, StackSlotKind, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use lpir::module::IrModule;
use lpir::op::Op;
use lpir::{FloatMode, GlslModuleMeta};

use crate::builtins;
use crate::emit::{self, LpirBuiltinRefs, translate_function};
use crate::error::{CompileError, CompilerError};

/// Options for LPIR → JIT compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub float_mode: FloatMode,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
        }
    }
}

/// Finalized JIT shader module with GLSL metadata for typed calls.
pub struct JitModule {
    pub(crate) inner: JITModule,
    pub(crate) glsl_meta: GlslModuleMeta,
    pub(crate) func_names: Vec<String>,
    pub(crate) func_ids: Vec<FuncId>,
    pub(crate) name_to_index: BTreeMap<String, usize>,
    pub(crate) signatures: BTreeMap<String, Signature>,
    pub(crate) ir_param_counts: Vec<u16>,
    pub(crate) call_conv: CallConv,
    pub(crate) pointer_type: types::Type,
    pub(crate) float_mode: FloatMode,
}

impl JitModule {
    /// Raw finalized code pointer for a function index (same order as source [`IrModule::functions`]).
    pub fn finalized_ptr_by_index(&self, index: usize) -> *const u8 {
        self.inner.get_finalized_function(self.func_ids[index])
    }

    /// Raw finalized code pointer by GLSL / LPIR function name.
    pub fn finalized_ptr(&self, name: &str) -> Option<*const u8> {
        let i = *self.name_to_index.get(name)?;
        Some(self.finalized_ptr_by_index(i))
    }

    /// Cranelift signature recorded for `name`.
    pub fn signature(&self, name: &str) -> Option<&Signature> {
        self.signatures.get(name)
    }

    pub fn call_conv(&self) -> CallConv {
        self.call_conv
    }

    pub fn pointer_type(&self) -> types::Type {
        self.pointer_type
    }

    pub fn float_mode(&self) -> FloatMode {
        self.float_mode
    }

    pub fn glsl_meta(&self) -> &GlslModuleMeta {
        &self.glsl_meta
    }

    /// LPIR function names in module order (same indices as [`Self::finalized_ptr_by_index`]).
    pub fn func_names(&self) -> &[String] {
        &self.func_names
    }
}

pub(crate) fn build_jit_module(
    ir: &IrModule,
    glsl_meta: GlslModuleMeta,
    options: CompileOptions,
) -> Result<JitModule, CompilerError> {
    let mode = options.float_mode;
    if mode == FloatMode::F32 && !ir.imports.is_empty() {
        return Err(CompilerError::Codegen(CompileError::unsupported(
            "LPIR imports require FloatMode::Q32 in lpir-cranelift",
        )));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "regalloc_algorithm: {e}"
            )))
        })?;
    let flags = settings::Flags::new(flag_builder);

    let isa = cranelift_native::builder()
        .map_err(|m| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "native ISA detection: {m}"
            )))
        })?
        .finish(flags)
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("ISA: {e}"))))?;

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    jit_builder.symbol_lookup_fn(builtins::symbol_lookup_fn());
    let mut jit_module = JITModule::new(jit_builder);

    let call_conv = jit_module.isa().default_call_conv();
    let pointer_type = jit_module.isa().pointer_type();

    let import_func_ids = if mode == FloatMode::Q32 {
        builtins::declare_module_imports(&mut jit_module, ir, pointer_type)
            .map_err(CompilerError::Codegen)?
    } else {
        Vec::new()
    };

    let lpir_builtin_ids = if mode == FloatMode::Q32 {
        Some(
            builtins::declare_lpir_opcode_builtins(&mut jit_module, pointer_type)
                .map_err(CompilerError::Codegen)?,
        )
    } else {
        None
    };

    let mut func_ids = Vec::with_capacity(ir.functions.len());
    let mut signatures = BTreeMap::new();
    let mut func_names = Vec::with_capacity(ir.functions.len());
    let mut ir_param_counts = Vec::with_capacity(ir.functions.len());

    for f in &ir.functions {
        let sig = emit::signature_for_ir_func(f, call_conv, mode);
        let id = jit_module
            .declare_function(&f.name, Linkage::Export, &sig)
            .map_err(|e| {
                CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                    "declare {}: {e}",
                    f.name
                )))
            })?;
        signatures.insert(f.name.clone(), sig);
        func_names.push(f.name.clone());
        ir_param_counts.push(f.param_count);
        func_ids.push(id);
    }

    let mut name_to_index = BTreeMap::new();
    for (i, name) in func_names.iter().enumerate() {
        name_to_index.insert(name.clone(), i);
    }

    let mut ctx = jit_module.make_context();

    for (f, fid) in ir.functions.iter().zip(&func_ids) {
        ctx.clear();
        ctx.func.signature = emit::signature_for_ir_func(f, call_conv, mode);
        let mut func_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);

            let slots: Vec<StackSlot> = f
                .slots
                .iter()
                .map(|sd| {
                    builder.func.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        sd.size,
                        0,
                    ))
                })
                .collect();

            let func_refs: Vec<FuncRef> = func_ids
                .iter()
                .map(|id| jit_module.declare_func_in_func(*id, builder.func))
                .collect();

            let import_func_refs: Vec<FuncRef> = import_func_ids
                .iter()
                .map(|id| jit_module.declare_func_in_func(*id, builder.func))
                .collect();

            let lpir_builtins = lpir_builtin_ids.as_ref().map(|ids| LpirBuiltinRefs {
                fadd: jit_module.declare_func_in_func(ids.fadd, builder.func),
                fsub: jit_module.declare_func_in_func(ids.fsub, builder.func),
                fmul: jit_module.declare_func_in_func(ids.fmul, builder.func),
                fdiv: jit_module.declare_func_in_func(ids.fdiv, builder.func),
                fsqrt: jit_module.declare_func_in_func(ids.fsqrt, builder.func),
                fnearest: jit_module.declare_func_in_func(ids.fnearest, builder.func),
            });

            let mut vreg_is_stack_addr = vec![false; f.vreg_types.len()];
            for op in &f.body {
                if let Op::SlotAddr { dst, .. } = op {
                    let i = dst.0 as usize;
                    if let Some(slot) = vreg_is_stack_addr.get_mut(i) {
                        *slot = true;
                    }
                }
            }

            let emit_ctx = emit::EmitCtx {
                func_refs: &func_refs,
                import_func_refs: &import_func_refs,
                slots: &slots,
                ir,
                pointer_type,
                vreg_is_stack_addr,
                float_mode: mode,
                lpir_builtins,
            };

            translate_function(f, &mut builder, &emit_ctx).map_err(|e| {
                CompilerError::Codegen(match e {
                    CompileError::Unsupported(s) => {
                        CompileError::unsupported(alloc::format!("in function `{}`: {s}", f.name))
                    }
                    CompileError::Cranelift(s) => {
                        CompileError::cranelift(alloc::format!("in function `{}`: {s}", f.name))
                    }
                })
            })?;
            builder.finalize();
        }
        jit_module.define_function(*fid, &mut ctx).map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "define {}: {e}",
                f.name
            )))
        })?;
    }

    jit_module.finalize_definitions().map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(alloc::format!(
            "finalize_definitions: {e}"
        )))
    })?;

    Ok(JitModule {
        inner: jit_module,
        glsl_meta,
        func_names,
        func_ids,
        name_to_index,
        signatures,
        ir_param_counts,
        call_conv,
        pointer_type,
        float_mode: mode,
    })
}
