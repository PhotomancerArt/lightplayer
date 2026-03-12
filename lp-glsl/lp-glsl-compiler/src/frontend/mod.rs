//! GLSL compilation logic.
//!
//! This module contains the core compilation components that transform GLSL source
//! into Cranelift IR, including parsing, semantic analysis, code generation, and linking.

pub(crate) mod glsl_compiler;
pub(crate) mod pipeline;
// Public modules
pub mod codegen;
pub mod semantic;
pub mod src_loc;
pub mod src_loc_manager;

// Re-exports used by crate root; suppress unused warnings within this module.
#[allow(unused_imports, reason = "Re-exports for crate root")]
pub use glsl_compiler::GlslCompiler;
#[allow(unused_imports, reason = "Re-exports for crate root")]
pub use pipeline::{
    Backend, CompilationPipeline, CompiledShader, ParseResult, SemanticResult, TransformationPass,
    parse_program_with_registry,
};

// ============================================================================
// Public API functions
// ============================================================================

#[cfg(feature = "emulator")]
use crate::backend::codegen::emu::EmulatorOptions;
use crate::backend::module::gl_module::GlModule;
use crate::backend::target::Target;
use crate::error::{ErrorCode, GlslDiagnostics, GlslError};
use crate::exec::executable::{GlslExecutable, GlslOptions, RunMode};
#[cfg(not(feature = "std"))]
use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::JITModule;
use cranelift_module::Module;
#[cfg(feature = "emulator")]
use cranelift_object::ObjectModule;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Build target for JIT compilation (shared by compile_glsl_to_gl_module_jit and glsl_jit_streaming)
fn build_target_for_jit(options: &GlslOptions) -> Result<Target, GlslDiagnostics> {
    if let Some(ref t) = options.target_override {
        return Ok(t.clone());
    }
    #[cfg(feature = "std")]
    {
        match &options.run_mode {
            RunMode::HostJit => Target::host_jit().map_err(GlslDiagnostics::from),
            RunMode::Emulator { .. } => Err(GlslDiagnostics::from(GlslError::new(
                ErrorCode::E0400,
                "Emulator mode not supported for JIT compilation",
            ))),
        }
    }
    #[cfg(not(feature = "std"))]
    {
        match &options.run_mode {
            RunMode::HostJit => {
                let mut flag_builder = settings::builder();
                flag_builder.set("is_pic", "false").map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("failed to set is_pic: {e}"),
                    )
                })?;
                flag_builder
                    .set("use_colocated_libcalls", "false")
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            alloc::format!("failed to set use_colocated_libcalls: {e}"),
                        )
                    })?;
                flag_builder
                    .set("enable_multi_ret_implicit_sret", "true")
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            alloc::format!("failed to set enable_multi_ret_implicit_sret: {e}"),
                        )
                    })?;
                flag_builder
                    .set("regalloc_algorithm", "single_pass")
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            alloc::format!("failed to set regalloc_algorithm: {e}"),
                        )
                    })?;
                let flags = settings::Flags::new(flag_builder);
                Ok(Target::HostJit {
                    arch: None,
                    flags,
                    isa: None,
                })
            }
            RunMode::Emulator { .. } => Err(GlslDiagnostics::from(GlslError::new(
                ErrorCode::E0400,
                "Emulator mode not supported for JIT compilation",
            ))),
        }
    }
}

/// Compile GLSL to GlModule<JITModule> (internal, reusable)
/// This is the core compilation step for JIT execution
pub fn compile_glsl_to_gl_module_jit(
    source: &str,
    options: &GlslOptions,
) -> Result<GlModule<JITModule>, GlslDiagnostics> {
    options.validate().map_err(GlslDiagnostics::from)?;
    use crate::exec::executable::DecimalFormat;

    let target = build_target_for_jit(options)?;

    use crate::frontend::codegen::numeric::{FloatStrategy, NumericMode, Q32Strategy};
    let numeric_mode = match options.decimal_format {
        DecimalFormat::Q32 => NumericMode::Q32(Q32Strategy::new(options.q32_opts)),
        DecimalFormat::Float => NumericMode::Float(FloatStrategy),
    };
    let mut compiler = GlslCompiler::new();
    let module =
        compiler.compile_to_gl_module_jit(source, target, options.max_errors, numeric_mode)?;

    Ok(module)
}

