use crate::backend::transform::q32::types::float_to_fixed16x16;

/// Convert float to 16.16 fixed-point for comparison
#[allow(dead_code, reason = "Test utility function")]
fn float_to_q32(f: f32) -> i32 {
    float_to_fixed16x16(f)
}

/// Convert fixed-point back to float
#[allow(dead_code, reason = "Test utility function")]
fn q32_to_float(fixed: i32) -> f32 {
    fixed as f32 / 65536.0
}

/// Parse CLIF, transform it, run on emulator, and verify result matches expected float
///
/// # Parameters
/// * `clif_input` - CLIF function text (should have a function named "main")
/// * `expected_float` - Expected float result (will be compared as fixed-point)
#[cfg(feature = "emulator")]
pub fn run_q32_test(clif_input: &str, expected_float: f32) {
    // Print input CLIF
    eprintln!("\n=== CLIF IR (INPUT) ===");
    eprintln!("{clif_input}");

    // Parse CLIF
    let test_file =
        parse_test(clif_input, ParseOptions::default()).expect("Failed to parse CLIF module");

    // Store original functions for printing
    let mut original_funcs = Vec::new();
    let mut gl_module = {
        let target = Target::riscv32_emulator().unwrap();
        GlModule::<ObjectModule>::new_object(target).unwrap()
    };

    // Add all functions to the module
    for (func, _) in test_file.functions {
        let func_name = format!("{}", func.name);
        let func_name = func_name.strip_prefix('%').unwrap_or(&func_name);
        original_funcs.push((String::from(func_name), func.clone()));
        gl_module
            .add_function(func_name, Linkage::Local, func.signature.clone(), func)
            .expect("Failed to add function to module");
    }

    use crate::backend::codegen::emu::EmulatorOptions;
    use crate::backend::module::gl_module::GlModule;
    use crate::backend::target::Target;
    use crate::backend::transform::q32::Q32Transform;
    use cranelift_codegen::write_function;
    use cranelift_module::Linkage;
    #[cfg(feature = "emulator")]
    use cranelift_object::ObjectModule;
    use cranelift_reader::{ParseOptions, parse_test};
    use std::prelude::rust_2015::{String, Vec}; // Print parsed CLIF (before transformation)
    eprintln!("\n=== CLIF IR (BEFORE transformation) ===");
    for (name, func) in &original_funcs {
        eprintln!("function {name}:");
        let mut buf = String::new();
        write_function(&mut buf, func).unwrap();
        eprintln!("{buf}");
    }

    // Apply q32 transform
    let transform = Q32Transform::default();
    let transformed_module = gl_module
        .apply_transform(transform)
        .expect("Failed to apply q32 transform");

    // Print transformed CLIF (after transformation)
    eprintln!("\n=== CLIF IR (AFTER transformation) ===");
    for (name, _) in &original_funcs {
        if let Some(gl_func) = transformed_module.get_func(name) {
            eprintln!("function {name}:");
            let mut buf = String::new();
            write_function(&mut buf, &gl_func.function).unwrap();
            eprintln!("{buf}");
        }
    }

    // Build executable
    let options = EmulatorOptions {
        max_memory: 1024 * 1024,
        stack_size: 64 * 1024,
        max_instructions: 10000,
        log_level: lp_riscv_emu::LogLevel::None,
    };

    eprintln!("\n=== Building executable ===");
    let mut executable = transformed_module
        .build_executable(&options, None, None)
        .expect("Failed to build executable");

    // Call main function and get result
    eprintln!("\n=== Executing main function ===");
    let result_i32 = match executable.call_i32("main", &[]) {
        Ok(result) => result,
        Err(e) => {
            // On error, output emulator state and execution log like filetests do
            if let Some(ref emulator_state) = executable.format_emulator_state() {
                eprintln!("{emulator_state}");
            }
            panic!("Failed to execute main function: {e}");
        }
    };

    // Convert expected float to fixed-point
    let expected_fixed = float_to_q32(expected_float);

    eprintln!("\n=== Results ===");
    eprintln!("Expected: {expected_fixed} (fixed-point) = {expected_float} (float)");
    eprintln!(
        "Got:      {} (fixed-point) = {} (float)",
        result_i32,
        q32_to_float(result_i32)
    );

    // Compare results (allow small tolerance for rounding)
    let tolerance = 1; // 1 fixed-point unit ≈ 0.000015
    if (result_i32 - expected_fixed).abs() > tolerance {
        // On failure, output emulator state and execution log like filetests do
        if let Some(ref emulator_state) = executable.format_emulator_state() {
            eprintln!("{emulator_state}");
        }
        panic!(
            "Expected fixed-point value {} (float {}), got {} (float {})\n\n\
             See debug output above for CLIF before/after transformation.",
            expected_fixed,
            expected_float,
            result_i32,
            q32_to_float(result_i32)
        );
    }
}
