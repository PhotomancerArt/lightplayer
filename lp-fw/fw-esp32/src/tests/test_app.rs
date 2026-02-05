//! Minimal test app to isolate alignment issues
//!
//! This test app starts with a minimal GLSL compilation test (similar to esp32-glsl-jit)
//! and we'll add dependencies incrementally until we find what causes the alignment issue.
//!
//! Current setup: Basic GLSL compilation (similar to esp32-glsl-jit which works)
//!
//! Next steps to isolate the issue:
//! 1. Test this version - should work (boots successfully)
//! 2. Add lp-model dependency and use it
//! 3. Add lp-shared dependency and use it  
//! 4. Add lp-server dependency and use it
//! 5. Add fw-core dependency and use it
//! 6. When it breaks, we've found the culprit!

extern crate alloc;

use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};

use cranelift_codegen::isa::riscv32::isa_builder;
use cranelift_codegen::settings::{self, Configurable};
use lp_glsl_compiler::Compiler;
use lp_glsl_compiler::backend::transform::q32::{FixedPointFormat, Q32Transform};
use target_lexicon::Triple;

use esp_println::println;
use lp_glsl_compiler::backend::codegen::jit::build_jit_executable_memory_optimized;

/// Run minimal test app
///
/// Compiles a very simple GLSL shader to test if the basic setup works
/// before adding more dependencies.
pub async fn run_test_app() -> ! {
    esp_println::logger::init_logger_from_env();

    // Configure CPU clock to maximum speed (160MHz for ESP32-C6)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 300_000);

    println!("Test app: Initializing...");

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("======================================");
    println!("ESP32-C6 Minimal Test App");
    println!("Testing basic GLSL compilation");
    println!("======================================\n");

    // Very simple GLSL shader - just return a solid color
    let source = r#"
vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Simple solid red color
    return vec4(1.0, 0.0, 0.0, 1.0);
}
    "#;

    println!("GLSL Source:");
    println!("{}", source);
    println!();

    // Create RISC-V32 ISA
    println!("Step 1: Creating RISC-V32 ISA...");
    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "none").unwrap();
    flag_builder.set("is_pic", "false").unwrap();
    flag_builder.set("enable_verifier", "false").unwrap();
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .unwrap();
    let isa_flags = settings::Flags::new(flag_builder);

    let triple = Triple {
        architecture: target_lexicon::Architecture::Riscv32(
            target_lexicon::Riscv32Architecture::Riscv32imac,
        ),
        vendor: target_lexicon::Vendor::Unknown,
        operating_system: target_lexicon::OperatingSystem::None_,
        environment: target_lexicon::Environment::Unknown,
        binary_format: target_lexicon::BinaryFormat::Elf,
    };

    let isa = match isa_builder(triple).finish(isa_flags) {
        Ok(isa) => {
            println!("  ✓ ISA created");
            isa
        }
        Err(_e) => {
            panic!("ISA creation failed");
        }
    };

    // Compile GLSL
    println!("Step 2: Compiling GLSL to RISC-V machine code...");
    let mut compiler = Compiler::new();

    use lp_glsl_compiler::backend::target::Target;
    let flags = isa.flags().clone();
    let target = Target::HostJit {
        arch: None,
        flags,
        isa: None,
    };

    let gl_module = match compiler.compile_to_gl_module_jit(source, target) {
        Ok(module) => {
            println!("  ✓ GLSL compilation successful");
            module
        }
        Err(e) => {
            use alloc::format;
            let mut error_msg =
                format!("GLSL compilation failed!\nError: {}\n", e.message.as_str());

            if let Some(ref loc) = e.location {
                error_msg.push_str(&format!("At line {}, column {}\n", loc.line, loc.column));
            }

            if let Some(ref span) = e.span_text {
                error_msg.push_str(&format!("Source code: {}\n", span));
            }

            panic!("{}", error_msg.as_str());
        }
    };

    drop(compiler);

    // Apply q32 transform
    println!("Step 3: Applying q32 transform...");
    let q32_module =
        match gl_module.apply_transform(Q32Transform::new(FixedPointFormat::Fixed16x16)) {
            Ok(module) => {
                println!("  ✓ Q32 transform applied");
                module
            }
            Err(e) => {
                println!("Failed to apply q32 transform: {}", e.message.as_str());
                panic!("Q32 transform failed");
            }
        };

    // Build JIT executable
    println!("Step 4: Building executable...");
    let jit_module = match build_jit_executable_memory_optimized(q32_module) {
        Ok(module) => {
            println!("  ✓ JIT executable built");
            module
        }
        Err(e) => {
            println!("Failed to build executable: {}", e.message.as_str());
            panic!("JIT executable build failed");
        }
    };

    // Get function pointer
    let _func_ptr = match jit_module.get_function_ptr("main") {
        Ok(ptr) => {
            println!("  ✓ Function pointer obtained");
            ptr
        }
        Err(e) => {
            println!("Failed to get function pointer: {}", e.message.as_str());
            panic!("Function pointer not found");
        }
    };

    println!();
    println!("======================================");
    println!("Test app: SUCCESS!");
    println!("GLSL shader compiled and ready to run");
    println!("======================================");
    println!();
    println!("This test app works! Now we can add dependencies one by one");
    println!("to find what causes the alignment issue.");
    println!();

    // Keep running
    loop {
        Timer::after(embassy_time::Duration::from_secs(5)).await;
        println!("Test app still running...");
    }
}