/// Compile GLSL to GlModule<ObjectModule> (internal, reusable)
/// This is the core compilation step for emulator execution
/// Returns the module along with CLIF IR strings for debugging
#[cfg(feature = "emulator")]
pub fn compile_glsl_to_gl_module_object(
    source: &str,
    options: &GlslOptions,
) -> Result<(GlModule<ObjectModule>, Option<String>, Option<String>), GlslDiagnostics> {
    #[cfg(feature = "std")]
    use crate::backend::util::clif_format::format_clif_module;
    use crate::exec::executable::DecimalFormat;

    options.validate().map_err(GlslDiagnostics::from)?;

    let mut compiler = GlslCompiler::new();

    // Determine target based on run mode
    let target = match &options.run_mode {
        RunMode::Emulator { .. } => Target::riscv32_emulator()?,
        RunMode::HostJit => {
            return Err(GlslDiagnostics::from(GlslError::new(
                crate::error::ErrorCode::E0400,
                "HostJit mode not supported for object compilation",
            )));
        }
    };

    use crate::frontend::codegen::numeric::{FloatStrategy, NumericMode, Q32Strategy};
    let numeric_mode = match options.decimal_format {
        DecimalFormat::Q32 => NumericMode::Q32(Q32Strategy::new(options.q32_opts)),
        DecimalFormat::Float => NumericMode::Float(FloatStrategy),
    };
    let module =
        compiler.compile_to_gl_module_object(source, target, options.max_errors, numeric_mode)?;

    #[cfg(feature = "std")]
    let clif = format_clif_module(&module).ok();
    #[cfg(not(feature = "std"))]
    let clif = None;

    Ok((module, clif.clone(), clif))
}

/// Compile and JIT execute GLSL
/// Works in both std and no_std environments
pub fn glsl_jit(
    source: &str,
    options: GlslOptions,
) -> Result<Box<dyn GlslExecutable>, GlslDiagnostics> {
    let module = compile_glsl_to_gl_module_jit(source, &options)?;
    let jit = if options.memory_optimized {
        crate::backend::codegen::jit::build_jit_executable_memory_optimized(module)
    } else {
        crate::backend::codegen::jit::build_jit_executable(module)
    }?;
    Ok(alloc::boxed::Box::new(jit))
}

