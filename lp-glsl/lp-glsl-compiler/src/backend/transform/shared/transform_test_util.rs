use crate::backend::transform::identity::IdentityTransform;
use crate::backend::transform::pipeline::Transform;
use alloc::string::ToString;
use cranelift_module::Linkage;
use cranelift_reader::{ParseOptions, parse_test};
use std::prelude::rust_2015::{String, Vec};

/// Normalize CLIF strings for comparison
fn normalize_clif(clif: &str) -> String {
    clif.lines()
        .map(|line| {
            let line = if let Some(comment_pos) = line.find(';') {
                &line[..comment_pos]
            } else {
                line
            };
            line.trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format all functions from a GlModule as CLIF text
fn format_module<M: cranelift_module::Module>(
    module: &crate::backend::module::gl_module::GlModule<M>,
) -> String {
    use crate::backend::util::clif_format::format_function;
    use hashbrown::HashMap;

    // Build mapping from func_id string to function name for updating external references
    let mut name_mapping: HashMap<String, String> = HashMap::new();
    for (name, gl_func) in &module.fns {
        name_mapping.insert(gl_func.func_id.as_u32().to_string(), name.clone());
    }

    let mut result = String::new();
    // Sort functions by name for deterministic output
    let mut funcs: Vec<_> = module.fns.iter().collect();
    funcs.sort_by_key(|(name, _)| *name);
    for (name, gl_func) in funcs {
        // Use format_function to convert User names back to TestCase names for comparison
        match format_function(&gl_func.function, name, &name_mapping) {
            Ok(func_text) => {
                result.push_str(&func_text);
                result.push('\n');
            }
            Err(_) => {
                // Fallback to write_function if format_function fails
                use cranelift_codegen::write_function;
                write_function(&mut result, &gl_func.function).unwrap();
                result.push('\n');
            }
        }
    }
    result
}

/// Parse CLIF module input, transform it, and return CLIF strings for comparison
fn parse_and_transform<T: Transform>(clif_input: &str, transform: T) -> (String, String) {
    // Parse the CLIF module
    let test_file =
        parse_test(clif_input, ParseOptions::default()).expect("Failed to parse CLIF module");

    // Build GlModule from parsed functions
    let target = crate::backend::target::Target::host_jit().unwrap();
    let mut original_module =
        crate::backend::module::gl_module::GlModule::<cranelift_jit::JITModule>::new_jit(
            target.clone(),
        )
        .unwrap();

    // Add all functions to the module
    for (func, _) in test_file.functions {
        let func_name = format!("{}", func.name);
        // Remove leading % if present
        let func_name = func_name.strip_prefix('%').unwrap_or(&func_name);
        original_module
            .add_function(func_name, Linkage::Local, func.signature.clone(), func)
            .expect("Failed to add function to module");
    }

    // Format the parsed module (before transformation)
    let parsed_buf = format_module(&original_module);

    // Transform the whole module
    let transformed_module = original_module
        .apply_transform(transform)
        .expect("Failed to apply transform");

    // Format the transformed module
    let transformed_buf = format_module(&transformed_module);

    (parsed_buf, transformed_buf)
}

/// Assert that identity transform produces identical CLIF output
pub fn assert_identity_transform(message: &str, clif_input: &str) {
    let (parsed_buf, transformed_buf) = parse_and_transform(clif_input, IdentityTransform);

    let normalized_parsed = normalize_clif(&parsed_buf);
    let normalized_transformed = normalize_clif(&transformed_buf);

    assert_eq!(
        normalized_parsed, normalized_transformed,
        "{message}\n\
     PARSED:\n{parsed_buf}\n\n\
     TRANSFORMED:\n{transformed_buf}"
    );
}

/// Assert that q32 transform produces identical CLIF output for code without floats
/// (i.e., q32 should be a no-op for integer-only code)
pub fn assert_nop_q32_transform(message: &str, clif_input: &str) {
    use crate::backend::transform::q32::{FixedPointFormat, Q32Transform};

    let transform = Q32Transform::new(FixedPointFormat::Fixed16x16);
    let (parsed_buf, transformed_buf) = parse_and_transform(clif_input, transform);

    let normalized_parsed = normalize_clif(&parsed_buf);
    let normalized_transformed = normalize_clif(&transformed_buf);

    assert_eq!(
        normalized_parsed, normalized_transformed,
        "{message}\n\
     PARSED:\n{parsed_buf}\n\n\
     TRANSFORMED:\n{transformed_buf}"
    );
}

/// Build and run a module, returning the result
#[cfg(feature = "emulator")]
fn build_and_run_module(
    gl_module: crate::backend::module::gl_module::GlModule<cranelift_object::ObjectModule>,
    transform_name: &str,
) -> i32 {
    use crate::backend::codegen::emu::EmulatorOptions;
    use cranelift_codegen::write_function;

    // Print transformed CLIF
    eprintln!("\n=== CLIF IR (AFTER {transform_name} transformation) ===");
    let mut funcs: Vec<_> = gl_module.fns.iter().collect();
    funcs.sort_by_key(|(name, _)| *name);
    for (name, gl_func) in funcs {
        eprintln!("function {name}:");
        let mut buf = String::new();
        write_function(&mut buf, &gl_func.function).unwrap();
        eprintln!("{buf}");
    }

    // Build executable
    let options = EmulatorOptions {
        max_memory: 1024 * 1024,
        stack_size: 64 * 1024,
        max_instructions: 10000,
        log_level: lp_riscv_emu::LogLevel::None,
    };

    eprintln!("\n=== Building executable ({transform_name}) ===");
    let mut executable = gl_module
        .build_executable(&options, None, None)
        .expect("Failed to build executable");

    // Call main function and get result
    eprintln!("\n=== Executing main function ({transform_name}) ===");
    executable
        .call_i32("main", &[])
        .expect("Failed to execute main function")
}

/// Compile GLSL, run it raw and with transforms, verify all results match
///
/// # Parameters
/// * `glsl_source` - GLSL source code (should have a function named "main" that calls the test function)
/// * `expected_int` - Expected integer result
#[cfg(feature = "emulator")]
pub fn run_int32_test(glsl_source: &str, expected_int: i32) {
    use crate::backend::target::Target;
    use crate::backend::transform::q32::{FixedPointFormat, Q32Transform};
    use crate::frontend::glsl_compiler::GlslCompiler;

    // Print input GLSL
    eprintln!("\n=== GLSL Source (INPUT) ===");
    eprintln!("{glsl_source}");

    let target = Target::riscv32_emulator().unwrap();
    let mut compiler = GlslCompiler::new();

    // Compile GLSL to raw module (no transform)
    eprintln!("\n=== Compiling GLSL (raw, no transform) ===");
    let raw_module = compiler
        .compile_to_gl_module_object(glsl_source, target.clone())
        .expect("Failed to compile GLSL");

    // Print CLIF before transformation
    eprintln!("\n=== CLIF IR (BEFORE transformation) ===");
    use cranelift_codegen::write_function;
    let mut funcs: Vec<_> = raw_module.fns.iter().collect();
    funcs.sort_by_key(|(name, _)| *name);
    for (name, gl_func) in funcs {
        eprintln!("function {name}:");
        let mut buf = String::new();
        write_function(&mut buf, &gl_func.function).unwrap();
        eprintln!("{buf}");
    }

    // Run raw (no transform)
    let raw_result = build_and_run_module(raw_module, "raw");

    // Compile GLSL for identity transform
    eprintln!("\n=== Compiling GLSL (identity transform) ===");
    let identity_module = compiler
        .compile_to_gl_module_object(glsl_source, target.clone())
        .expect("Failed to compile GLSL");
    let identity_module = identity_module
        .apply_transform(IdentityTransform)
        .expect("Failed to apply identity transform");
    let identity_result = build_and_run_module(identity_module, "identity");

    // Compile GLSL for q32 transform
    eprintln!("\n=== Compiling GLSL (q32 transform) ===");
    let q32_module = compiler
        .compile_to_gl_module_object(glsl_source, target.clone())
        .expect("Failed to compile GLSL");
    let q32_transform = Q32Transform::new(FixedPointFormat::Fixed16x16);
    let q32_module = q32_module
        .apply_transform(q32_transform)
        .expect("Failed to apply q32 transform");
    let q32_result = build_and_run_module(q32_module, "q32");

    // Verify all results match expected value
    eprintln!("\n=== Results ===");
    eprintln!("Expected: {expected_int}");
    eprintln!("Raw:      {raw_result}");
    eprintln!("Identity: {identity_result}");
    eprintln!("Q32:  {q32_result}");

    assert_eq!(
        raw_result, expected_int,
        "Raw execution failed: expected {expected_int}, got {raw_result}"
    );
    assert_eq!(
        identity_result, expected_int,
        "Identity transform failed: expected {expected_int}, got {identity_result}"
    );
    assert_eq!(
        q32_result, expected_int,
        "Q32 transform failed: expected {expected_int}, got {q32_result}"
    );
    assert_eq!(
        raw_result, identity_result,
        "Raw and identity results differ: raw={raw_result}, identity={identity_result}"
    );
    assert_eq!(
        raw_result, q32_result,
        "Raw and q32 results differ: raw={raw_result}, q32={q32_result}"
    );
}