/// Compile and JIT execute GLSL using streaming per-function pipeline.
///
/// Compiles functions one at a time (smallest AST first), freeing each function's
/// IR before starting the next. Reduces peak heap usage on memory-constrained
/// targets (e.g. ESP32). Uses Q32 fixed-point format.
pub fn glsl_jit_streaming(
    source: &str,
    options: GlslOptions,
) -> Result<Box<dyn GlslExecutable>, GlslDiagnostics> {
    use crate::backend::builtins::registry::BuiltinId;
    use crate::exec::executable::DecimalFormat;
    use crate::frontend::codegen::numeric::{NumericMode, Q32Strategy};
    use crate::frontend::codegen::signature::SignatureBuilder;
    use crate::frontend::semantic::MAIN_FUNCTION_NAME;
    use cranelift_module::{FuncId, FuncOrDataId, Linkage};
    use hashbrown::HashMap;

    options.validate().map_err(GlslDiagnostics::from)?;

    let target = build_target_for_jit(&options)?;

    let semantic_result = CompilationPipeline::parse_and_analyze(source, options.max_errors)?;
    let typed_ast = semantic_result.typed_ast;

    if options.decimal_format != DecimalFormat::Q32 {
        return Err(GlslDiagnostics::from(GlslError::new(
            ErrorCode::E0400,
            "Streaming JIT only supports Q32 format",
        )));
    }

    let numeric_mode = NumericMode::Q32(Q32Strategy::new(options.q32_opts));

    let mut target_for_isa = target.clone();
    let isa_ref = target_for_isa.create_isa().map_err(GlslDiagnostics::from)?;
    let pointer_type = isa_ref.pointer_type();
    let triple = isa_ref.triple();

    let mut module =
        GlModule::new_jit(target, DecimalFormat::Q32).map_err(GlslDiagnostics::from)?;

    let mut sorted_names: Vec<String> = typed_ast
        .user_functions
        .iter()
        .map(|f| f.name.clone())
        .collect();
    if typed_ast.main_function.is_some() {
        sorted_names.push(String::from(MAIN_FUNCTION_NAME));
    }
    sorted_names.sort();

    let num_functions = sorted_names.len();
    let num_builtins = BuiltinId::all().len();
    let mut func_ids: HashMap<String, FuncId> =
        HashMap::with_capacity(num_functions + num_builtins);

    struct StreamingFuncInfo {
        name: String,
        func_id: FuncId,
        ast_size: usize,
    }

    let mut sorted_functions: Vec<StreamingFuncInfo> = Vec::with_capacity(num_functions);

    for name in &sorted_names {
        let typed_func = typed_ast
            .user_functions
            .iter()
            .find(|f| &f.name == name)
            .or_else(|| {
                typed_ast
                    .main_function
                    .as_ref()
                    .filter(|_| *name == MAIN_FUNCTION_NAME)
            })
            .ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("Function '{name}' not found"),
                ))
            })?;

        let sig = SignatureBuilder::build_with_triple(
            &typed_func.return_type,
            &typed_func.parameters,
            pointer_type,
            triple,
            numeric_mode.scalar_type(),
        );

        let linkage = if *name == MAIN_FUNCTION_NAME {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let func_id = module
            .module_mut_internal()
            .declare_function(name, linkage, &sig)
            .map_err(|e| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("Failed to declare '{name}': {e}"),
                ))
            })?;

        func_ids.insert(name.clone(), func_id);

        sorted_functions.push(StreamingFuncInfo {
            name: name.clone(),
            func_id,
            ast_size: typed_func.ast_node_count(),
        });
    }

    for builtin in BuiltinId::all() {
        let name = builtin.name();
        if let Some(FuncOrDataId::Func(func_id)) =
            module.module_internal().declarations().get_name(name)
        {
            func_ids.insert(String::from(name), func_id);
        }
    }

    sorted_functions.sort_by_key(|f| f.ast_size);

    use crate::frontend::src_loc::GlSourceMap;
    use crate::frontend::src_loc_manager::SourceLocManager;

    let mut source_loc_manager = SourceLocManager::new();
    let mut source_map = GlSourceMap::new();
    let main_file_id = source_map.add_file(
        crate::frontend::src_loc::GlFileSource::Synthetic(String::from("main.glsl")),
        String::from(source),
    );

    let mut glsl_signatures = HashMap::with_capacity(num_functions);

    let mut compiler = GlslCompiler::new();
    let mut ctx = module.module_internal().make_context();

    for func_info in &sorted_functions {
        let typed_func = typed_ast
            .user_functions
            .iter()
            .find(|f| f.name == func_info.name)
            .or_else(|| {
                typed_ast
                    .main_function
                    .as_ref()
                    .filter(|_| func_info.name == MAIN_FUNCTION_NAME)
            })
            .ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("Function '{}' not found", func_info.name),
                ))
            })?;

        let source_text_for_main = if func_info.name == MAIN_FUNCTION_NAME {
            Some(source)
        } else {
            None
        };

        let func = compiler
            .compile_single_function_to_clif(
                typed_func,
                func_info.func_id,
                &func_ids,
                &typed_ast.function_registry,
                &typed_ast.global_constants,
                &mut module,
                isa_ref.as_ref(),
                &mut source_loc_manager,
                &mut source_map,
                main_file_id,
                source_text_for_main,
                numeric_mode.clone(),
            )
            .map_err(GlslDiagnostics::from)?;

        let return_type = typed_func.return_type.clone();
        let parameters = typed_func.parameters.clone();

        ctx.func = func;
        module
            .module_mut_internal()
            .define_function(func_info.func_id, &mut ctx)
            .map_err(|e| {
                let error_str = alloc::format!("{e}");
                let error_msg = if error_str.contains("Verifier errors") {
                    #[cfg(feature = "cranelift-verifier")]
                    {
                        let module_ref = module.module_internal();
                        let isa = module_ref.isa();
                        use cranelift_codegen::verify_function;
                        if let Err(verifier_errors) = verify_function(&ctx.func, isa) {
                            #[cfg(feature = "std")]
                            {
                                use cranelift_codegen::print_errors::pretty_verifier_error;
                                alloc::format!(
                                    "Failed to define function '{}': Verifier errors\n\n{}",
                                    func_info.name,
                                    pretty_verifier_error(&ctx.func, None, verifier_errors)
                                )
                            }
                            #[cfg(not(feature = "std"))]
                            {
                                alloc::format!(
                                    "Failed to define function '{}': Verifier errors\n\n{}",
                                    func_info.name,
                                    verifier_errors
                                )
                            }
                        } else {
                            alloc::format!("Failed to define function '{}': {e}", func_info.name)
                        }
                    }
                    #[cfg(not(feature = "cranelift-verifier"))]
                    {
                        alloc::format!("Failed to define function '{}': {e}", func_info.name)
                    }
                } else {
                    alloc::format!("Failed to define function '{}': {e}", func_info.name)
                };
                GlslDiagnostics::from(GlslError::new(ErrorCode::E0400, error_msg))
            })?;
        {
            let module_ref = module.module_internal();
            module_ref.clear_context(&mut ctx);
        }

        glsl_signatures.insert(
            func_info.name.clone(),
            crate::frontend::semantic::functions::FunctionSignature {
                name: func_info.name.clone(),
                return_type,
                parameters,
            },
        );
    }

    let cranelift_signatures: HashMap<String, cranelift_codegen::ir::Signature> = glsl_signatures
        .iter()
        .map(|(name, glsl_sig)| {
            let sig = SignatureBuilder::build_with_triple(
                &glsl_sig.return_type,
                &glsl_sig.parameters,
                pointer_type,
                triple,
                numeric_mode.scalar_type(),
            );
            (name.clone(), sig)
        })
        .collect();

    let jit_func_id_map: HashMap<String, FuncId> = sorted_functions
        .iter()
        .map(|f| (f.name.clone(), f.func_id))
        .collect();

    let jit = crate::backend::codegen::jit::build_jit_executable_streaming(
        module,
        &jit_func_id_map,
        glsl_signatures,
        cranelift_signatures,
    )
    .map_err(GlslDiagnostics::from)?;

    Ok(alloc::boxed::Box::new(jit))
}

/// Compile and execute GLSL in RISC-V 32-bit emulator
/// Requires `emulator` feature flag to be enabled
#[cfg(feature = "emulator")]
pub fn glsl_emu_riscv32(
    source: &str,
    options: GlslOptions,
) -> Result<Box<dyn GlslExecutable>, GlslDiagnostics> {
    glsl_emu_riscv32_with_metadata(source, options, None)
}

/// Requires `emulator` feature flag to be enabled
/// Version that accepts source file path for better error messages
#[cfg(feature = "emulator")]
pub fn glsl_emu_riscv32_with_metadata(
    source: &str,
    options: GlslOptions,
    source_file_path: Option<String>,
) -> Result<Box<dyn GlslExecutable>, GlslDiagnostics> {
    // Compile to GlModule (direct emission; single CLIF output)
    let (module, original_clif, transformed_clif) =
        compile_glsl_to_gl_module_object(source, &options)?;

    let emulator_options = match &options.run_mode {
        RunMode::Emulator {
            max_memory,
            stack_size,
            max_instructions,
            log_level,
        } => {
            use lp_riscv_emu::LogLevel;
            EmulatorOptions {
                max_memory: *max_memory,
                stack_size: *stack_size,
                max_instructions: *max_instructions,
                log_level: log_level.unwrap_or(LogLevel::None),
            }
        }
        _ => {
            return Err(GlslDiagnostics::from(GlslError::new(
                crate::error::ErrorCode::E0400,
                "Invalid run mode for emulator",
            )));
        }
    };

    // Note: source_file_path is stored in GlModule but not currently used in build_emu_executable
    // This can be added later if needed
    let _ = source_file_path;

    module
        .build_executable(&emulator_options, original_clif, transformed_clif)
        .map_err(GlslDiagnostics::from)
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::*;
    use crate::exec::GlslValue;
    use crate::exec::executable::{DecimalFormat, GlslOptions};
    use crate::exec::execute_fn::execute_function;

    fn q32_jit_options() -> GlslOptions {
        let mut opts = GlslOptions::jit();
        opts.decimal_format = DecimalFormat::Q32;
        opts
    }

    #[test]
    fn test_glsl_jit_streaming_basic() {
        let source = r#"
            vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
                return vec4(1.0, 0.0, 0.0, 1.0);
            }
        "#;
        let options = q32_jit_options();
        let executable = glsl_jit_streaming(source, options).unwrap();
        assert!(executable.get_direct_call_info("main").is_some());
    }

    #[test]
    fn test_glsl_jit_streaming_multi_function() {
        let source = r#"
            float helper(float x) {
                return x * 2.0;
            }
            vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
                float v = helper(0.5);
                return vec4(v, 0.0, 0.0, 1.0);
            }
        "#;
        let options = q32_jit_options();
        let executable = glsl_jit_streaming(source, options).unwrap();
        assert!(executable.get_direct_call_info("main").is_some());
    }

    #[test]
    fn test_streaming_returns_correct_value() {
        let source = r#"
            vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
                return vec4(1.0, 0.0, 0.0, 1.0);
            }
        "#;
        let options = q32_jit_options();
        let args = [
            GlslValue::Vec2([0.0, 0.0]),
            GlslValue::Vec2([256.0, 256.0]),
            GlslValue::F32(0.0),
        ];
        let mut streaming = glsl_jit_streaming(source, options.clone()).unwrap();
        let mut batch = glsl_jit(source, options).unwrap();

        let streaming_result = execute_function(&mut *streaming, "main", &args).unwrap();
        let batch_result = execute_function(&mut *batch, "main", &args).unwrap();

        assert!(streaming_result.approx_eq(&batch_result, 0.01));
    }

    #[test]
    fn test_streaming_multi_function_cross_calls() {
        let source = r#"
            float double_it(float x) {
                return x * 2.0;
            }
            float quad_it(float x) {
                return double_it(double_it(x));
            }
            vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
                float v = quad_it(0.25);
                return vec4(v, 0.0, 0.0, 1.0);
            }
        "#;
        let options = q32_jit_options();
        let args = [
            GlslValue::Vec2([0.0, 0.0]),
            GlslValue::Vec2([256.0, 256.0]),
            GlslValue::F32(0.0),
        ];
        let mut streaming = glsl_jit_streaming(source, options.clone()).unwrap();
        let mut batch = glsl_jit(source, options).unwrap();

        let streaming_result = execute_function(&mut *streaming, "main", &args).unwrap();
        let batch_result = execute_function(&mut *batch, "main", &args).unwrap();

        assert!(streaming_result.approx_eq(&batch_result, 0.01));
    }
}
